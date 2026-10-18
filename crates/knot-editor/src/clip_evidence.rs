// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Endpoint-owned retention for source artifacts attached to clips.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use chirograph::{
    KnotClipArtifactRoleV1, KnotClipArtifactV1, KnotClipSelectorV1, PortableContentRefV1,
};
use serde::{Deserialize, Serialize};
use transport::{BlobHash, BlobLease, BlobReadAuthorizer, BlobScope, BlobStore};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Portable content identity written into clip provenance.
///
/// New references serialize a shared [`PortableContentRefV1`]: RFC 6920
/// SHA-256 identity beside the BLAKE3 address iroh uses. The public normalized
/// fields preserve the established Knot API, while deserialization also
/// accepts the legacy `urn:blake3` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotClipEvidenceRef {
    pub content_uri: String,
    pub digest: String,
    pub byte_size: u64,
    pub media_type: String,
    pub canonical_uri: String,
    pub role: KnotClipArtifactRoleV1,
    portable: Option<PortableContentRefV1>,
}

impl KnotClipEvidenceRef {
    /// Read a selected text range from already retained bytes. This has no
    /// fetch path: size, portable SHA-256 and transport BLAKE3 are checked
    /// before the selector can expose any text.
    pub fn selected_text(
        &self,
        bytes: &[u8],
        selector: &KnotClipSelectorV1,
    ) -> Result<String, String> {
        self.verify_bytes(bytes)?;
        selected_text(selector, self.role, bytes)
    }
    fn portable(artifact: &KnotClipArtifactV1) -> Self {
        let content = PortableContentRefV1::of(&artifact.bytes);
        Self {
            content_uri: content.portable_id.to_string(),
            digest: content.transport.to_string(),
            byte_size: content.byte_size,
            media_type: artifact.media_type.clone(),
            canonical_uri: artifact.canonical_uri.clone(),
            role: artifact.role,
            portable: Some(content),
        }
    }

    /// The shared portable reference, absent only for a decoded legacy clip.
    pub fn portable_content(&self) -> Option<&PortableContentRefV1> {
        self.portable.as_ref()
    }

    /// Resolve the portable URI into the transport blob hash it names.
    pub fn blob_hash(&self) -> Result<BlobHash, String> {
        if let Some(content) = &self.portable {
            if self.content_uri != content.portable_id.to_string()
                || self.digest != content.transport.to_string()
                || self.byte_size != content.byte_size
            {
                return Err("clip evidence portable and normalized fields disagree".into());
            }
            return Ok(BlobHash::from_bytes(*content.transport.as_bytes()));
        }
        let named = self
            .content_uri
            .strip_prefix("urn:blake3:")
            .ok_or_else(|| "legacy clip evidence URI is not a urn:blake3 reference".to_string())?;
        if named != self.digest {
            return Err("legacy clip evidence URI and digest disagree".into());
        }
        parse_digest(&self.digest).map(BlobHash::from_bytes)
    }

