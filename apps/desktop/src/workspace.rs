// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::appearance::Appearance;
use crate::documents::{DocIdentity, DocKey, DocumentWorkspace, TileRole};
use crate::preferences::PreferencesStore;
use cambium::{
    AnyView, GenetCtx, GenetElement, Keyed, Slot, TabMark, TextInput, WorkspaceModel, button, el,
    lens, span, text_field_typed, workspace_view_with_marks,
};
use cambium_genet_winit_host::{
    AppCtx, CloseDisposition, CloseRequest, FocusedTextSlot, HostWake, Key, KeyPress, Runner,
    WindowCommands,
};
use knot_capture::{KnotRetainError, KnotRetainPort, KnotRetainReceiptV1, KnotRetainTargetV1};
use knot_document::{
    KnotDiskComparisonV1, KnotDocumentIntentErrorV1, KnotDocumentIntentV1, KnotDocumentRefusalV1,
    KnotDocumentSession, KnotDocumentSurfaceState, KnotOutlineItemV1, KnotOutlineSnapshotV1,
    knot_document_view_with_highlighting,
};
use knot_file_catalog::{KnotFileCatalog, KnotFileRevisionV1};
use knot_readings::{ReadingBudget, ReadingError, ReadingInput, ReadingResult, ReadingScript};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, TryRecvError},
};
use workbench::{TileEvent, WorkspaceEvent};

const SCRATCH_ADDRESS: &str = "scratch:untitled";

fn hex32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn abbreviated_id(value: &[u8; 32], other_ids: impl Iterator<Item = [u8; 32]>) -> String {
    let full = hex32(value);
    let mut length = 8;
    let others = other_ids
        .filter(|other| other != value)
        .map(|other| hex32(&other))
        .collect::<Vec<_>>();
    while length < full.len()
        && others
            .iter()
            .any(|other| other.starts_with(&full[..length]))
    {
        length += 1;
    }
    full[..length].to_owned()
}

fn retention_target_label(
    target: &KnotRetainTargetV1,
    targets: &[Arc<dyn KnotRetainPort>],
) -> String {
    let space = abbreviated_id(
        &target.space_id,
        targets.iter().map(|port| port.target().space_id),
    );
    let same_space_writers = targets.iter().filter_map(|port| {
        let other = port.target();
        (other.persona.stable_id == target.persona.stable_id && other.space_id == target.space_id)
            .then_some(other.writer)
    });
    let writer = abbreviated_id(&target.writer, same_space_writers);
    let writer_needed = targets.iter().any(|port| {
        let other = port.target();
        other.persona.stable_id == target.persona.stable_id
            && other.space_id == target.space_id
            && other.writer != target.writer
    });
    if writer_needed {
        format!(
            "{} · space {} · writer {}",
            target.persona.label, space, writer
        )
    } else {
        format!("{} · space {}", target.persona.label, space)
    }
}

fn encryption_label(profile: knot_capture::KnotRetainEncryptionV1) -> &'static str {
    match profile {
        knot_capture::KnotRetainEncryptionV1::PersonalVaultV1 => "Personal vault encryption",
        knot_capture::KnotRetainEncryptionV1::CommonsDataV1 => "Shared space encryption",
    }
}
pub const DEFAULT_CAPTURE_MAX_BYTES: usize = 1_048_576;
/// Ceilings on the readings folder listing: enough for a working set, small
/// enough that a stuffed directory cannot become a startup cost.
const MAX_READING_SCRIPTS: usize = 64;
const MAX_READING_SOURCE_BYTES: usize = 64 * 1024;

pub type DesktopView = Box<dyn AnyView<DesktopState, (), GenetCtx, GenetElement>>;
pub type DesktopRunner = Runner<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingAction {
    /// Closing the window while documents have unsaved changes: one prompt
    /// for all of them.
    Quit,
    /// Closing one document's tab over its unsaved changes.
    CloseDocument(DocKey),
    /// Reloading the focused document over its unsaved changes.
    Reload,
}

/// What the desktop keeps per open document: its surface and every reading
/// and binding derived from it. The window's own state stays on
/// [`DesktopState`].
pub struct DocumentEntry {
    pub document: KnotDocumentSurfaceState,
    catalog_source_path: Option<PathBuf>,
    catalog_sync_attempted: bool,
    catalog_id: Option<String>,
    catalog_error: Option<String>,
    prepared_capture: Option<KnotFileRevisionV1>,
    prepared_capture_source_path: Option<PathBuf>,
    prepared_capture_error: Option<String>,
    comparison: Option<KnotDiskComparisonV1>,
    comparison_error: Option<String>,
    pub(crate) fold_snapshot: Option<knot_document::KnotFoldSnapshotV1>,
    pub(crate) collapsed_folds: std::collections::BTreeSet<usize>,
    pub(crate) fold_error: Option<String>,
    outline_snapshot: Option<KnotOutlineSnapshotV1>,
    outline_error: Option<String>,
    pub(crate) reading_result: Option<ReadingResult>,
    pub(crate) reading_error: Option<ReadingError>,
    /// The exact text the retained reading ran against. A reading is host-side
    /// derived state with no snapshot type of its own, so the text it was bound
    /// to is kept here and handed back to the document when a row is selected.
    reading_source: Option<String>,
    focus_source_requested: bool,
    /// Reader fold state of the Micron preview, reconciled after each dispatch.
    pub(crate) micron_folds: crate::scroll_site::MicronPreviewFolds,
}

impl DocumentEntry {
    pub fn new(session: KnotDocumentSession) -> Self {
        Self {
            document: KnotDocumentSurfaceState::new(session),
            catalog_source_path: None,
            catalog_sync_attempted: false,
            catalog_id: None,
            catalog_error: None,
            prepared_capture: None,
            prepared_capture_source_path: None,
            prepared_capture_error: None,
            comparison: None,
            comparison_error: None,
            fold_snapshot: None,
            collapsed_folds: std::collections::BTreeSet::new(),
            fold_error: None,
            outline_snapshot: None,
            outline_error: None,
            reading_result: None,
            reading_error: None,
            reading_source: None,
            focus_source_requested: false,
            micron_folds: Default::default(),
        }
    }

    /// The tab title and duplicate-open identity of this entry's source.
    fn tab(&self) -> (String, DocIdentity) {
        let session = self.document.session();
        let title = self.document.snapshot().display_label;
        match session.source_path() {
            Some(path) => (
                title,
                DocIdentity::Path(
                    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
                ),
            ),
            None => (title, DocIdentity::Scratch),
        }
    }
}

/// State owned by the standalone application around the reusable document surface.
///
/// The path field is an explicit capability supplied by this desktop host. It is
/// intentionally not part of `knot.document.v1`, whose source admission remains
/// the embedding host's responsibility.
pub struct DesktopState {
    /// The open documents and the tiles that show them.
    pub docs: DocumentWorkspace<DocumentEntry>,
    /// What readers see while no document is open: an empty read-only source,
    /// never a save target.
    placeholder: DocumentEntry,
    pub appearance: Appearance,
    pub scroll: crate::scroll_site::ScrollWorkspace,
    pub path: TextInput,
    pub message: Option<String>,
    catalog: Option<KnotFileCatalog>,
    capture_limit: usize,
    outline_visible: bool,
    pub(crate) document_preview_visible: bool,
    pub(crate) fold_visible: bool,
    appearance_open: bool,
    preferences: Option<PreferencesStore>,
    pub(crate) readings_visible: bool,
    readings_root: Option<PathBuf>,
    pub(crate) readings: Vec<ReadingScript>,
    pub(crate) readings_load_notes: Vec<String>,
    pub(crate) readings_selected: Option<usize>,
    window: WindowCommands,
    pending: Option<PendingAction>,
    discard_close: bool,
    retention_targets: Vec<Arc<dyn KnotRetainPort>>,
    retention_wake: Option<HostWake>,
    retention_selected: Option<usize>,
    retention_receiver: Option<Receiver<RetentionUpdate>>,
    retention_busy: Option<RetentionRequest>,
    retention_receipt: Option<KnotRetainReceiptV1>,
    retention_error: Option<String>,
}

#[derive(Clone)]
struct RetentionRequest {
    target: KnotRetainTargetV1,
    document_id: String,
}

enum RetentionUpdate {
    Completed {
        request: RetentionRequest,
        result: Result<KnotRetainReceiptV1, KnotRetainError>,
    },
}

impl DesktopState {
    pub fn new(session: KnotDocumentSession, window: WindowCommands) -> Self {
        Self::with_catalog(session, window, None, None)
    }

    pub fn with_path(
        session: KnotDocumentSession,
        window: WindowCommands,
        initial_path: Option<PathBuf>,
    ) -> Self {
        Self::with_catalog(session, window, initial_path, None)
    }

    pub fn with_catalog(
        session: KnotDocumentSession,
        window: WindowCommands,
        initial_path: Option<PathBuf>,
        catalog: Option<KnotFileCatalog>,
    ) -> Self {
        let site_folder = initial_path.as_ref().filter(|path| path.is_dir()).cloned();
        let path = initial_path.map_or_else(TextInput::default, |path| {
            TextInput::new(path.to_string_lossy())
        });
        let mut state = Self {
            docs: DocumentWorkspace::new(),
            placeholder: DocumentEntry::new(KnotDocumentSession::read_only(SCRATCH_ADDRESS, "")),
            appearance: Appearance::default(),
            scroll: Default::default(),
            path,
            message: None,
            catalog,
            capture_limit: DEFAULT_CAPTURE_MAX_BYTES,
            outline_visible: false,
            document_preview_visible: false,
            fold_visible: false,
            appearance_open: false,
            preferences: None,
            readings_visible: false,
            readings_root: None,
            readings: Vec::new(),
            readings_load_notes: Vec::new(),
            readings_selected: None,
            window,
            pending: None,
            discard_close: false,
            retention_targets: Vec::new(),
            retention_wake: None,
            retention_selected: None,
            retention_receiver: None,
            retention_busy: None,
            retention_receipt: None,
            retention_error: None,
        };
        state.open_entry(DocumentEntry::new(session));
        if let Some(folder) = site_folder {
            match knot_site::Site::open(&folder) {
                Ok(site) => {
                    let page = site.page_path(site.config.format.index_file()).ok();
                    state.scroll.folder = TextInput::new(folder.to_string_lossy());
                    state.scroll.format = site.config.format;
                    if let Some(port) = site.config.format.default_port() {
                        state.scroll.port = TextInput::new(port.to_string());
                    }
                    state.scroll.site = Some(site);
                    state.scroll.visible = true;
                    state.scroll.sync_page(page.as_deref());
                    if let Some(page) = page {
                        state.path = TextInput::new(page.to_string_lossy());
                    }
                },
                Err(error) => state.message = Some(format!("Site: {error}")),
            }
        }
        state.sync_catalog();
        state
    }

    /// The focused document's key, if a document is open.
    pub fn focused_key(&self) -> Option<DocKey> {
        self.docs.focused()
    }

    /// The focused document's entry, or the empty read-only placeholder while
    /// no document is open.
    pub(crate) fn entry(&self) -> &DocumentEntry {
        self.docs
            .focused()
            .and_then(|key| self.docs.doc(key))
            .unwrap_or(&self.placeholder)
    }

    pub(crate) fn entry_mut(&mut self) -> &mut DocumentEntry {
        match self.docs.focused() {
            Some(key) if self.docs.doc(key).is_some() => {
                self.docs.doc_mut(key).expect("focused entry")
            },
            _ => &mut self.placeholder,
        }
    }

    /// The focused document's surface.
    pub fn document(&self) -> &KnotDocumentSurfaceState {
        &self.entry().document
    }

    pub fn document_mut(&mut self) -> &mut KnotDocumentSurfaceState {
        &mut self.entry_mut().document
    }

    /// Open `entry` in a new tab, or focus the tab already showing the same
    /// source. Returns the entry's key.
    fn open_entry(&mut self, entry: DocumentEntry) -> DocKey {
        let (title, identity) = entry.tab();
        self.docs.open(identity, title, entry).0
    }

    /// A document's surface by key, or the placeholder's once that document
    /// has closed: a tile's editor edits its own document, focused or not.
    pub(crate) fn surface_for(&self, key: DocKey) -> &KnotDocumentSurfaceState {
        self.docs
            .doc(key)
            .map_or(&self.placeholder.document, |entry| &entry.document)
    }

    pub(crate) fn surface_mut_for(&mut self, key: DocKey) -> &mut KnotDocumentSurfaceState {
        if self.docs.doc(key).is_some() {
            &mut self.docs.doc_mut(key).expect("open entry").document
        } else {
            &mut self.placeholder.document
        }
    }

    pub fn set_retention_targets(&mut self, targets: Vec<Arc<dyn KnotRetainPort>>, wake: HostWake) {
        self.scroll.set_submission_wake(wake.clone());
        self.retention_targets = targets;
        self.retention_wake = Some(wake);
        self.retention_selected = None;
        self.retention_error = None;
    }

    fn select_retention_target(&mut self, index: usize) {
        if index < self.retention_targets.len() {
            self.retention_selected = Some(index);
            self.retention_error = None;
        }
    }

