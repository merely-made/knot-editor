// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A bounded, read-only Mark projection beside Knot's private publish lane.
//!
//! The two protocols deliberately remain separate. Native publishing is an
//! admitted Personae session over the `mere/knot-publish/v1` carrier, with
//! causal operation identifiers. Mark is a public QUIC/TLS protocol with
//! numeric, append-only versions and CommonMark bodies. This module therefore
//! snapshots an owner-selected Knot publication into a separate Mark history;
//! it never relabels raw Knot or Djot bytes as CommonMark and never exposes a
//! Personae delegation as a Mark token.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use muniment::Backend;
use quinn::crypto::rustls::QuicServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::{
    DocumentFormat, KnotPublishCatalog, KnotPublishError, KnotPublishedDocument, KnotSyncStore,
    KnotVault, PublicationId,
};

/// The ALPN registered by the Mark Protocol working draft.
pub const MARK_ALPN: &[u8] = b"mark";
/// Mark's assigned UDP port. Callers may bind another port explicitly.
pub const MARK_DEFAULT_PORT: u16 = 6309;
/// Mark's maximum request-line size.
pub const MARK_MAX_REQUEST_BYTES: usize = 4096;
/// Mark's maximum YAML metadata block size.
pub const MARK_MAX_METADATA_BYTES: usize = 64 * 1024;
/// Mark's recommended document bound, made a default here rather than a hard
/// global policy for native Knot publishing.
pub const MARK_MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

/// A configured bound for the Mark projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkReadAdapterLimits {
    /// Maximum CommonMark bytes an owner may snapshot into one Mark version.
    pub max_document_bytes: usize,
}

impl Default for MarkReadAdapterLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: MARK_MAX_DOCUMENT_BYTES,
        }
    }
}

impl MarkReadAdapterLimits {
    fn clamped(self) -> Self {
        Self {
            max_document_bytes: self.max_document_bytes.min(MARK_MAX_DOCUMENT_BYTES),
        }
    }
}

/// An RFC 3339 UTC timestamp held with a Mark snapshot.
///
/// Knot causal operations have no authored wall-clock time. The timestamp is
/// consequently the owner's projection time, not a claim about the causal
/// operation's time of authorship.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MarkTimestamp(String);

impl MarkTimestamp {
    /// Parse the UTC, second-precision form emitted by this adapter.
    pub fn parse(value: impl Into<String>) -> Result<Self, MarkAdapterError> {
        let value = value.into();
        if !is_utc_rfc3339_seconds(&value) {
            return Err(MarkAdapterError::InvalidTimestamp);
        }
        Ok(Self(value))
    }

    /// Capture the local projection time in Mark's required UTC form.
    pub fn now() -> Result<Self, MarkAdapterError> {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MarkAdapterError::Clock)?
            .as_secs();
        let days = (seconds / 86_400) as i64;
        let seconds_of_day = seconds % 86_400;
        let (year, month, day) = civil_from_days(days);
        let hour = seconds_of_day / 3_600;
        let minute = (seconds_of_day % 3_600) / 60;
        let second = seconds_of_day % 60;
        Self::parse(format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
        ))
    }

    /// The serialized response value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One-based numeric Mark version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarkVersionId(u64);

impl MarkVersionId {
    /// Numeric representation used in Mark paths and metadata.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A snapshot outcome. Identical CommonMark bodies deliberately remain the
/// current Mark version, matching Mark's no-op publish rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkSnapshotOutcome {
    Created(MarkVersionId),
    Unchanged(MarkVersionId),
}

/// A Mark read access rule. The token form stores only a SHA-256 digest and is
/// a separately-issued adapter credential, never a serialized Personae grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkReadAccess {
    Public,
    TokenHash([u8; 32]),
}

impl MarkReadAccess {
    /// Construct an adapter-local protected-read rule from a token the owner
    /// distributes out of band. The raw token is not retained.
    pub fn protected(token: impl AsRef<[u8]>) -> Self {
        Self::TokenHash(sha256(token.as_ref()))
    }

    fn allows(&self, candidate: Option<&str>) -> bool {
        match self {
            Self::Public => true,
            Self::TokenHash(expected) => candidate
                .is_some_and(|candidate| constant_time_eq(expected, &sha256(candidate.as_bytes()))),
        }
    }
}

