// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Clip evidence retained in the iroh blob store, behind `peers`.
//!
//! The reference types, the citation reader and the file-backed store stay in
//! the parent module and build without a peer transport.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use chirograph::KnotClipArtifactV1;
use transport::{BlobLease, BlobReadAuthorizer, BlobScope, BlobStore};

use super::{KnotClipEvidenceRef, KnotClipEvidenceStore};

/// Clip evidence retained in the Murm-owned iroh blob store.
pub struct BlobClipEvidenceStore {
    blobs: Arc<BlobStore>,
    max_artifact_bytes: u64,
    custody: Option<(BlobReadAuthorizer, BlobScope)>,
}

impl BlobClipEvidenceStore {
    /// Open the source-owned actor used by synchronous endpoint adapters.
    ///
    /// The associated constructor retains its old spelling for callers, but
    /// returns the resident port rather than hiding a runtime inside this
    /// store handle.
    pub fn open(
        root: impl AsRef<Path>,
        max_artifact_bytes: u64,
    ) -> Result<KnotContentRetentionPort, String> {
        KnotContentRetentionPort::open(root, max_artifact_bytes)
    }

    /// Open a persistent store on the resident host's async runtime.
    pub async fn open_async(
        root: impl AsRef<Path>,
        max_artifact_bytes: u64,
    ) -> Result<Self, String> {
        let blobs = BlobStore::open(root)
            .await
            .map(Arc::new)
            .map_err(|error| format!("could not open clip evidence blob store: {error}"))?;
        Ok(Self {
            blobs,
            max_artifact_bytes,
            custody: None,
        })
    }

    /// Shared store handle for a resident p2p transport.
    ///
    /// The direct async handle and [`KnotContentRetentionPort`] can both expose
    /// the same resident blob actor to the transport host. The port keeps that
    /// actor and its runtime under source-owned shutdown.
    pub fn resident_blob_store(&self) -> Result<Arc<BlobStore>, String> {
        Ok(Arc::clone(&self.blobs))
    }

    /// Retain from an async resident host without nesting runtimes.
    pub async fn retain_async(
        &self,
        artifact: &KnotClipArtifactV1,
    ) -> Result<KnotClipEvidenceRef, String> {
        let scope = self.custody.as_ref().map(|(_, scope)| *scope);
        let reference =
            retain_blob_artifact(&self.blobs, self.max_artifact_bytes, artifact, scope).await?;
        if let Some((authority, scope)) = &self.custody {
            authority.retain(*scope, reference.blob_hash()?);
        }
        Ok(reference)
    }
}

enum RetentionCommand {
    Retain {
        artifact: KnotClipArtifactV1,
        reply: tokio::sync::oneshot::Sender<Result<KnotClipEvidenceRef, String>>,
    },
    Close,
}

struct RetentionPortInner {
    commands: mpsc::Sender<RetentionCommand>,
    join: Mutex<Option<JoinHandle<()>>>,
    blobs: Arc<BlobStore>,
}

impl Drop for RetentionPortInner {
    fn drop(&mut self) {
        let _ = self.commands.send(RetentionCommand::Close);
        if let Some(join) = self
            .join
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            let _ = join.join();
        }
    }
}

/// Cloneable source-owned port to the async clip-evidence actor.
///
/// Synchronous Graphshell endpoint traits submit work through this port; the
/// blob store and its Tokio runtime stay in one resident actor. Dropping the
/// final port flushes and shuts down the store, then joins the actor thread.
#[derive(Clone)]
pub struct KnotContentRetentionPort {
    inner: Arc<RetentionPortInner>,
}

impl KnotContentRetentionPort {
    /// Open one persistent actor-backed retention service.
    pub fn open(root: impl AsRef<Path>, max_artifact_bytes: u64) -> Result<Self, String> {
        Self::open_inner(
            RetentionBacking::Open(root.as_ref().to_path_buf()),
            max_artifact_bytes,
            None,
        )
    }

    /// Open retention with serving custody bound to one domain scope.
    ///
    /// The binding lands only after the bytes have been retained and flushed,
    /// so an authorized reader is never pointed at content the store lacks.
    pub fn open_scoped(
        root: impl AsRef<Path>,
        max_artifact_bytes: u64,
        authority: BlobReadAuthorizer,
        scope: BlobScope,
    ) -> Result<Self, String> {
        Self::open_inner(
            RetentionBacking::Open(root.as_ref().to_path_buf()),
            max_artifact_bytes,
            Some((authority, scope)),
        )
    }

