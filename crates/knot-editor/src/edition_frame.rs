// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! An edition opened by address as a frame, and republished as a fork.
//!
//! Knot owns release intent; the address grammar, verification and fork rules
//! live in Gemot's site library, which Turnstone reaches through here too.
//! Opening verifies the author's log from the address key alone. Republishing
//! never inherits the author's authority: it signs a new log under the
//! republisher's own writer whose first edition names the source in `fork_of`,
//! and the clip's original source URL travels with it. The caller supplies the
//! writer, so key custody stays outside this module.

use site_model::clip::{self, ClipError, ClipFrame};

pub use site_model::address::{AddressError, AddressHost, KnotAddress};
pub use site_model::clip::{CLIP_PATH, ClipPayloadV1, clip_address, clip_body};
/// Edition record types at the revision the site library signs with, for
/// consumers (Turnstone) that sign clip editions.
pub use site_model::editions::{self, EditionId, EditionOperation, EditionWriter, SigningKey};

/// Provenance `source` for editions Knot signs.
pub const KNOT_TOOL: &str = "knot";

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    Address(AddressError),
    Clip(ClipError),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Address(e) => e.fmt(f),
            Self::Clip(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for FrameError {}

impl From<ClipError> for FrameError {
    fn from(error: ClipError) -> Self {
        Self::Clip(error)
    }
}

/// A clip edition open for reading, editing and republishing.
#[derive(Clone, Debug)]
pub struct EditionFrame {
    inner: ClipFrame,
}

/// The republished edition and the payload bytes it names.
pub struct Republished {
    pub operation: EditionOperation,
    pub payload: Vec<u8>,
    /// Address of the new edition, for Copy link.
    pub address: KnotAddress,
}

impl EditionFrame {
    /// Open the edition `address` names from the evidence and payload bytes.
    ///
    /// # Errors
    /// Refuses a malformed address, unverifiable evidence or a payload that is
    /// not the one the edition names.
    pub fn open(
        address: &str,
        operations: &[EditionOperation],
        payload: &[u8],
    ) -> Result<Self, FrameError> {
        let address = KnotAddress::parse(address).map_err(FrameError::Address)?;
        let inner = clip::open_clip(&address, operations, payload)?;
        Ok(Self { inner })
    }

    pub fn payload(&self) -> &ClipPayloadV1 {
        &self.inner.payload
    }

    /// The clipped passage, as a Knot body to render in the frame.
    pub fn knot_body(&self) -> &str {
        &self.inner.payload.knot_body
    }

    /// The page the clip was taken from.
    pub fn source_url(&self) -> &str {
        &self.inner.payload.source_url
    }

    pub fn edition_id(&self) -> EditionId {
        self.inner.edition.id
    }

    /// The address of the edition this frame shows.
    pub fn address(&self) -> KnotAddress {
        clip::clip_address(self.inner.key, self.inner.edition.id)
    }

    /// Republish as a fork under `writer`'s log, with `edited_body` replacing
    /// the passage when given.
    ///
    /// # Errors
    /// Refuses a writer that is not at the start of a log.
    pub fn republish(
        &self,
        writer: &mut EditionWriter,
        author_key: [u8; 32],
        edited_body: Option<&str>,
        created_ms: u64,
    ) -> Result<Republished, FrameError> {
        let edited = edited_body.map(|body| ClipPayloadV1 {
            knot_body: body.into(),
            ..self.inner.payload.clone()
        });
        let (operation, payload) =
            clip::republish(&self.inner, writer, edited.as_ref(), KNOT_TOOL, created_ms)?;
        let address = clip::clip_address(author_key, EditionId(*operation.hash.as_bytes()));
        Ok(Republished {
            operation,
            payload,
            address,
        })
    }
}