    /// Check bytes before they are exposed as the retained source artifact.
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        if u64::try_from(bytes.len()).ok() != Some(self.byte_size) {
            return Err("clip evidence byte length does not match its reference".into());
        }
        let actual = blake3::hash(bytes);
        if actual.as_bytes() != self.blob_hash()?.as_bytes() {
            return Err("clip evidence bytes do not match their BLAKE3 reference".into());
        }
        if let Some(content) = &self.portable {
            content
                .verify_bytes(bytes)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// One authored clip occurrence. Unlike the blob inventory, this preserves
/// document order and repeated citations, including equal text from different
/// source identities. The note owns these facts, not a live origin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnotClipCitation {
    pub source_url: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub selectors: Vec<KnotClipSelectorV1>,
    pub evidence: Vec<KnotClipEvidenceRef>,
}

impl KnotClipCitation {
    /// Verify one retained artifact and resolve its single exact scalar
    /// range. A supplied quote must agree with that range. DOM selectors need
    /// their own capable reader and are never silently treated as text ranges.
    pub fn read_passage(&self, artifact: usize, bytes: &[u8]) -> Result<String, String> {
        let reference = self
            .evidence
            .get(artifact)
            .ok_or("The citation has no such artifact.")?;
        let positions: Vec<_> = self.selectors.iter().filter(|s| matches!(s,
            KnotClipSelectorV1::TextPosition { artifact_role, .. } if *artifact_role == reference.role)).collect();
        let [position] = positions.as_slice() else {
            return Err("The citation needs one exact text position for this artifact.".into());
        };
        let text = reference.selected_text(bytes, position)?;
        for quote in self.selectors.iter().filter(|s| {
            matches!(s,
            KnotClipSelectorV1::TextQuote { artifact_role, .. } if *artifact_role == reference.role)
        }) {
            if reference.selected_text(bytes, quote)? != text {
                return Err("The citation quote disagrees with its exact scalar range.".into());
            }
        }
        Ok(text)
    }
}

/// Decode evidence-bearing clip occurrences from the note itself. Unknown
/// fields remain forward-compatible; legacy reference-only notes still work
/// through `clip_evidence_references` without claiming an exact passage.
pub fn clip_citations(source: &[u8]) -> Result<Vec<KnotClipCitation>, String> {
    provenance_records(source)?
        .into_iter()
        .filter(|v| {
            v.get("schema").and_then(|s| s.as_str()) == Some(chirograph::KNOT_CLIP_INSERT_SCHEMA_V2)
        })
        .map(|value| {
            serde_json::from_value(value).map_err(|e| format!("Clip citation is malformed: {e}"))
        })
        .collect()
}

pub(crate) fn selected_text(
    selector: &KnotClipSelectorV1,
    role: KnotClipArtifactRoleV1,
    bytes: &[u8],
) -> Result<String, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| "A text selector requires a UTF-8 artifact.")?;
    match selector {
        KnotClipSelectorV1::TextPosition {
            artifact_role,
            start,
            end,
        } if *artifact_role == role => {
            let start = usize::try_from(*start).map_err(|_| "Text position is out of range.")?;
            let end = usize::try_from(*end).map_err(|_| "Text position is out of range.")?;
            if start >= end || end > text.chars().count() {
                return Err(
                    "Text position is not a nonempty Unicode scalar range in this artifact.".into(),
                );
            }
            Ok(text.chars().skip(start).take(end - start).collect())
        },
        KnotClipSelectorV1::TextQuote {
            artifact_role,
            exact,
            prefix,
            suffix,
        } if *artifact_role == role => {
            if exact.is_empty() {
                return Err("A text quote must be nonempty.".into());
            }
            let found = text.match_indices(exact).any(|(start, quote)| {
                prefix.as_deref().is_none_or(|p| text[..start].ends_with(p))
                    && suffix
                        .as_deref()
                        .is_none_or(|s| text[start + quote.len()..].starts_with(s))
            });
            if !found {
                return Err(
                    "The exact quote or its context is absent from the retained artifact.".into(),
                );
            }
            Ok(exact.clone())
        },
        _ => Err("The selector does not name a supported text range in this artifact role.".into()),
    }
}

fn provenance_records(source: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    const OPEN: &str = "```knot.clip.provenance\n";
    const CLOSE: &str = "\n```";
    let source = std::str::from_utf8(source)
        .map_err(|_| "Evidence references require a UTF-8 Djot source.")?;
    let mut rest = source;
    let mut records = Vec::new();
    while let Some(start) = rest.find(OPEN) {
        let body = &rest[start + OPEN.len()..];
        let end = body
            .find(CLOSE)
            .ok_or("Clip provenance fence is not closed.")?;
        records.push(
            serde_json::from_str(&body[..end])
                .map_err(|e| format!("Clip provenance is malformed: {e}"))?,
        );
        rest = &body[end + CLOSE.len()..];
    }
    Ok(records)
}

#[derive(Serialize)]
struct PortableEvidenceWire<'a> {
    content: &'a PortableContentRefV1,
    media_type: &'a str,
    canonical_uri: &'a str,
    role: KnotClipArtifactRoleV1,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EvidenceWire {
    Portable {
        content: PortableContentRefV1,
        media_type: String,
        canonical_uri: String,
        role: KnotClipArtifactRoleV1,
    },
    Legacy {
        content_uri: String,
        digest: String,
        byte_size: u64,
        media_type: String,
        canonical_uri: String,
        role: KnotClipArtifactRoleV1,
    },
}

impl Serialize for KnotClipEvidenceRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(content) = &self.portable {
            if self.content_uri != content.portable_id.to_string()
                || self.digest != content.transport.to_string()
                || self.byte_size != content.byte_size
            {
                return Err(serde::ser::Error::custom(
                    "clip evidence portable and normalized fields disagree",
                ));
            }
            PortableEvidenceWire {
                content,
                media_type: &self.media_type,
                canonical_uri: &self.canonical_uri,
                role: self.role,
            }
            .serialize(serializer)
        } else {
            serde_json::json!({
                "content_uri": self.content_uri,
                "digest": self.digest,
                "byte_size": self.byte_size,
                "media_type": self.media_type,
                "canonical_uri": self.canonical_uri,
                "role": self.role,
            })
            .serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for KnotClipEvidenceRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match EvidenceWire::deserialize(deserializer)? {
            EvidenceWire::Portable {
                content,
                media_type,
                canonical_uri,
                role,
            } => Self {
                content_uri: content.portable_id.to_string(),
                digest: content.transport.to_string(),
                byte_size: content.byte_size,
                media_type,
                canonical_uri,
                role,
                portable: Some(content),
            },
            EvidenceWire::Legacy {
                content_uri,
                digest,
                byte_size,
                media_type,
                canonical_uri,
                role,
            } => Self {
                content_uri,
                digest,
                byte_size,
                media_type,
                canonical_uri,
                role,
                portable: None,
            },
        })
    }
}

