// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What a domain supplies to the store.

use serde::Serialize;
use serde::de::DeserializeOwned;
use stickleback::{CausalError, CausalIndex, GroupCryptoError, ProcessError};
use muniment::StoreError;
use zeroize::Zeroize;

use super::{StoredOperation, SyncCipher, SyncError};

/// The strings a domain writes into its own signed and stored bytes. Changing
/// one orphans what was written under it.
#[derive(Clone, Copy, Debug)]
pub struct Vocabulary {
    /// AEAD context bound into every sealed operation body.
    pub operation_aad: &'static [u8],
    /// Protocol-lane kind; the lane id adds the space.
    pub lane_kind: &'static str,
    /// Prefix of every record key the store keeps beside the log.
    pub key_prefix: &'static str,
    /// Envelope format name of sealed local records.
    pub storage_format: &'static str,
    /// Plaintext of the clean-storage marker.
    pub storage_marker: &'static [u8],
    /// Why a signing writer is refused locally.
    pub writer_refusal: &'static str,
    /// Admission refusals: (code, message) for a wrong space, a wrong
    /// encryption profile, an unadmitted writer and a missing body.
    pub wrong_space: (&'static str, &'static str),
    pub wrong_profile: (&'static str, &'static str),
    pub unadmitted_writer: (&'static str, &'static str),
    pub missing_body: (&'static str, &'static str),
    /// Code and message prefix when a group body is not a data envelope.
    pub bad_group_body: (&'static str, &'static str),
    /// Code when causal metadata is invalid.
    pub bad_causality: &'static str,
}

/// The facts the domain's local check sees before an event is signed.
pub struct LocalCheck<'a, D: SyncDomain> {
    pub event: &'a D::Event,
    pub author: [u8; 32],
    pub records: &'a [StoredOperation],
    /// Indexes into `records` of the causally closed history, in fold order.
    pub order: &'a [usize],
    pub cipher: SyncCipher<'a>,
    pub causal: &'a CausalIndex<'a, u64>,
    pub parents: &'a [[u8; 32]],
    pub space_id: [u8; 32],
}

/// A domain over the store: its events, its refusals and its words.
///
/// The store cannot be built without one, and the crate exports no domain
/// that accepts everything.
pub trait SyncDomain: Clone + Send + Sync + 'static {
    /// The plaintext event sealed inside an operation body.
    type Event: Serialize + DeserializeOwned + Zeroize + Send + Sync;
    /// The error the store reports for this domain.
    type Error: From<SyncError>
        + From<StoreError>
        + From<ProcessError>
        + From<CausalError>
        + From<GroupCryptoError>
        + std::error::Error
        + Send
        + Sync
        + 'static;

    /// This domain's strings.
    const VOCABULARY: Vocabulary;

    /// Judge an event the local writer is about to sign.
    ///
    /// # Errors
    /// Returns the refusal; nothing is signed or stored.
    fn validate_local(&self, check: LocalCheck<'_, Self>) -> Result<(), Self::Error>;
}