    fn retain_reviewed(&mut self, wake: HostWake) {
        if self.retention_busy.is_some() {
            self.retention_error =
                Some("Retention is already in progress for the reviewed request.".to_owned());
            return;
        }
        let Some(revision) = self.entry().prepared_capture.clone() else {
            self.retention_error = Some("Review a saved revision before retaining it.".to_owned());
            return;
        };
        let Some(index) = self.retention_selected else {
            self.retention_error = Some("Choose a persona and space before retaining.".to_owned());
            return;
        };
        let Some(port) = self.retention_targets.get(index).cloned() else {
            self.retention_error =
                Some("The selected retention destination is unavailable.".to_owned());
            return;
        };
        let target = port.target().clone();
        let request = RetentionRequest {
            target: target.clone(),
            document_id: revision.document_id.clone(),
        };
        let (sender, receiver) = mpsc::channel();
        self.retention_receiver = Some(receiver);
        self.retention_busy = Some(request.clone());
        self.retention_error = None;
        let wake_for_worker = wake.clone();
        let spawned = std::thread::Builder::new()
            .name("knot-retain".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let retained = port.retain_reviewed(&target, revision);
                    // Completion permits close/reopen. Release the worker's
                    // owner capability before notifying the host.
                    drop(port);
                    retained
                }))
                .map_err(|_| KnotRetainError("Retention could not be confirmed.".to_owned()))
                .and_then(|result| result);
                let _ = sender.send(RetentionUpdate::Completed { request, result });
                wake_for_worker.wake();
            });
        if spawned.is_err() {
            self.retention_receiver = None;
            self.retention_busy = None;
            self.retention_error =
                Some("Retention could not be confirmed: worker could not start.".to_owned());
        }
    }

    fn drain_retention(&mut self) {
        let Some(receiver) = self.retention_receiver.as_ref() else {
            return;
        };
        let mut update = None;
        let mut disconnected = false;
        loop {
            match receiver.try_recv() {
                Ok(value) => update = Some(value),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                },
            }
        }
        if disconnected && update.is_none() {
            self.retention_receiver = None;
            self.retention_busy = None;
            self.retention_error = Some("Retention could not be confirmed; outcome is uncertain. Retry only after checking the destination.".to_owned());
        }
        if let Some(RetentionUpdate::Completed { request, result }) = update {
            self.retention_receiver = None;
            self.retention_busy = None;
            match result {
                Ok(receipt)
                    if receipt.target == request.target
                        && receipt.document_id == request.document_id =>
                {
                    self.retention_receipt = Some(receipt);
                    self.retention_error = None;
                },
                Ok(_) => self.retention_error = Some(
                    "Retention could not be confirmed: destination or document identity changed."
                        .to_owned(),
                ),
                Err(error) => {
                    self.retention_error = Some(format!(
                        "Retention of {} in {} ({}, space {}, writer {}) could not be confirmed: {error}",
                        request.document_id,
                        request.target.persona.label,
                        request.target.persona.stable_id,
                        hex32(&request.target.space_id),
                        hex32(&request.target.writer)
                    ))
                },
            }
        }
    }

    fn dirty(&self) -> bool {
        self.document().snapshot().dirty
    }

    /// The facts a scenario asserts on. Grows as scenarios need more.
    pub(crate) fn scenario_snapshot(&self) -> cambium_genet_winit_host::ProbeSnapshot {
        let snapshot = self.document().snapshot();
        cambium_genet_winit_host::ProbeSnapshot::default()
            .with_field("document", snapshot.display_label)
            .with_field("format", format!("{:?}", snapshot.format))
            .with_field("dirty", snapshot.dirty.to_string())
            .with_field("appearance_open", self.appearance_open.to_string())
            .with_field("message", self.message.clone().unwrap_or_default())
    }

    /// Set the maximum number of source bytes prepared by the saved revision
    /// review control. This only affects the next explicit preparation.
    pub fn set_capture_limit(&mut self, max_bytes: usize) {
        self.capture_limit = max_bytes;
    }

    fn clear_prepared_capture(&mut self) {
        self.entry_mut().prepared_capture = None;
        self.entry_mut().prepared_capture_source_path = None;
        self.entry_mut().prepared_capture_error = None;
    }

    fn prepare_capture(&mut self) {
        self.clear_prepared_capture();
        let Some(id) = self.entry().catalog_id.clone() else {
            self.entry_mut().prepared_capture_error = Some(
                self.entry()
                    .catalog_error
                    .clone()
                    .unwrap_or_else(|| "Current document is not catalogued.".to_owned()),
            );
            return;
        };
        let Some(source_path) = self
            .document()
            .session()
            .source_path()
            .map(Path::to_path_buf)
        else {
            self.entry_mut().prepared_capture_error =
                Some("Current document has no saved source path.".to_owned());
            return;
        };
        if !source_path.is_file() {
            self.entry_mut().prepared_capture_error =
                Some("Current saved source path is unavailable.".to_owned());
            return;
        }
        let Some(catalog) = self.catalog.as_ref() else {
            return;
        };
        match catalog.lookup(&source_path) {
            Ok(Some(record)) if record.id == id => {},
            Ok(_) => {
                self.entry_mut().prepared_capture_error = Some(
                    "Current source no longer matches its catalog binding. Retry catalog registration."
                        .to_owned(),
                );
                return;
            },
            Err(error) => {
                self.entry_mut().prepared_capture_error =
                    Some(format!("Catalog lookup failed: {error}"));
                return;
            },
        }
        match catalog.capture_file_revision(&id, self.capture_limit) {
            Ok(revision) => match std::str::from_utf8(&revision.body) {
                Ok(_) => {
                    self.entry_mut().prepared_capture_source_path = Some(source_path);
                    self.entry_mut().prepared_capture = Some(revision);
                },
                Err(_) => {
                    self.entry_mut().prepared_capture_error = Some(
                        "Saved revision is not valid UTF-8; source text is unavailable.".to_owned(),
                    );
                },
            },
            Err(error) => {
                self.entry_mut().prepared_capture_error =
                    Some(format!("Preparation failed: {error}"))
            },
        }
    }

    fn discard_prepared_capture(&mut self) {
        self.clear_prepared_capture();
    }

    fn path_value(&self) -> Result<PathBuf, String> {
        let value = self.path.text().trim();
        if value.is_empty() {
            Err("Enter a file path first.".to_owned())
        } else {
            Ok(PathBuf::from(value))
        }
    }

    fn sync_catalog(&mut self) {
        if self.catalog.is_none() {
            return;
        }
        let source_path = self
            .document()
            .session()
            .source_path()
            .map(Path::to_path_buf);
        if self.entry().catalog_sync_attempted && self.entry().catalog_source_path == source_path {
            return;
        }
        self.entry_mut().catalog_sync_attempted = true;
        self.entry_mut().catalog_source_path = source_path.clone();
        self.entry_mut().catalog_id = None;
        self.entry_mut().catalog_error = None;
        let Some(source_path) = source_path else {
            return;
        };
        if let Some(catalog) = self.catalog.as_mut() {
            match catalog.bind(source_path) {
                Ok(id) => self.entry_mut().catalog_id = Some(id),
                Err(error) => self.entry_mut().catalog_error = Some(error),
            }
        }
    }

    fn retry_catalog_binding(&mut self) {
        if self.catalog.is_none() {
            return;
        }
        self.entry_mut().catalog_sync_attempted = false;
        self.sync_catalog();
    }

    fn clear_comparison(&mut self) {
        self.entry_mut().comparison = None;
        self.entry_mut().comparison_error = None;
    }

    fn clear_outline(&mut self) {
        self.entry_mut().outline_snapshot = None;
        self.entry_mut().outline_error = None;
    }

    pub(crate) fn clear_folding(&mut self) {
        self.fold_visible = false;
        self.entry_mut().fold_snapshot = None;
        self.entry_mut().collapsed_folds.clear();
        self.entry_mut().fold_error = None;
    }

    fn toggle_folding(&mut self) {
        if !crate::document_folding::supported(self.document().snapshot().format) {
            return;
        }
        self.fold_visible = !self.fold_visible;
        if self.fold_visible {
            self.entry_mut().fold_snapshot = Some(self.document().session().fold_snapshot());
            self.entry_mut().collapsed_folds.clear();
            self.entry_mut().fold_error = None;
        } else {
            self.clear_folding();
        }
    }

    pub(crate) fn edit_source(&mut self) {
        self.clear_folding();
        self.entry_mut().focus_source_requested = true;
    }

    pub(crate) fn toggle_fold(
        &mut self,
        action_snapshot: knot_document::KnotFoldSnapshotV1,
        source_index: usize,
    ) {
        if !self
            .entry()
            .fold_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot == &action_snapshot)
            || !crate::document_folding::snapshot_matches(
                self.document().session(),
                &action_snapshot,
            )
        {
            self.entry_mut().fold_snapshot = Some(self.document().session().fold_snapshot());
            self.entry_mut().collapsed_folds.clear();
            self.entry_mut().fold_error =
                Some("Fold snapshot is stale for the current source.".to_owned());
            return;
        }
        if !crate::document_folding::normalized_folds(&action_snapshot)
            .iter()
            .any(|fold| fold.source_index == source_index)
        {
            self.entry_mut().fold_error =
                Some("That fold is no longer part of the current source.".to_owned());
            return;
        }
        if !self.entry_mut().collapsed_folds.insert(source_index) {
            self.entry_mut().collapsed_folds.remove(&source_index);
        }
        self.entry_mut().fold_error = None;
    }

    pub(crate) fn collapse_all_folds(
        &mut self,
        action_snapshot: knot_document::KnotFoldSnapshotV1,
    ) {
        if !self
            .entry()
            .fold_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot == &action_snapshot)
            || !crate::document_folding::snapshot_matches(
                self.document().session(),
                &action_snapshot,
            )
        {
            self.entry_mut().fold_snapshot = Some(self.document().session().fold_snapshot());
            self.entry_mut().collapsed_folds.clear();
            self.entry_mut().fold_error =
                Some("Fold snapshot is stale for the current source.".to_owned());
            return;
        }
        self.entry_mut().collapsed_folds =
            crate::document_folding::collapse_all_indices(&action_snapshot);
        self.entry_mut().fold_error = None;
    }

    pub(crate) fn expand_all_folds(&mut self, action_snapshot: knot_document::KnotFoldSnapshotV1) {
        if self
            .entry()
            .fold_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot == &action_snapshot)
            && crate::document_folding::snapshot_matches(
                self.document().session(),
                &action_snapshot,
            )
        {
            self.entry_mut().collapsed_folds.clear();
            self.entry_mut().fold_error = None;
        } else {
            self.entry_mut().fold_snapshot = Some(self.document().session().fold_snapshot());
            self.entry_mut().collapsed_folds.clear();
            self.entry_mut().fold_error =
                Some("Fold snapshot is stale for the current source.".to_owned());
        }
    }

    fn sync_outline_snapshot(&mut self) {
        if !self.outline_visible {
            return;
        }
        let current = self.document().snapshot();
        let stale = self
            .entry()
            .outline_snapshot
            .as_ref()
            .is_none_or(|snapshot| {
                snapshot.address != current.source.address || snapshot.source_text != current.text
            });
        if stale {
            self.entry_mut().outline_snapshot = Some(self.document().session().outline_snapshot());
        }
    }

    fn toggle_outline(&mut self) {
        self.outline_visible = !self.outline_visible;
        if self.outline_visible {
            self.entry_mut().outline_snapshot = Some(self.document().session().outline_snapshot());
            self.entry_mut().outline_error = None;
        } else {
            self.clear_outline();
        }
    }

    /// The readings directory this host offers. The panel never picks a path of
    /// its own; the launcher supplies one or the lane stays empty.
    pub fn set_readings_root(&mut self, root: Option<PathBuf>) {
        self.readings_root = root;
        self.refresh_readings();
    }

    /// The preferences file this host owns. Loading applies its appearance; a
    /// file that cannot be read leaves defaults and says why, and a file a
    /// newer Knot wrote loads what this one knows and says that.
    pub fn set_preferences_path(&mut self, path: Option<PathBuf>) {
        self.preferences = path.map(PreferencesStore::open);
        let Some(store) = &self.preferences else {
            return;
        };
        self.appearance = store.preferences().appearance.known.clone();
        let line = store
            .unreadable()
            .map(|why| {
                format!(
                    "Preferences could not be read, so defaults are in use and changes are not saved: {why}"
                )
            })
            .or_else(|| store.written_by_newer());
        if let Some(line) = line {
            self.message = Some(match self.message.take() {
                Some(earlier) => format!("{earlier} {line}"),
                None => line,
            });
        }
    }

    pub fn preferences(&self) -> Option<&PreferencesStore> {
        self.preferences.as_ref()
    }

    /// Every appearance control goes through here, so each change is saved.
    pub(crate) fn update_appearance(&mut self, change: impl FnOnce(&mut Appearance)) {
        change(&mut self.appearance);
        let Some(store) = self.preferences.as_mut() else {
            return;
        };
        if let Err(error) = store.save_appearance(&self.appearance) {
            self.message = Some(format!("Appearance not saved: {error}"));
        }
    }

    fn reset_preferences(&mut self) {
        let Some(store) = self.preferences.as_mut() else {
            return;
        };
        self.message = Some(match store.reset(&self.appearance) {
            Ok(()) => format!("Preferences file reset: {}.", store.path().display()),
            Err(error) => format!("Preferences reset failed: {error}"),
        });
    }

    pub(crate) fn readings_root_label(&self) -> String {
        match &self.readings_root {
            Some(root) => format!("Scripts: {}", root.display()),
            None => "Scripts: this window has no readings folder".to_owned(),
        }
    }

    pub(crate) fn refresh_readings(&mut self) {
        let Some(root) = self.readings_root.clone() else {
            self.readings.clear();
            self.readings_load_notes.clear();
            self.readings_selected = None;
            return;
        };
        // Selection follows the script's name, not its index, so a refresh that
        // adds or drops a file cannot silently arm a different reading.
        let selected = self
            .readings_selected
            .and_then(|index| self.readings.get(index))
            .map(|script| script.name.clone());
        let (scripts, notes) =
            knot_readings::load_dir(&root, MAX_READING_SCRIPTS, MAX_READING_SOURCE_BYTES);
        self.readings = scripts;
        self.readings_load_notes = notes;
        self.readings_selected =
            selected.and_then(|name| self.readings.iter().position(|script| script.name == name));
    }

    pub(crate) fn toggle_readings(&mut self) {
        self.readings_visible = !self.readings_visible;
        if self.readings_visible {
            self.refresh_readings();
        }
    }

    pub(crate) fn select_reading(&mut self, index: usize) {
        if index < self.readings.len() {
            self.readings_selected = Some(index);
            self.entry_mut().reading_error = None;
        }
    }

    /// Run the chosen reading over the current source, synchronously. The
    /// budget is the lane's own; a runaway returns a receipt, not a hang.
    pub(crate) fn run_reading(&mut self) {
        let Some(script) = self
            .readings_selected
            .and_then(|index| self.readings.get(index))
            .cloned()
        else {
            self.message = Some("Choose a reading before running one.".to_owned());
            return;
        };
        let input = match ReadingInput::from_session(self.document().session()) {
            Ok(input) => input,
            Err(error) => {
                self.clear_readings();
                self.entry_mut().reading_error = Some(ReadingError::Runtime { message: error });
                return;
            },
        };
        let source = input.text.clone();
        match knot_readings::run(&script, &input, ReadingBudget::default()) {
            Ok(result) => {
                self.entry_mut().reading_result = Some(result);
                self.entry_mut().reading_source = Some(source);
                self.entry_mut().reading_error = None;
            },
            Err(error) => {
                self.clear_readings();
                self.entry_mut().reading_error = Some(error);
            },
        }
    }

    /// A reading is derived state: it is stale the moment its source moves.
    /// The panel says so and keeps showing it rather than closing itself.
    pub(crate) fn reading_is_stale(&self) -> bool {
        let Some(result) = self.entry().reading_result.as_ref() else {
            return false;
        };
        let current = self.document().snapshot();
        result.provenance.source.address != current.source.address
            || self.entry().reading_source.as_deref() != Some(current.text.as_str())
    }

    pub(crate) fn select_reading_row(&mut self, index: usize) {
        let Some(result) = self.entry().reading_result.as_ref() else {
            return;
        };
        let Some(row) = result.rows.get(index) else {
            return;
        };
        let Some((start, end)) = row.span else {
            self.entry_mut().reading_error = Some(ReadingError::Runtime {
                message: format!("\"{}\" carries no source range.", row.label),
            });
            return;
        };
        let address = result.provenance.source.address.clone();
        let Some(text) = self.entry().reading_source.clone() else {
            return;
        };
        match self
            .document_mut()
            .session_mut()
            .select_source_span(&address, &text, start, end)
        {
            Ok(()) => {
                self.entry_mut().reading_error = None;
                self.entry_mut().focus_source_requested = true;
            },
            Err(error) => {
                self.entry_mut().reading_error = Some(ReadingError::Runtime {
                    message: error.clone(),
                });
                self.message = Some(format!("Reading row selection failed: {error}"));
            },
        }
    }

    /// Drop the derived reading. The script list and the panel itself belong to
    /// the window, not to the document, so they stay.
    fn clear_readings(&mut self) {
        self.entry_mut().reading_result = None;
        self.entry_mut().reading_error = None;
        self.entry_mut().reading_source = None;
    }

    fn toggle_appearance(&mut self) {
        self.appearance_open = !self.appearance_open;
    }

    pub(crate) fn select_preview_heading(
        &mut self,
        address: &str,
        source_text: &str,
        heading: &KnotOutlineItemV1,
        index: usize,
    ) {
        let result = self.document().session().preview_snapshot();
        let result = result.and_then(|snapshot| {
            if snapshot.address != address
                || snapshot.source_text != source_text
                || snapshot.headings.get(index) != Some(heading)
            {
                return Err("preview heading is stale for the current source".to_owned());
            }
            self.document_mut()
                .session_mut()
                .select_preview_heading(&snapshot, index)
        });
        match result {
            Ok(()) => self.entry_mut().focus_source_requested = true,
            Err(error) => self.message = Some(format!("Preview heading selection failed: {error}")),
        }
    }

    fn select_outline_item(&mut self, index: usize) {
        let Some(snapshot) = self.entry().outline_snapshot.clone() else {
            self.entry_mut().outline_error =
                Some("Outline is not ready; show it again.".to_owned());
            return;
        };
        let result = self
            .document_mut()
            .session_mut()
            .select_outline_item(&snapshot, index);
        match result {
            Ok(()) => {
                self.entry_mut().outline_error = None;
                self.entry_mut().focus_source_requested = true;
            },
            Err(error) => {
                self.entry_mut().outline_error = Some(error.clone());
                self.message = Some(format!("Outline selection failed: {error}"));
            },
        }
    }

    fn compare_disk(&mut self) {
        match self.document().session().compare_disk() {
            Ok(comparison) => {
                self.entry_mut().comparison = Some(comparison);
                self.entry_mut().comparison_error = None;
            },
            Err(error) => {
                self.entry_mut().comparison = None;
                self.entry_mut().comparison_error = Some(error);
            },
        }
    }

    fn hide_comparison(&mut self) {
        self.clear_comparison();
    }

    pub(crate) fn request(&mut self, action: PendingAction) {
        if self.scroll.metadata_dirty() {
            self.message = Some("Save or discard metadata edits before changing documents.".into());
            return;
        }
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::Quit => self.window.close(),
            PendingAction::CloseDocument(key) => self.close_document(key),
            PendingAction::Reload => {
                match self.document_mut().apply(KnotDocumentIntentV1::Reload) {
                    Ok(_) => {
                        self.clear_comparison();
                        self.clear_prepared_capture();
                        self.clear_outline();
                        self.clear_readings();
                        self.clear_folding();
                        self.sync_outline_snapshot();
                        self.sync_catalog();
                        self.message = Some("Reloaded from disk.".to_owned());
                    },
                    Err(error) => self.message = Some(intent_error_label(error)),
                }
            },
        }
    }

    /// Readings, bindings and the site panel's page follow the focused
    /// document: bring them up to date after the focus moves.
    fn after_focus_change(&mut self) {
        self.clear_folding();
        self.sync_outline_snapshot();
        self.sync_catalog();
        let page = self
            .document()
            .session()
            .source_path()
            .map(Path::to_path_buf);
        self.scroll.sync_page(page.as_deref());
    }

    /// Metadata edits belong to the site page they were made on, so the focus
    /// stays there until they are saved or discarded.
    fn metadata_holds_focus(&mut self) -> bool {
        if self.scroll.metadata_dirty() {
            self.message = Some("Save or discard metadata edits before changing documents.".into());
        }
        self.scroll.metadata_dirty()
    }

    fn new_document(&mut self) {
        if self.scroll.metadata_dirty() {
            self.message = Some("Save or discard metadata edits before changing documents.".into());
            return;
        }
        self.open_entry(DocumentEntry::new(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "",
        )));
        self.path = TextInput::default();
        self.after_focus_change();
        self.message = Some("New untitled Djot document.".to_owned());
    }

    fn open_document(&mut self) {
        match self.path_value() {
            Ok(path) => self.open_path(path),
            Err(error) => self.message = Some(error),
        }
    }

    /// Open `path` in a new tab, or switch to the tab already showing it.
    pub(crate) fn open_path(&mut self, path: PathBuf) {
        if self.scroll.metadata_dirty() {
            self.message = Some("Save or discard metadata edits before changing documents.".into());
            return;
        }
        let resolved = std::fs::canonicalize(&path).ok();
        let identity = DocIdentity::Path(resolved.clone().unwrap_or_else(|| path.clone()));
        if let Some(key) = self.docs.find(&identity) {
            self.docs.focus(key);
            self.after_focus_change();
            let label = self.document().snapshot().display_label;
            self.message = Some(format!("{label} is already open."));
            return;
        }
        match KnotDocumentSession::open(&path) {
            Ok(session) => {
                self.path = TextInput::new(path.to_string_lossy().into_owned());
                self.open_entry(DocumentEntry::new(session));
                self.after_focus_change();
                self.message = Some(format!("Opened {}.", path.display()));
            },
            Err(error) => self.message = Some(format!("Open failed: {error}")),
        }
    }

    fn save(&mut self) {
        if self.focused_key().is_none() {
            self.message = Some("No document is open.".to_owned());
            return;
        }
        match self.document_mut().apply(KnotDocumentIntentV1::Save) {
            Ok(_) => {
                self.sync_catalog();
                self.message = Some("Saved.".to_owned());
            },
            Err(error) => self.message = Some(intent_error_label(error)),
        }
    }

    fn save_as(&mut self) -> bool {
        if self.scroll.metadata_dirty() {
            self.message = Some("Save or discard metadata edits before Save As.".into());
            return false;
        }
        let Some(key) = self.focused_key() else {
            self.message = Some("No document is open.".to_owned());
            return false;
        };
        let path = match self.path_value() {
            Ok(path) => path,
            Err(error) => {
                self.message = Some(error);
                return false;
            },
        };
        match self
            .document_mut()
            .apply(KnotDocumentIntentV1::SaveAs(path.clone()))
        {
            Ok(_) => {
                self.clear_comparison();
                self.clear_prepared_capture();
                self.clear_outline();
                self.clear_readings();
                self.clear_folding();
                self.sync_outline_snapshot();
                self.sync_catalog();
                let resolved = std::fs::canonicalize(&path).ok();
                let (title, identity) = self.entry().tab();
                self.docs.set_identity(key, identity);
                self.docs.set_title(key, title);
                self.scroll.sync_page(resolved.as_deref());
                self.message = Some(format!("Saved as {}.", path.display()));
                true
            },
            Err(error) => {
                self.message = Some(intent_error_label(error));
                false
            },
        }
    }

    fn reload(&mut self) {
        if self.focused_key().is_none() {
            self.message = Some("No document is open.".to_owned());
            return;
        }
        self.request(PendingAction::Reload);
    }

    /// Save the focused document for a pending action: Save As from the path
    /// field for scratch, an ordinary save otherwise.
    fn save_for_pending(&mut self) -> bool {
        if self.document().snapshot().write_posture
            == knot_document::KnotDocumentWritePostureV1::Scratch
        {
            self.save_as()
        } else {
            match self.document_mut().apply(KnotDocumentIntentV1::Save) {
                Ok(_) => {
                    self.sync_catalog();
                    true
                },
                Err(error) => {
                    self.message = Some(intent_error_label(error));
                    false
                },
            }
        }
    }

    /// Documents with unsaved changes, in the order they were opened.
    pub(crate) fn dirty_documents(&self) -> Vec<DocKey> {
        self.docs
            .docs()
            .filter(|(_, entry)| entry.document.snapshot().dirty)
            .map(|(key, _)| key)
            .collect()
    }

    fn confirm_save(&mut self) {
        if self.scroll.metadata_dirty() {
            self.message = Some("Save or discard metadata edits before changing documents.".into());
            return;
        }
        let Some(action) = self.pending.take() else {
            return;
        };
        match action {
            PendingAction::Quit => {
                // Save every file-backed document. A scratch document or a
                // refused save stays listed, and the window stays open.
                let focused = self.focused_key();
                let mut failures = Vec::new();
                for key in self.dirty_documents() {
                    self.docs.focus(key);
                    if self.document().snapshot().write_posture
                        == knot_document::KnotDocumentWritePostureV1::Scratch
                    {
                        continue;
                    }
                    match self.document_mut().apply(KnotDocumentIntentV1::Save) {
                        Ok(_) => self.sync_catalog(),
                        Err(error) => failures.push(intent_error_label(error)),
                    }
                }
                if let Some(key) = focused {
                    self.docs.focus(key);
                }
                if self.dirty_documents().is_empty() {
                    self.window.close();
                } else {
                    self.pending = Some(PendingAction::Quit);
                    self.message = Some(match failures.last() {
                        Some(error) => error.clone(),
                        None => {
                            "Scratch documents need Save As or a discard before closing.".to_owned()
                        },
                    });
                }
            },
            PendingAction::CloseDocument(key) => {
                self.docs.focus(key);
                if self.save_for_pending() {
                    self.close_document(key);
                } else {
                    self.pending = Some(PendingAction::CloseDocument(key));
                }
            },
            PendingAction::Reload => {
                if self.save_for_pending() {
                    self.perform(PendingAction::Reload);
                } else {
                    self.pending = Some(PendingAction::Reload);
                }
            },
        }
    }

    fn confirm_discard(&mut self) {
        if self.scroll.metadata_dirty() {
            self.message = Some("Save or discard metadata edits before changing documents.".into());
            return;
        }
        let Some(action) = self.pending.take() else {
            return;
        };
        if matches!(action, PendingAction::Quit) {
            self.discard_close = true;
            self.window.close();
        } else {
            self.perform(action);
        }
    }

    /// Leave the quit prompt and show the first document it listed.
    fn review_pending(&mut self) {
        let first = self.dirty_documents().first().copied();
        if first.is_some() && first != self.focused_key() && self.metadata_holds_focus() {
            return;
        }
        self.pending = None;
        if let Some(key) = first {
            self.docs.focus(key);
            self.after_focus_change();
        }
    }

    fn cancel_pending(&mut self) {
        self.pending = None;
    }

    /// Apply a gesture from the frame: activation moves the focus, a close
    /// preflights its document, a divider move applies. Drags between stacks
    /// are declined until the workspace adopts them.
    fn on_workspace_event(&mut self, event: WorkspaceEvent) {
        match &event {
            WorkspaceEvent::Tile(TileEvent::Activated(tile)) => {
                let before = self.focused_key();
                let target = match self.docs.role(*tile) {
                    Some(TileRole::Document(key)) => Some(*key),
                    _ => None,
                };
                if target.is_some() && target != before && self.metadata_holds_focus() {
                    return;
                }
                self.docs.activate(*tile);
                if self.focused_key() != before {
                    self.after_focus_change();
                }
            },
            WorkspaceEvent::Tile(TileEvent::Closed(tile)) => self.request_close_tile(*tile),
            WorkspaceEvent::Tile(TileEvent::DividerMoved { .. }) => {
                self.docs.apply_layout(&event);
            },
            _ => {
                self.message = Some("Moving tabs between stacks is not available yet.".to_owned());
            },
        }
    }

    fn request_close_tile(&mut self, tile: workbench::TileId) {
        let Some(TileRole::Document(key)) = self.docs.role(tile).cloned() else {
            self.docs.close(tile);
            return;
        };
        let dirty = self
            .docs
            .doc(key)
            .is_some_and(|entry| entry.document.snapshot().dirty);
        // Asking about a tab focuses it, and closing the focused tab moves
        // the focus on.
        if (dirty || self.focused_key() == Some(key)) && self.metadata_holds_focus() {
            return;
        }
        if dirty {
            self.docs.focus(key);
            self.after_focus_change();
            self.pending = Some(PendingAction::CloseDocument(key));
        } else {
            self.close_document(key);
        }
    }

    /// Close a document's tab without asking; its entry goes with it.
    fn close_document(&mut self, key: DocKey) {
        let retaining = self
            .retention_busy
            .as_ref()
            .zip(
                self.docs
                    .doc(key)
                    .and_then(|entry| entry.catalog_id.as_ref()),
            )
            .is_some_and(|(busy, id)| &busy.document_id == id);
        if retaining {
            self.message =
                Some("Wait for retention to finish before closing this document.".to_owned());
            return;
        }
        let label = self
            .docs
            .doc(key)
            .map(|entry| entry.document.snapshot().display_label);
        let before = self.focused_key();
        if let Some(tile) = self.docs.tile_of(key) {
            self.docs.close(tile);
        }
        if self.focused_key() != before {
            self.after_focus_change();
        }
        if let Some(label) = label {
            self.message = Some(format!("Closed {label}."));
        }
    }

    fn close_request(&mut self, request: CloseRequest) -> CloseDisposition {
        if self.scroll.metadata_dirty() {
            self.scroll.visible = true;
            self.message = Some("Save or discard metadata edits before closing.".into());
            return CloseDisposition::KeepVisible;
        }
        if self.retention_busy.is_some() {
            self.message = Some("Wait for retention to finish before closing.".to_owned());
            self.discard_close = false;
            return CloseDisposition::KeepVisible;
        }
        if self.discard_close {
            self.discard_close = false;
            return CloseDisposition::Exit;
        }
        if !self.dirty_documents().is_empty() {
            self.pending = Some(PendingAction::Quit);
            return CloseDisposition::KeepVisible;
        }
        if matches!(request, CloseRequest::Native | CloseRequest::Command) {
            CloseDisposition::Exit
        } else {
            CloseDisposition::KeepVisible
        }
    }
}

