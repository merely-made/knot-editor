// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The admission policy handed to Stickleback.

use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

use p2panda_core::cbor::decode_cbor_strict;
use p2panda_core::{Operation, Topic};
use stickleback::{
    Admission, CausalLimits, GroupCiphertext, OperationPolicy, Reject, StoreTarget,
    validate_causal_metadata,
};

use super::{EncryptionProfile, LOG_ID, SyncExt, Vocabulary};

pub(crate) const CAUSAL_LIMITS: CausalLimits = CausalLimits {
    max_parents: 64,
    max_payload_bytes: 16 * 1024 * 1024,
};

#[derive(Clone)]
pub(crate) struct SyncPolicy {
    pub(crate) space_id: [u8; 32],
    pub(crate) writers: Arc<RwLock<BTreeSet<[u8; 32]>>>,
    pub(crate) encryption: EncryptionProfile,
    pub(crate) words: &'static Vocabulary,
}

impl OperationPolicy<SyncExt> for SyncPolicy {
    type LogId = u64;

    fn admit(&self, operation: &Operation<SyncExt>) -> Result<Admission<u64>, Reject> {
        let words = self.words;
        if operation.header.extensions.space_id != self.space_id {
            return Err(Reject::new(words.wrong_space.0, words.wrong_space.1));
        }
        if operation.header.extensions.encryption != self.encryption {
            return Err(Reject::new(words.wrong_profile.0, words.wrong_profile.1));
        }
        if !self
            .writers
            .read()
            .is_ok_and(|writers| writers.contains(operation.header.verifying_key.as_bytes()))
        {
            return Err(Reject::new(
                words.unadmitted_writer.0,
                words.unadmitted_writer.1,
            ));
        }
        let body = operation
            .body
            .as_ref()
            .ok_or_else(|| Reject::new(words.missing_body.0, words.missing_body.1))?;
        if self.encryption == EncryptionProfile::CommonsDataV1 {
            decode_cbor_strict::<GroupCiphertext, _>(body.to_bytes().as_slice()).map_err(
                |error| {
                    Reject::new(
                        words.bad_group_body.0,
                        format!("{}: {error}", words.bad_group_body.1),
                    )
                },
            )?;
        }
        validate_causal_metadata(
            operation,
            &operation.header.extensions.parents,
            CAUSAL_LIMITS,
        )
        .map_err(|error| Reject::new(words.bad_causality, error.to_string()))?;
        Ok(Admission::keep(StoreTarget::new(
            Topic::from(self.space_id),
            LOG_ID,
        )))
    }
}