/// Extract the portable evidence references authored by Knot clip v2 blocks.
///
/// These fenced JSON records are Knot-owned metadata inside an ordinary Djot
/// document. Unknown provenance fields remain forward-compatible; malformed or
/// internally inconsistent evidence references fail closed.
pub fn clip_evidence_references(source: &[u8]) -> Result<Vec<KnotClipEvidenceRef>, String> {
    let mut references = BTreeMap::<String, KnotClipEvidenceRef>::new();
    let mut identities = BTreeMap::<String, (u64, Option<PortableContentRefV1>)>::new();
    for value in provenance_records(source)? {
        if let Some(evidence) = value.get("evidence") {
            let entries = evidence
                .as_array()
                .ok_or_else(|| "clip provenance evidence must be an array".to_string())?;
            for entry in entries {
                let reference: KnotClipEvidenceRef = serde_json::from_value(entry.clone())
                    .map_err(|error| format!("clip evidence reference is malformed: {error}"))?;
                reference.blob_hash()?;
                if let Some((size, portable)) = identities.get(&reference.digest)
                    && (*size != reference.byte_size
                        || portable
                            .as_ref()
                            .zip(reference.portable.as_ref())
                            .is_some_and(|(a, b)| a != b))
                {
                    return Err(
                        "One clip evidence digest carries conflicting content identity.".into(),
                    );
                }
                let identity = identities
                    .entry(reference.digest.clone())
                    .or_insert((reference.byte_size, reference.portable.clone()));
                if identity.1.is_none() {
                    identity.1 = reference.portable.clone();
                }
                // A content digest owns bytes. URI, role and media type belong
                // to each citation, and may legitimately differ for those bytes.
                let key = serde_json::to_string(&reference).map_err(|e| e.to_string())?;
                references.entry(key).or_insert(reference);
            }
        }
    }
    Ok(references.into_values().collect())
}

/// Host-injected authority for retaining clip evidence.
pub trait KnotClipEvidenceStore: Send {
    /// Retain an artifact under its content identity. Implementations must be
    /// idempotent for identical bytes.
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String>;
}

/// A content-addressed local store rooted at an explicitly configured path.
pub struct FileClipEvidenceStore {
    root: PathBuf,
    max_artifact_bytes: u64,
}

impl FileClipEvidenceStore {
    pub fn new(root: impl Into<PathBuf>, max_artifact_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_artifact_bytes,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn artifact_path(&self, digest: &str) -> PathBuf {
        self.root.join("blake3").join(digest)
    }
}

impl KnotClipEvidenceStore for FileClipEvidenceStore {
    fn retain(&mut self, artifact: &KnotClipArtifactV1) -> Result<KnotClipEvidenceRef, String> {
        let byte_size = u64::try_from(artifact.bytes.len())
            .map_err(|_| "clip artifact byte length does not fit u64".to_string())?;
        if byte_size > self.max_artifact_bytes {
            return Err(format!(
                "clip artifact is {byte_size} bytes; configured evidence limit is {}",
                self.max_artifact_bytes
            ));
        }
        let digest = blake3::hash(&artifact.bytes).to_hex().to_string();
        let path = self.artifact_path(&digest);
        if path.exists() {
            let existing = fs::read(&path)
                .map_err(|error| format!("could not verify retained clip evidence: {error}"))?;
            if existing != artifact.bytes {
                return Err("retained clip evidence does not match its BLAKE3 address".into());
            }
        } else {
            let parent = path
                .parent()
                .ok_or_else(|| "clip evidence path has no parent".to_string())?;
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create clip evidence directory: {error}"))?;
            let temporary = parent.join(format!(
                ".{digest}.{}.{}.tmp",
                std::process::id(),
                NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
            ));
            if let Err(error) = write_new(&temporary, &artifact.bytes) {
                let _ = fs::remove_file(&temporary);
                return Err(format!("could not stage clip evidence: {error}"));
            }
            match fs::rename(&temporary, &path) {
                Ok(()) => {},
                Err(error) if path.exists() => {
                    let _ = fs::remove_file(&temporary);
                    let existing = fs::read(&path).map_err(|read_error| {
                        format!(
                            "clip evidence raced with another writer ({error}) and could not be verified: {read_error}"
                        )
                    })?;
                    if existing != artifact.bytes {
                        return Err(
                            "retained clip evidence does not match its BLAKE3 address".into()
                        );
                    }
                },
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    return Err(format!("could not install clip evidence: {error}"));
                },
            }
        }
        let reference = KnotClipEvidenceRef::portable(artifact);
        debug_assert_eq!(reference.digest, digest);
        debug_assert_eq!(reference.byte_size, byte_size);
        Ok(reference)
    }
}

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

