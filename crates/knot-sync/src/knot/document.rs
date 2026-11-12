// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The document record the sync fold produces and consumes.

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// One authored document in the sealed vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub struct VaultDocument {
    /// Stable caller-selected identity.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Authored source bytes, unsealed only inside the endpoint.
    pub body: Vec<u8>,
    /// Source media type.
    pub media_type: String,
}

/// Lowercase hexadecimal text of 32 bytes.
#[must_use]
pub fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