fn intent_error_label(error: KnotDocumentIntentErrorV1) -> String {
    match error {
        KnotDocumentIntentErrorV1::Refused(KnotDocumentRefusalV1::ExternalChange) => {
            "Save refused: file changed on disk. Compare, Reload, or choose a new Save As path."
                .to_owned()
        },
        KnotDocumentIntentErrorV1::Refused(refusal) => format!("Action refused: {refusal:?}"),
        KnotDocumentIntentErrorV1::SaveFailed(failure) => {
            format!("Save failed: {}", failure.message)
        },
    }
}

pub fn desktop_view(state: &DesktopState) -> DesktopView {
    let retention_panel: DesktopView = {
        let destinations = if state.retention_targets.is_empty() {
            Box::new(span("No storage destinations available.")) as DesktopView
        } else {
            let rows = state
                .retention_targets
                .iter()
                .enumerate()
                .map(|(index, port)| {
                    let target = port.target();
                    let selected = state.retention_selected == Some(index);
                    let label = retention_target_label(target, &state.retention_targets);
                    (
                        index,
                        button(label, move |state: &mut DesktopState, _| {
                            state.select_retention_target(index)
                        })
                        .attr("data-retention-target", index.to_string())
                        .attr("aria-pressed", selected.to_string()),
                    )
                })
                .collect::<Vec<_>>();
            Box::new(el("div", Keyed::new(rows)).attr("class", "knot-retention-targets"))
                as DesktopView
        };
        let selected_detail: DesktopView = if let Some(port) = state
            .retention_selected
            .and_then(|index| state.retention_targets.get(index))
        {
            let target = port.target();
            Box::new(
                el(
                    "div",
                    (
                        span(format!("Persona: {}", target.persona.label)),
                        span(format!("Persona stable ID: {}", target.persona.stable_id)),
                        span(format!("Space ID: {}", hex32(&target.space_id))),
                        span(format!("Writer ID: {}", hex32(&target.writer))),
                        span(format!(
                            "Encryption: {}",
                            encryption_label(target.encryption)
                        )),
                    ),
                )
                .attr("class", "knot-retention-selected")
                .attr("role", "region")
                .attr("aria-label", "Selected retention destination")
                .attr("aria-live", "polite"),
            )
        } else {
            Box::new(
                el("div", span("No destination selected."))
                    .attr("class", "knot-retention-selected")
                    .attr("role", "region")
                    .attr("aria-label", "Selected retention destination")
                    .attr("aria-live", "polite"),
            )
        };
        let busy = state.retention_busy.as_ref().map(|request| {
            span(format!(
                "Retaining {} in {} ({}, space {}, writer {})…",
                request.document_id,
                request.target.persona.label,
                request.target.persona.stable_id,
                hex32(&request.target.space_id),
                hex32(&request.target.writer)
            ))
            .attr("class", "knot-retention-busy")
        });
        let error = state
            .retention_error
            .as_ref()
            .map(|error| span(error.clone()).attr("class", "knot-retention-error"));
        let receipt = state.retention_receipt.as_ref().map(|receipt| {
            span(format!(
                "Last confirmed retention: {} in {} ({}, {}, space {}, writer {}, operation {}){}",
                receipt.document_id,
                receipt.target.persona.label,
                receipt.target.persona.stable_id,
                encryption_label(receipt.target.encryption),
                hex32(&receipt.target.space_id),
                hex32(&receipt.target.writer),
                hex32(&receipt.operation),
                if receipt.already_retained {
                    "; already retained"
                } else {
                    ""
                }
            ))
            .attr("class", "knot-retention-receipt")
        });
        let retain_button = if state.retention_busy.is_none()
            && state.entry().prepared_capture.is_some()
            && state.retention_selected.is_some()
        {
            state.retention_wake.clone().map(|wake| {
                button(
                    "Retain reviewed revision",
                    move |state: &mut DesktopState, _| state.retain_reviewed(wake.clone()),
                )
            })
        } else {
            None
        };
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Retain reviewed revision"),
                            span("Choose a persona and space"),
                        ),
                    ),
                    destinations,
                    selected_detail,
                    retain_button,
                    busy,
                    error,
                    receipt,
                ),
            )
            .attr("class", "knot-retention")
            .attr("role", "region")
            .attr("aria-label", "Retain reviewed revision"),
        )
    };
    let review_panel: DesktopView = if state.catalog.is_none() {
        Box::new(el("div", ()))
    } else if let Some(error) = &state.entry().prepared_capture_error {
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Saved revision review"),
                            span(format!("Limit: {} bytes", state.capture_limit)),
                        ),
                    )
                    .attr("class", "knot-review-header"),
                    span(format!("Review unavailable: {error}")).attr("class", "knot-review-error"),
                    state.entry().catalog_id.as_ref().map(|_| {
                        button("Refresh saved revision", |state: &mut DesktopState, _| {
                            state.prepare_capture()
                        })
                    }),
                    button("Discard saved revision", |state: &mut DesktopState, _| {
                        state.discard_prepared_capture()
                    }),
                ),
            )
            .attr("class", "knot-review knot-review-error-panel")
            .attr("role", "region")
            .attr("aria-label", "Saved revision review error"),
        )
    } else if let Some(revision) = &state.entry().prepared_capture {
        let source_path = state
            .entry()
            .prepared_capture_source_path
            .as_ref()
            .map_or_else(
                || "(source path unavailable)".to_owned(),
                |path| path.display().to_string(),
            );
        let current = state.document().snapshot();
        let differs = current.text.as_bytes() != revision.body.as_slice();
        let status = if differs {
            "Editor differs from prepared revision; refresh to read disk again"
        } else {
            "Prepared snapshot; refresh to read disk again"
        };
        let body = String::from_utf8(revision.body.clone()).expect("validated review body");
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Saved revision review"),
                            span(format!("Limit: {} bytes", state.capture_limit)),
                        ),
                    )
                    .attr("class", "knot-review-header"),
                    span(format!("Document ID: {}", revision.document_id)),
                    span(format!("Title: {}", revision.title)),
                    span(format!("Media type: {}", revision.media_type)),
                    span(format!("Source path: {source_path}")),
                    span(format!("Prepared bytes: {}", revision.body.len())),
                    span("Unsaved changes are excluded."),
                    span("Reviewing does not store or share bytes. Retain uses the selected destination."),
                    span(status).attr("class", "knot-review-status"),
                    button("Refresh saved revision", |state: &mut DesktopState, _| {
                        state.prepare_capture()
                    }),
                    button("Discard saved revision", |state: &mut DesktopState, _| {
                        state.discard_prepared_capture()
                    }),
                    el("pre", body).attr("class", "knot-review-source"),
                ),
            )
            .attr("class", "knot-review")
            .attr("role", "region")
            .attr("aria-label", "Saved revision review"),
        )
    } else if let Some(id) = &state.entry().catalog_id {
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Saved revision review"),
                            span(format!("Limit: {} bytes", state.capture_limit)),
                        ),
                    )
                    .attr("class", "knot-review-header"),
                    button("Review saved revision", |state: &mut DesktopState, _| {
                        state.prepare_capture()
                    }),
                ),
            )
            .attr("class", "knot-review")
            .attr("aria-label", format!("Saved revision review for {id}")),
        )
    } else {
        Box::new(el("div", ()))
    };
    let appearance_panel: DesktopView = if state.appearance_open {
        let appearance = &state.appearance;
        // Saves refuse while the file is unreadable; say so where the controls
        // are, and offer the one action that may overwrite it.
        let preferences_note: DesktopView = match state
            .preferences
            .as_ref()
            .and_then(PreferencesStore::unreadable)
        {
            Some(why) => Box::new(
                el(
                    "div",
                    (
                        span(format!(
                            "Not saving appearance: the preferences file could not be read ({why})."
                        )),
                        button("Reset preferences file", |state: &mut DesktopState, _| {
                            state.reset_preferences();
                        })
                        .attr(
                            "aria-label",
                            "Reset preferences file, replacing its unreadable contents",
                        ),
                    ),
                )
                .attr("class", "knot-appearance-row knot-preferences-error")
                .attr("role", "status"),
            ),
            None => Box::new(el("div", ())),
        };
        Box::new(
            el(
                "section",
                (
                    el(
                        "div",
                        (
                            button("Light", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| appearance.dark = false);
                            })
                            .attr("aria-pressed", (!appearance.dark).to_string()),
                            button("Dark", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| appearance.dark = true);
                            })
                            .attr("aria-pressed", appearance.dark.to_string()),
                            button("Highlight", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| {
                                    appearance.highlight = !appearance.highlight;
                                });
                            })
                            .attr("aria-pressed", appearance.highlight.to_string()),
                        ),
                    )
                    .attr("class", "knot-appearance-row"),
                    el(
                        "div",
                        (
                            button("Smaller", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| {
                                    appearance.font_size = appearance
                                        .font_size
                                        .saturating_sub(1)
                                        .max(Appearance::MIN_FONT_SIZE);
                                });
                            }),
                            span(format!("Text size: {}", appearance.font_size)),
                            button("Larger", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| {
                                    appearance.font_size = appearance
                                        .font_size
                                        .saturating_add(1)
                                        .min(Appearance::MAX_FONT_SIZE);
                                });
                            }),
                        ),
                    )
                    .attr("class", "knot-appearance-row"),
                    el(
                        "div",
                        (
                            button("Narrow", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| appearance.wide = false);
                            })
                            .attr("aria-pressed", (!appearance.wide).to_string()),
                            button("Wide", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| appearance.wide = true);
                            })
                            .attr("aria-pressed", appearance.wide.to_string()),
                            button("Compact", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| appearance.relaxed = false);
                            })
                            .attr("aria-pressed", (!appearance.relaxed).to_string()),
                            button("Relaxed", |state: &mut DesktopState, _| {
                                state.update_appearance(|appearance| appearance.relaxed = true);
                            })
                            .attr("aria-pressed", appearance.relaxed.to_string()),
                        ),
                    )
                    .attr("class", "knot-appearance-row"),
                    preferences_note,
                ),
            )
            .attr("class", "knot-appearance-panel")
            .attr("id", "knot-appearance-panel")
            .attr("role", "region")
            .attr("aria-label", "Appearance controls"),
        )
    } else {
        Box::new(el("div", ()))
    };
    let prompt: DesktopView = match state.pending.as_ref() {
        Some(PendingAction::Quit) => {
            let dirty = state.dirty_documents();
            let title = match dirty.as_slice() {
                [key] => format!(
                    "{} has unsaved changes.",
                    state
                        .docs
                        .doc(*key)
                        .map_or_else(String::new, |entry| entry.document.snapshot().display_label)
                ),
                keys => format!("{} documents have unsaved changes.", keys.len()),
            };
            let listed = dirty
                .iter()
                .filter_map(|key| {
                    let entry = state.docs.doc(*key)?;
                    Some((key.0, span(entry.document.snapshot().display_label)))
                })
                .collect::<Vec<_>>();
            Box::new(
                el(
                    "aside",
                    (
                        span(title).attr("id", "knot-confirm-message"),
                        el("div", Keyed::new(listed)).attr("class", "knot-confirm-list"),
                        button("Save all", |state: &mut DesktopState, _| {
                            state.confirm_save()
                        })
                        .attr("id", "knot-confirm-save"),
                        button("Discard all", |state: &mut DesktopState, _| {
                            state.confirm_discard()
                        }),
                        button("Review", |state: &mut DesktopState, _| {
                            state.review_pending()
                        })
                        .attr("id", "knot-confirm-review"),
                        button("Cancel", |state: &mut DesktopState, _| {
                            state.cancel_pending()
                        }),
                    ),
                )
                .attr("class", "knot-confirm")
                .attr("role", "dialog")
                .attr("aria-label", "Unsaved changes"),
            )
        },
        Some(action) => {
            let title = match action {
                PendingAction::CloseDocument(key) => format!(
                    "{} has unsaved changes.",
                    state
                        .docs
                        .doc(*key)
                        .map_or_else(String::new, |entry| entry.document.snapshot().display_label)
                ),
                _ => "Reload will replace unsaved changes with disk text.".to_owned(),
            };
            Box::new(
                el(
                    "aside",
                    (
                        span(title).attr("id", "knot-confirm-message"),
                        button("Save", |state: &mut DesktopState, _| state.confirm_save())
                            .attr("id", "knot-confirm-save"),
                        button("Discard", |state: &mut DesktopState, _| {
                            state.confirm_discard()
                        }),
                        button("Cancel", |state: &mut DesktopState, _| {
                            state.cancel_pending()
                        }),
                    ),
                )
                .attr("class", "knot-confirm")
                .attr("role", "dialog")
                .attr("aria-label", "Unsaved changes"),
            )
        },
        None => Box::new(el("div", ())),
    };
    let message: DesktopView = Box::new(
        span(state.message.clone().unwrap_or_else(|| "Ready.".to_owned()))
            .attr("class", "knot-workspace-message")
            .attr("aria-live", "polite"),
    );
    let catalog_status: DesktopView = if state.catalog.is_none() {
        Box::new(el("div", ()))
    } else if let Some(id) = &state.entry().catalog_id {
        Box::new(
            span(format!("Document ID: {id}"))
                .attr("class", "knot-catalog-status")
                .attr("aria-live", "polite"),
        )
    } else if let Some(error) = &state.entry().catalog_error {
        Box::new(
            el(
                "div",
                (
                    span(format!("Not catalogued: {error}")),
                    button("Retry catalog", |state: &mut DesktopState, _| {
                        state.retry_catalog_binding();
                    }),
                ),
            )
            .attr("class", "knot-catalog-status knot-catalog-error")
            .attr("role", "status"),
        )
    } else {
        Box::new(
            span("Not catalogued: unsaved document")
                .attr("class", "knot-catalog-status")
                .attr("aria-live", "polite"),
        )
    };
    let document_preview_button: DesktopView = if matches!(
        state.document().snapshot().format,
        knot_document::DocumentFormat::Djot | knot_document::DocumentFormat::Knot
    ) {
        Box::new(
            button(
                if state.document_preview_visible {
                    "Hide Preview"
                } else {
                    "Show Preview"
                },
                |state: &mut DesktopState, _| {
                    state.document_preview_visible = !state.document_preview_visible;
                },
            )
            .attr("aria-expanded", state.document_preview_visible.to_string())
            .attr("aria-controls", "knot-document-preview"),
        )
    } else {
        Box::new(el("div", ()))
    };
    let document_folding_button: DesktopView =
        if crate::document_folding::supported(state.document().snapshot().format) {
            Box::new(
                button(
                    if state.fold_visible {
                        "Hide folds"
                    } else {
                        "Show folds"
                    },
                    |state: &mut DesktopState, _| state.toggle_folding(),
                )
                .attr("aria-expanded", state.fold_visible.to_string())
                .attr("aria-controls", "knot-document-folding"),
            )
        } else {
            Box::new(el("div", ()))
        };
    Box::new(
        el(
            "main",
            (
                el(
                    "nav",
                    (
                        button("Site", |state: &mut DesktopState, _| {
                            state.scroll.visible = !state.scroll.visible
                        }),
                        button("New", |state: &mut DesktopState, _| state.new_document()),
                        button("Open", |state: &mut DesktopState, _| state.open_document()),
                        button("Save As", |state: &mut DesktopState, _| {
                            state.save_as();
                        }),
                        button("Reload", |state: &mut DesktopState, _| state.reload()),
                        button("Compare", |state: &mut DesktopState, _| {
                            state.compare_disk();
                        }),
                        document_preview_button,
                        document_folding_button,
                        button("Appearance", |state: &mut DesktopState, _| {
                            state.toggle_appearance();
                        })
                        .attr("aria-expanded", state.appearance_open.to_string())
                        .attr("aria-controls", "knot-appearance-panel"),
                        button(
                            if state.outline_visible {
                                "Hide Outline"
                            } else {
                                "Show Outline"
                            },
                            |state: &mut DesktopState, _| state.toggle_outline(),
                        ),
                        button(
                            if state.readings_visible {
                                "Hide Readings"
                            } else {
                                "Readings"
                            },
                            |state: &mut DesktopState, _| state.toggle_readings(),
                        )
                        .attr("aria-expanded", state.readings_visible.to_string())
                        .attr("aria-controls", "knot-readings"),
                        el(
                            "label",
                            (
                                "Path",
                                lens(
                                    |input: &mut TextInput| text_field_typed(input),
                                    |state: &mut DesktopState| &mut state.path,
                                ),
                            ),
                        )
                        .attr("id", "knot-path-field")
                        .attr("class", "knot-path-field"),
                    ),
                )
                .attr("class", "knot-workspace-toolbar"),
                message,
                crate::scroll_site::site_panel(state),
                catalog_status,
                review_panel,
                if state.document().snapshot().format.native_source()
                    && state.retention_targets.is_empty()
                {
                    Box::new(el("div", ())) as DesktopView
                } else {
                    retention_panel
                },
                appearance_panel,
                document_frame(state),
                prompt,
            ),
        )
        .attr(
            "class",
            format!(
                "{}{}{}{}",
                state.appearance.root_class(),
                if state.document().snapshot().format.native_source() {
                    " knot-native-site-mode"
                } else {
                    ""
                },
                if state.document_preview_visible
                    && matches!(
                        state.document().snapshot().format,
                        knot_document::DocumentFormat::Djot | knot_document::DocumentFormat::Knot
                    )
                {
                    " knot-document-preview-mode"
                } else {
                    ""
                },
                if state.fold_visible
                    && crate::document_folding::supported(state.document().snapshot().format)
                {
                    " knot-document-folding-mode"
                } else {
                    ""
                }
            ),
        ),
    )
}

