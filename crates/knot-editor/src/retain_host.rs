// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Adapter from a host-issued resident capture capability to desktop retention.
//!
//! The host supplies the Persona display binding.  Knot deliberately does not
//! derive it from a filesystem location or reopen a persona vault here.

use knot_capture::{
    KnotPersonaDisplayV1, KnotRetainEncryptionV1, KnotRetainError, KnotRetainPort,
    KnotRetainReceiptV1, KnotRetainTargetV1,
};

use crate::{KnotEncryptionProfile, KnotFileCapturePort, KnotFileRevisionV1};

/// A host-bound view of one existing resident capture authority.
///
/// Construct this only from a display identity selected by the host and a port
/// issued by that resident. It never opens a vault or accepts raw keys.
pub struct KnotResidentRetainPort {
    target: KnotRetainTargetV1,
    port: KnotFileCapturePort,
}

impl KnotResidentRetainPort {
    /// Bind a host-issued persona display identity to one live capture port.
    ///
    /// The initial destination snapshot prevents the UI from presenting a
    /// target that the port did not authorize. `retain_reviewed` verifies the
    /// live destination again before it signs anything.
    pub fn new(
        persona: KnotPersonaDisplayV1,
        port: KnotFileCapturePort,
    ) -> Result<Self, KnotRetainError> {
        let destination = port.destination().map_err(retain_error)?;
        Ok(Self {
            target: KnotRetainTargetV1 {
                persona,
                space_id: destination.space_id,
                writer: destination.writer,
                encryption: encryption(destination.encryption),
            },
            port,
        })
    }
}

impl KnotRetainPort for KnotResidentRetainPort {
    fn target(&self) -> &KnotRetainTargetV1 {
        &self.target
    }

    fn retain_reviewed(
        &self,
        expected_target: &KnotRetainTargetV1,
        revision: KnotFileRevisionV1,
    ) -> Result<KnotRetainReceiptV1, KnotRetainError> {
        if expected_target != &self.target {
            return Err(KnotRetainError(
                "selected retain destination no longer matches this host capability".into(),
            ));
        }

        let destination = self.port.destination().map_err(retain_error)?;
        let live_target = KnotRetainTargetV1 {
            persona: self.target.persona.clone(),
            space_id: destination.space_id,
            writer: destination.writer,
            encryption: encryption(destination.encryption),
        };
        if live_target != self.target {
            return Err(KnotRetainError(
                "resident retain destination changed; select its current destination before retaining"
                    .into(),
            ));
        }

        let prepared = self.port.prepare(revision).map_err(retain_error)?;
        let prepared_destination = prepared.destination();
        if prepared_destination.space_id != self.target.space_id
            || prepared_destination.writer != self.target.writer
            || encryption(prepared_destination.encryption) != self.target.encryption
        {
            return Err(KnotRetainError(
                "resident retain destination changed while preparing the reviewed revision".into(),
            ));
        }
        let receipt = self.port.retain(&prepared).map_err(retain_error)?;
        Ok(KnotRetainReceiptV1 {
            target: self.target.clone(),
            document_id: receipt.document_id,
            operation: receipt.operation,
            already_retained: receipt.already_retained,
        })
    }
}

fn encryption(profile: KnotEncryptionProfile) -> KnotRetainEncryptionV1 {
    match profile {
        KnotEncryptionProfile::PersonalVaultV1 => KnotRetainEncryptionV1::PersonalVaultV1,
        KnotEncryptionProfile::CommonsDataV1 => KnotRetainEncryptionV1::CommonsDataV1,
    }
}

fn retain_error(error: impl std::fmt::Display) -> KnotRetainError {
    KnotRetainError(error.to_string())
}
