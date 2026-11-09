// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Always-on Knot sync.
//!
//! [`KnotSyncStore::join`](crate::KnotSyncStore::join) has been production code
//! for a while, but nothing shipped ever called it: `transport` was a
//! dev-dependency, so Knot's p2panda convergence was real and exercised only by
//! tests. The shipped endpoint bound no transport at all and therefore never
//! synchronised anything. This is the missing half.
//!
//! Identity here is deliberately split, because carrying a persona's private
//! epoch between devices makes the two halves pull in opposite directions:
//!
//! - the **vault key** and **space id** must be identical across a persona's
//!   devices, or they cannot decrypt or address the same space;
//! - the **writer key** must not be, because its public half is also the
//!   transport node id. Two devices deriving one writer would be a single node
//!   on the network and a single author in a per-author log.
//!
//! [`StartupUnlockedPersonalVault`](crate::StartupUnlockedPersonalVault)
//! resolves that by mixing the device's own Personae root into the writer
//! derivation only.

use std::collections::BTreeSet;
use std::sync::Arc;

use murm_sync::{JoinedSpace, SyncStatus};
use stickleback::JoinError;
use transport::p2panda_transport::{KnownPeer, RelayUrl};
use transport::{
    BlobPeerAuthorizer, BlobReadAuthorizer, BlobScope, BlobStore, P2pandaHostPolicy,
    P2pandaOverlayHost, P2pandaTransport, PeerID, sync_overlay_topic,
};

use crate::VaultDocument;
use crate::authority::{KnotAuthoritySource, KnotSpaceAuthoritySnapshot};
use crate::clip_evidence::{KnotClipEvidenceRef, clip_evidence_references};
use crate::sync::{KnotEncryptionProfile, KnotSyncExt, KnotSyncFileStore};

/// How this device reaches the persona's other devices.
#[derive(Clone, Debug, Default)]
pub struct KnotSyncHostConfig {
    /// One materialization consumed by operation, evidence, and route policy.
    pub authority: KnotSpaceAuthoritySnapshot,
    /// iroh relays. Empty leaves this device LAN-only: p2panda registers no
    /// relay by default.
    pub relay_urls: Vec<RelayUrl>,
}

#[derive(Debug, thiserror::Error)]
pub enum KnotSyncHostError {
    #[error("Knot sync transport failed: {0}")]
    Transport(String),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("Knot evidence reference failed: {0}")]
    EvidenceReference(String),
    #[error("Knot evidence blob failed: {0}")]
    EvidenceBlob(String),
    #[error("peer is not authorized for this Knot evidence store")]
    EvidenceUnauthorized,
    #[error("Knot evidence is {actual} bytes; configured limit is {limit}")]
    EvidenceTooLarge { actual: u64, limit: u64 },
    #[error("Knot resident authority failed: {0}")]
    Authority(String),
}

/// Result of resolving one portable evidence reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotEvidenceFetchStatus {
    /// Verified bytes were already present locally.
    AlreadyPresent,
    /// Verified bytes were fetched from the named peer.
    Fetched,
}

/// Receipt for one verified evidence reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotEvidenceFetchReceipt {
    pub reference: KnotClipEvidenceRef,
    pub status: KnotEvidenceFetchStatus,
}

/// Where the host's evidence bytes and serving authority live.
enum EvidenceStore {
    /// A blob store and authorizer in this process.
    Local {
        blobs: Arc<BlobStore>,
        readers: BlobReadAuthorizer,
    },
    /// The resident's custody, through its blob door (`door-custody`).
    #[cfg(feature = "door-custody")]
    Door(door_evidence::DoorEvidence),
}

struct KnotEvidenceHost {
    store: EvidenceStore,
    scope: BlobScope,
    sources: BlobPeerAuthorizer,
    max_artifact_bytes: u64,
}

impl KnotEvidenceHost {
    async fn retain(&self, hash: transport::BlobHash) -> Result<bool, String> {
        match &self.store {
            EvidenceStore::Local { readers, .. } => Ok(readers.retain(self.scope, hash)),
            #[cfg(feature = "door-custody")]
            EvidenceStore::Door(door) => door.retain(hash).await,
        }
    }

