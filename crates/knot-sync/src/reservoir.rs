// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Opening Knot's operation store through the reservoir: the `notes` mere's
//! archive, one owner per process, under the shared root.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use catchment::{
    DomainId, MereId, ReservoirError, ReservoirStore, mere_dir, open_mere_archive_backend,
    open_reservoir_backend,
};
use muniment::RedbBackend;
use personae::PersonaId;

use crate::{KnotDomain, KnotEncryptionProfile, KnotSyncStore, RecipeValidator};

/// The reservoir domain Knot's operation store belongs to.
pub const NOTES_DOMAIN: &str = "notes";

const ARCHIVE_FILE: &str = "archive.redb";

/// Why the store could not be opened through the reservoir.
#[derive(Debug)]
pub enum ReservoirOpenError {
    /// The reservoir refused: another owner, a corrupt index or a bad path.
    Reservoir(ReservoirError),
}

impl fmt::Display for ReservoirOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reservoir(error) => write!(f, "reservoir refused the notes store: {error}"),
        }
    }
}

impl std::error::Error for ReservoirOpenError {}

impl From<ReservoirError> for ReservoirOpenError {
    fn from(error: ReservoirError) -> Self {
        Self::Reservoir(error)
    }
}

/// Open the `notes` mere of `persona`'s reservoir under `shared_root` and wrap
/// its archive as a Knot sync store. The mere is created when the reservoir has
/// none; `created_at_ms` stamps that creation.
///
/// `recipes` is required: it judges every projection recipe the store admits.
///
/// # Errors
/// Fails when the reservoir or the mere's archive cannot be opened, including
/// when another process already owns the archive.
pub fn open_through_reservoir(
    shared_root: &Path,
    persona: PersonaId,
    created_at_ms: u64,
    space_id: [u8; 32],
    writers: impl IntoIterator<Item = [u8; 32]>,
    encryption: KnotEncryptionProfile,
    recipes: Arc<dyn RecipeValidator>,
) -> Result<KnotSyncStore<RedbBackend>, ReservoirOpenError> {
    let domain = DomainId::new(NOTES_DOMAIN)?;
    let mere: MereId = pollster::block_on(async {
        let index = open_reservoir_backend(shared_root, persona)?;
        let mut reservoir = ReservoirStore::<_, serde_json::Value>::open(index, persona).await?;
        let (record, _) = reservoir.ensure(domain, created_at_ms).await?;
        Ok::<_, ReservoirError>(record.id)
    })?;
    let fresh = !mere_dir(shared_root, persona, mere)
        .join(ARCHIVE_FILE)
        .exists();
    let backend = open_mere_archive_backend(shared_root, persona, mere)?;
    Ok(KnotSyncStore::from_backend(
        backend,
        space_id,
        writers,
        encryption,
        KnotDomain::new(recipes),
        fresh,
    ))
}
