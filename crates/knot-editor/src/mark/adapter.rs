// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl MarkReadAdapter {
    pub fn new(limits: MarkReadAdapterLimits) -> Self {
        Self {
            limits: limits.clamped(),
            documents: BTreeMap::new(),
            current_content: BTreeMap::new(),
        }
    }

    /// Create or update an owner-selected Mark path. A path keeps its existing
    /// immutable history when its access rule changes, but cannot be rebound to
    /// another native publication.
    pub fn configure_export(
        &mut self,
        path: impl Into<String>,
        publication: PublicationId,
        access: MarkReadAccess,
    ) -> Result<(), MarkAdapterError> {
        let path = validate_document_path(path.into())?;
        if let Some(document) = self.documents.get_mut(&path) {
            if document.publication != publication {
                return Err(MarkAdapterError::RebindPath);
            }
            document.access = access;
            return Ok(());
        }
        self.documents.insert(
            path,
            MarkDocument {
                access,
                publication,
                versions: Vec::new(),
            },
        );
        Ok(())
    }

    /// Withdraw a Mark export and all future reads through this adapter.
    pub fn withdraw(&mut self, path: &str) -> bool {
        let removed = self.documents.remove(path).is_some();
        if removed {
            self.rebuild_content_index();
        }
        removed
    }

    /// Convert and append one explicit native source snapshot.
    pub fn snapshot(
        &mut self,
        path: &str,
        source: &KnotPublishedDocument,
        modified: MarkTimestamp,
    ) -> Result<MarkSnapshotOutcome, MarkAdapterError> {
        let document = self
            .documents
            .get_mut(path)
            .ok_or(MarkAdapterError::UnknownExport)?;
        if document.publication != source.publication {
            return Err(MarkAdapterError::PublicationMismatch);
        }
        if !source.body_digest_matches() {
            return Err(MarkAdapterError::InvalidSourceDigest);
        }
        let format = DocumentFormat::from_media_type(&source.media_type)
            .filter(|format| matches!(*format, DocumentFormat::Knot | DocumentFormat::Djot))
            .ok_or_else(|| MarkAdapterError::UnsupportedSource(source.media_type.clone()))?;
        let body = format
            .to_commonmark(
                &format!("knot-publication:{}", source.publication.as_uuid()),
                &source.body,
            )
            .map_err(MarkAdapterError::CommonMarkConversion)?;
        if body.len() > self.limits.max_document_bytes {
            return Err(MarkAdapterError::DocumentTooLarge);
        }
        if let Some(current) = document.versions.last()
            && current.body == body
        {
            return Ok(MarkSnapshotOutcome::Unchanged(current.id));
        }

        let next = document
            .versions
            .len()
            .checked_add(1)
            .and_then(|value| u64::try_from(value).ok())
            .map(MarkVersionId)
            .ok_or(MarkAdapterError::VersionOverflow)?;
        let previous_hash = document.versions.last().map(|version| version.etag);
        let stored = mark_stored_version(next, previous_hash, source, &body);
        let version = MarkVersion {
            id: next,
            modified,
            source_operation: source.operation,
            source_media_type: source.media_type.clone(),
            etag: sha256(&stored),
            content_hash: sha256(&body),
            body,
            stored,
        };
        document.versions.push(version);
        self.rebuild_content_index();
        Ok(MarkSnapshotOutcome::Created(next))
    }

    /// Materialize the holder's *currently eligible* native publication and
    /// append it only when the owner explicitly invokes this method.
    pub async fn snapshot_current<B>(
        &mut self,
        path: &str,
        catalog: &KnotPublishCatalog,
        store: &KnotSyncStore<B>,
        vault: &KnotVault,
        publication: PublicationId,
        modified: MarkTimestamp,
    ) -> Result<MarkSnapshotOutcome, MarkAdapterError>
    where
        B: Backend + Clone,
    {
        let source = catalog
            .current_for_mark_export(store, vault, publication)
            .await
            .map_err(MarkAdapterError::NativeSource)?
            .ok_or(MarkAdapterError::NativeNotAvailable)?;
        self.snapshot(path, &source, modified)
    }

    /// Handle one complete Mark request. Absence and a protected export denied
    /// by this adapter both use `not-found`, preserving the native lane's
    /// catalog non-disclosure discipline.
    pub fn respond(&self, request: &[u8]) -> MarkResponse {
        match decode_mark_request(request) {
            Ok(request) => self.respond_to(request),
            Err(_) => MarkResponse::bad_request(),
        }
    }

    pub(super) fn respond_to(&self, request: MarkRequest) -> MarkResponse {
        match request {
            MarkRequest::Fetch {
                path,
                auth,
                if_none_match,
                if_modified_since,
            } => self.fetch(
                &path,
                auth.as_deref(),
                if_none_match.as_deref(),
                if_modified_since,
            ),
            MarkRequest::Versions { path, auth } => self.versions(&path, auth.as_deref()),
            MarkRequest::Other { .. } => MarkResponse::not_permitted(),
        }
    }

    pub(super) fn fetch(
        &self,
        path: &str,
        auth: Option<&str>,
        if_none_match: Option<&str>,
        if_modified_since: Option<MarkTimestamp>,
    ) -> MarkResponse {
        if path == "/health" {
            return MarkResponse::ok(
                [(
                    "content-hash",
                    format!("sha256-{}", hex(&sha256(HEALTH_BODY))),
                )],
                HEALTH_BODY.to_vec(),
            );
        }
        let (path, requested_version) = match resolve_mark_path(path) {
            Ok(value) => value,
            Err(_) => return MarkResponse::not_found(),
        };
        let path = if let Some(content_hash) = parse_content_hash(&path) {
            let Some(path) = self.current_content.get(&content_hash) else {
                return MarkResponse::not_found();
            };
            path.as_str()
        } else {
            path.as_str()
        };
        let Some(document) = self.documents.get(path) else {
            return MarkResponse::not_found();
        };
        if !document.access.allows(auth) {
            return MarkResponse::not_found();
        }
        let version = match requested_version {
            Some(id) => document.versions.get(id.0.saturating_sub(1) as usize),
            None => document.versions.last(),
        };
        let Some(version) = version else {
            return MarkResponse::not_found();
        };
        if if_none_match.is_some_and(|candidate| candidate == version.etag())
            || if_modified_since.is_some_and(|since| version.modified <= since)
        {
            return MarkResponse::not_modified();
        }
        let mut metadata = vec![
            ("modified", version.modified.as_str().to_string()),
            ("etag", version.etag()),
            ("version", version.id.get().to_string()),
            ("content-hash", version.content_hash()),
        ];
        if requested_version.is_some() {
            metadata.push(("current-version", document.versions.len().to_string()));
        }
        MarkResponse::ok(metadata, version.body.clone())
    }

    pub(super) fn versions(&self, path: &str, auth: Option<&str>) -> MarkResponse {
        let Ok((path, requested_version)) = resolve_mark_path(path) else {
            return MarkResponse::not_found();
        };
        if requested_version.is_some() {
            return MarkResponse::not_found();
        }
        let Some(document) = self.documents.get(&path) else {
            return MarkResponse::not_found();
        };
        if !document.access.allows(auth) || document.versions.is_empty() {
            return MarkResponse::not_found();
        }
        let mut body = format!("# Version History: {path}\n");
        for version in document.versions.iter().rev() {
            body.push_str(&format!(
                "- [v{}]({path}/v{}) - {}\n",
                version.id.get(),
                version.id.get(),
                version.modified.as_str()
            ));
        }
        let chain_valid = mark_chain_is_valid(&document.versions);
        let mut metadata = vec![
            ("total", document.versions.len().to_string()),
            ("current", document.versions.len().to_string()),
            ("chain-valid", chain_valid.to_string()),
        ];
        if !chain_valid {
            metadata.push(("chain-error", "stored version hash chain is broken".into()));
        }
        MarkResponse::ok(metadata, body.into_bytes())
    }

    pub(super) fn rebuild_content_index(&mut self) {
        self.current_content.clear();
        for (path, document) in &self.documents {
            if let Some(version) = document.versions.last() {
                self.current_content
                    .entry(version.content_hash)
                    .or_insert_with(|| path.clone());
            }
        }
    }
}
