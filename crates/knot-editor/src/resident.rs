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

struct KnotEvidenceHost {
    blobs: Arc<BlobStore>,
    readers: BlobReadAuthorizer,
    scope: BlobScope,
    sources: BlobPeerAuthorizer,
    max_artifact_bytes: u64,
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

mod operations;