/// One document tile: its editor and the panels read from it, the writing
/// area the window had before tabs. The panels read the focused entry, which
/// is the document a single stack shows.
fn document_tile(state: &DesktopState, key: DocKey) -> DesktopView {
    let outline_panel: DesktopView = if !state.outline_visible {
        Box::new(el("div", ()))
    } else if state.document().snapshot().format.native_source() {
        Box::new(span("A source outline is not available for this native protocol format yet. Use its preview when available.").attr("class", "knot-outline"))
    } else if let Some(snapshot) = &state.entry().outline_snapshot {
        let rows = snapshot
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let label = item.label.clone();
                let level = item.level;
                let key = item.start;
                let accessible_label = format!("Heading level {level}: {label}");
                (
                    key,
                    button(label, move |state: &mut DesktopState, _| {
                        state.select_outline_item(index);
                    })
                    .attr("class", "knot-outline-row")
                    .attr("data-outline-index", index.to_string())
                    .attr("data-outline-level", level.to_string())
                    .attr("aria-label", accessible_label),
                )
            })
            .collect::<Vec<_>>();
        let rows_view = if rows.is_empty() {
            Box::new(span("No headings in this document.")) as DesktopView
        } else {
            Box::new(el("div", Keyed::new(rows)).attr("class", "knot-outline-rows")) as DesktopView
        };
        let error = state.entry().outline_error.as_ref().map(|error| {
            span(format!("Outline error: {error}")).attr("class", "knot-outline-error")
        });
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Outline"),
                            button("Hide Outline", |state: &mut DesktopState, _| {
                                state.toggle_outline();
                            }),
                        ),
                    )
                    .attr("class", "knot-outline-header"),
                    error,
                    rows_view,
                ),
            )
            .attr("class", "knot-outline")
            .attr("role", "region")
            .attr("aria-label", "Document outline"),
        )
    } else {
        Box::new(
            el(
                "section",
                (
                    el(
                        "header",
                        (
                            span("Outline"),
                            button("Hide Outline", |state: &mut DesktopState, _| {
                                state.toggle_outline();
                            }),
                        ),
                    )
                    .attr("class", "knot-outline-header"),
                    span("Outline is updating; try again shortly."),
                ),
            )
            .attr("class", "knot-outline")
            .attr("role", "region")
            .attr("aria-label", "Document outline"),
        )
    };
    let comparison_panel: DesktopView = if let Some(error) = &state.entry().comparison_error {
        Box::new(
            el(
                "section",
                (
                    span("Comparison unavailable"),
                    span(error.clone()).attr("class", "knot-comparison-error"),
                    span("Disk text could not be read; refresh to try again."),
                    button("Refresh comparison", |state: &mut DesktopState, _| {
                        state.compare_disk();
                    }),
                    button("Hide comparison", |state: &mut DesktopState, _| {
                        state.hide_comparison();
                    }),
                ),
            )
            .attr("class", "knot-comparison knot-comparison-error-panel")
            .attr("role", "region")
            .attr("aria-label", "Disk comparison error"),
        )
    } else if let Some(comparison) = &state.entry().comparison {
        let snapshot = state.document().snapshot();
        let stale = comparison.buffer_text != snapshot.text
            || comparison.address != snapshot.source.address;
        let status = if stale {
            "Snapshot: stale because the source or address changed since comparison"
        } else {
            "Buffer snapshot matches current source"
        };
        let disk_status = if comparison.disk_changed_since_baseline {
            "Disk changed since the saved baseline at comparison: yes"
        } else {
            "Disk changed since the saved baseline at comparison: no"
        };
        Box::new(
            el(
                "section",
                (
                    el("header", (span("Disk comparison"), span(status)))
                        .attr("class", "knot-comparison-header"),
                    span(format!("Compared address: {}", comparison.address)),
                    span(disk_status),
                    span("Disk text was read when compared; refresh to read it again."),
                    button("Refresh comparison", |state: &mut DesktopState, _| {
                        state.compare_disk();
                    }),
                    button("Hide comparison", |state: &mut DesktopState, _| {
                        state.hide_comparison();
                    }),
                    el(
                        "div",
                        (
                            el(
                                "section",
                                (
                                    span("Buffer at comparison"),
                                    el("pre", comparison.buffer_text.clone()),
                                ),
                            )
                            .attr("class", "knot-comparison-version")
                            .attr("aria-label", "Buffer source at comparison"),
                            el(
                                "section",
                                (
                                    span("Disk at comparison"),
                                    el("pre", comparison.disk_text.clone()),
                                ),
                            )
                            .attr("class", "knot-comparison-version")
                            .attr("aria-label", "Disk source at comparison"),
                        ),
                    )
                    .attr("class", "knot-comparison-versions"),
                ),
            )
            .attr("class", "knot-comparison")
            .attr("role", "region")
            .attr("aria-label", "Disk comparison"),
        )
    } else {
        Box::new(el("div", ()))
    };
    let highlight = state.appearance.highlight;
    let document: DesktopView = Box::new(lens(
        move |state: &mut KnotDocumentSurfaceState| {
            knot_document_view_with_highlighting(state, highlight)
        },
        move |state: &mut DesktopState| state.surface_mut_for(key),
    ));
    let source_wrapper: DesktopView = if state.fold_visible
        && crate::document_folding::supported(state.document().snapshot().format)
    {
        Box::new(
            el("div", crate::document_folding::view(state))
                .attr("class", "knot-source-wrapper")
                .attr("style", state.appearance.writing_style()),
        )
    } else {
        Box::new(
            el("div", document)
                .attr("class", "knot-source-wrapper")
                .attr("style", state.appearance.writing_style()),
        )
    };
    Box::new(
        el(
            "div",
            (
                el(
                    "div",
                    (
                        source_wrapper,
                        outline_panel,
                        crate::readings::view(state),
                        crate::document_preview::view(state),
                        crate::scroll_site::preview(state),
                    ),
                )
                .attr("class", "knot-writing-area"),
                comparison_panel,
            ),
        )
        .attr("class", "knot-document-tile")
        .attr("data-knot-document", key.0.to_string()),
    )
}

/// The Workbench frame: every open document as a tab, its tile rendered on
/// demand through an identity lens so the tile can read the whole window's
/// state. A dirty document's tab carries the unsaved mark.
fn document_frame(state: &DesktopState) -> DesktopView {
    let current = state.docs.focused().and_then(|key| state.docs.tile_of(key));
    let model = WorkspaceModel {
        workspace: state.docs.workspace(),
        current,
        float_layer_visible: false,
    };
    let marks = |tile: workbench::TileId| match state.docs.role(tile) {
        Some(TileRole::Document(key)) => state
            .docs
            .doc(*key)
            .filter(|entry| entry.document.snapshot().dirty)
            .map(|_| TabMark::Modified),
        _ => None,
    };
    if state.docs.is_empty() {
        return Box::new(
            el(
                "div",
                span("No document is open. Use New or Open to start one.")
                    .attr("class", "knot-empty-frame"),
            )
            .attr("class", "knot-frame"),
        );
    }
    Box::new(
        el(
            "div",
            workspace_view_with_marks(
                &model,
                &marks,
                |state: &mut DesktopState, event| state.on_workspace_event(event),
                tile_fill,
            ),
        )
        .attr("class", "knot-frame"),
    )
}

fn tile_fill(tile: &workbench::Tile) -> Slot<DesktopState, ()> {
    let id = tile.id;
    Slot::View(Box::new(lens(
        move |state: &mut DesktopState| tile_view(state, id),
        |state: &mut DesktopState| state,
    )))
}

fn tile_view(state: &DesktopState, tile: workbench::TileId) -> DesktopView {
    match state.docs.role(tile) {
        Some(TileRole::Document(key)) => document_tile(state, *key),
        _ => Box::new(el("div", ())),
    }
}

fn ancestor_has_id<D: LayoutDom>(dom: &D, focused: D::NodeId, id: &str) -> bool {
    let namespace = Namespace::from("");
    let local = LocalName::from("id");
    let mut node = Some(focused);
    while let Some(current) = node {
        if dom.attribute(current, &namespace, &local) == Some(id) {
            return true;
        }
        node = dom.parent(current);
    }
    false
}

/// The document whose tile holds `node`, read off the tile's
/// `data-knot-document`.
fn document_key_of<D: LayoutDom>(dom: &D, node: D::NodeId) -> Option<DocKey> {
    let namespace = Namespace::from("");
    let local = LocalName::from("data-knot-document");
    let mut node = Some(node);
    while let Some(current) = node {
        if let Some(value) = dom.attribute(current, &namespace, &local) {
            return value.parse().ok().map(DocKey);
        }
        node = dom.parent(current);
    }
    None
}

fn ancestor_has_class<D: LayoutDom>(dom: &D, focused: D::NodeId, class: &str) -> bool {
    let mut node = Some(focused);
    while let Some(current) = node {
        if dom.has_class(current, class) {
            return true;
        }
        node = dom.parent(current);
    }
    false
}