/// One served Mark snapshot. The stored bytes are retained privately so the
/// adapter can compute the specification's ETag and `previous-hash` chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkVersion {
    pub id: MarkVersionId,
    pub modified: MarkTimestamp,
    pub source_operation: [u8; 32],
    pub source_media_type: String,
    pub body: Vec<u8>,
    stored: Vec<u8>,
    etag: [u8; 32],
    content_hash: [u8; 32],
}

impl MarkVersion {
    /// SHA-256 of the complete Mark store representation, formatted for ETag.
    pub fn etag(&self) -> String {
        hex(&self.etag)
    }

    /// SHA-256 of the served CommonMark body, formatted for `content-hash`.
    pub fn content_hash(&self) -> String {
        format!("sha256-{}", hex(&self.content_hash))
    }

    #[cfg(test)]
    fn stored(&self) -> &[u8] {
        &self.stored
    }
}

#[derive(Clone, Debug)]
struct MarkDocument {
    access: MarkReadAccess,
    publication: PublicationId,
    versions: Vec<MarkVersion>,
}

/// Explicit Mark export selections and their independent immutable snapshots.
///
/// `configure_export` and `snapshot_*` are owner actions. A native source
/// changing does not silently mutate a Mark document, and an unresolved causal
/// history cannot be invented as a numeric Mark history.
#[derive(Clone, Debug)]
pub struct MarkReadAdapter {
    limits: MarkReadAdapterLimits,
    documents: BTreeMap<String, MarkDocument>,
    current_content: BTreeMap<[u8; 32], String>,
}

/// A parsed bounded Mark request. The read adapter recognizes FETCH and
/// VERSIONS; the remaining specified verbs parse but are refused as writes or
/// unsupported discovery rather than leaking a native catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkRequest {
    Fetch {
        path: String,
        auth: Option<String>,
        if_none_match: Option<String>,
        if_modified_since: Option<MarkTimestamp>,
    },
    Versions {
        path: String,
        auth: Option<String>,
    },
    Other {
        verb: String,
        path: String,
    },
}

/// A textual Mark response. `to_wire` always emits the mandatory YAML
/// frontmatter and preserves a body exactly as supplied by the projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkResponse {
    status: &'static str,
    metadata: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl MarkResponse {
    fn ok(metadata: impl IntoIterator<Item = (&'static str, String)>, body: Vec<u8>) -> Self {
        Self {
            status: "ok",
            metadata: metadata.into_iter().collect(),
            body,
        }
    }

    fn not_modified() -> Self {
        Self {
            status: "not-modified",
            metadata: Vec::new(),
            body: Vec::new(),
        }
    }

    fn not_found() -> Self {
        Self {
            status: "not-found",
            metadata: Vec::new(),
            body: b"# Not found\n\nThis document is not available.\n".to_vec(),
        }
    }

    fn bad_request() -> Self {
        Self {
            status: "bad-request",
            metadata: Vec::new(),
            body: b"# Bad request\n\nThe Mark request is malformed.\n".to_vec(),
        }
    }

    fn not_permitted() -> Self {
        Self {
            status: "not-permitted",
            metadata: Vec::new(),
            body: b"# Not permitted\n\nThis Mark adapter is read-only.\n".to_vec(),
        }
    }

    /// The Mark status string, useful to an embedding host before writing.
    pub fn status(&self) -> &str {
        self.status
    }

    /// Serialize the response's mandatory YAML frontmatter and body.
    pub fn to_wire(&self) -> Vec<u8> {
        let mut output = format!("---\nstatus: {}\n", self.status).into_bytes();
        for (key, value) in &self.metadata {
            output.extend_from_slice(format!("{key}: {value}\n").as_bytes());
        }
        output.extend_from_slice(b"---\n");
        output.extend_from_slice(&self.body);
        output
    }
}

/// A standard QUIC/TLS Mark listener. It is intentionally not layered over
/// the private p2panda carrier: external Mark clients connect with ALPN `mark`.
pub struct MarkQuicHost {
    endpoint: quinn::Endpoint,
    adapter: Arc<RwLock<MarkReadAdapter>>,
}

impl MarkQuicHost {
    /// Bind an independently configured direct QUIC listener.
    pub fn bind(
        address: SocketAddr,
        server_config: quinn::ServerConfig,
        adapter: Arc<RwLock<MarkReadAdapter>>,
    ) -> Result<Self, MarkServerError> {
        let endpoint = quinn::Endpoint::server(server_config, address)
            .map_err(|error| MarkServerError::Bind(error.to_string()))?;
        Ok(Self { endpoint, adapter })
    }

    /// The actual socket address, including an OS-selected port when requested.
    pub fn local_addr(&self) -> Result<SocketAddr, MarkServerError> {
        self.endpoint
            .local_addr()
            .map_err(|error| MarkServerError::Bind(error.to_string()))
    }

    /// Accept one Mark connection and serve its bidirectional request streams
    /// until the client closes it. Embedders call this repeatedly or place it
    /// in their own task supervision for subsequent connections.
    pub async fn serve_once(&self) -> Result<(), MarkServerError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(MarkServerError::Closed)?;
        let connection = incoming
            .await
            .map_err(|error| MarkServerError::Connection(error.to_string()))?;
        while let Ok((mut send, mut receive)) = connection.accept_bi().await {
            let request = receive
                .read_to_end(MARK_MAX_REQUEST_BYTES + MARK_MAX_METADATA_BYTES)
                .await
                .map_err(|error| MarkServerError::Stream(error.to_string()))?;
            let response = self.adapter.read().await.respond(&request).to_wire();
            send.write_all(&response)
                .await
                .map_err(|error| MarkServerError::Stream(error.to_string()))?;
            send.finish()
                .map_err(|error| MarkServerError::Stream(error.to_string()))?;
        }
        Ok(())
    }
}