    async fn replace_readers(
        &self,
        peers: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<(), String> {
        match &self.store {
            EvidenceStore::Local { readers, .. } => {
                readers.replace_readers(self.scope, peers);
                Ok(())
            },
            #[cfg(feature = "door-custody")]
            EvidenceStore::Door(door) => {
                let peers: Vec<[u8; 32]> = peers.into_iter().collect();
                door.replace_readers(&peers).await.map(|_| ())
            },
        }
    }

    async fn has(&self, hash: transport::BlobHash) -> Result<bool, String> {
        match &self.store {
            EvidenceStore::Local { blobs, .. } => {
                blobs.has(hash).await.map_err(|error| error.to_string())
            },
            #[cfg(feature = "door-custody")]
            EvidenceStore::Door(door) => door.has(hash).await,
        }
    }

    async fn get(&self, hash: transport::BlobHash) -> Result<Vec<u8>, String> {
        match &self.store {
            EvidenceStore::Local { blobs, .. } => blobs
                .get_bytes(hash)
                .await
                .map(|bytes| bytes.to_vec())
                .map_err(|error| error.to_string()),
            #[cfg(feature = "door-custody")]
            EvidenceStore::Door(door) => door.get(hash).await,
        }
    }

    /// Fetch `hash` from `peer` into custody (`tag` names the claim of a local store).
    async fn fetch(
        &self,
        transport: &P2pandaTransport,
        peer: PeerID,
        hash: transport::BlobHash,
        tag: &str,
    ) -> Result<(), String> {
        match &self.store {
            EvidenceStore::Local { blobs, .. } => blobs
                .fetch_from_named(transport, peer, hash, tag.as_bytes())
                .await
                .map_err(|error| error.to_string()),
            #[cfg(feature = "door-custody")]
            EvidenceStore::Door(door) => door.fetch(transport, peer, hash).await,
        }
    }

    /// Give up a fetched claim that failed verification.
    async fn discard(&self, tag: &str) {
        if let EvidenceStore::Local { blobs, .. } = &self.store {
            let _ = blobs.release(tag.as_bytes()).await;
        }
    }
}

/// Which peers' addresses are worth writing back to settings.
///
/// Only the ones this host currently holds a live path to. The distinction
/// between `reachable` and `connected` is the whole of this function, and it
/// is not pedantry: an address the endpoint holds for a peer it is *not*
/// talking to may be exactly the stale route a working hint would replace, so
/// writing it back would overwrite good information with bad. A firewall can
/// drop every packet to a device while its address stays in the book, which
/// makes `reachable` look healthy while nothing replicates at all.
fn writers_to_refresh(peers: &[KnownPeer]) -> Vec<[u8; 32]> {
    peers
        .iter()
        .filter(|peer| peer.connected)
        .map(|peer| peer.peer.to_bytes())
        .collect()
}

/// A bound transport and live LogSync session over one Knot space.
pub struct KnotSyncHost {
    joined: JoinedSpace<KnotSyncExt>,
    network: P2pandaOverlayHost,
    store: KnotSyncFileStore,
    space_id: [u8; 32],
    evidence: Option<KnotEvidenceHost>,
    authority: KnotSpaceAuthoritySnapshot,
}

fn validate_authority_source(
    profile: KnotEncryptionProfile,
    source: KnotAuthoritySource,
) -> Result<(), KnotSyncHostError> {
    match (profile, source) {
        (KnotEncryptionProfile::PersonalVaultV1, KnotAuthoritySource::PersonalPairing)
        | (KnotEncryptionProfile::CommonsDataV1, KnotAuthoritySource::GemotCapabilities) => Ok(()),
        (KnotEncryptionProfile::PersonalVaultV1, KnotAuthoritySource::GemotCapabilities) => {
            Err(KnotSyncHostError::Authority(
                "Gemot authority cannot alter a personal Knot space".into(),
            ))
        },
        (KnotEncryptionProfile::CommonsDataV1, KnotAuthoritySource::PersonalPairing) => {
            Err(KnotSyncHostError::Authority(
                "Personae pairing cannot alter a communal Knot space".into(),
            ))
        },
    }
}

fn evidence_tag(space_id: [u8; 32], reference: &KnotClipEvidenceRef) -> String {
    format!(
        "knot/evidence/{}/{}",
        crate::hex32(&space_id),
        reference.digest
    )
}

#[cfg(test)]
mod tests;

#[cfg(feature = "door-custody")]
pub mod door_evidence;
mod operations;