    /// Borrow the resident's physical blob store while retaining Knot's own
    /// scoped custody and serving authority.
    ///
    /// Dropping the port flushes this lane's writes but does not shut down the
    /// resident store. The process owner remains its sole lifetime authority.
    pub fn borrow_scoped(
        blobs: Arc<BlobStore>,
        max_artifact_bytes: u64,
        authority: BlobReadAuthorizer,
        scope: BlobScope,
    ) -> Result<Self, String> {
        Self::open_inner(
            RetentionBacking::Borrowed(blobs),
            max_artifact_bytes,
            Some((authority, scope)),
        )
    }

    fn open_inner(
        backing: RetentionBacking,
        max_artifact_bytes: u64,
        custody: Option<(BlobReadAuthorizer, BlobScope)>,
    ) -> Result<Self, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready, opened) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("knot-content-retention".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready.send(Err(format!(
                            "could not create content-retention runtime: {error}"
                        )));
                        return;
                    },
                };
                let (blobs, owns_store) = match backing {
                    RetentionBacking::Open(root) => match runtime.block_on(BlobStore::open(root)) {
                        Ok(blobs) => (Arc::new(blobs), true),
                        Err(error) => {
                            let _ = ready.send(Err(format!(
                                "could not open clip evidence blob store: {error}"
                            )));
                            return;
                        },
                    },
                    RetentionBacking::Borrowed(blobs) => (blobs, false),
                };
                let store = BlobClipEvidenceStore {
                    blobs: Arc::clone(&blobs),
                    max_artifact_bytes,
                    custody,
                };
                if ready.send(Ok(blobs)).is_err() {
                    if owns_store {
                        let _ = runtime.block_on(store.blobs.shutdown());
                    } else {
                        let _ = runtime.block_on(store.blobs.flush());
                    }
                    return;
                }
                while let Ok(command) = receiver.recv() {
                    match command {
                        RetentionCommand::Retain { artifact, reply } => {
                            let result = runtime.block_on(store.retain_async(&artifact));
                            let _ = reply.send(result);
                        },
                        RetentionCommand::Close => break,
                    }
                }
                if owns_store {
                    let _ = runtime.block_on(store.blobs.shutdown());
                } else {
                    let _ = runtime.block_on(store.blobs.flush());
                }
            })
            .map_err(|error| format!("could not start content-retention actor: {error}"))?;
        let blobs = match opened.recv() {
            Ok(Ok(blobs)) => blobs,
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            },
            Err(_) => {
                let _ = join.join();
                return Err("content-retention actor stopped during startup".into());
            },
        };
        Ok(Self {
            inner: Arc::new(RetentionPortInner {
                commands,
                join: Mutex::new(Some(join)),
                blobs,
            }),
        })
    }

    /// Retain one artifact through the resident actor.
    pub async fn retain_async(
        &self,
        artifact: &KnotClipArtifactV1,
    ) -> Result<KnotClipEvidenceRef, String> {
        let (reply, result) = tokio::sync::oneshot::channel();
        self.inner
            .commands
            .send(RetentionCommand::Retain {
                artifact: artifact.clone(),
                reply,
            })
            .map_err(|_| "content-retention actor has stopped".to_string())?;
        result
            .await
            .map_err(|_| "content-retention actor dropped its reply".to_string())?
    }

    /// Shared blob handle for the source's sync host.
    pub fn blob_store(&self) -> Arc<BlobStore> {
        Arc::clone(&self.inner.blobs)
    }
}

enum RetentionBacking {
    Open(PathBuf),
    Borrowed(Arc<BlobStore>),
}

impl KnotClipEvidenceStore for KnotContentRetentionPort {
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String> {
        pollster::block_on(self.retain_async(artifact))
    }
}