fn parse_digest(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("clip evidence digest must be 64 lowercase hexadecimal characters".into());
    }
    let mut digest = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(pair).expect("ASCII hex was already checked");
        digest[index] = u8::from_str_radix(hex, 16)
            .map_err(|_| "clip evidence digest is not hexadecimal".to_string())?;
    }
    Ok(digest)
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn file_store_is_content_addressed_and_idempotent() {
        let temp = tempdir().unwrap();
        let mut store = FileClipEvidenceStore::new(temp.path(), 1024);
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: "https://example.test/post".into(),
            bytes: b"<p>evidence</p>".to_vec(),
        };
        let first = store.retain(&artifact).unwrap();
        let second = store.retain(&artifact).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fs::read(store.artifact_path(&first.digest)).unwrap(),
            artifact.bytes
        );
        assert!(
            first
                .content_uri
                .starts_with(chirograph::Sha256NamedInformation::PREFIX)
        );
        assert_eq!(
            first.portable_content().unwrap().transport.to_string(),
            first.digest
        );
    }

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
    fn djot_provenance_yields_deduplicated_validated_references() {
        let bytes = b"portable evidence";
        let digest = blake3::hash(bytes).to_hex().to_string();
        let reference = KnotClipEvidenceRef {
            content_uri: format!("urn:blake3:{digest}"),
            digest,
            byte_size: bytes.len() as u64,
            media_type: "text/plain".into(),
            canonical_uri: "https://example.test/evidence".into(),
            role: KnotClipArtifactRoleV1::SourceResponse,
            portable: None,
        };
        let provenance = serde_json::json!({
            "schema": "knot.clip.insert/v2",
            "evidence": [reference.clone(), reference.clone()]
        });
        let source = format!(
            "# Note\n\n```knot.clip.provenance\n{}\n```\n",
            serde_json::to_string(&provenance).unwrap()
        );
        let decoded = clip_evidence_references(source.as_bytes()).unwrap();
        assert_eq!(decoded, vec![reference]);
        assert!(decoded[0].portable_content().is_none());
        decoded[0].verify_bytes(bytes).unwrap();
    }

    #[test]
    fn portable_and_transport_hashes_must_both_match() {
        let artifact = KnotClipArtifactV1 {
            role: KnotClipArtifactRoleV1::SourceResponse,
            media_type: "text/plain".into(),
            canonical_uri: "https://example.test/evidence".into(),
            bytes: b"portable evidence".to_vec(),
        };
        let reference = KnotClipEvidenceRef::portable(&artifact);
        let mut value = serde_json::to_value(&reference).unwrap();
        value["content"]["portable_id"] =
            serde_json::to_value(chirograph::Sha256NamedInformation::of(b"different bytes"))
                .unwrap();
        let conflicting: KnotClipEvidenceRef = serde_json::from_value(value).unwrap();
        assert!(
            conflicting.blob_hash().is_ok(),
            "the transport hash is valid"
        );
        assert!(
            conflicting.verify_bytes(&artifact.bytes).is_err(),
            "the conflicting portable identity fails closed",
        );
    }
    #[test]
    fn equal_bytes_from_distinct_saved_sources_keep_distinct_provenance() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = FileClipEvidenceStore::new(temp.path(), 4096);
        let mut artifacts = Vec::new();
        for source in ["A1", "B1"] {
            artifacts.push(KnotClipArtifactV1 {
                role: KnotClipArtifactRoleV1::ObservedRepresentation,
                media_type: "text/plain".into(),
                canonical_uri: format!("urn:saved:{source}"),
                bytes: "repeat café".as_bytes().to_vec(),
            });
        }
        let references: Vec<_> = artifacts.iter().map(|a| store.retain(a).unwrap()).collect();
        assert_eq!(references[0].content_uri, references[1].content_uri);
        let source = references.iter().map(|reference| format!("```knot.clip.provenance\n{}\n```\n",
            serde_json::json!({"schema":"knot.clip.insert/v2", "source_url":reference.canonical_uri,"selectors":[], "evidence":[reference]})))
            .collect::<Vec<_>>().join("\n");
        let reopened = clip_evidence_references(source.as_bytes()).unwrap();
        assert_eq!(
            reopened.len(),
            2,
            "deduplicate bytes, not distinct source citations"
        );
        for reference in &reopened {
            reference.verify_bytes(&artifacts[0].bytes).unwrap();
        }
        assert_eq!(fs::read_dir(temp.path().join("blake3")).unwrap().count(), 1);
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