/// Build the direct QUIC/TLS configuration required by a Mark listener. The
/// caller owns certificate issuance and renewal; no self-signed certificate is
/// silently generated for a resident service.
pub fn mark_server_config(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<quinn::ServerConfig, MarkServerError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|error| MarkServerError::Config(error.to_string()))?;
    tls.alpn_protocols = vec![MARK_ALPN.to_vec()];
    let crypto = QuicServerConfig::try_from(tls)
        .map_err(|error| MarkServerError::Config(error.to_string()))?;
    Ok(quinn::ServerConfig::with_crypto(Arc::new(crypto)))
}

/// Source conversion, request parsing, or snapshot failure.
#[derive(Debug, thiserror::Error)]
pub enum MarkAdapterError {
    #[error("Mark export paths must be absolute .md paths without traversal")]
    InvalidPath,
    #[error("the Mark request is malformed")]
    MalformedRequest,
    #[error("the Mark request exceeds its fixed limit")]
    RequestTooLarge,
    #[error("the Mark timestamp must be RFC 3339 UTC at second precision")]
    InvalidTimestamp,
    #[error("the system clock is before the Unix epoch")]
    Clock,
    #[error("the Mark export does not exist")]
    UnknownExport,
    #[error("a Mark path cannot be rebound to a different native publication")]
    RebindPath,
    #[error("the native source does not match this export selection")]
    PublicationMismatch,
    #[error("the native source digest does not match its bytes")]
    InvalidSourceDigest,
    #[error("the native media type cannot be converted to CommonMark: {0}")]
    UnsupportedSource(String),
    #[error("the Knot-to-CommonMark projection failed: {0}")]
    CommonMarkConversion(String),
    #[error("the projected CommonMark document exceeds the configured Mark limit")]
    DocumentTooLarge,
    #[error("the Mark numeric version sequence is exhausted")]
    VersionOverflow,
    #[error("the selected native publication is not available for export")]
    NativeNotAvailable,
    #[error(transparent)]
    NativeSource(KnotPublishError),
}

/// Listener setup or stream-service failure.
#[derive(Debug, thiserror::Error)]
pub enum MarkServerError {
    #[error("could not configure Mark TLS: {0}")]
    Config(String),
    #[error("could not bind Mark QUIC listener: {0}")]
    Bind(String),
    #[error("Mark listener closed")]
    Closed,
    #[error("Mark QUIC connection failed: {0}")]
    Connection(String),
    #[error("Mark QUIC stream failed: {0}")]
    Stream(String),
}

const HEALTH_BODY: &[u8] = b"# Knot Mark read adapter\n\nReady.\n";

#[cfg(test)]
mod tests;

mod adapter;

mod validation;

pub use validation::decode_mark_request;

use validation::{
    civil_from_days, constant_time_eq, hex, is_utc_rfc3339_seconds, mark_chain_is_valid,
    mark_stored_version, parse_content_hash, resolve_mark_path, sha256, validate_document_path,
};