pub fn focused_text(runner: &DesktopRunner) -> Option<FocusedTextSlot<DesktopState>> {
    let focused = runner.focus()?;
    let dom = runner.dom();
    let dom_ref = dom.borrow();
    let name = LayoutDom::element_name(&*dom_ref, focused)?;
    let is_text_control = name.local.as_ref() == "textarea" || name.local.as_ref() == "input";
    let is_document_textarea = name.local.as_ref() == "textarea";
    if !is_text_control {
        return None;
    }
    let folder = ancestor_has_id(&*dom_ref, focused, "knot-scroll-folder");
    let port = ancestor_has_id(&*dom_ref, focused, "knot-scroll-port");
    let submission_target = ancestor_has_id(&*dom_ref, focused, "knot-submission-target");
    let submission_mime = ancestor_has_id(&*dom_ref, focused, "knot-submission-mime");
    let submission_body = ancestor_has_id(&*dom_ref, focused, "knot-spartan-body");
    let submission_token = ancestor_has_id(&*dom_ref, focused, "knot-submission-token");
    let metadata =
        (0..6).find(|i| ancestor_has_id(&*dom_ref, focused, &format!("knot-scroll-meta-{i}")));
    let path = ancestor_has_id(&*dom_ref, focused, "knot-path-field");
    let document = document_key_of(&*dom_ref, focused);
    drop(dom_ref);
    if folder {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|s| &s.scroll.folder),
            get_mut: Box::new(|s| &mut s.scroll.folder),
        });
    }
    if port {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|s| &s.scroll.port),
            get_mut: Box::new(|s| &mut s.scroll.port),
        });
    }
    if submission_target {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|s| &s.scroll.submission_target),
            get_mut: Box::new(|s| &mut s.scroll.submission_target),
        });
    }
    if submission_mime {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|s| &s.scroll.submission_mime),
            get_mut: Box::new(|s| &mut s.scroll.submission_mime),
        });
    }
    if submission_body {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|s| &s.scroll.submission_body),
            get_mut: Box::new(|s| &mut s.scroll.submission_body),
        });
    }
    if submission_token {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|s| &s.scroll.submission_token),
            get_mut: Box::new(|s| &mut s.scroll.submission_token),
        });
    }
    if let Some(i) = metadata {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(move |s| &s.scroll.fields[i]),
            get_mut: Box::new(move |s| &mut s.scroll.fields[i]),
        });
    }
    if path {
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(|state| &state.path),
            get_mut: Box::new(|state| &mut state.path),
        });
    }
    if is_document_textarea {
        let key = document.or_else(|| runner.state().focused_key())?;
        if runner.state().surface_for(key).snapshot().write_posture
            == knot_document::KnotDocumentWritePostureV1::ReadOnly
        {
            return None;
        }
        return Some(FocusedTextSlot {
            node: focused,
            get: Box::new(move |state| state.surface_for(key).session().input()),
            get_mut: Box::new(move |state| {
                state
                    .surface_mut_for(key)
                    .session_mut()
                    .input_mut()
                    .expect("editable document focus")
            }),
        });
    }
    None
}

pub fn key_intercept(runner: &mut DesktopRunner, press: &KeyPress) -> bool {
    if !press.modifiers.is_command_chord() {
        return false;
    }
    let Key::Character(key) = &press.key else {
        return false;
    };
    let key = key.to_ascii_lowercase();
    match (key.as_str(), press.modifiers.shift) {
        ("n", false) => runner.update(DesktopState::new_document),
        ("o", false) => runner.update(DesktopState::open_document),
        ("s", true) => runner.update(|state| {
            state.save_as();
        }),
        ("s", false) => runner.update(DesktopState::save),
        _ => return false,
    }
    true
}

pub fn close_request(runner: &mut DesktopRunner, request: CloseRequest) -> CloseDisposition {
    let mut disposition = CloseDisposition::KeepVisible;
    runner.update(|state| disposition = state.close_request(request));
    disposition
}

/// Complete an outline activation after the click has rebuilt the retained
/// tree. The click target itself is a button, so focus must be returned to the
/// existing document textarea before the next key or IME event is routed.
pub fn after_dispatch(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
    if ctx.runner.state().retention_receiver.is_some() {
        ctx.runner.update(|state| state.drain_retention());
    }
    if ctx.runner.state().scroll.submission_busy() {
        ctx.runner.update(|state| {
            let current = state.document().snapshot();
            state
                .scroll
                .drain_submission(&current.text, &current.source.address);
        });
    }
    // An edit or page change since the last dispatch: drop the fold state of
    // headings the source no longer has before the next toggle can see it.
    if ctx.runner.state().micron_folds_need_sync() {
        ctx.runner.update(DesktopState::sync_micron_folds);
    }
    crate::scroll_site::scroll_to_micron_jump(ctx);
    let state = ctx.runner.state();
    let mut focus_requested = state.entry().focus_source_requested;
    let outline_needs_sync = if state.outline_visible {
        let current = state.document().snapshot();
        state
            .entry()
            .outline_snapshot
            .as_ref()
            .is_none_or(|snapshot| {
                snapshot.address != current.source.address || snapshot.source_text != current.text
            })
    } else {
        false
    };
    let fold_needs_sync = if state.fold_visible {
        let current = state.document().snapshot();
        state.entry().fold_snapshot.as_ref().is_none_or(|snapshot| {
            snapshot.address != current.source.address
                || snapshot.source_text != current.text
                || !crate::document_folding::snapshot_matches(state.document().session(), snapshot)
        })
    } else {
        false
    };
    let fold_must_close = state.fold_visible && (focus_requested || fold_needs_sync);
    if fold_needs_sync {
        // Source edits and document/address transitions invalidate the entire
        // transient reading. Return to the one ordinary editor surface so a
        // stale folded view can never become an editing affordance.
        focus_requested = true;
    }
    if !focus_requested && !outline_needs_sync && !fold_must_close {
        return;
    }
    ctx.runner.update(|state| {
        if focus_requested {
            state.entry_mut().focus_source_requested = false;
        }
        if outline_needs_sync {
            state.sync_outline_snapshot();
        }
        if fold_must_close {
            state.clear_folding();
        }
    });
    if !focus_requested {
        return;
    }
    let focused_key = ctx.runner.state().focused_key();
    let target = {
        let dom = ctx.runner.dom();
        let dom_ref = dom.borrow();
        ctx.runner.focusables().into_iter().find(|node| {
            LayoutDom::element_name(&*dom_ref, *node)
                .is_some_and(|name| name.local.as_ref() == "textarea")
                && ancestor_has_class(&*dom_ref, *node, "knot-document-body")
                && document_key_of(&*dom_ref, *node) == focused_key
        })
    };
    if let Some(target) = target {
        ctx.runner.set_focus(Some(target));
    }
}

/// Drain worker completions after a host wake and rebuild the retained view.
pub fn after_wake(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
    if ctx.runner.state().retention_receiver.is_some() {
        ctx.runner.update(|state| state.drain_retention());
    }
    if ctx.runner.state().scroll.submission_busy() {
        ctx.runner.update(|state| {
            let current = state.document().snapshot();
            state
                .scroll
                .drain_submission(&current.text, &current.source.address);
        });
    }
}

