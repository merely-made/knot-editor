// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Evidence custody held by the resident and reached through its blob door
//! (cycle breaks P2 steps 3 and 4).
//!
//! A supervised sync host holds no blob store. Its evidence lives in the
//! resident's custody, under the leases the in-process lane already made
//! (`knot.evidence`, subject the blob's BLAKE3 digest, scope the Knot space id),
//! so nothing is re-leased. This type is the host's whole view of it:
//!
//! - **serving** is a Murm `BlobSource` over a dedicated door session, so peers
//!   fetch verified streams from this process's endpoint although the bytes and
//!   their outboards stay in the resident;
//! - **retain, readers and reads** are the route's calls on the host's custody
//!   session;
//! - **fetching** lands a peer's blob in a throwaway in-memory store (an iroh
//!   fetch needs a store to verify into), then puts it under the lease through
//!   the door, which verifies the hash again, and retains it.

use std::sync::Arc;
use std::time::Duration;

use graphshell_door::blob_door::BlobDoorClient;
use graphshell_door::blob_door_source::DoorBlobSource;
use tokio::sync::Mutex;
use transport::blob_source::SharedBlobSource;
use transport::{BlobHash, BlobScope, BlobStore, P2pandaTransport, PeerID};

/// The lease namespace Knot's evidence has always used.
pub const EVIDENCE_LEASE: &str = "knot.evidence";

pub struct DoorEvidence {
    custody: Mutex<BlobDoorClient>,
    source: Arc<DoorBlobSource>,
    scope: BlobScope,
    landing: BlobStore,
}

impl DoorEvidence {
    /// Take two sessions on the application's blob door: `custody` for the
    /// host's own calls, `serving` for the provider, so a peer's download
    /// never waits behind a retain. Both bind `scope`.
    pub async fn open(
        mut custody: BlobDoorClient,
        serving: BlobDoorClient,
        scope: BlobScope,
    ) -> Result<Self, String> {
        custody
            .bind_scope(scope.to_bytes())
            .await
            .map_err(|error| error.to_string())?;
        let source = DoorBlobSource::new(serving);
        source
            .bind_scope(scope)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self {
            custody: Mutex::new(custody),
            source,
            scope,
            landing: BlobStore::new_collecting(Duration::from_secs(5)),
        })
    }

    pub(crate) fn source(&self) -> SharedBlobSource {
        self.source.clone()
    }

    pub(crate) async fn retain(&self, hash: BlobHash) -> Result<bool, String> {
        self.custody
            .lock()
            .await
            .retain(self.scope.to_bytes(), hash.to_bytes())
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn replace_readers(&self, peers: &[[u8; 32]]) -> Result<bool, String> {
        self.custody
            .lock()
            .await
            .replace_readers(self.scope.to_bytes(), peers)
            .await
            .map_err(|error| error.to_string())
    }

    /// Whether the scope holds a lease on `hash` (the resident has the bytes).
    pub(crate) async fn has(&self, hash: BlobHash) -> Result<bool, String> {
        let leased = self
            .custody
            .lock()
            .await
            .lease_hash(self.scope.to_bytes(), EVIDENCE_LEASE, hash.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
        Ok(leased == Some(hash.to_bytes()))
    }

    pub(crate) async fn get(&self, hash: BlobHash) -> Result<Vec<u8>, String> {
        self.custody
            .lock()
            .await
            .read(hash.to_bytes())
            .await
            .map_err(|error| error.to_string())
    }

    /// Fetch `hash` from `peer`, verify it, put it under its lease in the
    /// resident's custody and retain it for serving.
    pub(crate) async fn fetch(
        &self,
        transport: &P2pandaTransport,
        peer: PeerID,
        hash: BlobHash,
    ) -> Result<(), String> {
        self.landing
            .fetch_from(transport, peer, hash)
            .await
            .map_err(|error| error.to_string())?;
        let bytes = self
            .landing
            .get_bytes(hash)
            .await
            .map_err(|error| error.to_string())?;
        let mut custody = self.custody.lock().await;
        let stored = custody
            .put(self.scope.to_bytes(), EVIDENCE_LEASE, hash.as_bytes(), &bytes)
            .await
            .map_err(|error| error.to_string())?;
        if stored != hash.to_bytes() {
            return Err("the resident stored other bytes than were fetched".into());
        }
        Ok(())
    }
}