async fn retain_blob_artifact(
    blobs: &BlobStore,
    max_artifact_bytes: u64,
    artifact: &KnotClipArtifactV1,
    scope: Option<BlobScope>,
) -> Result<KnotClipEvidenceRef, String> {
    let byte_size = u64::try_from(artifact.bytes.len())
        .map_err(|_| "clip artifact byte length does not fit u64".to_string())?;
    if byte_size > max_artifact_bytes {
        return Err(format!(
            "clip artifact is {byte_size} bytes; configured evidence limit is {max_artifact_bytes}"
        ));
    }
    let digest = blake3::hash(&artifact.bytes);
    let digest_hex = digest.to_hex().to_string();
    let stored = match scope {
        Some(scope) => {
            let lease = BlobLease::new(scope, "knot.evidence", digest.as_bytes())
                .map_err(|error| format!("could not name clip evidence custody: {error}"))?;
            blobs.put_bytes_leased(artifact.bytes.clone(), &lease).await
        },
        None => {
            let tag = format!("knot/clip-evidence/{digest_hex}");
            blobs
                .put_bytes_named(artifact.bytes.clone(), tag.as_bytes())
                .await
        },
    }
    .map_err(|error| format!("could not retain clip evidence in blob store: {error}"))?;
    if stored.as_bytes() != digest.as_bytes() {
        return Err("transport blob store returned the wrong clip evidence digest".into());
    }
    blobs
        .flush()
        .await
        .map_err(|error| format!("could not flush retained clip evidence: {error}"))?;
    let reference = KnotClipEvidenceRef::portable(artifact);
    if reference.digest != digest_hex || reference.byte_size != byte_size {
        return Err("portable clip evidence disagrees with its retained transport bytes".into());
    }
    Ok(reference)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chirograph::{KnotClipArtifactRoleV1, KnotClipSelectorV1};
    use tempfile::tempdir;

    use super::super::{KnotClipCitation, clip_citations};
    use super::*;

    #[test]
    fn transport_blob_store_retains_and_reopens_verified_evidence() {
        let temp = tempdir().unwrap();
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/post".into(),
            bytes: b"<p>portable evidence</p>".to_vec(),
        };
        let reference = {
            let mut store = BlobClipEvidenceStore::open(temp.path(), 1024).unwrap();
            let reference = store.retain(&artifact).unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let bytes = runtime
                .block_on(store.blob_store().get_bytes(reference.blob_hash().unwrap()))
                .unwrap();
            reference.verify_bytes(&bytes).unwrap();
            reference
        };

        let reopened = BlobClipEvidenceStore::open(temp.path(), 1024).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let bytes = runtime
            .block_on(
                reopened
                    .blob_store()
                    .get_bytes(reference.blob_hash().unwrap()),
            )
            .unwrap();
        reference.verify_bytes(&bytes).unwrap();
        let mut tampered = bytes.to_vec();
        tampered[0] ^= 1;
        assert!(reference.verify_bytes(&tampered).is_err());
    }

    #[test]
    fn saved_passage_reopens_with_unicode_position_and_both_hashes() {
        let temp = tempdir().unwrap();
        let bytes = "café · 🐦 · café".as_bytes().to_vec();
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::ObservedRepresentation,
            media_type: "text/plain; charset=utf-8".into(),
            canonical_uri: "urn:saved:A1".into(),
            bytes: bytes.clone(),
        };
        let reference = {
            let mut store = BlobClipEvidenceStore::open(temp.path(), 4096).unwrap();
            store.retain(&artifact).unwrap()
        };
        let role = artifact.role;
        let citation = KnotClipCitation {
            source_url: "urn:saved:A1?start=11&end=15".into(),
            title: Some("A1".into()),
            selectors: vec![
                KnotClipSelectorV1::TextPosition {
                    artifact_role: role,
                    start: 11,
                    end: 15,
                },
                KnotClipSelectorV1::TextQuote {
                    artifact_role: role,
                    exact: "café".into(),
                    prefix: Some(" · ".into()),
                    suffix: None,
                },
            ],
            evidence: vec![reference.clone()],
        };
        let note = format!(
            "Authored commentary.\n\n```knot.clip.provenance\n{}\n```",
            serde_json::json!({
                "schema": chirograph::KNOT_CLIP_INSERT_SCHEMA_V2,
                "source_url":citation.source_url, "title":citation.title,
                "selectors":citation.selectors, "evidence":citation.evidence,
            })
        );
        let path = temp.path().join("note.knot");
        fs::write(&path, note).unwrap();
        let reopened = BlobClipEvidenceStore::open(temp.path(), 4096).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let retained = runtime
            .block_on(
                reopened
                    .blob_store()
                    .get_bytes(reference.blob_hash().unwrap()),
            )
            .unwrap();
        let mut decoded = clip_citations(&fs::read(&path).unwrap()).unwrap().remove(0);
        assert_eq!(decoded.read_passage(0, &retained).unwrap(), "café");
        let mut corrupt = retained.to_vec();
        corrupt[0] ^= 1;
        assert!(decoded.read_passage(0, &corrupt).is_err());
        assert!(decoded.read_passage(1, &retained).is_err());
        decoded.selectors.push(KnotClipSelectorV1::TextQuote {
            artifact_role: role,
            exact: "🐦".into(),
            prefix: None,
            suffix: None,
        });
        assert!(decoded.read_passage(0, &retained).is_err());
        decoded.selectors = vec![KnotClipSelectorV1::TextPosition {
            artifact_role: role,
            start: 11,
            end: 16,
        }];
        assert!(decoded.read_passage(0, &retained).is_err());
        decoded.selectors = vec![KnotClipSelectorV1::TextPosition {
            artifact_role: KnotClipArtifactRoleV1::SourceResponse,
            start: 11,
            end: 15,
        }];
        assert!(decoded.read_passage(0, &retained).is_err());
    }
}