pub const DESKTOP_CSS: &str = concat!(
    ".knot-workspace { display:flex; flex-direction:column; gap:12px; padding:20px; }",
    ".knot-workspace-toolbar { display:flex; align-items:center; gap:8px; flex-wrap:wrap; }",
    ".knot-appearance-panel { display:flex; flex-direction:column; gap:8px; padding:10px; border:1px solid; }",
    ".knot-appearance-row { display:flex; align-items:center; flex-wrap:wrap; gap:6px; }",
    ".knot-source-wrapper { flex:1; min-width:0; width:100%; }",
    ".knot-path-field { display:flex; align-items:center; gap:6px; flex:1; }",
    ".knot-path-field input { min-width:280px; flex:1; }",
    ".knot-workspace-message { min-height:1.4em; }",
    ".knot-catalog-status { min-height:1.4em; overflow-wrap:anywhere; }",
    ".knot-catalog-error { color:crimson; display:flex; align-items:center; gap:8px; }",
    ".knot-review { max-height:420px; overflow:auto; padding:12px; border:1px solid; display:flex; flex-direction:column; gap:6px; }",
    ".knot-review-header { display:flex; flex-wrap:wrap; align-items:baseline; justify-content:space-between; gap:8px; }",
    ".knot-review-error { color:crimson; }",
    ".knot-review-source { max-height:240px; overflow:auto; white-space:pre-wrap; overflow-wrap:anywhere; user-select:text; }",
    ".knot-retention { padding:12px; border:1px solid; display:flex; flex-direction:column; gap:8px; }",
    ".knot-retention header { display:flex; flex-wrap:wrap; justify-content:space-between; gap:8px; }",
    ".knot-retention-targets { display:flex; flex-wrap:wrap; gap:6px; }",
    ".knot-retention-targets button { flex:1 1 180px; min-width:0; text-align:left; }",
    ".knot-retention-targets button[aria-pressed=true] { outline:2px solid currentColor; outline-offset:1px; }",
    ".knot-retention-selected { display:flex; flex-wrap:wrap; gap:4px 12px; padding:8px; border:1px solid; overflow-wrap:anywhere; }",
    ".knot-retention-selected span { min-width:0; }",
    ".knot-retention-error { color:crimson; }",
    ".knot-retention-receipt, .knot-retention-busy, .knot-retention-error, .knot-retention-targets button { overflow-wrap:anywhere; max-width:100%; }",
    ".knot-confirm { display:flex; align-items:center; gap:8px; padding:12px; border:1px solid; }",
    ".knot-confirm [id=knot-confirm-message] { margin-right:auto; }",
    ".knot-confirm-list { display:flex; flex-direction:column; gap:2px; }",
    ".knot-frame { position:relative; min-width:0; }",
    ".knot-frame .frisket-stack { height:auto; }",
    ".knot-frame .frisket-tabbar { flex:0 0 30px; height:30px; align-items:flex-end; gap:2px; padding:0 6px; border-bottom:1px solid; overflow:hidden; }",
    ".knot-frame .frisket-tab { flex:0 1 auto; max-width:240px; height:26px; margin-right:0; padding:0 6px 0 12px; gap:6px; font-size:13px; border:1px solid transparent; border-bottom:none; border-radius:6px 6px 0 0; }",
    ".knot-frame .frisket-tab.active { height:27px; margin-bottom:-1px; }",
    ".knot-frame .frisket-label { flex:0 1 auto; text-overflow:ellipsis; }",
    ".knot-frame .frisket-close { flex:0 0 18px; width:18px; height:auto; margin-left:0; padding:0; font-size:13px; visibility:hidden; }",
    ".knot-frame .frisket-tab.active .frisket-close, .knot-frame .frisket-tab:hover .frisket-close { visibility:visible; }",
    ".knot-frame .frisket-content { flex:0 0 auto; padding:12px; }",
    ".knot-empty-frame { display:block; padding:24px 0; }",
    ".knot-document-tile { display:flex; flex-direction:column; gap:12px; }",
    ".knot-writing-area { display:flex; align-items:flex-start; gap:12px; }",
    ".knot-document { flex:1; min-width:0; }",
    ".knot-document-body textarea { width:100%; min-height:360px; line-height:1.5; box-sizing:border-box; }",
    ".knot-outline { flex:0 0 280px; width:280px; box-sizing:border-box; max-height:480px; overflow:auto; padding:12px; border:1px solid; }",
    ".knot-outline-header { display:flex; align-items:center; justify-content:space-between; gap:8px; }",
    ".knot-outline-rows { display:flex; flex-direction:column; gap:2px; margin-top:8px; }",
    ".knot-outline-row { display:block; width:100%; text-align:left; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }",
    ".knot-outline-row[data-outline-level=2] { padding-left:16px; }",
    ".knot-outline-row[data-outline-level=3] { padding-left:32px; }",
    ".knot-outline-row[data-outline-level=4] { padding-left:48px; }",
    ".knot-outline-row[data-outline-level=5] { padding-left:64px; }",
    ".knot-outline-row[data-outline-level=6] { padding-left:80px; }",
    ".knot-outline-error { color:crimson; }",
    ".knot-comparison { max-height:360px; overflow:auto; padding:12px; border:1px solid; }",
    ".knot-comparison-header { display:flex; flex-wrap:wrap; gap:8px; align-items:baseline; }",
    ".knot-comparison-versions { display:flex; flex-wrap:wrap; gap:12px; }",
    ".knot-comparison-version { flex:1 1 360px; min-width:0; }",
    ".knot-comparison-version pre { max-height:240px; overflow:auto; white-space:pre-wrap; overflow-wrap:anywhere; user-select:text; }",
    "@media (max-width:700px) { .knot-workspace { padding:12px; } .knot-path-field input { min-width:160px; } .knot-writing-area { flex-direction:column; align-items:stretch; } .knot-outline { flex-basis:auto; width:100%; max-height:240px; } }",
);

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::TextCommand;
    use cambium_genet_winit_host::{Harness, Init, Modifiers, inert_hooks};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use taproot::Selector;
    use tempfile::tempdir;

    fn harness(
        session: KnotDocumentSession,
    ) -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
        harness_with_catalog(session, None)
    }

    fn harness_with_catalog(
        session: KnotDocumentSession,
        catalog: Option<KnotFileCatalog>,
    ) -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
        let mut host = Harness::with_hooks(
            Init {
                state: DesktopState::with_catalog(session, WindowCommands::new(), None, catalog),
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            {
                let mut hooks = inert_hooks();
                hooks.after_dispatch = Box::new(after_dispatch);
                hooks.after_wake = Box::new(after_wake);
                hooks.close_request = Box::new(|ctx, request| close_request(ctx.runner, request));
                hooks.focused_text = Box::new(focused_text);
                hooks.key_intercept = Box::new(key_intercept);
                hooks
            },
        );
        let commands = host.commands();
        host.update(|state| state.window = commands.clone());
        host
    }

    fn input_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom
            .element_name(node)
            .is_some_and(|name| name.local.as_ref() == "input")
        {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| input_node(dom, child))
    }

    fn named_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        name: &str,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom
            .element_name(node)
            .is_some_and(|element| element.local.as_ref() == name)
        {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| named_node(dom, child, name))
    }

    fn attr_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        name: &str,
        expected: &str,
    ) -> Option<genet_scripted_dom::NodeId> {
        let namespace = Namespace::from("");
        let local = LocalName::from(name);
        if dom.attribute(node, &namespace, &local).as_deref() == Some(expected) {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| attr_node(dom, child, name, expected))
    }

    fn class_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        class: &str,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom.has_class(node, class) {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| class_node(dom, child, class))
    }

    #[test]
    fn appearance_controls_preserve_source_selection_and_clean_baseline() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# First\n\n## Second\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        assert!(host.click_on(&Selector::role("button").containing("Second")));
        let before = host.state().document().snapshot();
        assert!(!before.dirty);
        assert_ne!(before.selection.anchor.byte, before.selection.focus.byte);
        assert!(host.click_on(&Selector::role("button").containing("Appearance")));
        assert!(host.click_on(&Selector::role("button").containing("Dark")));
        assert!(host.click_on(&Selector::role("button").containing("Highlight")));
        assert!(host.click_on(&Selector::role("button").containing("Wide")));
        assert!(host.click_on(&Selector::role("button").containing("Relaxed")));
        let after = host.state().document().snapshot();
        assert_eq!(after.text, before.text);
        assert_eq!(after.selection, before.selection);
        assert_eq!(after.dirty, before.dirty);
        assert!(host.state().appearance.dark);
        assert!(!host.state().appearance.highlight);
        assert!(host.state().appearance.wide);
        assert!(host.state().appearance.relaxed);
    }

    #[test]
    fn ordinary_document_preview_is_hidden_by_default_and_toggle_preserves_source() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# Héading\n\nBody",
        ));
        host.layout_at(900.0, 640.0);
        let before = host.state().document().snapshot();
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .set_preedit("仮入力");
        });
        let dom = host.runner().dom();
        let dom = dom.borrow();
        assert!(class_node(&dom, dom.document(), "knot-document-preview").is_none());
        drop(dom);
        assert!(host.click_on(&Selector::role("button").containing("Show Preview")));
        let after = host.state().document().snapshot();
        assert_eq!(after.text, before.text);
        assert_eq!(after.selection, before.selection);
        assert_eq!(after.dirty, before.dirty);
        assert_eq!(
            host.state().document().session().input().preedit(),
            "仮入力"
        );
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let preview = class_node(&dom, dom.document(), "knot-document-preview").unwrap();
        let preview_text = text_content(&dom, preview);
        assert!(preview_text.contains("Source: scratch:untitled"));
        assert!(preview_text.contains("Preview diagnostics: none"));
        assert!(!preview_text.contains("仮入力"));
    }

    #[test]
    fn folded_source_is_hidden_by_default_and_keeps_source_immutable() {
        let source = "# α\n\n## 二\nbody\n\n- first\n- second\n";
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, source));
        host.layout_at(900.0, 640.0);
        let before = host.state().document().snapshot();
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            assert!(class_node(&dom, dom.document(), "knot-document-folding").is_none());
            assert!(!text_content(&dom, dom.document()).contains("Folded source"));
        }
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .set_preedit("仮入力");
        });
        assert!(host.click_on(&Selector::role("button").containing("Show folds")));
        let after_show = host.state().document().snapshot();
        assert_eq!(after_show, before);
        assert_eq!(
            host.state().document().session().input().preedit(),
            "仮入力"
        );
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let folding = class_node(&dom, dom.document(), "knot-folding").unwrap();
            let folded_text = text_content(&dom, folding);
            assert!(folded_text.contains("# α"));
            assert!(!folded_text.contains("仮入力"));
        }
        assert!(
            host.click_on(&Selector::role("button").with_attr("aria-label", "Collapse all folds"))
        );
        let after_collapse = host.state().document().snapshot();
        assert_eq!(after_collapse, before);
        assert!(
            !host.state().entry().collapsed_folds.is_empty(),
            "fold error: {:?}; snapshot: {:?}",
            host.state().entry().fold_error,
            host.state().entry().fold_snapshot
        );
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            assert!(count_class(&dom, dom.document(), "fold-marker") > 0);
            assert!(attr_node(&dom, dom.document(), "aria-label", "Folded content").is_some());
        }
        assert!(
            host.click_on(&Selector::role("button").with_attr("aria-label", "Expand all folds"))
        );
        assert!(host.state().entry().collapsed_folds.is_empty());
        assert_eq!(host.state().document().snapshot(), before);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let folding = class_node(&dom, dom.document(), "knot-folding").unwrap();
        assert!(text_content(&dom, folding).contains("body"));
        drop(dom);
        assert!(host.click_on(&Selector::role("button").containing("Edit source")));
        assert!(!host.state().fold_visible);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        assert!(class_node(&dom, dom.document(), "knot-document-body").is_some());
    }

    #[test]
    fn folded_source_state_resets_after_a_source_edit() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# First\n\n- one\n- two\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show folds")));
        assert!(
            host.click_on(&Selector::role("button").with_attr("aria-label", "Collapse all folds"))
        );
        assert!(
            !host.state().entry().collapsed_folds.is_empty(),
            "fold error: {:?}; snapshot: {:?}",
            host.state().entry().fold_error,
            host.state().entry().fold_snapshot
        );
        host.update(|state| {
            state
                .document_mut()
                .apply(knot_document::KnotDocumentIntentV1::Edit(
                    cambium::TextCommand::SelectAll,
                ))
                .unwrap();
            state
                .document_mut()
                .apply(knot_document::KnotDocumentIntentV1::Edit(
                    cambium::TextCommand::Insert("# Changed\n\nnew body\n".into()),
                ))
                .unwrap();
        });
        host.after_dispatch();
        assert!(host.state().entry().collapsed_folds.is_empty());
        assert!(!host.state().fold_visible);
        assert!(host.state().entry().fold_snapshot.is_none());
        assert_eq!(
            host.state().document().snapshot().text,
            "# Changed\n\nnew body\n"
        );
    }

    #[test]
    fn folded_source_returns_to_editing_for_an_outline_selection() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# First\n\n## Second\n\nbody\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        assert!(host.click_on(&Selector::role("button").containing("Show folds")));
        // Fold rows name their heading too, so match the outline row exactly.
        assert!(host.click_on(
            &Selector::role("button").with_attr("aria-label", "Heading level 2: Second")
        ));

        let snapshot = host.state().document().snapshot();
        assert_eq!(
            &snapshot.text[snapshot.selection.anchor.byte..snapshot.selection.focus.byte],
            "## Second\n"
        );
        assert!(!host.state().fold_visible);
        assert!(host.focus().is_some());
    }

    #[test]
    fn folded_row_action_rejects_a_stale_full_snapshot() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# First\n\n## Second\nbody\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show folds")));
        let stale = host.state().entry().fold_snapshot.clone().unwrap();
        host.update(|state| {
            state
                .document_mut()
                .apply(knot_document::KnotDocumentIntentV1::Edit(
                    cambium::TextCommand::SelectAll,
                ))
                .unwrap();
            state
                .document_mut()
                .apply(knot_document::KnotDocumentIntentV1::Edit(
                    cambium::TextCommand::Insert("# New\n\nchanged\n".into()),
                ))
                .unwrap();
        });
        host.update(|state| state.toggle_fold(stale, 0));
        assert!(host.state().entry().collapsed_folds.is_empty());
        assert!(
            host.state()
                .entry()
                .fold_error
                .as_deref()
                .is_some_and(|error| error.contains("stale"))
        );
    }

    #[test]
    fn ordinary_preview_heading_selects_source_and_returns_focus() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# Héading\n\nBody",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Preview")));
        assert!(host.click_on(
            &Selector::role("button").with_attr("aria-label", "Select source heading: Héading")
        ));
        let snapshot = host.state().document().snapshot();
        assert_eq!(
            &snapshot.text[snapshot.selection.anchor.byte..snapshot.selection.focus.byte],
            "# Héading\n"
        );
        assert!(host.focus().is_some());
    }

    #[test]
    fn ordinary_preview_renders_current_unicode_edit() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "# Start\n"));
        host.layout_at(900.0, 640.0);
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("\n本文")
        });
        assert!(host.click_on(&Selector::role("button").containing("Show Preview")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let preview = class_node(&dom, dom.document(), "knot-document-preview").unwrap();
        assert!(text_content(&dom, preview).contains("本文"));
    }

    #[test]
    fn ordinary_preview_tracks_new_open_and_reload() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("preview-transitions.djot");
        std::fs::write(&path, "# Opened preview\n").unwrap();
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# Initial preview\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Preview")));
        assert!(host.click_on(&Selector::role("button").containing("New")));
        assert!(host.state().document_preview_visible);
        host.update(|state| state.path = TextInput::new(path.to_string_lossy()));
        assert!(host.click_on(&Selector::role("button").containing("Open")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let preview = class_node(&dom, dom.document(), "knot-document-preview").unwrap();
        assert!(text_content(&dom, preview).contains("Opened preview"));
        drop(dom);
        std::fs::write(&path, "# Reloaded preview\n").unwrap();
        assert!(host.click_on(&Selector::role("button").containing("Reload")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let preview = class_node(&dom, dom.document(), "knot-document-preview").unwrap();
        assert!(text_content(&dom, preview).contains("Reloaded preview"));
        assert!(!text_content(&dom, preview).contains("Opened preview"));
    }

    #[test]
    fn ordinary_preview_is_absent_for_native_formats() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("native.gmi");
        std::fs::write(&path, "# Native\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| state.document_preview_visible = true);
        host.layout_at(900.0, 640.0);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        assert!(class_node(&dom, dom.document(), "knot-document-preview").is_none());
        let native_preview = class_node(&dom, dom.document(), "knot-scroll-preview").unwrap();
        assert!(text_content(&dom, native_preview).contains("Native"));
        assert!(!text_content(&dom, dom.document()).contains("Show folds"));
        assert!(host.state().entry().fold_snapshot.is_none());
    }

    #[test]
    fn appearance_settings_persist_through_new_open_and_reload() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("appearance.djot");
        std::fs::write(&path, "# Opened\n").unwrap();
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "draft"));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Appearance")));
        assert!(host.click_on(&Selector::role("button").containing("Dark")));
        assert!(host.click_on(&Selector::role("button").containing("Highlight")));
        assert!(host.click_on(&Selector::role("button").containing("Wide")));
        assert!(host.click_on(&Selector::role("button").containing("Compact")));
        assert!(host.click_on(&Selector::role("button").containing("Larger")));
        assert!(host.click_on(&Selector::role("button").containing("New")));
        assert_eq!(host.state().document().snapshot().text, "");
        assert_eq!(host.state().document().session().source_path(), None);
        host.update(|state| state.path = TextInput::new(path.to_string_lossy()));
        assert!(host.click_on(&Selector::role("button").containing("Open")));
        assert_eq!(host.state().document().snapshot().text, "# Opened\n");
        assert_eq!(
            host.state().document().session().source_path(),
            Some(path.canonicalize().unwrap().as_path())
        );
        std::fs::write(&path, "# Reloaded\n").unwrap();
        assert!(host.click_on(&Selector::role("button").containing("Reload")));
        assert_eq!(host.state().document().snapshot().text, "# Reloaded\n");
        let appearance = &host.state().appearance;
        assert!(appearance.dark);
        assert!(!appearance.highlight);
        assert_eq!(appearance.font_size, 17);
        assert!(appearance.wide);
        assert!(!appearance.relaxed);
    }

    #[test]
    fn highlighting_toggle_keeps_preedit_unicode_input_and_saved_bytes() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("ime.djot");
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Appearance")));
        assert!(host.click_on(&Selector::role("button").containing("Highlight")));
        host.update(|state| {
            let input = state.document_mut().session_mut().input_mut().unwrap();
            input.set_preedit("仮入力");
        });
        assert_eq!(
            host.state().document().session().input().preedit(),
            "仮入力"
        );
        assert!(host.click_on(&Selector::role("button").containing("Highlight")));
        assert_eq!(
            host.state().document().session().input().preedit(),
            "仮入力"
        );
        host.update(|state| state.path = TextInput::new(path.to_string_lossy()));
        assert!(host.click_on(&Selector::role("textbox").with_attr("aria-label", "Document text")));
        host.key_injected("確定");
        assert_eq!(host.state().document().snapshot().text, "確定");
        assert!(host.click_on(&Selector::role("button").containing("Save As")));
        assert_eq!(std::fs::read(&path).unwrap(), "確定".as_bytes());
    }

    fn text_content(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
    ) -> String {
        let own = dom.text(node).unwrap_or_default();
        let children = dom
            .dom_children(node)
            .map(|child| text_content(dom, child))
            .collect::<String>();
        format!("{own}{children}")
    }

    fn count_class(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        class: &str,
    ) -> usize {
        usize::from(dom.has_class(node, class))
            + dom
                .dom_children(node)
                .map(|child| count_class(dom, child, class))
                .sum::<usize>()
    }

    fn request_native_close(
        host: &mut Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
    ) {
        host.request_close(CloseRequest::Native);
        // KeepVisible requests a native redraw. Supply that frame explicitly
        // before resolving newly created prompt controls in the windowless host.
        host.layout_at(900.0, 640.0);
    }

    #[test]
    fn path_control_and_save_as_are_real_controls() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("saved.djot");
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "# Draft\n"));
        host.layout_at(1000.0, 720.0);
        let path_input = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            input_node(&dom, dom.document()).expect("path input")
        };
        let (x, y, width, height) = host.painted_rect(path_input).expect("path input layout");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.key_injected(&path.to_string_lossy());
        assert!(host.click_on(&Selector::role("button").containing("Save As")));
        assert!(path.exists());
    }

    #[test]
    fn dirty_native_close_stays_open_until_discard_and_clean_close_exits() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        host.layout_at(900.0, 640.0);
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("edit");
        });
        request_native_close(&mut host);
        assert!(!host.close_requested());
        assert!(host.click_on(&Selector::role("button").containing("Discard")));
        assert!(host.close_requested());
    }

    #[test]
    fn dirty_native_close_save_writes_and_accepts_queued_close() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("close-save.djot");
        std::fs::write(&path, "# Original\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("draft");
        });
        host.layout_at(900.0, 640.0);
        request_native_close(&mut host);
        assert!(!host.close_requested());
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(host.close_requested());
        assert!(std::fs::read_to_string(&path).unwrap().contains("draft"));
        assert!(!host.state().document().snapshot().dirty);
    }

    #[test]
    fn canceled_dirty_close_preserves_the_document() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "draft"));
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edit");
        });
        host.layout_at(900.0, 640.0);
        request_native_close(&mut host);
        assert!(host.click_on(&Selector::role("button").containing("Cancel")));
        assert!(!host.close_requested());
        assert!(
            host.state()
                .document()
                .snapshot()
                .text
                .contains("draft edit")
        );
        assert!(host.state().document().snapshot().dirty);
    }

    #[test]
    fn open_adds_a_tab_beside_the_dirty_document() {
        let temp = tempdir().unwrap();
        let replacement = temp.path().join("replacement.djot");
        std::fs::write(&replacement, "replacement").unwrap();
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "draft"));
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edit");
            state.path = TextInput::new(replacement.to_string_lossy().into_owned());
        });
        host.layout_at(900.0, 640.0);
        let draft = host.state().focused_key().unwrap();
        assert!(host.click_on(&Selector::role("button").containing("Open")));
        assert!(host.state().pending.is_none());
        assert_eq!(host.state().docs.len(), 2);
        assert_eq!(host.state().document().snapshot().text, "replacement");
        let kept = &host.state().docs.doc(draft).unwrap().document;
        assert_eq!(kept.snapshot().text, "draft edit");
        assert!(kept.snapshot().dirty);
    }

    type DesktopHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

    fn insert(host: &mut DesktopHarness, text: &str) {
        host.update(|state| {
            state
                .document_mut()
                .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                    text.to_owned(),
                )))
                .unwrap();
        });
    }

    fn open_through_path(host: &mut DesktopHarness, path: &Path) {
        host.update(|state| state.path = TextInput::new(path.to_string_lossy()));
        assert!(host.click_on(&Selector::role("button").containing("Open")));
    }

    fn close_tab(host: &mut DesktopHarness, label: &str) -> bool {
        host.click_on(&Selector::role("button").with_attr("aria-label", format!("Close {label}")))
    }

    fn confirm_text(host: &DesktopHarness) -> String {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        class_node(&dom, dom.document(), "knot-confirm")
            .map(|node| text_content(&dom, node))
            .unwrap_or_default()
    }

    fn document_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempdir().unwrap();
        let first = temp.path().join("first.djot");
        let second = temp.path().join("second.djot");
        std::fs::write(&first, "first\n").unwrap();
        std::fs::write(&second, "second\n").unwrap();
        (temp, first, second)
    }

    #[test]
    fn two_files_and_a_scratch_keep_their_state_across_activation() {
        let (_temp, first, second) = document_fixture();
        let mut host = harness(KnotDocumentSession::open(&first).unwrap());
        host.layout_at(900.0, 640.0);
        insert(&mut host, "one ");
        let first_key = host.state().focused_key().unwrap();
        let selection = host.state().document().snapshot().selection;
        open_through_path(&mut host, &second);
        assert!(host.click_on(&Selector::role("button").containing("New")));
        insert(&mut host, "draft");
        assert_eq!(host.state().docs.len(), 3);
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            assert_eq!(count_class(&dom, dom.document(), "frisket-tab"), 3);
            // The edited file and the draft carry the unsaved mark; the
            // untouched file does not.
            assert_eq!(count_class(&dom, dom.document(), "tab-mark"), 2);
            let marked = attr_node(&dom, dom.document(), "data-mark", "modified");
            assert!(marked.is_some());
        }

        assert!(host.click_on(&Selector::role("tab").containing("first.djot")));
        assert_eq!(host.state().focused_key(), Some(first_key));
        let snapshot = host.state().document().snapshot();
        assert_eq!(snapshot.text, "first\none ");
        assert_eq!(snapshot.selection, selection);
        assert!(snapshot.dirty);
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let tile = attr_node(
                &dom,
                dom.document(),
                "data-knot-document",
                &first_key.0.to_string(),
            );
            assert!(tile.is_some(), "the tile in view carries its document key");
        }
        host.update(|state| {
            state
                .document_mut()
                .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
                .unwrap();
        });
        assert_eq!(host.state().document().snapshot().text, "first\n");

        assert!(host.click_on(&Selector::role("tab").containing("second.djot")));
        assert_eq!(host.state().document().snapshot().text, "second\n");
        assert!(!host.state().document().snapshot().dirty);
        assert!(host.click_on(&Selector::role("tab").containing(SCRATCH_ADDRESS)));
        assert_eq!(host.state().document().snapshot().text, "draft");
        assert!(host.state().document().snapshot().dirty);
    }

    #[test]
    fn typing_after_switching_tabs_edits_the_shown_document() {
        let (_temp, first, second) = document_fixture();
        let mut host = harness(KnotDocumentSession::open(&first).unwrap());
        host.layout_at(900.0, 640.0);
        open_through_path(&mut host, &second);
        let second_key = host.state().focused_key().unwrap();
        assert!(host.click_on(&Selector::role("tab").containing("first.djot")));
        assert!(host.click_on(&Selector::role("tab").containing("second.djot")));
        let editor = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let tile = attr_node(
                &dom,
                dom.document(),
                "data-knot-document",
                &second_key.0.to_string(),
            )
            .expect("second tile");
            named_node(&dom, tile, "textarea").expect("second editor")
        };
        let (x, y, width, height) = host.painted_rect(editor).expect("editor layout");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.key_injected("typed ");
        let second_text = &host.state().docs.doc(second_key).unwrap().document;
        assert!(second_text.snapshot().text.contains("typed "));
        let first_text = host
            .state()
            .docs
            .docs()
            .find(|(key, _)| *key != second_key)
            .map(|(_, entry)| entry.document.snapshot().text)
            .unwrap();
        assert_eq!(first_text, "first\n");
    }

    #[test]
    fn a_duplicate_open_activates_the_existing_tab() {
        let (temp, first, second) = document_fixture();
        let mut host = harness(KnotDocumentSession::open(&first).unwrap());
        host.layout_at(900.0, 640.0);
        let first_key = host.state().focused_key().unwrap();
        open_through_path(&mut host, &second);
        open_through_path(&mut host, &temp.path().join(".").join("first.djot"));
        assert_eq!(host.state().docs.len(), 2);
        assert_eq!(host.state().focused_key(), Some(first_key));
        assert!(
            host.state()
                .message
                .as_deref()
                .is_some_and(|message| message.contains("first.djot is already open"))
        );
    }

    #[test]
    fn a_clean_tab_closes_at_once_and_a_dirty_one_asks() {
        let (_temp, first, second) = document_fixture();
        let mut host = harness(KnotDocumentSession::open(&first).unwrap());
        host.layout_at(900.0, 640.0);
        let first_key = host.state().focused_key().unwrap();
        open_through_path(&mut host, &second);
        assert!(close_tab(&mut host, "second.djot"));
        assert_eq!(host.state().docs.len(), 1);
        assert!(host.state().pending.is_none());
        assert_eq!(host.state().message.as_deref(), Some("Closed second.djot."));

        insert(&mut host, "unsaved ");
        assert!(close_tab(&mut host, "first.djot"));
        assert_eq!(
            host.state().pending,
            Some(PendingAction::CloseDocument(first_key))
        );
        assert_eq!(host.state().docs.len(), 1);
        assert!(confirm_text(&host).contains("first.djot has unsaved changes."));
        assert!(host.click_on(&Selector::role("button").containing("Cancel")));
        assert!(host.state().pending.is_none());
        assert_eq!(host.state().docs.len(), 1);
        assert!(host.state().document().snapshot().dirty);

        // A refused save keeps both the prompt and the tab.
        std::fs::write(&first, "external\n").unwrap();
        assert!(close_tab(&mut host, "first.djot"));
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert_eq!(
            host.state().pending,
            Some(PendingAction::CloseDocument(first_key))
        );
        assert_eq!(host.state().docs.len(), 1);
        assert!(
            host.state()
                .message
                .as_deref()
                .is_some_and(|message| message.contains("changed on disk"))
        );

        assert!(host.click_on(&Selector::role("button").containing("Discard")));
        assert!(host.state().docs.is_empty());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "external\n");
    }

    #[test]
    fn saving_a_dirty_tab_on_close_writes_it_and_closes_it() {
        let (_temp, first, second) = document_fixture();
        let mut host = harness(KnotDocumentSession::open(&first).unwrap());
        host.layout_at(900.0, 640.0);
        open_through_path(&mut host, &second);
        insert(&mut host, "kept ");
        assert!(close_tab(&mut host, "second.djot"));
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(host.state().pending.is_none());
        assert_eq!(host.state().docs.len(), 1);
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second\nkept ");
        assert_eq!(
            host.state().document().snapshot().display_label,
            "first.djot"
        );
    }

    #[test]
    fn closing_the_last_tab_leaves_an_empty_frame_that_new_refills() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        host.layout_at(900.0, 640.0);
        assert!(close_tab(&mut host, SCRATCH_ADDRESS));
        assert!(host.state().docs.is_empty());
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let frame = class_node(&dom, dom.document(), "knot-frame").expect("frame");
            assert!(text_content(&dom, frame).contains("No document is open."));
            assert_eq!(count_class(&dom, dom.document(), "frisket-tab"), 0);
        }
        assert!(host.click_on(&Selector::role("button").containing("Save")));
        assert_eq!(
            host.state().message.as_deref(),
            Some("No document is open.")
        );
        assert!(host.click_on(&Selector::role("button").containing("New")));
        assert_eq!(host.state().docs.len(), 1);
    }

    #[test]
    fn quitting_asks_once_for_every_dirty_document() {
        let (_temp, first, second) = document_fixture();
        let mut host = harness(KnotDocumentSession::open(&first).unwrap());
        host.layout_at(900.0, 640.0);
        insert(&mut host, "a ");
        open_through_path(&mut host, &second);
        insert(&mut host, "b ");
        assert!(host.click_on(&Selector::role("button").containing("New")));
        insert(&mut host, "scratch");
        let scratch = host.state().focused_key().unwrap();

        request_native_close(&mut host);
        assert_eq!(host.state().pending, Some(PendingAction::Quit));
        let prompt = confirm_text(&host);
        assert!(prompt.contains("3 documents have unsaved changes."));
        for label in ["first.djot", "second.djot", SCRATCH_ADDRESS] {
            assert!(prompt.contains(label), "{label} is listed");
        }

        // Cancel changes nothing.
        assert!(host.click_on(&Selector::role("button").containing("Cancel")));
        assert!(host.state().pending.is_none());
        assert_eq!(host.state().dirty_documents().len(), 3);

        // Save all writes the files; the scratch stays listed and the window stays.
        request_native_close(&mut host);
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(!host.close_requested());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first\na ");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second\nb ");
        assert_eq!(host.state().dirty_documents(), vec![scratch]);
        assert_eq!(host.state().pending, Some(PendingAction::Quit));
        assert!(confirm_text(&host).contains(&format!("{SCRATCH_ADDRESS} has unsaved changes.")));

        // Review shows the first listed document and dismisses the prompt.
        assert!(host.click_on(&Selector::role("tab").containing("first.djot")));
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-review")));
        assert!(host.state().pending.is_none());
        assert_eq!(host.state().focused_key(), Some(scratch));

        // Discard all closes the window over the remaining scratch.
        request_native_close(&mut host);
        assert!(host.click_on(&Selector::role("button").containing("Discard all")));
        assert!(host.close_requested());
    }

    #[test]
    fn outline_rows_select_unicode_source_and_return_focus_for_repeat_clicks() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# Café\n\n## Second heading\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        assert!(host.click_on(&Selector::role("button").containing("Café")));
        let snapshot = host.state().document().session().outline_snapshot();
        let item = snapshot
            .items
            .iter()
            .find(|item| item.label == "Café")
            .expect("unicode heading");
        assert_eq!(
            host.state().document().snapshot().selection.anchor.byte,
            item.start
        );
        assert_eq!(
            host.state().document().snapshot().selection.focus.byte,
            item.end
        );
        let textarea = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            named_node(&dom, dom.document(), "textarea").expect("document textarea")
        };
        assert_eq!(host.focus(), Some(textarea));

        let path_input = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            input_node(&dom, dom.document()).expect("path input")
        };
        let (x, y, width, height) = host.painted_rect(path_input).expect("path layout");
        host.click_at(x + width / 2.0, y + height / 2.0);
        assert_eq!(host.focus(), Some(path_input));
        assert!(host.click_on(&Selector::role("button").containing("Second heading")));
        assert_eq!(host.focus(), Some(textarea));
        host.key_injected("!");
        assert_eq!(host.state().path.text(), "");
        assert_eq!(host.state().document().snapshot().text, "# Café\n\n!");
    }

    #[test]
    fn outline_tracks_direct_committed_edits_and_excludes_ime_preedit() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, "# First\n"));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("\n## Direct edit\n");
        });
        host.after_dispatch();
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let outline = class_node(&dom, dom.document(), "knot-outline").expect("outline panel");
        assert!(text_content(&dom, outline).contains("Direct edit"));
        drop(dom);

        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .set_preedit("仮入力");
        });
        host.after_dispatch();
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let outline = class_node(&dom, dom.document(), "knot-outline").expect("outline panel");
        assert!(!text_content(&dom, outline).contains("仮入力"));
    }

    #[test]
    fn stale_outline_activation_reports_refusal_without_changing_selection() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "# First\n\n## Second\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("changed");
        });
        let before = host.state().document().snapshot();
        // Deliberately omit the normal after-dispatch refresh: this click uses
        // the old retained row and exercises the exact-source guard.
        assert!(host.click_on(&Selector::role("button").containing("First")));
        assert_eq!(host.state().document().snapshot(), before);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let workspace = class_node(&dom, dom.document(), "knot-workspace").expect("workspace");
        assert!(text_content(&dom, workspace).contains("Outline selection failed:"));
    }

    #[test]
    fn empty_outline_has_an_accessible_empty_state() {
        let mut host = harness(KnotDocumentSession::scratch(
            SCRATCH_ADDRESS,
            "plain text\n",
        ));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let outline = class_node(&dom, dom.document(), "knot-outline").expect("outline panel");
        assert!(text_content(&dom, outline).contains("No headings in this document."));
    }

    #[test]
    #[ignore = "diagnostic timing receipt; run with --ignored --nocapture"]
    fn outline_long_document_probe() {
        use std::time::Instant;

        let mut source = String::new();
        for index in 0..200 {
            source.push_str(&format!("# Heading {index}\n\n"));
            source
                .push_str(&"A long writing paragraph keeps the layout representative. ".repeat(10));
            source.push_str("\n\n");
        }
        let source_bytes = source.len();
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, source));
        host.layout_at(1100.0, 700.0);
        let show_started = Instant::now();
        assert!(host.click_on(&Selector::role("button").containing("Show Outline")));
        host.layout_at(1100.0, 700.0);
        let show_layout_us = show_started.elapsed().as_micros();
        let row_count = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            count_class(&dom, dom.document(), "knot-outline-row")
        };
        let edit_started = Instant::now();
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("\n## Appended heading\n");
        });
        host.after_dispatch();
        host.layout_at(1100.0, 700.0);
        let edit_layout_us = edit_started.elapsed().as_micros();
        println!(
            "outline_probe source_bytes={source_bytes} heading_rows={row_count} show_layout_us={show_layout_us} edit_layout_us={edit_layout_us}"
        );
        assert_eq!(row_count, 200);
    }

    #[test]
    fn command_shortcuts_reach_save_and_save_as() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shortcut.djot");
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        host.layout_at(900.0, 640.0);
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("edit");
            state.path = TextInput::new(path.to_string_lossy().into_owned());
        });
        host.set_modifiers(Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        });
        host.key_char("s");
        host.set_modifiers(Modifiers::NONE);
        assert!(path.exists());
        assert!(std::fs::read_to_string(&path).unwrap().contains("edit"));
        assert!(!host.state().document().snapshot().dirty);
        assert!(host.state().path.text().ends_with("shortcut.djot"));

        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" again");
        });
        host.set_modifiers(Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        });
        host.key_char("s");
        host.set_modifiers(Modifiers::NONE);
        assert!(!host.state().document().snapshot().dirty);
        assert!(std::fs::read_to_string(&path).unwrap().contains("again"));
    }

    #[test]
    fn failed_save_keeps_native_close_prompt_open() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("changed.djot");
        std::fs::write(&path, "# Original\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("draft");
        });
        std::fs::write(&path, "# External\n").unwrap();
        request_native_close(&mut host);
        assert!(!host.close_requested());
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(!host.close_requested());
        assert!(
            host.state()
                .message
                .as_deref()
                .unwrap()
                .contains("changed on disk")
        );
    }

    #[test]
    fn compare_captures_disk_and_buffer_without_writing_or_mutating_source() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("compare.djot");
        std::fs::write(&path, "buffer\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edited");
        });
        let buffer_before = host.state().document().snapshot();
        std::fs::write(&path, "disk\n").unwrap();
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        let comparison = host
            .state()
            .entry()
            .comparison
            .as_ref()
            .expect("comparison snapshot");
        assert_eq!(comparison.buffer_text, buffer_before.text);
        assert_eq!(comparison.disk_text, "disk\n");
        assert!(comparison.disk_changed_since_baseline);
        assert_eq!(host.state().document().snapshot().text, buffer_before.text);
        assert_eq!(
            host.state().document().snapshot().selection,
            buffer_before.selection
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "disk\n");

        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" later");
        });
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let panel = class_node(&dom, dom.document(), "knot-comparison").expect("comparison panel");
        assert!(text_content(&dom, panel).contains("Snapshot: stale"));
        drop(dom);
        assert!(host.click_on(&Selector::role("button").containing("Refresh comparison")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let panel = class_node(&dom, dom.document(), "knot-comparison").expect("comparison panel");
        assert!(text_content(&dom, panel).contains("Buffer snapshot matches current source"));
    }

    #[test]
    fn comparison_refreshes_explicitly_and_stays_with_its_document() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("refresh.djot");
        std::fs::write(&path, "first\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        std::fs::write(&path, "second\n").unwrap();
        assert_eq!(
            host.state().entry().comparison.as_ref().unwrap().disk_text,
            "first\n"
        );
        assert!(host.click_on(&Selector::role("button").containing("Refresh comparison")));
        assert_eq!(
            host.state().entry().comparison.as_ref().unwrap().disk_text,
            "second\n"
        );
        let compared = host.state().focused_key().unwrap();
        assert!(host.click_on(&Selector::role("button").containing("New")));
        assert!(host.state().entry().comparison.is_none());
        assert!(host.state().entry().comparison_error.is_none());
        assert_eq!(
            host.state()
                .docs
                .doc(compared)
                .and_then(|entry| entry.comparison.as_ref())
                .map(|comparison| comparison.disk_text.as_str()),
            Some("second\n")
        );
    }

    #[test]
    fn missing_disk_replaces_the_old_comparison_with_an_error() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("missing.djot");
        std::fs::write(&path, "source\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        std::fs::remove_file(&path).unwrap();
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        assert!(host.state().entry().comparison.is_none());
        assert!(
            host.state()
                .entry()
                .comparison_error
                .as_deref()
                .is_some_and(|error| error.contains("read"))
        );
    }

    #[test]
    fn comparison_renders_markup_as_plain_source_text() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("markup.djot");
        std::fs::write(&path, "<script>alert(1)</script>\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let pre = named_node(&dom, dom.document(), "pre").expect("comparison pre");
        assert!(text_content(&dom, pre).contains("<script>alert(1)</script>"));
        assert!(named_node(&dom, pre, "script").is_none());
    }

    #[test]
    fn compare_leaves_a_pending_dirty_close_untouched() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("pending-compare.djot");
        std::fs::write(&path, "source\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edited");
        });
        host.layout_at(900.0, 640.0);
        request_native_close(&mut host);
        let before = host.state().document().snapshot();
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        assert_eq!(host.state().pending, Some(PendingAction::Quit));
        assert_eq!(host.state().document().snapshot().text, before.text);
        assert_eq!(
            host.state().document().snapshot().selection,
            before.selection
        );
    }

    #[test]
    fn inner_save_keeps_comparison_disk_text_historical() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("historical.djot");
        std::fs::write(&path, "original\n").unwrap();
        let mut host = harness(KnotDocumentSession::open(&path).unwrap());
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str(" edited");
        });
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Compare")));
        assert!(host.click_on(&Selector::class("knot-document-save")));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n edited");
        assert!(!host.state().document().snapshot().dirty);
        assert_eq!(
            host.state().entry().comparison.as_ref().unwrap().disk_text,
            "original\n"
        );
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let panel = class_node(&dom, dom.document(), "knot-comparison").expect("comparison panel");
        let rendered = text_content(&dom, panel);
        assert!(rendered.contains("Disk text was read when compared; refresh to read it again."));
        assert!(rendered.contains("Buffer snapshot matches current source"));
    }

    #[test]
    fn read_only_save_as_refusal_is_visible_and_does_not_write() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("read-only.djot");
        let mut host = harness(KnotDocumentSession::read_only(SCRATCH_ADDRESS, "read only"));
        host.update(|state| state.path = TextInput::new(path.to_string_lossy().into_owned()));
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Save As")));
        assert!(!path.exists());
        assert_eq!(
            host.state().document().snapshot().write_posture,
            knot_document::KnotDocumentWritePostureV1::ReadOnly
        );
        assert!(host.state().message.as_deref().unwrap().contains("refused"));
    }

    #[test]
    fn catalog_status_uses_source_path_and_save_as_gets_a_new_id() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("documents");
        let catalog_root = temp.path().join("catalog");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&catalog_root).unwrap();
        let original = root.join("original.djot");
        let saved_as = root.join("saved-as.djot");
        std::fs::write(&original, "source\n").unwrap();
        let catalog = KnotFileCatalog::open(&root, catalog_root.join("catalog.redb")).unwrap();
        let mut host =
            harness_with_catalog(KnotDocumentSession::open(&original).unwrap(), Some(catalog));
        host.layout_at(900.0, 640.0);
        let original_id = host
            .state()
            .entry()
            .catalog_id
            .clone()
            .expect("initial catalog id");

        host.update(|state| state.path = TextInput::new("path field edit"));
        host.after_dispatch();
        assert_eq!(
            host.state().entry().catalog_id.as_deref(),
            Some(original_id.as_str())
        );

        host.update(|state| state.path = TextInput::new(saved_as.to_string_lossy()));
        assert!(host.click_on(&Selector::role("button").containing("Save As")));
        let saved_id = host
            .state()
            .entry()
            .catalog_id
            .clone()
            .expect("Save As catalog id");
        assert_ne!(saved_id, original_id);
        assert_eq!(
            host.state().document().session().source_path(),
            Some(std::fs::canonicalize(&saved_as).unwrap().as_path())
        );
        assert!(saved_as.exists());
        assert!(
            host.state().entry().catalog_error.is_none(),
            "successful in-root Save As must not report a catalog error"
        );

        let dom = host.runner().dom();
        let dom = dom.borrow();
        let workspace = class_node(&dom, dom.document(), "knot-workspace").expect("workspace");
        assert!(text_content(&dom, workspace).contains(&format!("Document ID: {saved_id}")));
        drop(dom);
        assert!(host.click_on(&Selector::role("button").containing("New")));
        assert!(host.state().entry().catalog_id.is_none());
        assert!(host.state().entry().catalog_error.is_none());
    }

    #[test]
    fn catalog_failure_does_not_block_outside_save_or_dirty_close() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("documents");
        let catalog_root = temp.path().join("catalog");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&catalog_root).unwrap();
        let outside = temp.path().join("outside.djot");
        std::fs::write(&outside, "").unwrap();
        let catalog = KnotFileCatalog::open(&root, catalog_root.join("catalog.redb")).unwrap();
        let mut host =
            harness_with_catalog(KnotDocumentSession::open(&outside).unwrap(), Some(catalog));
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("draft");
        });
        assert!(host.state().document().snapshot().dirty);
        host.layout_at(900.0, 640.0);
        request_native_close(&mut host);
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(host.close_requested());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "draft");
        assert!(host.state().entry().catalog_id.is_none());
        assert!(
            host.state()
                .entry()
                .catalog_error
                .as_deref()
                .is_some_and(|error| error.contains("outside catalog root"))
        );
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let workspace = class_node(&dom, dom.document(), "knot-workspace").expect("workspace");
        assert!(text_content(&dom, workspace).contains("Not catalogued:"));
    }

    #[test]
    fn retry_catalog_recovers_an_in_root_file_without_mutating_document_state() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("documents");
        let catalog_root = temp.path().join("catalog");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&catalog_root).unwrap();
        let path = root.join("recover.djot");
        std::fs::write(&path, "recover me\n").unwrap();
        let session = KnotDocumentSession::open(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let catalog = KnotFileCatalog::open(&root, catalog_root.join("catalog.redb")).unwrap();
        let mut host = harness_with_catalog(session, Some(catalog));
        assert!(host.state().entry().catalog_id.is_none());
        assert!(host.state().entry().catalog_error.is_some());
        let before = host.state().document().snapshot();

        host.update(|state| state.path = TextInput::new("temporary path edit"));
        host.after_dispatch();
        assert!(host.state().entry().catalog_id.is_none());
        assert_eq!(host.state().document().snapshot(), before);

        std::fs::write(&path, "recover me\n").unwrap();
        host.layout_at(900.0, 640.0);
        assert!(host.click_on(&Selector::role("button").containing("Retry catalog")));
        assert!(host.state().entry().catalog_id.is_some());
        assert!(host.state().entry().catalog_error.is_none());
        assert_eq!(host.state().document().snapshot(), before);
    }

    #[test]
    fn opening_a_site_at_startup_uses_its_format_and_default_port() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("gemini");
        let site = knot_site::Site::create_for(&root, knot_site::SiteFormat::Gemini).unwrap();
        let index = site.page_path(site.config.format.index_file()).unwrap();
        let state = DesktopState::with_catalog(
            KnotDocumentSession::open(index).unwrap(),
            WindowCommands::new(),
            Some(root),
            None,
        );

        assert_eq!(state.scroll.format, knot_site::SiteFormat::Gemini);
        assert_eq!(state.scroll.port.text(), "1965");
        assert_eq!(
            state.scroll.site.as_ref().unwrap().config.format,
            knot_site::SiteFormat::Gemini
        );
    }

    type ReviewHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

    fn review_fixture(source: &str) -> (tempfile::TempDir, PathBuf, ReviewHarness) {
        let temp = tempdir().unwrap();
        let root = temp.path().join("notes");
        std::fs::create_dir(&root).unwrap();
        let path = root.join("review.djot");
        std::fs::write(&path, source).unwrap();
        let catalog = KnotFileCatalog::open(&root, temp.path().join("catalog.redb")).unwrap();
        let mut host =
            harness_with_catalog(KnotDocumentSession::open(&path).unwrap(), Some(catalog));
        host.layout_at(1000.0, 800.0);
        (temp, path, host)
    }

    fn review_text(host: &ReviewHarness) -> String {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let panel = class_node(&dom, dom.document(), "knot-review").expect("review panel");
        text_content(&dom, panel)
    }

    #[test]
    fn saved_revision_review_excludes_dirty_edits_and_survives_inner_save_until_refresh() {
        let source = "<script>saved α</script>\n";
        let (_temp, path, mut host) = review_fixture(source);
        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("UNSAVED");
            state.path = TextInput::new("unrelated future target");
        });
        let before = host.state().document().snapshot();
        assert!(before.dirty);
        assert!(host.click_on(&Selector::role("button").containing("Review saved revision")));
        assert_eq!(
            host.state().entry().prepared_capture.as_ref().unwrap().body,
            source.as_bytes()
        );
        assert_eq!(
            host.state().entry().prepared_capture_source_path.as_deref(),
            Some(std::fs::canonicalize(&path).unwrap().as_path())
        );
        assert_eq!(host.state().document().snapshot(), before);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        let rendered = review_text(&host);
        assert!(rendered.contains(source));
        assert!(!rendered.contains("UNSAVED"));
        assert!(rendered.contains("Unsaved changes are excluded"));
        assert!(rendered.contains("Reviewing does not store or share bytes."));
        host.update(|state| {
            state
                .document_mut()
                .apply(KnotDocumentIntentV1::Save)
                .unwrap();
        });
        host.after_dispatch();
        assert_eq!(
            host.state().entry().prepared_capture.as_ref().unwrap().body,
            source.as_bytes()
        );
        assert!(review_text(&host).contains("Editor differs"));
        assert!(host.click_on(&Selector::role("button").containing("Refresh saved revision")));
        assert_eq!(
            host.state().entry().prepared_capture.as_ref().unwrap().body,
            std::fs::read(&path).unwrap()
        );
        assert!(review_text(&host).contains("UNSAVED"));
        assert!(host.click_on(&Selector::role("button").containing("Discard saved revision")));
        assert!(host.state().entry().prepared_capture.is_none());
        assert!(host.state().entry().prepared_capture_error.is_none());
    }

    #[test]
    fn saved_revision_failed_refresh_replaces_old_bytes_and_preserves_document() {
        let (_temp, path, mut host) = review_fixture("old reading");
        assert!(host.click_on(&Selector::role("button").containing("Review saved revision")));
        let before = host.state().document().snapshot();
        for bytes in [vec![0xff, 0xfe], b"larger than the cap".to_vec()] {
            std::fs::write(&path, &bytes).unwrap();
            host.update(|state| state.set_capture_limit(4));
            assert!(host.click_on(&Selector::role("button").containing("Refresh saved revision")));
            assert!(host.state().entry().prepared_capture.is_none());
            assert!(host.state().entry().prepared_capture_source_path.is_none());
            assert!(host.state().entry().prepared_capture_error.is_some());
            assert!(!review_text(&host).contains("old reading"));
            assert_eq!(host.state().document().snapshot(), before);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        std::fs::remove_file(&path).unwrap();
        assert!(host.click_on(&Selector::role("button").containing("Refresh saved revision")));
        assert!(host.state().entry().prepared_capture.is_none());
        assert!(
            host.state()
                .entry()
                .prepared_capture_error
                .as_deref()
                .unwrap()
                .contains("unavailable")
        );
        assert_eq!(host.state().document().snapshot(), before);
    }

    #[test]
    fn saved_revision_clears_only_after_successful_source_transitions() {
        let (_temp, path, mut host) = review_fixture("saved");
        assert!(host.click_on(&Selector::role("button").containing("Review saved revision")));
        let saved_as = path.with_file_name("copy.djot");
        host.update(|state| state.path = TextInput::new(saved_as.to_string_lossy()));
        assert!(host.click_on(&Selector::role("button").containing("Save As")));
        assert!(host.state().entry().prepared_capture.is_none());
        assert!(host.click_on(&Selector::role("button").containing("Review saved revision")));
        host.update(|state| {
            state.path = TextInput::new(path.with_file_name("absent.djot").to_string_lossy())
        });
        assert!(host.click_on(&Selector::role("button").containing("Open")));
        assert!(host.state().entry().prepared_capture.is_some());
        let copy = host.state().focused_key().unwrap();
        host.update(|state| state.path = TextInput::new(path.to_string_lossy()));
        assert!(host.click_on(&Selector::role("button").containing("Open")));
        assert!(host.state().entry().prepared_capture.is_none());
        assert!(
            host.state()
                .docs
                .doc(copy)
                .is_some_and(|entry| entry.prepared_capture.is_some())
        );
        assert!(host.click_on(&Selector::role("button").containing("Review saved revision")));
        assert!(host.click_on(&Selector::role("button").containing("Reload")));
        assert!(host.state().entry().prepared_capture.is_none());
        assert!(host.click_on(&Selector::role("button").containing("Review saved revision")));
        let reviewed = host.state().focused_key().unwrap();
        assert!(host.click_on(&Selector::role("button").containing("New")));
        assert!(host.state().entry().prepared_capture.is_none());
        assert!(!host.click_on(&Selector::role("button").containing("Review saved revision")));
        assert!(
            host.state()
                .docs
                .doc(reviewed)
                .is_some_and(|entry| entry.prepared_capture.is_some())
        );
    }

    #[test]
    fn saved_revision_refuses_a_binding_rebound_away_from_the_session_source() {
        let (_temp, path, mut host) = review_fixture("original");
        let moved = path.with_file_name("moved.djot");
        std::fs::rename(&path, &moved).unwrap();
        host.update(|state| {
            let id = state.entry().catalog_id.clone().unwrap();
            state.catalog.as_mut().unwrap().rebind(&id, &moved).unwrap();
        });
        std::fs::write(&path, "replacement").unwrap();
        assert!(host.click_on(&Selector::role("button").containing("Review saved revision")));
        assert!(host.state().entry().prepared_capture.is_none());
        assert!(
            host.state()
                .entry()
                .prepared_capture_error
                .as_deref()
                .unwrap()
                .contains("no longer matches")
        );
        assert_eq!(host.state().catalog.as_ref().unwrap().records().len(), 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "original");
    }

    struct GatePort {
        target: KnotRetainTargetV1,
        calls: AtomicUsize,
        entered: mpsc::Sender<()>,
        replies: Mutex<mpsc::Receiver<Result<KnotRetainReceiptV1, KnotRetainError>>>,
        seen: Mutex<Vec<KnotFileRevisionV1>>,
    }

    impl KnotRetainPort for GatePort {
        fn target(&self) -> &KnotRetainTargetV1 {
            &self.target
        }

        fn retain_reviewed(
            &self,
            expected_target: &KnotRetainTargetV1,
            revision: KnotFileRevisionV1,
        ) -> Result<KnotRetainReceiptV1, KnotRetainError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if expected_target != &self.target {
                return Err(KnotRetainError("wrong target".into()));
            }
            self.seen.lock().unwrap().push(revision);
            let _ = self.entered.send(());
            self.replies
                .lock()
                .unwrap()
                .recv()
                .unwrap_or_else(|_| Err(KnotRetainError("reply channel disconnected".into())))
        }
    }

    fn target(label: &str, byte: u8) -> KnotRetainTargetV1 {
        KnotRetainTargetV1 {
            persona: knot_capture::KnotPersonaDisplayV1 {
                stable_id: format!("persona:{byte}"),
                label: label.into(),
            },
            space_id: [byte; 32],
            writer: [byte + 1; 32],
            encryption: knot_capture::KnotRetainEncryptionV1::PersonalVaultV1,
        }
    }

    fn revision(id: &str, body: &[u8]) -> KnotFileRevisionV1 {
        KnotFileRevisionV1 {
            document_id: id.into(),
            title: id.into(),
            media_type: "text/x-djot".into(),
            body: body.to_vec(),
        }
    }

    fn gated_port(
        target: KnotRetainTargetV1,
    ) -> (
        Arc<GatePort>,
        mpsc::Receiver<()>,
        mpsc::Sender<Result<KnotRetainReceiptV1, KnotRetainError>>,
    ) {
        let (entered_send, entered) = mpsc::channel();
        let (reply, replies) = mpsc::channel();
        (
            Arc::new(GatePort {
                target,
                calls: AtomicUsize::new(0),
                entered: entered_send,
                replies: Mutex::new(replies),
                seen: Mutex::new(Vec::new()),
            }),
            entered,
            reply,
        )
    }

    fn retention_harness(
        ports: Vec<Arc<dyn KnotRetainPort>>,
        reviewed: KnotFileRevisionV1,
    ) -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        let wake = host.wake();
        host.update(|state| {
            state.set_retention_targets(ports, wake);
            state.entry_mut().prepared_capture = Some(reviewed);
            state.retention_selected = Some(0);
        });
        host
    }

    fn drain_wake(host: &mut Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>) {
        for _ in 0..1_000 {
            if host.process_wake() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("retention worker did not wake the harness");
    }

    #[test]
    fn retention_worker_freezes_the_review_refuses_duplicates_and_preserves_original_receipts() {
        let original = revision("file:original", b"saved bytes");
        let first_target = target("First", 1);
        let second_target = target("Second", 3);
        let (first, entered, reply) = gated_port(first_target.clone());
        let (second, _, _) = gated_port(second_target.clone());
        let mut host = retention_harness(vec![first.clone(), second], original.clone());
        let wake = host.wake();

        host.update(|state| state.retain_reviewed(wake));
        entered
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        let duplicate_wake = host.wake();
        host.update(|state| {
            state.retain_reviewed(duplicate_wake);
            state.retention_selected = Some(1);
            state.entry_mut().prepared_capture = Some(revision("file:replacement", b"new review"));
        });
        assert_eq!(first.calls.load(Ordering::SeqCst), 1);
        assert!(
            host.state()
                .retention_error
                .as_deref()
                .unwrap()
                .contains("already")
        );
        assert_eq!(first.seen.lock().unwrap().as_slice(), &[original.clone()]);

        host.update(|state| {
            state
                .document_mut()
                .session_mut()
                .input_mut()
                .unwrap()
                .insert_str("unsaved");
            state.discard_close = true;
        });
        host.request_close(CloseRequest::Native);
        assert!(!host.state().discard_close);
        assert!(!host.hidden());
        assert!(!host.close_requested());
        assert!(
            host.state()
                .message
                .as_deref()
                .unwrap()
                .contains("retention")
        );

        reply
            .send(Ok(KnotRetainReceiptV1 {
                target: first_target.clone(),
                document_id: original.document_id.clone(),
                operation: [9; 32],
                already_retained: false,
            }))
            .unwrap();
        drain_wake(&mut host);
        assert_eq!(
            Arc::strong_count(&first),
            2,
            "worker released its owner handle"
        );
        assert_eq!(host.state().retention_selected, Some(1));
        assert_eq!(
            host.state()
                .entry()
                .prepared_capture
                .as_ref()
                .unwrap()
                .document_id,
            "file:replacement"
        );
        assert_eq!(
            host.state().retention_receipt.as_ref().unwrap().target,
            first_target
        );
        assert_eq!(
            host.state().retention_receipt.as_ref().unwrap().document_id,
            "file:original"
        );
        host.request_close(CloseRequest::Native);
        assert!(!host.close_requested());
        assert!(matches!(host.state().pending, Some(PendingAction::Quit)));
    }

    #[test]
    fn retention_failure_and_disconnect_clear_busy_state_for_an_explicit_retry() {
        let reviewed = revision("file:retry", b"exact bytes");
        let selected = target("Retry", 5);
        let (port, entered, reply) = gated_port(selected);
        let mut host = retention_harness(vec![port.clone()], reviewed);
        let wake = host.wake();

        host.update(|state| state.retain_reviewed(wake));
        entered
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        reply.send(Err(KnotRetainError("denied".into()))).unwrap();
        drain_wake(&mut host);
        assert!(host.state().retention_busy.is_none());
        assert!(
            host.state()
                .retention_error
                .as_deref()
                .unwrap()
                .contains("denied")
        );

        let wake = host.wake();
        host.update(|state| state.retain_reviewed(wake));
        entered
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        drop(reply);
        drain_wake(&mut host);
        assert!(host.state().retention_busy.is_none());
        assert!(
            host.state()
                .retention_error
                .as_deref()
                .unwrap()
                .contains("could not be confirmed")
        );
        assert_eq!(port.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn retention_rejects_a_mismatched_receipt_without_creating_a_confirmed_result() {
        let reviewed = revision("file:expected", b"exact bytes");
        let selected = target("Expected", 7);
        let (port, entered, reply) = gated_port(selected);
        let mut host = retention_harness(vec![port], reviewed);
        let wake = host.wake();
        host.update(|state| state.retain_reviewed(wake));
        entered
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        reply
            .send(Ok(KnotRetainReceiptV1 {
                target: target("Wrong", 8),
                document_id: "file:other".into(),
                operation: [8; 32],
                already_retained: false,
            }))
            .unwrap();
        drain_wake(&mut host);
        assert!(host.state().retention_receipt.is_none());
        assert!(
            host.state()
                .retention_error
                .as_deref()
                .unwrap()
                .contains("identity changed")
        );
    }

    #[test]
    fn retention_never_auto_selects_or_starts_a_write() {
        let reviewed = revision("file:manual", b"exact bytes");
        let (port, _entered, _reply) = gated_port(target("Manual", 11));
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        let wake = host.wake();
        host.update(|state| {
            state.set_retention_targets(vec![port.clone()], wake.clone());
            state.entry_mut().prepared_capture = Some(reviewed);
            state.retain_reviewed(wake);
        });
        assert!(host.state().retention_selected.is_none());
        assert!(
            host.state()
                .retention_error
                .as_deref()
                .unwrap()
                .contains("Choose a persona")
        );
        assert_eq!(port.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn retention_target_rows_select_the_matching_same_persona_space_and_show_full_detail() {
        let reviewed = revision("file:targets", b"exact bytes");
        let first_target = target("Shared persona", 1);
        let mut second_target = target("Shared persona", 3);
        second_target.persona.stable_id = first_target.persona.stable_id.clone();
        let mut third_target = target("Shared persona", 9);
        third_target.persona.stable_id = first_target.persona.stable_id.clone();
        third_target.space_id = second_target.space_id;
        third_target.writer = [5; 32];
        let (first, _, _) = gated_port(first_target);
        let (second, _, _) = gated_port(second_target.clone());
        let (third, _, _) = gated_port(third_target);
        let mut host = retention_harness(vec![first, second, third], reviewed);
        host.update(|state| state.retention_selected = None);
        host.layout_at(900.0, 640.0);

        assert!(host.click_on(&Selector::role("button").with_attr("data-retention-target", "1")));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let detail = class_node(&dom, dom.document(), "knot-retention-selected").unwrap();
        let detail_text = text_content(&dom, detail);
        assert!(detail_text.contains("Persona stable ID: persona:1"));
        assert!(detail_text.contains(&format!("Space ID: {}", hex32(&second_target.space_id))));
        assert!(detail_text.contains(&format!("Writer ID: {}", hex32(&second_target.writer))));
        assert!(detail_text.contains("Encryption: Personal vault encryption"));
        let selected = attr_node(&dom, dom.document(), "data-retention-target", "1").unwrap();
        assert_eq!(
            dom.attribute(
                selected,
                &Namespace::from(""),
                &LocalName::from("aria-pressed")
            ),
            Some("true".into())
        );
        let unselected = attr_node(&dom, dom.document(), "data-retention-target", "0").unwrap();
        assert_eq!(
            dom.attribute(
                unselected,
                &Namespace::from(""),
                &LocalName::from("aria-pressed")
            ),
            Some("false".into())
        );
        let targets = class_node(&dom, dom.document(), "knot-retention-targets").unwrap();
        let targets_text = text_content(&dom, targets);
        assert!(targets_text.contains("writer 04040404"));
        assert!(targets_text.contains("writer 05050505"));
    }

    #[test]
    fn retention_without_destinations_offers_no_action() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        let wake = host.wake();
        host.update(|state| {
            state.set_retention_targets(vec![], wake);
            state.entry_mut().prepared_capture = Some(revision("file:manual", b"saved"));
        });
        assert!(!host.click_on(&Selector::role("button").containing("Retain reviewed revision")));
        assert!(host.state().retention_busy.is_none());
        assert!(host.state().retention_selected.is_none());
        host.layout_at(900.0, 640.0);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let detail = class_node(&dom, dom.document(), "knot-retention-selected").unwrap();
        assert!(text_content(&dom, detail).contains("No destination selected."));
    }

    #[test]
    fn disconnected_worker_completion_releases_the_close_guard() {
        let mut host = harness(KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""));
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        host.update(|state| {
            state.retention_receiver = Some(receiver);
            state.retention_busy = Some(RetentionRequest {
                target: target("Disconnected", 12),
                document_id: "file:manual".into(),
            });
            state.drain_retention();
        });
        assert!(host.state().retention_busy.is_none());
        assert!(
            host.state()
                .retention_error
                .as_ref()
                .unwrap()
                .contains("uncertain")
        );
        host.request_close(CloseRequest::Native);
        assert!(host.close_requested());
    }
}
