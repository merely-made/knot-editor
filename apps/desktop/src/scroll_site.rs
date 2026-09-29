// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::documents::{DocKey, ReadingKind, SiteKey, TileRole};
use crate::workspace::{DesktopState, DesktopView, DocumentEntry};
use cambium::{
    El, GenetCtx, GenetElement, KeyEvent, Keyed, Popover, PopoverEvent, PopoverPlacement,
    PopoverState, TextFieldMode, TextInput, View, button, button_with, el, lens, on_key, popover,
    span, text_field_typed, textarea_typed,
};
use cambium_genet_winit_host::{AppCtx, HostWake, ScrollAlign};
use inker::{
    Block, Engine, EngineDocument, EngineError, EngineInput, FoldKey, FoldMarkers, FoldState,
    InPageTarget, InlineSpan, TableAlignment,
};
use knot_site::micron_submission::{MicronResponse, MicronSubmissionConfig, PreparedMicronRequest};
use knot_site::submission::{PreparedSubmission, SubmissionReceipt};
use knot_site::{LocalServer, Page, Site, SiteFormat};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use nematic::micron::forms::{FormLimits, FormState};
use nematic::micron::syntax::FieldKind;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use workbench::TileId;

const LABELS: [&str; 6] = [
    "Author",
    "Language",
    "Classification (0–9)",
    "Published (UTC or blank)",
    "Modified (UTC or blank)",
    "Abstract (native Scrolltext)",
];
const RESPONSE_DISPLAY_LIMIT: usize = 8 * 1024;

/// Ephemeral state for a Micron request form.  It deliberately lives beside the
/// preview rather than in the document: editing a field never changes authored
/// source, a site manifest, or an address.
struct MicronFormEditor {
    source: String,
    address: String,
    form: FormState,
    inputs: Vec<TextInput>,
    prepared: Option<MicronPreparedRequest>,
}

struct MicronPreparedRequest {
    target: String,
    values: BTreeMap<String, String>,
    masked: BTreeSet<String>,
    input_snapshot: Vec<String>,
}

/// The Micron preview's reader fold state (navigation plan decision 3). Like
/// the form editor it is preview state only: toggling or following an in-page
/// link never writes source, the document, the site manifest or an address.
#[derive(Default)]
pub(crate) struct MicronPreviewFolds {
    folds: FoldState,
    /// The shared marker token (decision 17); a theme may replace it.
    pub(crate) markers: FoldMarkers,
    /// Address and source text `folds` was last reconciled against.
    reconciled: Option<(String, String)>,
    /// An in-page link's target, awaiting its scroll in `after_dispatch`.
    jump: Option<InPageTarget>,
}

/// Names a top-level preview block's element by its block index.
const BLOCK_INDEX_ATTR: &str = "data-block-index";

fn lower_micron(address: &str, text: &str) -> Result<EngineDocument, EngineError> {
    nematic::MicronEngine::new().render(&EngineInput::new(address, text))
}

/// One open site: its manifest, the port it publishes on, its local server, how
/// many times it has been published, and the metadata drafts of its pages
/// whose metadata tiles are open.
pub struct SiteEntry {
    pub site: Site,
    pub port: TextInput,
    pub server: Option<LocalServer>,
    publication_number: usize,
    pub(crate) drafts: BTreeMap<String, MetadataDraft>,
}

impl SiteEntry {
    /// The site's name: its folder's.
    pub(crate) fn name(&self) -> String {
        let root = self.site.root();
        root.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| crate::workspace::display_path(root))
    }

    /// How many times the site has been published locally.
    pub(crate) fn publication_number(&self) -> usize {
        self.publication_number
    }

    /// The manifest page whose file is `path`, a canonical path.
    fn page_at(&self, path: &Path) -> Option<&Page> {
        self.site
            .config
            .pages
            .iter()
            .find(|page| self.site.page_path(&page.path).ok().as_deref() == Some(path))
    }
}

/// The window's site state: the open sites, the Site popover and the fields
/// that open or create the next site. What belongs to one document, its page,
/// Micron form and composer, is on its entry ([`DocumentSite`]).
pub struct ScrollWorkspace {
    sites: BTreeMap<SiteKey, SiteEntry>,
    next_site: u64,
    /// The Site popover: the folder, format, Create and Open.
    pub(crate) popover: PopoverState,
    pub folder: TextInput,
    /// The port the next site opened or created publishes on.
    pub port: TextInput,
    pub format: SiteFormat,
    submission_wake: Option<HostWake>,
    titan_submission_error: Option<String>,
}

impl Default for ScrollWorkspace {
    fn default() -> Self {
        Self {
            sites: BTreeMap::new(),
            next_site: 0,
            popover: PopoverState::default(),
            folder: TextInput::default(),
            port: TextInput::new("5699"),
            format: SiteFormat::Scroll,
            submission_wake: None,
            titan_submission_error: None,
        }
    }
}

impl ScrollWorkspace {
    pub fn set_titan_submission_error(&mut self, error: Option<String>) {
        self.titan_submission_error = error;
    }

    pub fn set_submission_wake(&mut self, wake: HostWake) {
        self.submission_wake = Some(wake);
    }

    pub(crate) fn site(&self, key: SiteKey) -> Option<&SiteEntry> {
        self.sites.get(&key)
    }

    /// The most recently opened site, for legacy single-site test fixtures.
    /// Product behavior must resolve a site from an explicit key or page.
    #[cfg(test)]
    pub(crate) fn current_key(&self) -> Option<SiteKey> {
        self.sites.keys().next_back().copied()
    }

    #[cfg(test)]
    pub(crate) fn current_site(&self) -> Option<&SiteEntry> {
        self.current_key().and_then(|key| self.site(key))
    }

    pub(crate) fn site_count(&self) -> usize {
        self.sites.len()
    }

    pub(crate) fn serving_count(&self) -> usize {
        self.sites
            .values()
            .filter(|entry| entry.server.is_some())
            .count()
    }

    fn key_for_root(&self, root: &Path) -> Option<SiteKey> {
        self.sites
            .iter()
            .find(|(_, entry)| entry.site.root() == root)
            .map(|(key, _)| *key)
    }

    /// Hold another open site. A repeated canonical folder resolves to its
    /// existing entry. When the requested port is this format's default and
    /// another site already claims it, defer to the OS with port 0.
    fn insert(&mut self, site: Site) -> (SiteKey, bool) {
        if let Some(key) = self.key_for_root(site.root()) {
            return (key, false);
        }
        let key = SiteKey(self.next_site);
        self.next_site += 1;
        let requested = self.port.text().parse::<u16>().ok();
        let default = site.config.format.default_port();
        let default_claimed = default.is_some_and(|port| {
            self.sites.values().any(|entry| {
                entry.server.as_ref().map(|server| server.address().port()) == Some(port)
                    || entry.port.text().parse::<u16>().ok() == Some(port)
            })
        });
        let port = if requested == default && default_claimed {
            TextInput::new("0")
        } else {
            TextInput::new(self.port.text())
        };
        self.sites.insert(
            key,
            SiteEntry {
                site,
                port,
                server: None,
                publication_number: 0,
                drafts: BTreeMap::new(),
            },
        );
        (key, true)
    }

    /// The open site page whose file is `path`, a canonical path.
    fn page_at(&self, path: &Path) -> Option<(SiteKey, &Page)> {
        self.sites
            .iter()
            .find_map(|(key, entry)| entry.page_at(path).map(|page| (*key, page)))
    }

    /// Drop `site`, which stops its server.
    fn remove(&mut self, site: SiteKey) {
        self.sites.remove(&site);
    }

    /// `site`'s port field, whose value its next publication binds.
    pub(crate) fn site_port(&self, site: SiteKey) -> Option<&TextInput> {
        self.sites.get(&site).map(|entry| &entry.port)
    }

    pub(crate) fn site_port_mut(&mut self, site: SiteKey) -> Option<&mut TextInput> {
        self.sites.get_mut(&site).map(|entry| &mut entry.port)
    }

    /// Stop `site`'s local server.
    pub(crate) fn stop_serving(&mut self, site: SiteKey) {
        if let Some(entry) = self.sites.get_mut(&site) {
            entry.server = None;
        }
    }
}

/// Which metadata fields a site format supports.
fn metadata_indices(format: SiteFormat) -> &'static [usize] {
    match format {
        SiteFormat::Scroll => &[0, 1, 2, 3, 4, 5],
        // Gemini has a native language parameter. The other Scroll-only
        // header/abstract fields are not silently projected into Gemtext.
        SiteFormat::Gemini => &[1],
        SiteFormat::Spartan | SiteFormat::Micron => &[],
    }
}

/// A document's place in an open site: which site, and which of its pages.
pub(crate) struct SitePage {
    pub(crate) site: SiteKey,
    pub(crate) name: String,
}

/// A page's metadata being edited, against the manifest's saved values. It
/// lives with the site while the page's metadata tile is open.
pub(crate) struct MetadataDraft {
    pub(crate) fields: [TextInput; 6],
    baseline: [String; 6],
}

impl MetadataDraft {
    fn new(page: &Page) -> Self {
        // Preserve the entire abstract in the text field when opening an
        // existing site; metadata saving never reformats its source.
        let baseline = [
            page.author.clone(),
            page.language.clone(),
            page.classification.to_string(),
            page.published.clone(),
            page.modified.clone(),
            page.abstract_source.clone(),
        ];
        Self {
            fields: baseline.clone().map(TextInput::new),
            baseline,
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.fields
            .iter()
            .zip(&self.baseline)
            .any(|(field, saved)| field.text() != saved)
    }

    fn discard(&mut self) {
        self.fields = self.baseline.clone().map(TextInput::new);
    }
}

/// A document's Titan or Spartan composer: what is being written, the reviewed
/// bytes, and the send in flight or its reply.
pub(crate) struct Submission {
    pub(crate) target: TextInput,
    pub(crate) mime: TextInput,
    pub(crate) body: TextInput,
    pub(crate) token: TextInput,
    pub(crate) prepared: Option<PreparedSubmission>,
    pub(crate) receiver: Option<Receiver<Result<SubmissionReceipt, String>>>,
    pub(crate) result: Option<String>,
    pub(crate) response: Option<String>,
}

impl Default for Submission {
    fn default() -> Self {
        Self {
            target: TextInput::default(),
            mime: TextInput::new("text/gemini"),
            body: TextInput::default(),
            token: TextInput::default(),
            prepared: None,
            receiver: None,
            result: None,
            response: None,
        }
    }
}

impl Submission {
    fn select_spartan_prompt(&mut self, target: String) {
        self.target = TextInput::new(target);
        self.mime = TextInput::new("text/plain");
        self.prepared = None;
        self.token = TextInput::default();
        self.result = None;
        self.response = None;
    }

    fn discard(&mut self) {
        self.prepared = None;
        self.token = TextInput::default();
        self.result = None;
        self.response = None;
    }

    fn take_for_send(&mut self) -> Result<(PreparedSubmission, Option<String>), String> {
        let prepared = self
            .prepared
            .take()
            .ok_or("Prepare reviewed bytes before sending.")?;
        let token = if prepared.target().starts_with("titan://") {
            let token = std::mem::take(&mut self.token).text().to_owned();
            (!token.is_empty()).then_some(token)
        } else {
            self.token = TextInput::default();
            None
        };
        Ok((prepared, token))
    }

    fn drain(&mut self) {
        let Some(rx) = self.receiver.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(r)) => {
                self.result = Some(format!(
                    "Reply {} {} ({} bytes)",
                    r.code,
                    r.meta,
                    r.body.len()
                ));
                self.response = response_display(&r.body);
                self.receiver = None
            },
            Ok(Err(e)) => {
                self.result = Some(format!("Send failed: {e}"));
                self.response = None;
                self.receiver = None
            },
            Err(TryRecvError::Disconnected) => {
                self.result =
                    Some("Send outcome unavailable; check target before retrying.".into());
                self.response = None;
                self.receiver = None
            },
            Err(TryRecvError::Empty) => {},
        }
    }
}

/// A document's Micron request: the form being filled, the request in flight,
/// and its reply.
#[derive(Default)]
pub(crate) struct MicronRequest {
    form: Option<MicronFormEditor>,
    pub(crate) receiver: Option<Receiver<(u64, Result<MicronResponse, String>)>>,
    next: u64,
    active: Option<(u64, String, String)>,
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    pub(crate) result: Option<String>,
    pub(crate) response: Option<String>,
}

impl MicronRequest {
    fn open_form(&mut self, source: String, address: String) -> Result<(), String> {
        let form = FormState::from_source(&source, FormLimits::default())?;
        if form.actions().is_empty() {
            return Err("This Micron page has no request action.".into());
        }
        let inputs = form
            .fields()
            .iter()
            .map(|field| TextInput::new(field.value()))
            .collect();
        self.form = Some(MicronFormEditor {
            source,
            address,
            form,
            inputs,
            prepared: None,
        });
        Ok(())
    }

    fn close_form(&mut self) {
        self.form = None;
        // A reopened form must not show the previous reply. An in-flight send
        // keeps its own status, which its completion replaces.
        if self.receiver.is_none() {
            self.result = None;
            self.response = None;
        }
    }

    fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
            self.active = None;
            self.result = Some(
                "Micron request cancelled locally. Its remote outcome may be unknown; do not retry automatically."
                    .into(),
            );
            self.response = None;
        }
    }

    fn set_checked(&mut self, index: usize, checked: bool) -> Result<(), String> {
        let editor = self.form.as_mut().ok_or("Open the Micron form first.")?;
        editor.form.set_checked(index, checked)?;
        editor.prepared = None;
        Ok(())
    }

    fn prepare(&mut self, source: &str, address: &str, action: usize) -> Result<(), String> {
        let editor = self.form.as_mut().ok_or("Open the Micron form first.")?;
        if editor.address != address {
            return Err(
                "The page address changed. Reopen the form before preparing a request.".into(),
            );
        }
        for (index, input) in editor.inputs.iter().enumerate() {
            if matches!(editor.form.fields()[index].kind, FieldKind::Text { .. }) {
                editor.form.set_text(index, input.text().to_owned())?;
            }
        }
        let prepared = editor.form.prepare(action, source)?;
        let masked = editor
            .form
            .fields()
            .iter()
            .filter(|field| field.masked())
            .filter_map(|field| match &field.kind {
                FieldKind::Text { name, .. } => Some(format!("field_{name}")),
                _ => None,
            })
            .collect();
        editor.prepared = Some(MicronPreparedRequest {
            target: prepared.target.clone(),
            values: prepared.into_values(),
            masked,
            input_snapshot: editor
                .inputs
                .iter()
                .map(|input| input.text().to_owned())
                .collect(),
        });
        Ok(())
    }

    /// Take a finished request's outcome. The reply shows only while the
    /// document's source and address are still the ones it was sent from.
    fn drain(&mut self, current_source: &str, current_address: &str) {
        let Some(rx) = self.receiver.as_ref() else {
            return;
        };
        let sent_from_current = |active: &Option<(u64, String, String)>, id: Option<u64>| {
            active.as_ref().is_some_and(|(active, source, address)| {
                id.is_none_or(|id| *active == id)
                    && source == current_source
                    && address == current_address
            })
        };
        match rx.try_recv() {
            Ok((id, outcome)) => {
                let current = sent_from_current(&self.active, Some(id));
                self.receiver = None;
                self.active = None;
                self.cancel = None;
                if !current {
                    return;
                }
                match outcome {
                    Ok(response) => {
                        self.result = Some(format!("Micron reply ({} bytes)", response.body.len()));
                        self.response = response_display(&response.body);
                    },
                    Err(error) => {
                        self.result = Some(format!("Micron request failed: {error}"));
                        self.response = None;
                    },
                }
            },
            Err(TryRecvError::Disconnected) => {
                let current = sent_from_current(&self.active, None);
                self.receiver = None;
                self.cancel = None;
                self.active = None;
                if current {
                    self.result =
                        Some("Micron request outcome unavailable; review before retrying.".into());
                    self.response = None;
                }
            },
            Err(TryRecvError::Empty) => {},
        }
    }
}

/// What one document holds of the site workspace. Each document keeps its own,
/// so switching tabs never carries a draft, form or reply across.
#[derive(Default)]
pub(crate) struct DocumentSite {
    pub(crate) page: Option<SitePage>,
    pub(crate) micron: MicronRequest,
    pub(crate) submission: Submission,
}

impl DocumentSite {
    /// Whether a send from this document is in flight.
    pub(crate) fn busy(&self) -> bool {
        self.submission.receiver.is_some() || self.micron.receiver.is_some()
    }

    pub(crate) fn drain(&mut self, source: &str, address: &str) {
        self.submission.drain();
        self.micron.drain(source, address);
    }
}

/// Parse-or-default for one numeric request bound, clamped to the supported
/// range. A malformed value falls back to the default rather than refusing.
fn env_bound<T>(raw: Option<&str>, default: T, min: T, max: T) -> T
where
    T: std::str::FromStr + Ord,
{
    raw.and_then(|value| value.trim().parse::<T>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

/// Request bounds for one Micron send. `KNOT_NOMADNET_TIMEOUT_SECS` (seconds,
/// default 30, clamped 1–120) and `KNOT_NOMADNET_MAX_RESPONSE_BYTES` (default
/// 4 MiB, clamped 1 byte–64 MiB) override the compile-time defaults.
fn micron_submission_config() -> MicronSubmissionConfig {
    let secs = env_bound(
        std::env::var("KNOT_NOMADNET_TIMEOUT_SECS").ok().as_deref(),
        30_u64,
        1,
        120,
    );
    let max_response_bytes = env_bound(
        std::env::var("KNOT_NOMADNET_MAX_RESPONSE_BYTES")
            .ok()
            .as_deref(),
        4 * 1024 * 1024_usize,
        1,
        64 * 1024 * 1024,
    );
    MicronSubmissionConfig {
        timeout: std::time::Duration::from_secs(secs),
        max_response_bytes,
        ..MicronSubmissionConfig::default()
    }
}

fn response_display(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let shown = String::from_utf8_lossy(&body[..body.len().min(RESPONSE_DISPLAY_LIMIT)]);
    let suffix = if body.len() > RESPONSE_DISPLAY_LIMIT {
        format!(
            "\n\n[Response truncated after {RESPONSE_DISPLAY_LIMIT} bytes; {} bytes received.]",
            body.len()
        )
    } else {
        String::new()
    };
    Some(format!("{shown}{suffix}"))
}

impl DesktopState {
    /// Whether fold overrides exist that the current source has not been
    /// reconciled against. With no overrides there is nothing to drop.
    pub(crate) fn micron_folds_need_sync(&self) -> bool {
        let folds = &self.entry().micron_folds;
        if folds.folds == FoldState::default() {
            return false;
        }
        let current = self.document().snapshot();
        folds.reconciled.as_ref().is_none_or(|(address, text)| {
            current.format != knot_document::DocumentFormat::Micron
                || *address != current.source.address
                || *text != current.text
        })
    }

    /// Reconcile fold overrides with the current source, as the shared session
    /// does on replacement: the same address keeps every heading key still
    /// present (decisions 10 and 11), a new address or a non-Micron document
    /// starts fresh.
    pub(crate) fn sync_micron_folds(&mut self) {
        if let Some(key) = self.focused_key() {
            self.sync_micron_folds_for(key);
        }
    }

    fn sync_micron_folds_for(&mut self, document: DocKey) {
        let current = self.surface_for(document).snapshot();
        let folds = &mut self.entry_mut_for(document).micron_folds;
        if current.format != knot_document::DocumentFormat::Micron {
            folds.folds = FoldState::default();
            folds.reconciled = None;
            return;
        }
        match &folds.reconciled {
            Some((address, text)) if *address == current.source.address => {
                if *text == current.text {
                    return;
                }
                if let Ok(document) = lower_micron(&current.source.address, &current.text) {
                    folds.folds.reconcile(&document.navigation);
                }
            },
            _ => folds.folds = FoldState::default(),
        }
        folds.reconciled = Some((current.source.address, current.text));
    }

    /// Open or close the preview fold whose heading key is `key`.
    fn toggle_micron_fold_for(&mut self, document: DocKey, key: &FoldKey) {
        self.sync_micron_folds_for(document);
        let current = self.surface_for(document).snapshot();
        let Ok(rendered) = lower_micron(&current.source.address, &current.text) else {
            return;
        };
        let navigation = &rendered.navigation;
        let fold = navigation
            .is_current(&rendered.blocks)
            .then(|| navigation.fold_keys().iter().position(|each| each == key))
            .flatten();
        match fold {
            Some(fold) => {
                self.entry_mut_for(document)
                    .micron_folds
                    .folds
                    .toggle(navigation, fold);
            },
            None => {
                self.message = Some("That heading is no longer in the current source.".into());
            },
        }
    }

    /// Follow an in-page link (decision 18): open the folds hiding its target,
    /// so this dispatch renders it, and queue the scroll for `after_dispatch`.
    /// A target with no block is inert.
    fn follow_micron_in_page_for(&mut self, document: DocKey, target: &InPageTarget) {
        let Some(block) = target.block else {
            return;
        };
        let current = self.surface_for(document).snapshot();
        if current.format != knot_document::DocumentFormat::Micron {
            return;
        }
        self.sync_micron_folds_for(document);
        let Ok(rendered) = lower_micron(&current.source.address, &current.text) else {
            return;
        };
        let navigation = &rendered.navigation;
        if !navigation.is_current(&rendered.blocks) || block >= rendered.blocks.len() {
            return;
        }
        self.entry_mut_for(document)
            .micron_folds
            .folds
            .open_ancestors(navigation, block);
        self.entry_mut_for(document).micron_folds.jump = Some(target.clone());
    }

    fn open_micron_form_for(&mut self, document: DocKey) {
        let source = self.surface_for(document).snapshot();
        if source.format != knot_document::DocumentFormat::Micron {
            self.message = Some("Micron forms are available only for a Micron document.".into());
            return;
        }
        match self
            .entry_mut_for(document)
            .site
            .micron
            .open_form(source.text, source.source.address)
        {
            Ok(()) => {
                self.message = Some(
                    "Form values are local and temporary. Prepare an action before any request can be considered."
                        .into(),
                )
            }
            Err(error) => self.message = Some(format!("Micron form: {error}")),
        }
    }

    fn prepare_micron_form_for(&mut self, document: DocKey, action: usize) {
        let source = self.surface_for(document).snapshot();
        match self
            .entry_mut_for(document)
            .site
            .micron
            .prepare(&source.text, &source.source.address, action)
        {
            Ok(()) => {
                self.message = Some(
                    "Request prepared locally. Knot's static preview does not execute Micron request handlers."
                        .into(),
                )
            }
            Err(error) => self.message = Some(format!("Micron request was not prepared: {error}")),
        }
    }

    fn send_micron_form_for(&mut self, document: DocKey) {
        if self.entry_for(document).site.busy() {
            self.message = Some("A submission is already sending.".into());
            return;
        }
        let Some(wake) = self.scroll.submission_wake.clone() else {
            self.message = Some("Submission worker is unavailable.".into());
            return;
        };
        let interface = match std::env::var("KNOT_NOMADNET_TCP") {
            Ok(value) => match value.parse() {
                Ok(interface) => interface,
                Err(_) => {
                    self.message = Some("KNOT_NOMADNET_TCP must be a host:port interface for the remote Reticulum peer.".into());
                    return;
                },
            },
            Err(_) => {
                self.message = Some("Set KNOT_NOMADNET_TCP to an explicit Reticulum TCP interface before sending a Micron request.".into());
                return;
            },
        };
        let current_document = self.surface_for(document).snapshot();
        let micron = &mut self.entry_mut_for(document).site.micron;
        let Some(editor) = micron.form.as_mut() else {
            self.message = Some("Open and prepare a Micron form first.".into());
            return;
        };
        if editor.source != current_document.text
            || editor.address != current_document.source.address
        {
            self.message = Some(
                "The source or page address changed. Reopen and review the Micron form before sending."
                    .into(),
            );
            return;
        }
        let Some(prepared) = editor.prepared.as_ref() else {
            self.message = Some("Prepare the current Micron request before sending.".into());
            return;
        };
        let current = editor
            .inputs
            .iter()
            .map(|input| input.text().to_owned())
            .collect::<Vec<_>>();
        if prepared.input_snapshot != current {
            self.message = Some(
                "Field values changed after review. Prepare the request again before sending."
                    .into(),
            );
            return;
        }
        let config = micron_submission_config();
        let request = match PreparedMicronRequest::new(
            prepared.target.clone(),
            prepared.values.clone(),
            config,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.message = Some(format!("Micron request cannot be sent: {error}"));
                return;
            },
        };
        editor.prepared = None;
        let source_binding = editor.source.clone();
        let address_binding = editor.address.clone();
        micron.result = Some("Sending reviewed Micron request…".into());
        micron.response = None;
        let (tx, rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        micron.receiver = Some(rx);
        micron.cancel = Some(cancel_tx);
        let id = micron.next;
        micron.next = micron.next.wrapping_add(1);
        micron.active = Some((id, source_binding, address_binding));
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| {
                    runtime.block_on(async move {
                        tokio::select! {
                            result = request.send(interface, config) => result,
                            _ = cancel_rx => Err("Micron request cancelled locally; remote outcome may be unknown.".into()),
                        }
                    })
                });
            let _ = tx.send((id, result));
            wake.wake();
        });
    }

    /// Prepare `key`'s saved file for Titan. The document and its page's open
    /// metadata must be saved first.
    fn prepare_titan(&mut self, key: DocKey) {
        let entry = self.entry_for(key);
        if entry.site.busy() {
            self.message = Some("A submission is already sending.".into());
            return;
        }
        if let Some(error) = &self.scroll.titan_submission_error {
            self.message = Some(format!("Titan upload is unavailable: {error}"));
            return;
        }
        if entry.document.snapshot().dirty || self.page_draft_dirty(key) {
            self.message = Some("Save source and metadata before preparing Titan upload.".into());
            return;
        }
        let Some(path) = entry
            .document
            .session()
            .source_path()
            .map(Path::to_path_buf)
        else {
            self.message = Some("Save the source file before preparing Titan upload.".into());
            return;
        };
        let submission = &mut self.entry_mut_for(key).site.submission;
        match PreparedSubmission::from_saved_file(
            &path,
            submission.target.text(),
            submission.mime.text(),
        ) {
            Ok(p) if p.target().starts_with("titan://") => {
                submission.prepared = Some(p);
                submission.result = None;
                submission.response = None
            },
            Ok(_) => self.message = Some("Titan preparation requires a titan:// target.".into()),
            Err(e) => self.message = Some(format!("Prepare failed: {e}")),
        }
    }

    fn prepare_spartan(&mut self, key: DocKey) {
        if self.entry_for(key).site.busy() {
            self.message = Some("A submission is already sending.".into());
            return;
        }
        let submission = &mut self.entry_mut_for(key).site.submission;
        submission.token = TextInput::default();
        match PreparedSubmission::from_body(
            submission.target.text(),
            submission.mime.text(),
            submission.body.text().as_bytes().to_vec(),
        ) {
            Ok(p) if p.target().starts_with("spartan://") => {
                submission.prepared = Some(p);
                submission.result = None;
                submission.response = None
            },
            Ok(_) => {
                self.message = Some("Spartan preparation requires a spartan:// target.".into())
            },
            Err(e) => self.message = Some(format!("Prepare failed: {e}")),
        }
    }

    fn send_submission(&mut self, key: DocKey) {
        if self.entry_for(key).site.busy() {
            self.message = Some("A submission is already sending.".into());
            return;
        }
        let Some(wake) = self.scroll.submission_wake.clone() else {
            self.message = Some("Submission worker is unavailable.".into());
            return;
        };
        let submission = &mut self.entry_mut_for(key).site.submission;
        let (prepared, token) = match submission.take_for_send() {
            Ok(send) => send,
            Err(error) => {
                self.message = Some(error);
                return;
            },
        };
        submission.result = Some("Sending reviewed bytes…".into());
        let (tx, rx) = mpsc::channel();
        submission.receiver = Some(rx);
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
                .and_then(|rt| rt.block_on(prepared.send(token)));
            let _ = tx.send(result);
            wake.wake();
        });
    }

    /// A Spartan prompt link picked in a preview: fill that document's
    /// composer with its target and bring up a Submit tile pinned to it.
    fn select_spartan_prompt_for(&mut self, document: DocKey, target: String) {
        self.entry_mut_for(document)
            .site
            .submission
            .select_spartan_prompt(target);
        if self.focused_key() == Some(document) {
            self.open_submit();
            return;
        }
        let open = self
            .docs
            .readings()
            .find(|(_, kind, pinned)| *kind == ReadingKind::Submit && *pinned == Some(document))
            .map(|(tile, _, _)| tile);
        if let Some(tile) = open {
            self.docs.activate(tile);
        } else {
            self.docs.open_reading(
                ReadingKind::Submit,
                Some(document),
                crate::workspace::reading_title(ReadingKind::Submit),
            );
        }
    }

    #[cfg(test)]
    fn select_spartan_prompt(&mut self, target: String) {
        if let Some(document) = self.focused_key() {
            self.select_spartan_prompt_for(document, target);
        }
    }

    /// Bring up the Submit tile that follows the focused document, opening it
    /// if none is open.
    pub(crate) fn open_submit(&mut self) {
        match self.docs.following_reading(ReadingKind::Submit) {
            Some(tile) => self.docs.activate(tile),
            None => {
                self.docs.open_reading(
                    ReadingKind::Submit,
                    None,
                    crate::workspace::reading_title(ReadingKind::Submit),
                );
            },
        }
    }

    /// The focused document's site page, if it is one.
    pub(crate) fn site_page(&self) -> Option<&SitePage> {
        self.entry().site.page.as_ref()
    }

    /// Whether `page`'s open metadata in `site` has unsaved edits.
    pub(crate) fn draft_dirty(&self, site: SiteKey, page: &str) -> bool {
        self.scroll
            .site(site)
            .and_then(|entry| entry.drafts.get(page))
            .is_some_and(MetadataDraft::dirty)
    }

    /// Whether the open metadata of `key`'s page has unsaved edits.
    fn page_draft_dirty(&self, key: DocKey) -> bool {
        self.entry_for(key)
            .site
            .page
            .as_ref()
            .is_some_and(|page| self.draft_dirty(page.site, &page.name))
    }

    /// The open pages of `site` with unsaved changes, in the order they opened.
    pub(crate) fn site_dirty_pages(&self, site: SiteKey) -> Vec<DocKey> {
        self.docs
            .docs()
            .filter(|(_, entry)| {
                entry
                    .site
                    .page
                    .as_ref()
                    .is_some_and(|page| page.site == site)
                    && entry.document.snapshot().dirty
            })
            .map(|(key, _)| key)
            .collect()
    }

    /// The metadata tiles of `site` with unsaved edits, in tile order.
    pub(crate) fn site_dirty_drafts(&self, site: SiteKey) -> Vec<TileId> {
        self.docs
            .metadata_tiles(site)
            .into_iter()
            .filter(|tile| {
                matches!(self.docs.role(*tile), Some(TileRole::Metadata { page, .. }) if self.draft_dirty(site, page))
            })
            .collect()
    }

    /// Whether `site` holds anything unsaved: a page or a metadata draft.
    fn site_unsaved(&self, site: SiteKey) -> bool {
        !self.site_dirty_pages(site).is_empty() || !self.site_dirty_drafts(site).is_empty()
    }

    /// Open metadata tiles with unsaved edits, in tile order.
    pub(crate) fn dirty_metadata(&self) -> Vec<TileId> {
        let mut tiles: Vec<TileId> = self
            .scroll
            .sites
            .keys()
            .flat_map(|site| self.docs.metadata_tiles(*site))
            .filter(|tile| {
                matches!(self.docs.role(*tile), Some(TileRole::Metadata { site, page }) if self.draft_dirty(*site, page))
            })
            .collect();
        tiles.sort_by_key(|tile| tile.0);
        tiles
    }

    /// Open `name`'s metadata tile in `site`, its draft starting from the
    /// manifest's saved values, or activate the one already open.
    pub(crate) fn open_metadata(&mut self, site: SiteKey, name: &str) {
        let Some(entry) = self.scroll.sites.get_mut(&site) else {
            self.message = Some("Open a site first".into());
            return;
        };
        let Some(page) = entry
            .site
            .config
            .pages
            .iter()
            .find(|page| page.path == name)
        else {
            self.message = Some("Page missing".into());
            return;
        };
        entry
            .drafts
            .entry(name.to_owned())
            .or_insert_with(|| MetadataDraft::new(page));
        self.docs
            .open_metadata(site, name, format!("Metadata · {name}"));
    }

    pub(crate) fn save_metadata(&mut self, site: SiteKey, name: &str) -> Result<(), String> {
        let entry = self
            .scroll
            .sites
            .get_mut(&site)
            .ok_or("Open a site first")?;
        let draft = entry
            .drafts
            .get_mut(name)
            .ok_or("Open this page's metadata first")?;
        let values: [String; 6] = std::array::from_fn(|i| draft.fields[i].text().to_owned());
        let updated = Page {
            path: name.to_owned(),
            author: values[0].clone(),
            language: values[1].clone(),
            classification: values[2]
                .parse()
                .map_err(|_| "Classification must be 0–9")?,
            published: values[3].clone(),
            modified: values[4].clone(),
            abstract_source: values[5].clone(),
        };
        let manifest = &mut entry.site;
        let index = manifest
            .config
            .pages
            .iter()
            .position(|each| each.path == name)
            .ok_or("Page missing")?;
        let previous = std::mem::replace(&mut manifest.config.pages[index], updated);
        if let Err(error) = manifest.save_config_with(|path, before, after| {
            knot_document::write_if_distinct(path, before, after).map(|_| ())
        }) {
            manifest.config.pages[index] = previous;
            return Err(error);
        }
        draft.baseline = values;
        Ok(())
    }

    fn discard_metadata(&mut self, site: SiteKey, name: &str) {
        if let Some(draft) = self
            .scroll
            .sites
            .get_mut(&site)
            .and_then(|entry| entry.drafts.get_mut(name))
        {
            draft.discard();
        }
    }

    /// Close a metadata tile without asking; its draft goes with it.
    pub(crate) fn close_metadata(&mut self, tile: TileId) {
        if let Some(TileRole::Metadata { site, page }) = self.docs.role(tile).cloned()
            && let Some(entry) = self.scroll.sites.get_mut(&site)
        {
            entry.drafts.remove(&page);
        }
        self.docs.close(tile);
    }

    /// Bind every open document to the site page its file is.
    pub(crate) fn bind_site_pages(&mut self) {
        let scroll = &self.scroll;
        for (_, entry) in self.docs.docs_mut() {
            bind_site_page(scroll, entry);
        }
    }

    /// Bind one document, as after it opens or is saved under a new name.
    pub(crate) fn bind_site_page_for(&mut self, key: DocKey) {
        let scroll = &self.scroll;
        if let Some(entry) = self.docs.doc_mut(key) {
            bind_site_page(scroll, entry);
        }
    }

    /// Create or open the site in the Site popover's folder, alongside every
    /// site already open in the window.
    fn enter_site(&mut self, create: bool) {
        let root = std::path::PathBuf::from(self.scroll.folder.text());
        let result = if create {
            Site::create_for(&root, self.scroll.format)
        } else {
            Site::open(&root)
        };
        match result.and_then(|site| {
            let path = site.page_path(site.config.format.index_file())?;
            Ok((site, path))
        }) {
            Ok((site, path)) => {
                self.scroll.format = site.config.format;
                self.scroll.popover.close();
                self.hold_site(site);
                let _ = self.open_path(path);
            },
            Err(error) => self.message = Some(format!("Site: {error}")),
        }
    }

    /// Hold `site` alongside the other sites, bind open documents that are its
    /// pages, and show its tile. Opening the same canonical folder activates
    /// the entry already held rather than duplicating its authority.
    pub(crate) fn hold_site(&mut self, site: Site) -> SiteKey {
        let (key, inserted) = self.scroll.insert(site);
        self.bind_site_pages();
        let name = self
            .scroll
            .site(key)
            .map(SiteEntry::name)
            .unwrap_or_default();
        self.docs.open_site_tile(key, name.clone());
        if !inserted {
            self.message = Some(format!("Site {name} is already open."));
        }
        key
    }

    /// Close `site`'s metadata tiles; their drafts went with the site.
    fn close_metadata_tiles(&mut self, site: SiteKey) {
        for tile in self.docs.metadata_tiles(site) {
            self.docs.close(tile);
        }
    }

    /// Close `site`, asking first when it holds anything unsaved: one prompt
    /// lists its unsaved pages and metadata.
    pub(crate) fn request_close_site(&mut self, site: SiteKey) {
        if self.site_unsaved(site) {
            self.pending = Some(crate::workspace::PendingAction::CloseSite(site));
        } else {
            self.close_site_now(site);
        }
    }

    /// Close `site` without asking: its page tabs, its metadata tiles and its
    /// tile close, and its server stops.
    pub(crate) fn close_site_now(&mut self, site: SiteKey) {
        let name = self
            .scroll
            .site(site)
            .map(SiteEntry::name)
            .unwrap_or_default();
        let pages: Vec<DocKey> = self
            .docs
            .docs()
            .filter(|(_, entry)| {
                entry
                    .site
                    .page
                    .as_ref()
                    .is_some_and(|page| page.site == site)
            })
            .map(|(key, _)| key)
            .collect();
        for key in pages {
            self.close_document(key);
        }
        self.close_metadata_tiles(site);
        if let Some(tile) = self.docs.site_tile(site) {
            self.docs.close(tile);
        }
        self.scroll.remove(site);
        self.bind_site_pages();
        self.message = Some(format!("Closed site {name}."));
    }

    /// Open the page `name` of `site`, or activate the tab already showing it.
    pub(crate) fn open_site_page(&mut self, site: SiteKey, name: &str) {
        let path = self
            .scroll
            .site(site)
            .ok_or("Open a site first".to_owned())
            .and_then(|entry| entry.site.page_path(name));
        match path {
            Ok(path) => {
                self.open_path(path);
            },
            Err(error) => self.message = Some(error),
        }
    }

    /// Publish `site`'s saved pages to its local server, starting it on the
    /// site's port the first time. Refused while the site holds anything
    /// unsaved, since publication reads saved files.
    pub(crate) fn publish_site(&mut self, site: SiteKey) {
        if self.site_unsaved(site) {
            self.message = Some("Save source and metadata before publishing locally.".into());
            return;
        }
        let result = (|| {
            let entry = self
                .scroll
                .sites
                .get_mut(&site)
                .ok_or("Open a site first")?;
            let publication = entry.site.publication()?;
            let count = publication.page_count();
            if let Some(server) = &entry.server {
                server.replace(publication)?;
            } else {
                let port = entry
                    .port
                    .text()
                    .parse::<u16>()
                    .map_err(|_| "Port must be 0–65535 (0 chooses a free port)")?;
                entry.server = Some(LocalServer::start(publication, port)?);
            }
            entry.publication_number += 1;
            Ok::<_, String>(format!(
                "Published {count} saved pages locally, revision {}. {}",
                entry.publication_number,
                entry.server.as_ref().unwrap().url()
            ))
        })();
        self.message = Some(result.unwrap_or_else(|e| format!("Publication failed: {e}")));
    }

    pub(crate) fn site_popover_event(&mut self, event: PopoverEvent) {
        self.scroll.popover.apply(event);
    }
}

/// Bind `entry` to the open site page its file is. A session's source path is
/// canonical, since knot-document opens and saves through the canonical path,
/// as `Site::page_path` is.
fn bind_site_page(scroll: &ScrollWorkspace, entry: &mut DocumentEntry) {
    entry.site.page = entry
        .document
        .session()
        .source_path()
        .and_then(|path| scroll.page_at(path))
        .map(|(site, page)| SitePage {
            site,
            name: page.path.clone(),
        });
}

fn input(
    label: &'static str,
    id: &'static str,
    get: fn(&mut DesktopState) -> &mut TextInput,
) -> DesktopView {
    Box::new(
        el(
            "label",
            (
                label,
                lens(|input: &mut TextInput| text_field_typed(input), get),
            ),
        )
        .attr("id", id),
    )
}

fn edit_password(input: &mut TextInput, event: KeyEvent) {
    input.apply_key(&event, TextFieldMode::SingleLine);
}

fn password_field(
    input: &TextInput,
) -> impl View<TextInput, (), GenetCtx, Element = GenetElement> + use<> {
    let shown = input
        .display()
        .chars()
        .map(|character| if character == '|' { '|' } else { '•' })
        .collect::<String>();
    on_key(el("input", shown).attr("type", "password"), edit_password)
}

/// The site and page a metadata tile shows.
fn metadata_key(state: &DesktopState, tile: TileId) -> (SiteKey, String) {
    match state.docs.role(tile) {
        Some(TileRole::Metadata { site, page }) => (*site, page.clone()),
        _ => unreachable!("metadata fields render only in a metadata tile"),
    }
}

/// The fields of a metadata tile's draft. They render only while the tile and
/// its draft exist, so a lens or text route over one always finds them.
pub(crate) fn draft_fields(state: &mut DesktopState, tile: TileId) -> &mut [TextInput; 6] {
    let (site, page) = metadata_key(state, tile);
    &mut state
        .scroll
        .sites
        .get_mut(&site)
        .and_then(|entry| entry.drafts.get_mut(&page))
        .expect("a metadata tile holds its draft")
        .fields
}

/// One field of a metadata tile's draft, to read.
pub(crate) fn draft_field(state: &DesktopState, tile: TileId, index: usize) -> &TextInput {
    let (site, page) = metadata_key(state, tile);
    &state
        .scroll
        .site(site)
        .and_then(|entry| entry.drafts.get(&page))
        .expect("a metadata tile holds its draft")
        .fields[index]
}

/// A site page's metadata form, as its own tile in the document stack. Its
/// fields carry their index by class and the tile by attribute, so two
/// metadata tiles never share an id.
pub(crate) fn metadata_view(
    state: &DesktopState,
    tile: TileId,
    site: SiteKey,
    page: &str,
) -> DesktopView {
    let Some(entry) = state.scroll.site(site) else {
        return Box::new(span("This page's site is closed.").attr("class", "knot-metadata"));
    };
    let Some(draft) = entry.drafts.get(page) else {
        return Box::new(span("This page's metadata is closed.").attr("class", "knot-metadata"));
    };
    let format = entry.site.config.format;
    let fields = metadata_indices(format)
        .iter()
        .map(|&i| {
            (
                i,
                el(
                    "label",
                    (
                        LABELS[i],
                        lens(
                            move |input: &mut TextInput| {
                                if i == 5 {
                                    textarea_typed(input)
                                } else {
                                    text_field_typed(input)
                                }
                            },
                            move |state: &mut DesktopState| &mut draft_fields(state, tile)[i],
                        ),
                    ),
                )
                .attr("class", format!("knot-meta-field knot-meta-field-{i}")),
            )
        })
        .collect::<Vec<_>>();
    let explanation = match format {
        SiteFormat::Scroll => {
            "Publication metadata for this page. The abstract is separate native Scrolltext and needs a # Title."
        },
        SiteFormat::Gemini => {
            "Gemini publication metadata for this page. Language is separate from native Gemtext source."
        },
        SiteFormat::Spartan | SiteFormat::Micron => {
            "This site format has no supported page metadata controls."
        },
    };
    let save_name = page.to_owned();
    let discard_name = page.to_owned();
    Box::new(
        el(
            "section",
            (
                el("h2", format!("Metadata · {page}")),
                span(explanation),
                el("div", Keyed::new(fields)).attr("class", "knot-scroll-fields"),
                button("Save metadata", move |state: &mut DesktopState, _| {
                    state.message = Some(
                        state
                            .save_metadata(site, &save_name)
                            .map(|_| "Metadata saved. Existing publication is unchanged.".into())
                            .unwrap_or_else(|e| e),
                    );
                }),
                button(
                    "Discard metadata edits",
                    move |state: &mut DesktopState, _| {
                        state.discard_metadata(site, &discard_name);
                    },
                ),
                span(if draft.dirty() {
                    "Unsaved metadata"
                } else {
                    "Metadata saved"
                }),
            ),
        )
        .attr("class", "knot-metadata")
        .attr("data-knot-metadata", tile.0.to_string()),
    )
}

/// The composer field a text route names by index: target, MIME, body or
/// token, in `key`'s composer.
pub(crate) fn composer_text_mut(
    state: &mut DesktopState,
    key: DocKey,
    field: usize,
) -> &mut TextInput {
    let submission = &mut state.entry_mut_for(key).site.submission;
    match field {
        0 => &mut submission.target,
        1 => &mut submission.mime,
        2 => &mut submission.body,
        _ => &mut submission.token,
    }
}

/// The composer field a text route names by index, to read.
pub(crate) fn composer_text(state: &DesktopState, key: DocKey, field: usize) -> &TextInput {
    let submission = &state.entry_for(key).site.submission;
    match field {
        0 => &submission.target,
        1 => &submission.mime,
        2 => &submission.body,
        _ => &submission.token,
    }
}

/// The classes of the composer's fields, in the order text routes index them.
pub(crate) const COMPOSER_FIELDS: [&str; 4] = [
    "knot-submission-target",
    "knot-submission-mime",
    "knot-spartan-body",
    "knot-submission-token",
];

/// A labelled one-line composer field over the text `get` finds.
fn composer_field(
    label: &'static str,
    class: &'static str,
    get: impl Fn(&mut DesktopState) -> &mut TextInput + 'static,
) -> DesktopView {
    Box::new(
        el(
            "label",
            (
                label,
                lens(|input: &mut TextInput| text_field_typed(input), get),
            ),
        )
        .attr("class", class),
    )
}

/// A labelled password field over the text `get` finds; it never paints its
/// value.
fn composer_password(
    label: &'static str,
    class: &'static str,
    get: impl Fn(&mut DesktopState) -> &mut TextInput + 'static,
) -> DesktopView {
    Box::new(
        el(
            "label",
            (
                label,
                lens(|input: &mut TextInput| password_field(input), get),
            ),
        )
        .attr("class", class),
    )
}

/// A document's Titan or Spartan composer, as a reading tile that follows the
/// focused document or is pinned to one. The document's key rides on the
/// tile, so its fields take text for that document, not the focused one.
pub(crate) fn submit_view(state: &DesktopState, key: DocKey) -> DesktopView {
    let entry = state.entry_for(key);
    let submission = &entry.site.submission;
    let busy = entry.site.busy();
    let review: DesktopView = if let Some(p) = submission.prepared.as_ref() {
        let is_titan = p.target().starts_with("titan://");
        let send_action: DesktopView = if busy {
            Box::new(span("Sending reviewed bytes…"))
        } else {
            Box::new(button(
                "Send reviewed bytes",
                move |s: &mut DesktopState, _| s.send_submission(key),
            ))
        };
        Box::new(
            el(
                "section",
                (
                    span(format!(
                        "Reviewed submission: {} · {} · {} bytes · {}",
                        p.target(),
                        p.mime(),
                        p.byte_len(),
                        p.digest()
                    )),
                    el("pre", String::from_utf8_lossy(p.body()).into_owned()),
                    if is_titan {
                        composer_password("Titan token", "knot-submission-token", move |s| {
                            &mut s.entry_mut_for(key).site.submission.token
                        })
                    } else {
                        Box::new(span(
                            "Spartan sends this reviewed body without a Titan token.",
                        ))
                    },
                    send_action,
                    button(
                        "Cancel reviewed submission",
                        move |s: &mut DesktopState, _| {
                            s.entry_mut_for(key).site.submission.discard()
                        },
                    ),
                ),
            )
            .attr("class", "knot-submission-review"),
        )
    } else if busy {
        Box::new(
            el("section", span("Sending reviewed bytes…")).attr("class", "knot-submission-review"),
        )
    } else {
        Box::new(el("div", ()))
    };
    let titan_prepare: DesktopView = if busy {
        Box::new(span("Sending reviewed bytes…"))
    } else if let Some(error) = &state.scroll.titan_submission_error {
        Box::new(span(format!("Titan upload disabled: {error}")))
    } else {
        Box::new(button(
            "Prepare saved source for Titan",
            move |s: &mut DesktopState, _| s.prepare_titan(key),
        ))
    };
    let spartan_prepare: DesktopView = if busy {
        Box::new(span("Spartan preparation is unavailable while sending."))
    } else {
        Box::new(button(
            "Prepare Spartan body",
            move |s: &mut DesktopState, _| s.prepare_spartan(key),
        ))
    };
    let composer = el(
        "section",
        (
            composer_field("Submission target", "knot-submission-target", move |s| {
                &mut s.entry_mut_for(key).site.submission.target
            }),
            composer_field("MIME", "knot-submission-mime", move |s| {
                &mut s.entry_mut_for(key).site.submission.mime
            }),
            titan_prepare,
            el(
                "label",
                (
                    "Spartan body",
                    lens(
                        |input: &mut TextInput| textarea_typed(input),
                        move |s: &mut DesktopState| &mut s.entry_mut_for(key).site.submission.body,
                    ),
                ),
            )
            .attr("class", "knot-spartan-body"),
            spartan_prepare,
            span("Preparing is local only. Sending is a separate action over the reviewed bytes."),
        ),
    )
    .attr("class", "knot-submission");
    Box::new(
        el(
            "div",
            (
                composer,
                review,
                span(submission.result.clone().unwrap_or_default())
                    .attr("class", "knot-submission-status"),
                submission
                    .response
                    .as_ref()
                    .map(|body| {
                        Box::new(el("pre", body.clone()).attr("class", "knot-submission-response"))
                            as DesktopView
                    })
                    .unwrap_or_else(|| Box::new(el("div", ()))),
            ),
        )
        .attr("class", "knot-submit")
        .attr("data-knot-submission", key.0.to_string()),
    )
}

/// An open site's tile, in the left stack: its pages, each opening its tab or
/// its metadata, and its publishing. Its key rides on the tile, so its port
/// field takes text for this site.
pub(crate) fn site_view(state: &DesktopState, site: SiteKey) -> DesktopView {
    let Some(entry) = state.scroll.site(site) else {
        return Box::new(span("This site is closed.").attr("class", "knot-site-tile"));
    };
    let rows = entry
        .site
        .config
        .pages
        .iter()
        .enumerate()
        .map(|(i, page)| {
            let open = page.path.clone();
            let metadata = page.path.clone();
            (
                i,
                el(
                    "div",
                    (
                        button(page.path.clone(), move |state: &mut DesktopState, _| {
                            state.open_site_page(site, &open)
                        }),
                        button("Metadata", move |state: &mut DesktopState, _| {
                            state.open_metadata(site, &metadata)
                        })
                        .attr("aria-label", format!("Metadata of {}", page.path)),
                    ),
                )
                .attr("class", "knot-site-page"),
            )
        })
        .collect::<Vec<_>>();
    let serving = entry
        .server
        .as_ref()
        .map(|server| {
            format!(
                "{} · revision {} · saved snapshot",
                server.url(),
                entry.publication_number
            )
        })
        .unwrap_or_else(|| {
            "Not published. Save writes drafts; Publish locally serves a saved snapshot over loopback when this site format has a local server.".into()
        });
    Box::new(
        el(
            "section",
            (
                el("h2", entry.name()),
                span(format!(
                    "{} site · {}",
                    entry.site.config.format.label(),
                    crate::workspace::display_path(entry.site.root())
                ))
                .attr("class", "knot-site-folder"),
                el("nav", Keyed::new(rows))
                    .attr("class", "knot-site-pages")
                    .attr("aria-label", "Pages"),
                el(
                    "div",
                    button("Upload / submit", |state: &mut DesktopState, _| {
                        state.open_submit()
                    }),
                )
                .attr("class", "knot-scroll-controls"),
                el(
                    "div",
                    (
                        el(
                            "label",
                            (
                                "Local port",
                                lens(
                                    |input: &mut TextInput| text_field_typed(input),
                                    move |state: &mut DesktopState| {
                                        site_port_field_mut(state, site)
                                    },
                                ),
                            ),
                        )
                        .attr("class", "knot-site-port"),
                        button("Publish locally", move |state: &mut DesktopState, _| {
                            state.publish_site(site)
                        }),
                        button("Stop serving", move |state: &mut DesktopState, _| {
                            state.scroll.stop_serving(site);
                            state.message = Some("Local serving stopped.".into());
                        }),
                    ),
                )
                .attr("class", "knot-scroll-controls"),
                el(
                    "footer",
                    (
                        span(serving).attr("class", "knot-site-serving"),
                        button("Close site", move |state: &mut DesktopState, _| {
                            state.request_close_site(site)
                        }),
                    ),
                )
                .attr("class", "knot-site-footer"),
            ),
        )
        .attr("class", "knot-site-tile")
        .attr("data-knot-site", site.0.to_string()),
    )
}

/// A site tile's port field. It renders only while its site is open, so a
/// lens or text route over it always finds it.
pub(crate) fn site_port_field_mut(state: &mut DesktopState, site: SiteKey) -> &mut TextInput {
    state
        .scroll
        .site_port_mut(site)
        .expect("a site tile's port renders only while its site is open")
}

/// A site tile's port field, to read.
pub(crate) fn site_port_field(state: &DesktopState, site: SiteKey) -> &TextInput {
    state
        .scroll
        .site_port(site)
        .expect("a site tile's port renders only while its site is open")
}

/// The command row's Site popover: the folder, the format, Create and Open.
pub(crate) fn site_popover(state: &DesktopState) -> DesktopView {
    site_popover_with_hidden_trigger(state, false)
}

pub(crate) fn site_popover_with_hidden_trigger(
    state: &DesktopState,
    hidden_trigger: bool,
) -> DesktopView {
    let format = state.scroll.format;
    let mut model = Popover::new("Site", &state.scroll.popover)
        .with_placement(PopoverPlacement::BelowStart)
        .with_trigger_attr("id", "knot-site");
    if hidden_trigger {
        model = model
            .with_trigger_attr("style", "display:none")
            .with_trigger_attr("aria-hidden", "true")
            .with_trigger_attr("tabindex", "-1");
    }
    Box::new(popover(
        model,
        |state: &mut DesktopState, event| state.site_popover_event(event),
        move || {
            let format_picker = [
                SiteFormat::Scroll,
                SiteFormat::Gemini,
                SiteFormat::Spartan,
                SiteFormat::Micron,
            ]
            .into_iter()
            .map(|candidate| {
                (
                    candidate as usize,
                    button(candidate.label(), move |s: &mut DesktopState, _| {
                        s.scroll.format = candidate;
                        if let Some(port) = candidate.default_port() {
                            s.scroll.port = TextInput::new(port.to_string());
                        }
                    })
                    .attr("aria-pressed", (format == candidate).to_string()),
                )
            })
            .collect::<Vec<_>>();
            Some(Box::new(
                el(
                    "div",
                    (
                        input("Site folder", "knot-scroll-folder", |s| {
                            &mut s.scroll.folder
                        }),
                        el("span", Keyed::new(format_picker))
                            .attr("class", "knot-site-format-picker"),
                        button("Create site", |s: &mut DesktopState, _| s.enter_site(true)),
                        button("Open site", |s: &mut DesktopState, _| s.enter_site(false)),
                    ),
                )
                .attr("class", "knot-site-popover"),
            ) as DesktopView)
        },
    ))
}

fn inline(items: &[InlineSpan], document: DocKey) -> DesktopView {
    let children = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let view: DesktopView = match item {
                InlineSpan::Text(text) => Box::new(span(text.clone())),
                InlineSpan::Presented {
                    presentation,
                    spans,
                } => Box::new(el("span", inline(spans, document)).attr(
                    "style",
                    crate::document_preview::inline_presentation_css(presentation),
                )),
                InlineSpan::Code(text) => Box::new(el("code", text.clone())),
                InlineSpan::Emphasis(items) => Box::new(
                    el("em", inline(items, document)).attr("class", "knot-preview-emphasis"),
                ),
                InlineSpan::Strong(items) => Box::new(
                    el("strong", inline(items, document)).attr("class", "knot-preview-strong"),
                ),
                InlineSpan::Link {
                    url,
                    spans,
                    predicate,
                    ..
                } => {
                    let destination = url.clone();
                    if inline_has_presentation(spans) {
                        Box::new(
                            button_with(
                                inline(spans, document),
                                move |state: &mut DesktopState, click| {
                                    click.stop_propagation();
                                    activate_preview_link(state, document, &destination);
                                },
                            )
                            .attr(
                                "title",
                                format!("{url} {}", predicate.as_deref().unwrap_or("")),
                            )
                            .attr("class", "knot-scroll-link"),
                        )
                    } else {
                        Box::new(
                            button(
                                inker::inline_text(spans),
                                move |state: &mut DesktopState, _| {
                                    activate_preview_link(state, document, &destination);
                                },
                            )
                            .attr(
                                "title",
                                format!("{url} {}", predicate.as_deref().unwrap_or("")),
                            )
                            .attr("class", "knot-scroll-link"),
                        )
                    }
                },
                InlineSpan::Submit { target, spans } => {
                    let label = inker::inline_text(spans);
                    let target = target.clone();
                    Box::new(
                        button(label, move |state: &mut DesktopState, _| {
                            state.select_spartan_prompt_for(document, target.clone());
                            state.message = Some(
                                "Enter a Spartan body, review it, then send explicitly.".into(),
                            );
                        })
                        .attr("class", "knot-spartan-submit"),
                    )
                },
                // Preview state only: opens folds and scrolls, never navigates.
                InlineSpan::InPage { target, spans } => {
                    let target = target.clone();
                    Box::new(
                        button_with(
                            inline(spans, document),
                            move |state: &mut DesktopState, click| {
                                click.stop_propagation();
                                state.follow_micron_in_page_for(document, &target);
                            },
                        )
                        .attr("class", "knot-scroll-link knot-preview-in-page"),
                    )
                },
                InlineSpan::LineBreak => Box::new(el("br", ())),
                InlineSpan::SoftBreak => Box::new(span(" ")),
            };
            (i, view)
        })
        .collect::<Vec<_>>();
    Box::new(el("span", Keyed::new(children)))
}

fn activate_preview_link(state: &mut DesktopState, document: DocKey, destination: &str) {
    if let Some((site, local_name)) = preview_manifest_page_for(state, document, destination) {
        state.open_site_page(site, local_name);
    } else {
        state.message = Some(format!(
            "Preview link: {destination}. Open with an independent client; this preview only navigates local site pages."
        ));
    }
}

fn inline_has_presentation(items: &[InlineSpan]) -> bool {
    items.iter().any(|item| match item {
        InlineSpan::Presented { .. } => true,
        InlineSpan::Emphasis(spans) | InlineSpan::Strong(spans) => inline_has_presentation(spans),
        _ => false,
    })
}

fn inline_has_link(items: &[InlineSpan]) -> bool {
    items.iter().any(|item| match item {
        InlineSpan::Link { .. } | InlineSpan::Submit { .. } | InlineSpan::InPage { .. } => true,
        InlineSpan::Presented { spans, .. }
        | InlineSpan::Emphasis(spans)
        | InlineSpan::Strong(spans) => inline_has_link(spans),
        _ => false,
    })
}

fn valid_preview_page_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.chars().any(|character| matches!(character, '#' | '?'))
}

fn preview_page_name<'a>(
    destination: &'a str,
    active_destination: Option<&str>,
) -> Option<&'a str> {
    let candidate = if let Some(name) = destination.strip_prefix(":/page/") {
        Some(name)
    } else if let Some((node, path)) = destination.split_once(':') {
        let expected = active_destination?;
        node.eq_ignore_ascii_case(expected)
            .then(|| path.strip_prefix("/page/"))?
    } else {
        Some(destination.trim_start_matches('/'))
    }?;
    valid_preview_page_name(candidate).then_some(candidate)
}

fn preview_manifest_page_for<'a>(
    state: &DesktopState,
    document: DocKey,
    destination: &'a str,
) -> Option<(SiteKey, &'a str)> {
    // A site can remain open while the editor displays an unrelated local
    // file. The manifest is an authority only while the current document is
    // one of that site's pages, and only for its own site.
    let site = state.entry_for(document).site.page.as_ref()?.site;
    let entry = state.scroll.site(site)?;
    let active_destination = entry
        .server
        .as_ref()
        .and_then(|server| server.nomadnet_destination())
        .map(|destination| destination.to_string());
    let name = preview_page_name(destination, active_destination.as_deref())?;
    entry.site.page_path(name).ok().map(|_| (site, name))
}

#[cfg(test)]
fn preview_manifest_page<'a>(
    state: &DesktopState,
    destination: &'a str,
) -> Option<(SiteKey, &'a str)> {
    preview_manifest_page_for(state, state.focused_key()?, destination)
}

fn blocks(items: &[Block], document: DocKey) -> DesktopView {
    let children = items
        .iter()
        .enumerate()
        .map(|(i, item)| (i, block(item, None, None, document)))
        .collect::<Vec<_>>();
    Box::new(el("div", Keyed::new(children)))
}

/// A top-level block's element carries its index; nested blocks carry none.
fn indexed<Seq>(
    element: El<Seq, DesktopState, ()>,
    index: Option<usize>,
) -> El<Seq, DesktopState, ()> {
    match index {
        Some(index) => element.attr(BLOCK_INDEX_ATTR, index.to_string()),
        None => element,
    }
}

/// The preview element of top-level block `index`, if rendered.
fn preview_block_node<D: LayoutDom>(dom: &D, node: D::NodeId, index: &str) -> Option<D::NodeId> {
    if dom.attribute(
        node,
        &Namespace::from(""),
        &LocalName::from(BLOCK_INDEX_ATTR),
    ) == Some(index)
    {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| preview_block_node(dom, child, index))
}

/// Finish an in-page jump queued by this dispatch. The dispatch already rebuilt
/// the preview with the target's folds open, so its element exists; the host
/// scrolls it to the top on the next layout.
pub(crate) fn scroll_to_micron_jump(
    ctx: &mut AppCtx<'_, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
) {
    let pending = ctx
        .runner
        .state()
        .docs
        .docs()
        .find(|(_, entry)| entry.micron_folds.jump.is_some())
        .map(|(key, _)| key);
    let Some(document) = pending else {
        return;
    };
    let mut jump = None;
    ctx.runner
        .update(|state| jump = state.entry_mut_for(document).micron_folds.jump.take());
    let Some(block) = jump.and_then(|target| target.block) else {
        return;
    };
    let node = {
        let dom = ctx.runner.dom();
        let dom = dom.borrow();
        fn preview_for<D: LayoutDom>(
            dom: &D,
            node: D::NodeId,
            document: DocKey,
        ) -> Option<D::NodeId> {
            if dom.attribute(
                node,
                &Namespace::from(""),
                &LocalName::from("data-knot-preview-document"),
            ) == Some(document.0.to_string().as_str())
            {
                return Some(node);
            }
            dom.dom_children(node)
                .find_map(|child| preview_for(dom, child, document))
        }
        preview_for(&*dom, dom.document(), document)
            .and_then(|preview| preview_block_node(&*dom, preview, &block.to_string()))
    };
    if let Some(node) = node {
        ctx.scroll_into_view(node, ScrollAlign::Start);
    }
}

/// A collapsible heading's toggle, resolved for one render.
struct FoldToggle {
    key: FoldKey,
    open: bool,
    marker: String,
}

/// A lowered document's top-level blocks. With fold state, closed extents are
/// skipped and collapsible headings toggle; navigation indices name top-level
/// blocks only, so nested blocks never carry a fold.
fn document_blocks(
    rendered: &EngineDocument,
    folds: Option<&MicronPreviewFolds>,
    document: DocKey,
) -> DesktopView {
    let navigation = &rendered.navigation;
    let folds = folds.filter(|_| navigation.is_current(&rendered.blocks));
    let hidden = folds.map_or_else(Vec::new, |folds| folds.folds.hidden(navigation));
    let keys = navigation.fold_keys();
    let children = rendered
        .blocks
        .iter()
        .enumerate()
        .filter(|(index, _)| !hidden.iter().any(|range| range.contains(index)))
        .map(|(index, item)| {
            let toggle = folds.and_then(|folds| {
                let fold = navigation
                    .folds
                    .iter()
                    .position(|fold| fold.heading == index)?;
                let open = folds.folds.is_open(navigation, fold);
                Some(FoldToggle {
                    key: keys[fold].clone(),
                    open,
                    marker: folds.markers.marker(open).to_owned(),
                })
            });
            (index, block(item, toggle.as_ref(), Some(index), document))
        })
        .collect::<Vec<_>>();
    Box::new(el("div", Keyed::new(children)))
}

fn block(
    item: &Block,
    fold: Option<&FoldToggle>,
    index: Option<usize>,
    document: DocKey,
) -> DesktopView {
    match item {
        Block::Presented {
            presentation,
            block: inner,
        } => Box::new(indexed(
            el(
                "div",
                el(
                    "div",
                    Keyed::new(vec![(0, block(inner, fold, None, document))]),
                ),
            )
            .attr(
                "style",
                crate::document_preview::block_presentation_css(presentation),
            ),
            index,
        )),
        Block::Heading { level, spans } => {
            let tag = match level {
                1 => "h1",
                2 => "h2",
                3 => "h3",
                4 => "h4",
                _ => "h5",
            };
            match fold {
                None => Box::new(indexed(el(tag, inline(spans, document)), index)),
                Some(toggle) => {
                    let key = toggle.key.clone();
                    if inline_has_link(spans) {
                        return Box::new(indexed(
                            el(
                                "div",
                                el(
                                    "div",
                                    (
                                        button(
                                            toggle.marker.clone(),
                                            move |state: &mut DesktopState, _| {
                                                state.toggle_micron_fold_for(document, &key)
                                            },
                                        )
                                        .attr("class", "knot-micron-fold-marker")
                                        .attr(
                                            "aria-label",
                                            format!("Toggle {}", inker::inline_text(spans)),
                                        )
                                        .attr(
                                            "aria-expanded",
                                            if toggle.open { "true" } else { "false" },
                                        ),
                                        inline(spans, document),
                                    ),
                                )
                                .attr("class", "knot-micron-heading-controls"),
                            )
                            .attr("role", "heading")
                            .attr("aria-level", level.to_string()),
                            index,
                        ));
                    }
                    Box::new(indexed(
                        el(
                            tag,
                            button_with(
                                (
                                    span(toggle.marker.clone())
                                        .attr("class", "knot-micron-fold-marker"),
                                    inline(spans, document),
                                ),
                                move |state: &mut DesktopState, _| {
                                    state.toggle_micron_fold_for(document, &key)
                                },
                            )
                            .attr("class", "knot-micron-fold")
                            .attr("aria-label", inker::inline_text(spans))
                            .attr("aria-expanded", if toggle.open { "true" } else { "false" }),
                        ),
                        index,
                    ))
                },
            }
        },
        Block::Paragraph { spans } => Box::new(indexed(el("p", inline(spans, document)), index)),
        Block::CodeBlock { text, .. } | Block::Preformatted { text } => {
            Box::new(indexed(el("pre", text.clone()), index))
        },
        Block::Quote { blocks: items } => {
            Box::new(indexed(el("blockquote", blocks(items, document)), index))
        },
        // Nematic retains ordered markers in the text; use one bullet list
        // to avoid assigning a second, invented set of ordered numbers.
        Block::List { items, .. } => Box::new(indexed(
            el(
                "ul",
                Keyed::new(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| (i, el("li", blocks(item, document))))
                        .collect::<Vec<_>>(),
                ),
            ),
            index,
        )),
        Block::Rule => Box::new(indexed(el("hr", ()), index)),
        Block::Table {
            alignments,
            header,
            rows,
        } => table_block(alignments, header, rows, index, document),
        Block::Badge { text } => Box::new(indexed(
            el("p", text.clone()).attr("class", "knot-preview-badge"),
            index,
        )),
        _ => Box::new(indexed(span("Unsupported preview block"), index)),
    }
}

fn table_cell(
    spans: &[InlineSpan],
    header: bool,
    alignment: Option<TableAlignment>,
    document: DocKey,
) -> DesktopView {
    let tag = if header { "th" } else { "td" };
    let mut cell = el(tag, inline(spans, document));
    if let Some(alignment) = alignment {
        let value = match alignment {
            TableAlignment::None | TableAlignment::Left => "left",
            TableAlignment::Center => "center",
            TableAlignment::Right => "right",
        };
        cell = cell.attr("style", format!("text-align:{value}"));
    }
    Box::new(cell)
}

fn table_block(
    alignments: &[TableAlignment],
    header: &[Vec<InlineSpan>],
    rows: &[Vec<Vec<InlineSpan>>],
    index: Option<usize>,
    document: DocKey,
) -> DesktopView {
    let header_view: DesktopView = if header.is_empty() {
        Box::new(el("thead", ()))
    } else {
        Box::new(el(
            "thead",
            el(
                "tr",
                Keyed::new(
                    header
                        .iter()
                        .enumerate()
                        .map(|(index, spans)| {
                            (
                                index,
                                table_cell(spans, true, alignments.get(index).copied(), document),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
        ))
    };
    let body_rows = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            (
                row_index,
                el(
                    "tr",
                    Keyed::new(
                        row.iter()
                            .enumerate()
                            .map(|(column, spans)| {
                                (
                                    column,
                                    table_cell(
                                        spans,
                                        false,
                                        alignments.get(column).copied(),
                                        document,
                                    ),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                ),
            )
        })
        .collect::<Vec<_>>();
    Box::new(indexed(
        el("table", (header_view, el("tbody", Keyed::new(body_rows)))),
        index,
    ))
}

fn micron_form_panel(
    state: &DesktopState,
    document: DocKey,
    source: &str,
    address: &str,
) -> DesktopView {
    let Some(editor) = state.entry_for(document).site.micron.form.as_ref() else {
        return Box::new(
            el(
                "section",
                button(
                    "Open Micron form controls",
                    move |state: &mut DesktopState, _| state.open_micron_form_for(document),
                ),
            )
            .attr("class", "knot-micron-form"),
        );
    };

    if state.entry_for(document).site.micron.receiver.is_some() {
        return Box::new(
            el(
                "section",
                (
                    span("Sending reviewed Micron request…"),
                    button(
                        "Cancel Micron request",
                        move |state: &mut DesktopState, _| {
                            state.entry_mut_for(document).site.micron.cancel()
                        },
                    ),
                ),
            )
            .attr("class", "knot-micron-form"),
        );
    }

    if editor.source != source || editor.address != address {
        return Box::new(
            el(
                "section",
                (
                    span("The source or page address changed. Reopen the form so the request matches the visible page."),
                    button("Discard stale form", move |state: &mut DesktopState, _| {
                        state.entry_mut_for(document).site.micron.close_form()
                    }),
                ),
            )
            .attr("class", "knot-micron-form"),
        );
    }

    let fields = editor
        .form
        .fields()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let view: DesktopView = match &field.kind {
                FieldKind::Text { name, masked, .. } => {
                    let label = if *masked {
                        format!("{name} (masked)")
                    } else {
                        name.clone()
                    };
                    let input: DesktopView = if *masked {
                        Box::new(lens(
                            |input: &mut TextInput| password_field(input),
                            move |state: &mut DesktopState| {
                                &mut state
                                    .entry_mut_for(document)
                                    .site
                                    .micron
                                    .form
                                    .as_mut()
                                    .expect("Micron form is present while it is rendered")
                                    .inputs[index]
                            },
                        ))
                    } else {
                        Box::new(lens(
                            |input: &mut TextInput| text_field_typed(input),
                            move |state: &mut DesktopState| {
                                &mut state
                                    .entry_mut_for(document)
                                    .site
                                    .micron
                                    .form
                                    .as_mut()
                                    .expect("Micron form is present while it is rendered")
                                    .inputs[index]
                            },
                        ))
                    };
                    Box::new(el("label", (label, input)))
                },
                FieldKind::Checkbox { name, value, .. } => {
                    let checked = field.checked();
                    let name = name.clone();
                    let value = value.clone();
                    Box::new(button(
                        format!("[{}] {name}: {value}", if checked { "x" } else { " " }),
                        move |state: &mut DesktopState, _| {
                            let result = state
                                .entry_mut_for(document)
                                .site
                                .micron
                                .set_checked(index, !checked);
                            if let Err(error) = result {
                                state.message = Some(format!("Micron form: {error}"));
                            }
                        },
                    ))
                },
                FieldKind::Radio { name, value, .. } => {
                    let checked = field.checked();
                    let name = name.clone();
                    let value = value.clone();
                    Box::new(button(
                        format!("[{}] {name}: {value}", if checked { "x" } else { " " }),
                        move |state: &mut DesktopState, _| {
                            if let Err(error) = state
                                .entry_mut_for(document)
                                .site
                                .micron
                                .set_checked(index, true)
                            {
                                state.message = Some(format!("Micron form: {error}"));
                            }
                        },
                    ))
                },
            };
            (index, view)
        })
        .collect::<Vec<_>>();
    let actions = editor
        .form
        .actions()
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let label = action.label.clone();
            (
                index,
                button(
                    format!("Prepare {label}"),
                    move |state: &mut DesktopState, _| {
                        state.prepare_micron_form_for(document, index)
                    },
                ),
            )
        })
        .collect::<Vec<_>>();
    let prepared_is_current = editor.prepared.as_ref().is_some_and(|prepared| {
        prepared.input_snapshot.len() == editor.inputs.len()
            && prepared
                .input_snapshot
                .iter()
                .zip(&editor.inputs)
                .all(|(before, input)| before == input.text())
    });
    let review: DesktopView = editor
        .prepared
        .as_ref()
        .filter(|_| prepared_is_current)
        .map(|prepared| {
            let values = prepared
                .values
                .iter()
                .map(|(name, value)| {
                    if prepared.masked.contains(name) {
                        format!("{name}=••••")
                    } else {
                        format!("{name}={value}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            Box::new(el(
                "section",
                (
                    span(format!("Prepared Micron request for {}", prepared.target)),
                    el("pre", values),
                    span("Send uses the separately configured KNOT_NOMADNET_TCP interface. A local :/ alias cannot be sent, and Knot's static publisher never becomes a request handler. KNOT_NOMADNET_TIMEOUT_SECS (default 30, 1–120) and KNOT_NOMADNET_MAX_RESPONSE_BYTES (default 4 MiB, up to 64 MiB) bound the request."),
                    button("Send reviewed Micron request", move |state: &mut DesktopState, _| {
                        state.send_micron_form_for(document)
                    }),
                    button("Discard prepared request", move |state: &mut DesktopState, _| {
                        if let Some(editor) = state
                            .entry_mut_for(document)
                            .site
                            .micron
                            .form
                            .as_mut()
                        {
                            editor.prepared = None;
                        }
                    }),
                ),
            )) as DesktopView
        })
        .unwrap_or_else(|| {
            if editor.prepared.is_some() {
                Box::new(span(
                    "Field values changed after review. Prepare the request again before it can be used.",
                ))
            } else {
                Box::new(el("div", ()))
            }
        });
    let result: DesktopView = state
        .entry_for(document)
        .site
        .micron
        .result
        .as_ref()
        .map(|message| Box::new(span(message.clone())) as DesktopView)
        .unwrap_or_else(|| Box::new(el("div", ())));
    let response: DesktopView = state
        .entry_for(document)
        .site
        .micron
        .response
        .as_ref()
        .map(|body| Box::new(el("pre", body.clone())) as DesktopView)
        .unwrap_or_else(|| Box::new(el("div", ())));
    Box::new(
        el(
            "section",
            (
                span("Micron form values are temporary and are never written into this page or its address."),
                el("div", Keyed::new(fields)).attr("class", "knot-micron-fields"),
                el("div", Keyed::new(actions)).attr("class", "knot-scroll-controls"),
                review,
                result,
                response,
                button("Close Micron form", move |state: &mut DesktopState, _| {
                    state.entry_mut_for(document).site.micron.close_form()
                }),
            ),
        )
        .attr("class", "knot-micron-form"),
    )
}

pub fn preview(state: &DesktopState, key: DocKey, tile: workbench::TileId) -> DesktopView {
    let source = state.surface_for(key).snapshot();
    if !matches!(
        source.format,
        knot_document::DocumentFormat::Scroll
            | knot_document::DocumentFormat::Gemtext
            | knot_document::DocumentFormat::Micron
    ) {
        return Box::new(el("div", ()));
    }
    let rendered = match source.format {
        knot_document::DocumentFormat::Scroll => nematic::ScrollEngine::new().render(
            &EngineInput::new(&source.source.address, &source.text)
                .with_content_type("text/scroll"),
        ),
        knot_document::DocumentFormat::Micron => lower_micron(&source.source.address, &source.text),
        knot_document::DocumentFormat::Gemtext => {
            let input = EngineInput::new(&source.source.address, &source.text)
                .with_content_type("text/gemini");
            if state
                .entry_for(key)
                .site
                .page
                .as_ref()
                .and_then(|page| state.scroll.site(page.site))
                .is_some_and(|entry| entry.site.config.format == SiteFormat::Spartan)
            {
                nematic::SpartanEngine::new().render(&input)
            } else {
                nematic::GemtextEngine::new().render(&input)
            }
        },
        _ => unreachable!("filtered above"),
    };
    let body: DesktopView = match rendered {
        Ok(document) => Box::new(el(
            "div",
            (
                document_blocks(
                    &document,
                    (source.format == knot_document::DocumentFormat::Micron)
                        .then_some(&state.entry_for(key).micron_folds),
                    key,
                ),
                span(if document.diagnostics.is_empty() {
                    String::new()
                } else {
                    format!("Rendering notes: {:?}", document.diagnostics)
                }),
            ),
        )),
        Err(error) => Box::new(span(error.to_string())),
    };
    Box::new(
        el(
            "div",
            (
                el(
                    "h2",
                    format!(
                        "{} preview · current source",
                        match source.format {
                            knot_document::DocumentFormat::Scroll => "Scroll",
                            knot_document::DocumentFormat::Gemtext => "Gemtext",
                            knot_document::DocumentFormat::Micron => "Micron",
                            _ => unreachable!("filtered above"),
                        }
                    ),
                ),
                body,
                if source.format == knot_document::DocumentFormat::Micron {
                    micron_form_panel(state, key, &source.text, &source.source.address)
                } else {
                    Box::new(el("div", ()))
                },
            ),
        )
        .attr("id", format!("knot-document-preview-{}", tile.0))
        .attr("class", "knot-document-preview knot-scroll-preview")
        .attr("data-knot-preview-document", key.0.to_string())
        .attr("role", "complementary")
        .attr("aria-label", "Document preview"),
    )
}

pub const CSS: &str = r#"
.knot-workspace { overflow:auto; }
.knot-native-site-mode .knot-source-wrapper { flex:1 1 auto; width:100%; }
.knot-native-site-mode .knot-document-body textarea { display:block; width:auto; min-width:0; min-height:260px; }
.knot-scroll-site input { min-height:32px; box-sizing:border-box; }
.knot-scroll-fields textarea { white-space:pre-wrap; min-height:80px; padding:8px; border:1px solid; background:transparent; color:inherit; }
.knot-scroll-site { padding: 8px; border-bottom: 1px solid #888; flex-shrink: 0; }
.knot-scroll-controls, .knot-scroll-pages, .knot-site-format-picker { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
.knot-scroll-fields { display: flex; flex-wrap: wrap; gap: 8px; }
.knot-scroll-fields label { display: flex; flex-direction: column; width: 230px; }
.knot-submission { display: flex; gap: 8px; flex-wrap: wrap; align-items: flex-start; margin-top: 8px; }
.knot-submission label { display: flex; flex-direction: column; min-width: 180px; }
.knot-submit { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.knot-metadata { display: flex; flex-direction: column; gap: 8px; padding: 12px; min-width: 0; }
.knot-spartan-body { display: flex; flex: 1 0 100%; flex-direction: column; min-width: 0; }
.knot-spartan-body textarea { display: block; box-sizing: border-box; width: 100%; min-width: 0; min-height: 120px; max-height: 180px; overflow: auto; padding: 8px; border: 1px solid; background: transparent; color: inherit; white-space: pre-wrap; }
.knot-submission-review { max-width: 100%; margin-top: 8px; }
.knot-submission-review pre { box-sizing: border-box; width: 100%; max-height: 220px; overflow: auto; white-space: pre-wrap; }
.knot-submission-status { display: block; min-height: 1.2em; margin-top: 4px; }
.knot-submission-response { box-sizing: border-box; width: 100%; max-height: 220px; overflow: auto; white-space: pre-wrap; }
.knot-micron-form { display: flex; flex-direction: column; gap: 8px; margin-top: 12px; padding: 8px; border: 1px solid currentColor; }
.knot-micron-fields { display: flex; flex-wrap: wrap; gap: 8px; align-items: flex-end; }
.knot-micron-fields label { display: flex; flex-direction: column; min-width: 180px; }
.knot-micron-form pre { box-sizing: border-box; width: 100%; max-height: 180px; overflow: auto; white-space: pre-wrap; }
.knot-scroll-preview { width:100%; min-width:0; box-sizing:border-box; padding:16px; }
.knot-site-footer { display:flex; flex-wrap:wrap; align-items:center; gap:8px; margin-top:8px; }
.knot-site-footer .knot-site-serving { flex:1 1 320px; min-width:0; overflow-wrap:anywhere; }
.knot-site-footer button { flex:0 0 auto; }
.knot-writing-area, .knot-scroll-preview { background: inherit; }
.knot-scroll-preview p { margin: 8px 0; }
.knot-scroll-preview pre { white-space: pre-wrap; }
.knot-preview-badge { display: block; margin: 8px 0; padding: 4px 8px; border: 1px solid currentColor; border-radius: 4px; }
.knot-preview-strong { font-weight: 700; }
.knot-preview-emphasis { font-style: italic; }
.knot-scroll-link { text-decoration: underline; }
.knot-micron-fold { display: block; width: 100%; text-align: left; }
.knot-micron-heading-controls { display:flex; align-items:baseline; gap:4px; }
#knot-scroll-folder input { width: 350px; }
#knot-scroll-port input { width: 70px; }
@media (max-width:700px) { #knot-scroll-folder input { width:220px; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DESKTOP_CSS, host_hooks, workspace::desktop_view};
    use cambium::TextCommand;
    use cambium_genet_winit_host::{
        CloseRequest, Harness, Init, KeyPress, NamedKey, WindowCommands,
    };
    use knot_document::{KnotDocumentIntentV1, KnotDocumentSession};
    use layout_dom_api::{LayoutDom, LocalName, Namespace};
    use taproot::Selector;

    /// The focused document's page name, if it is a site page.
    fn page_name(state: &DesktopState) -> Option<&str> {
        state.site_page().map(|page| page.name.as_str())
    }

    /// Set the open site's port.
    fn set_port(state: &mut DesktopState, port: &str) {
        let site = state.scroll.current_key().expect("a site is open");
        *state.scroll.site_port_mut(site).unwrap() = TextInput::new(port);
    }

    fn show_preview(state: &mut DesktopState) {
        state
            .docs
            .open_reading(ReadingKind::Preview, None, "Preview");
    }

    impl DesktopState {
        /// Open the page `name` of the open site.
        fn scroll_open_page(&mut self, name: &str) {
            let site = self.scroll.current_key().expect("a site is open");
            self.open_site_page(site, name);
        }

        /// Open the focused page's metadata tile.
        fn open_page_metadata(&mut self) {
            let page = self.site_page().expect("a site page has the focus");
            let (site, name) = (page.site, page.name.clone());
            self.open_metadata(site, &name);
        }
    }

    #[test]
    fn site_navigation_metadata_and_explicit_publication_keep_separate_authority() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("site");
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:test", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(root.to_string_lossy());
        state.enter_site(true);
        assert_eq!(
            state.document().snapshot().format,
            knot_document::DocumentFormat::Scroll
        );
        assert_eq!(page_name(&state), Some("index.scroll"));
        state.open_page_metadata();
        let site = state.site_page().unwrap().site;
        let index = state
            .docs
            .metadata_tile(site, "index.scroll")
            .expect("index.scroll's metadata tile");
        draft_fields(&mut state, index)[0] = TextInput::new("Writer");
        // The draft lives in its tile, so the focus moves on without it.
        state.scroll_open_page("about.scroll");
        assert_eq!(page_name(&state), Some("about.scroll"));
        state.publish_site(site);
        assert!(
            state.scroll.current_site().unwrap().server.is_none(),
            "published over unsaved metadata"
        );
        state.save_metadata(site, "index.scroll").unwrap();
        let manifest = std::fs::read_to_string(root.join(knot_site::CONFIG)).unwrap();
        assert!(manifest.contains("Writer"), "{manifest}");
        state.open_page_metadata();
        let about = state
            .docs
            .metadata_tile(site, "about.scroll")
            .expect("about.scroll's metadata tile");
        assert_eq!(draft_field(&state, about, 0).text(), "");
        assert_eq!(draft_field(&state, index, 0).text(), "Writer");
        set_port(&mut state, "0");
        state.publish_site(site);
        assert!(state.scroll.current_site().unwrap().server.is_some());
        assert_eq!(state.scroll.current_site().unwrap().publication_number(), 1);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "Unsaved ".into(),
            )))
            .unwrap();
        state.publish_site(site);
        assert_eq!(state.scroll.current_site().unwrap().publication_number(), 1);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Save)
            .unwrap();
        assert_eq!(state.scroll.current_site().unwrap().publication_number(), 1);
        state.publish_site(site);
        assert_eq!(state.scroll.current_site().unwrap().publication_number(), 2);
    }
    #[test]
    fn native_preview_panes_keep_the_theme_background_while_scrolling() {
        assert!(CSS.contains(".knot-writing-area, .knot-scroll-preview { background: inherit; }"));
    }

    #[test]
    fn native_site_selection_opens_gemtext_and_preserves_micron_as_raw_source() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:test", ""),
            WindowCommands::new(),
        );
        state.scroll.format = SiteFormat::Gemini;
        state.scroll.folder = TextInput::new(temp.path().join("gemini").to_string_lossy());
        state.enter_site(true);
        assert_eq!(
            state.document().snapshot().format,
            knot_document::DocumentFormat::Gemtext
        );
        assert_eq!(page_name(&state), Some("index.gmi"));
        assert_eq!(
            state.scroll.current_site().unwrap().site.config.format,
            SiteFormat::Gemini
        );

        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Save)
            .unwrap();
        state.scroll.format = SiteFormat::Micron;
        state.scroll.folder = TextInput::new(temp.path().join("micron").to_string_lossy());
        state.enter_site(true);
        assert_eq!(
            state.document().snapshot().format,
            knot_document::DocumentFormat::Micron
        );
        assert_eq!(page_name(&state), Some("index.mu"));
        assert_eq!(
            state.scroll.current_site().unwrap().site.config.format,
            SiteFormat::Micron
        );
    }
    #[test]
    fn micron_site_preview_renders_native_projection_and_diagnostics() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:micron", ""),
            WindowCommands::new(),
        );
        state.scroll.format = SiteFormat::Micron;
        state.scroll.folder = TextInput::new(temp.path().join("micron").to_string_lossy());
        state.enter_site(true);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
            .unwrap();
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                ">Heading\n---\nplain\n`[Local`:/page/next.mu]\n".into(),
            )))
            .unwrap();
        show_preview(&mut state);
        let host = desktop_harness_with(state);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let text = text_content(&dom, dom.document());
        assert!(text.contains("Micron preview · current source"), "{text}");
        assert!(text.contains("Heading"));
        assert!(text.contains("Local"));
        assert!(text.contains("Micron preview: some presentation or controls"));
        assert!(text.contains("Rendering notes:"));
    }

    #[test]
    fn micron_form_review_is_ephemeral_and_uses_the_declared_action_map() {
        let source = "`[Submit selected`0123456789abcdef0123456789abcdef:/capture`name|checks|fixed=ready]\nText: `<name`seed>\nChecks: `<?|checks|red|*`> Red\n`<?|checks|blue`> Blue\n";
        let mut micron = MicronRequest::default();
        micron
            .open_form(source.into(), "scratch:micron-form".into())
            .unwrap();
        micron.form.as_mut().unwrap().inputs[0] = TextInput::new("edited");
        micron.set_checked(1, false).unwrap();
        micron.set_checked(2, true).unwrap();
        assert!(micron.prepare("changed", "scratch:micron-form", 0).is_err());
        assert!(micron.prepare(source, "scratch:other-address", 0).is_err());
        micron.prepare(source, "scratch:micron-form", 0).unwrap();
        let prepared = micron.form.as_ref().unwrap().prepared.as_ref().unwrap();
        assert_eq!(prepared.target, "0123456789abcdef0123456789abcdef:/capture");
        assert_eq!(prepared.values.get("field_name"), Some(&"edited".into()));
        assert_eq!(prepared.values.get("field_checks"), Some(&"blue".into()));
        assert_eq!(prepared.values.get("var_fixed"), Some(&"ready".into()));
        assert!(!source.contains("edited"));
    }

    #[test]
    fn micron_request_bounds_parse_or_fall_back_and_clamp() {
        assert_eq!(env_bound(Some("45"), 30_u64, 1, 120), 45);
        assert_eq!(env_bound(Some(" 45 "), 30_u64, 1, 120), 45);
        assert_eq!(env_bound(None, 30_u64, 1, 120), 30);
        assert_eq!(env_bound(Some("not a number"), 30_u64, 1, 120), 30);
        assert_eq!(env_bound(Some("-1"), 30_u64, 1, 120), 30);
        assert_eq!(env_bound(Some("0"), 30_u64, 1, 120), 1);
        assert_eq!(env_bound(Some("999"), 30_u64, 1, 120), 120);
        let max = 64 * 1024 * 1024_usize;
        assert_eq!(env_bound(Some("4096"), 4 * 1024 * 1024_usize, 1, max), 4096);
        assert_eq!(env_bound(Some("0"), 4 * 1024 * 1024_usize, 1, max), 1);
        assert_eq!(
            env_bound(Some("999999999999"), 4 * 1024 * 1024_usize, 1, max),
            max
        );
    }

    #[test]
    fn stale_micron_completion_does_not_replace_the_visible_page_result() {
        let (sender, receiver) = mpsc::channel();
        let mut micron = MicronRequest::default();
        micron.receiver = Some(receiver);
        micron.active = Some((7, "old source".into(), "old:page".into()));
        sender
            .send((
                7,
                Ok(MicronResponse {
                    body: b"old reply".to_vec(),
                }),
            ))
            .unwrap();
        micron.drain("new source", "new:page");
        assert!(micron.result.is_none());
        assert!(micron.response.is_none());
        assert!(micron.active.is_none());
    }

    #[test]
    fn cancelling_micron_request_keeps_its_local_unknown_outcome_status() {
        let (sender, receiver) = mpsc::channel();
        let (cancel, _cancelled) = tokio::sync::oneshot::channel();
        let mut micron = MicronRequest::default();
        micron.receiver = Some(receiver);
        micron.active = Some((8, "source".into(), "address".into()));
        micron.cancel = Some(cancel);
        micron.cancel();
        drop(sender);
        micron.drain("source", "address");
        assert_eq!(
            micron.result.as_deref(),
            Some(
                "Micron request cancelled locally. Its remote outcome may be unknown; do not retry automatically."
            )
        );
        assert!(micron.active.is_none());
    }

    #[test]
    fn closing_a_micron_form_drops_the_previous_reply_unless_a_send_is_in_flight() {
        let source = "`[Submit`0123456789abcdef0123456789abcdef:/capture`name]\n`<name`seed>\n";
        let mut micron = MicronRequest::default();
        micron
            .open_form(source.into(), "scratch:reopen".into())
            .unwrap();
        micron.result = Some("Micron reply (8 bytes)".into());
        micron.response = Some("accepted".into());
        micron.close_form();
        assert!(micron.result.is_none());
        assert!(micron.response.is_none());
        micron
            .open_form(source.into(), "scratch:reopen".into())
            .unwrap();
        assert!(micron.response.is_none());

        let (_sender, receiver) = mpsc::channel();
        micron.receiver = Some(receiver);
        micron.result = Some("Sending reviewed Micron request…".into());
        micron.close_form();
        assert_eq!(
            micron.result.as_deref(),
            Some("Sending reviewed Micron request…")
        );
    }

    #[test]
    fn micron_result_remains_visible_in_preview_when_site_panel_is_closed() {
        let source = "`[Submit`0123456789abcdef0123456789abcdef:/capture`name]\n`<name`seed>\n";
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("loose.mu");
        std::fs::write(&path, source).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
        );
        show_preview(&mut state);
        let address = state.document().snapshot().source.address;
        state
            .entry_mut()
            .site
            .micron
            .open_form(source.into(), address)
            .unwrap();
        state.entry_mut().site.micron.result = Some("Micron reply (8 bytes)".into());
        state.entry_mut().site.micron.response = Some("accepted".into());
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: format!("{DESKTOP_CSS}{CSS}"),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let text = text_content(&dom, dom.document());
        assert!(text.contains("Micron reply (8 bytes)"));
        assert!(text.contains("accepted"));
    }

    /// The site page follows the focused tab. A page's metadata tile keeps its
    /// draft while the focus moves, marks its tab unsaved, and asks before it
    /// closes over unsaved edits (step 7b).
    #[test]
    fn the_site_page_follows_the_focused_tab_and_a_metadata_tile_keeps_its_draft() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("site");
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:site-tabs", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(root.to_string_lossy());
        state.enter_site(true);
        state.scroll_open_page("about.scroll");
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        assert_eq!(page_name(host.state()), Some("about.scroll"));
        assert!(host.click_on(&Selector::role("tab").containing("index.scroll")));
        assert_eq!(page_name(host.state()), Some("index.scroll"));
        assert!(host.click_on(&Selector::role("tab").containing("scratch:site-tabs")));
        assert_eq!(page_name(host.state()), None);

        assert!(host.click_on(&Selector::role("tab").containing("about.scroll")));
        host.update(DesktopState::open_page_metadata);
        let tile = {
            let state = host.state();
            let site = state.site_page().unwrap().site;
            state
                .docs
                .metadata_tile(site, "about.scroll")
                .expect("about.scroll's metadata tile")
        };
        host.update(|state| draft_fields(state, tile)[0] = TextInput::new("Writer"));
        host.relayout();
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let marked = attr_nodes(&dom, dom.document(), "aria-description", "Unsaved changes");
            assert_eq!(marked.len(), 1, "one tab is marked unsaved");
            assert!(text_content(&dom, marked[0]).contains("Metadata · about.scroll"));
        }

        // The focus moves on; the draft stays with its tile.
        assert!(host.click_on(&Selector::role("tab").containing("index.scroll")));
        assert_eq!(page_name(host.state()), Some("index.scroll"));
        assert_eq!(draft_field(host.state(), tile, 0).text(), "Writer");

        // Closing the tile over unsaved edits asks; Discard drops the draft and
        // leaves the manifest as it was.
        // A tab's close shows on the active tab, so bring the metadata tab up first.
        let manifest = std::fs::read(root.join(knot_site::CONFIG)).unwrap();
        assert!(host.click_on(&Selector::role("tab").containing("Metadata · about.scroll")));
        assert!(host.click_on(
            &Selector::role("button").with_attr("aria-label", "Close Metadata · about.scroll")
        ));
        let discard = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let prompt = class_nodes(&dom, dom.document(), "knot-confirm");
            assert_eq!(prompt.len(), 1, "the close asks first");
            assert!(
                text_content(&dom, prompt[0])
                    .contains("Metadata · about.scroll has unsaved changes.")
            );
            let button = dom
                .dom_children(prompt[0])
                .find(|child| text_content(&dom, *child) == "Discard")
                .expect("the prompt's Discard");
            let (x, y, width, height) = host.painted_rect(button).expect("Discard paints");
            (x + width / 2.0, y + height / 2.0)
        };
        host.click_at(discard.0, discard.1);
        host.relayout();
        let state = host.state();
        assert!(
            state
                .docs
                .metadata_tile(state.site_page().unwrap().site, "about.scroll")
                .is_none()
        );
        assert_eq!(
            std::fs::read(root.join(knot_site::CONFIG)).unwrap(),
            manifest
        );
        assert_eq!(state.docs.len(), 3);
    }

    /// Step 7a: each document keeps its own composer, so a target typed for
    /// one never shows in another.
    #[test]
    fn a_composer_stays_with_its_document() {
        let temp = tempfile::tempdir().unwrap();
        let one = temp.path().join("one.gmi");
        let two = temp.path().join("two.gmi");
        std::fs::write(&one, "# One\n").unwrap();
        std::fs::write(&two, "# Two\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&one).unwrap(),
            WindowCommands::new(),
        );
        assert!(state.open_path(two));
        assert!(state.open_path(one));
        let mut host = submission_harness_with(state);
        let target = |host: &DesktopHarness| {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let label = class_nodes(&dom, dom.document(), "knot-submission-target")
                .into_iter()
                .next()
                .expect("the target field");
            text_content(&dom, label)
        };
        let field = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let label = class_nodes(&dom, dom.document(), "knot-submission-target")
                .into_iter()
                .next()
                .expect("the target field");
            input_node(&dom, label).expect("the target input")
        };
        let (x, y, width, height) = host.painted_rect(field).expect("the target input paints");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.key_injected("spartan://one.test/");
        assert!(target(&host).contains("spartan://one.test/"));
        assert!(host.click_on(&Selector::role("tab").containing("two.gmi")));
        assert!(
            !target(&host).contains("spartan://one.test/"),
            "one.gmi's target followed the focus to two.gmi"
        );
        assert!(host.click_on(&Selector::role("tab").containing("one.gmi")));
        assert!(target(&host).contains("spartan://one.test/"));
    }

    /// A harness over the full desktop sheet whose window commands reach it.
    fn site_harness(state: DesktopState) -> DesktopHarness {
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        let commands = host.commands();
        host.update(|state| state.set_window(commands.clone()));
        host.layout_at(1100.0, 730.0);
        host
    }

    /// Every button under `node`, in document order.
    fn buttons_under(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        found: &mut Vec<genet_scripted_dom::NodeId>,
    ) {
        if dom
            .element_name(node)
            .is_some_and(|name| name.local.as_ref() == "button")
        {
            found.push(node);
        }
        for child in dom.dom_children(node) {
            buttons_under(dom, child, found);
        }
    }

    /// Click the button labelled `label` in the site tile's row for `page`.
    fn click_site_row(host: &mut DesktopHarness, page: &str, label: &str) {
        let (x, y) = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let row = class_nodes(&dom, dom.document(), "knot-site-page")
                .into_iter()
                .find(|row| text_content(&dom, *row).starts_with(page))
                .expect("the page's row");
            let mut found = Vec::new();
            buttons_under(&dom, row, &mut found);
            let button = found
                .into_iter()
                .find(|node| text_content(&dom, *node) == label)
                .expect("the row's button");
            let (x, y, width, height) = host.visible_rect(button).expect("the button shows");
            (x + width / 2.0, y + height / 2.0)
        };
        host.click_at(x, y);
        host.relayout();
    }

    /// The text of the one open prompt.
    fn prompt_text(host: &DesktopHarness) -> Option<String> {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        class_nodes(&dom, dom.document(), "knot-confirm")
            .first()
            .map(|prompt| text_content(&dom, *prompt))
    }

    /// Click a button of the open prompt by its label.
    fn click_prompt(host: &mut DesktopHarness, label: &str) {
        let (x, y) = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let prompt = class_nodes(&dom, dom.document(), "knot-confirm")[0];
            let mut found = Vec::new();
            buttons_under(&dom, prompt, &mut found);
            let button = found
                .into_iter()
                .find(|node| text_content(&dom, *node) == label)
                .expect("the prompt's button");
            let (x, y, width, height) = host.visible_rect(button).expect("the button shows");
            (x + width / 2.0, y + height / 2.0)
        };
        host.click_at(x, y);
        host.relayout();
    }

    /// Step 7c: the Site popover creates a site, whose tile opens left of the
    /// documents; its rows open a page's tab and a page's metadata, and the
    /// panel above the frame is gone.
    #[test]
    fn the_site_popover_creates_a_site_whose_tile_opens_pages_and_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = site_harness(DesktopState::new(
            KnotDocumentSession::scratch("scratch:tile", ""),
            WindowCommands::new(),
        ));
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-site")));
        let folder = temp.path().join("tidewater");
        host.update(|state| state.scroll.folder = TextInput::new(folder.to_string_lossy()));
        assert!(host.click_on(&Selector::role("button").containing("Create site")));
        let (site, tile) = {
            let state = host.state();
            let site = state.scroll.current_key().expect("the site opened");
            (site, state.docs.site_tile(site).expect("its tile"))
        };
        assert!(!host.state().scroll.popover.open, "the popover closed");
        assert_eq!(page_name(host.state()), Some("index.scroll"));
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            assert!(class_nodes(&dom, dom.document(), "knot-scroll-site").is_empty());
            let tiles = class_nodes(&dom, dom.document(), "knot-site-tile");
            assert_eq!(tiles.len(), 1);
            assert!(text_content(&dom, tiles[0]).starts_with("tidewater"));
            let document = class_nodes(&dom, dom.document(), "knot-document-tile")[0];
            let (site_x, ..) = host.painted_rect(tiles[0]).unwrap();
            let (document_x, ..) = host.painted_rect(document).unwrap();
            assert!(
                site_x < document_x,
                "the site tile sits left of the documents"
            );
        }
        assert!(host.state().docs.role(tile) == Some(&TileRole::Site(site)));

        click_site_row(&mut host, "about.scroll", "about.scroll");
        assert_eq!(page_name(host.state()), Some("about.scroll"));
        click_site_row(&mut host, "about.scroll", "Metadata");
        assert!(
            host.state()
                .docs
                .metadata_tile(site, "about.scroll")
                .is_some()
        );
    }

    /// Step 7c: closing a site with unsaved work asks once, listing its
    /// unsaved page and metadata. Cancel keeps everything; Save all writes both
    /// and closes the site, its tabs and tile, and leaves other documents.
    #[test]
    fn closing_a_site_asks_once_for_its_unsaved_pages_and_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("site");
        let loose = temp.path().join("loose.djot");
        std::fs::write(&loose, "loose\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&loose).unwrap(),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(root.to_string_lossy());
        state.enter_site(true);
        let site = state.scroll.current_key().unwrap();
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "Edited ".into(),
            )))
            .unwrap();
        state.open_metadata(site, "about.scroll");
        let metadata = state.docs.metadata_tile(site, "about.scroll").unwrap();
        draft_fields(&mut state, metadata)[0] = TextInput::new("Closer");
        let mut host = site_harness(state);

        assert!(host.click_on(&Selector::role("button").containing("Close site")));
        let prompt = prompt_text(&host).expect("closing asks");
        assert!(
            prompt.contains("2 tabs of site have unsaved changes."),
            "{prompt}"
        );
        assert!(prompt.contains("index.scroll") && prompt.contains("Metadata · about.scroll"));
        click_prompt(&mut host, "Cancel");
        assert!(prompt_text(&host).is_none());
        assert!(
            host.state().docs.site_tile(site).is_some(),
            "Cancel keeps the site"
        );
        assert_eq!(host.state().docs.len(), 2);

        assert!(host.click_on(&Selector::role("button").containing("Close site")));
        click_prompt(&mut host, "Save all");
        let state = host.state();
        assert!(state.scroll.current_site().is_none(), "the site closed");
        assert!(state.docs.site_tile(site).is_none());
        assert!(state.docs.metadata_tile(site, "about.scroll").is_none());
        assert_eq!(state.docs.len(), 1, "only the loose document stays");
        assert_eq!(state.document().snapshot().display_label, "loose.djot");
        assert!(
            std::fs::read_to_string(root.join("index.scroll"))
                .unwrap()
                .contains("Edited ")
        );
        assert!(
            std::fs::read_to_string(root.join(knot_site::CONFIG))
                .unwrap()
                .contains("Closer")
        );
    }

    /// Step 7c: a site with nothing unsaved closes without asking: its page
    /// tabs close and its server stops.
    #[test]
    fn closing_a_clean_site_closes_its_pages_and_stops_its_server() {
        let temp = tempfile::tempdir().unwrap();
        let loose = temp.path().join("loose.djot");
        std::fs::write(&loose, "loose\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&loose).unwrap(),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(temp.path().join("site").to_string_lossy());
        state.enter_site(true);
        let site = state.scroll.current_key().unwrap();
        state.open_site_page(site, "about.scroll");
        set_port(&mut state, "0");
        let mut host = site_harness(state);
        assert!(host.click_on(&Selector::role("button").containing("Publish locally")));
        assert!(host.state().scroll.current_site().unwrap().server.is_some());
        assert_eq!(host.state().docs.len(), 3);

        assert!(host.click_on(&Selector::role("button").containing("Close site")));
        assert!(prompt_text(&host).is_none(), "a clean site closes at once");
        let state = host.state();
        assert!(
            state.scroll.current_site().is_none(),
            "the server went with its site"
        );
        assert!(state.docs.site_tile(site).is_none());
        assert_eq!(state.docs.len(), 1);
        assert_eq!(state.message.as_deref(), Some("Closed site site."));
    }

    /// Step 7e: a second site opens alongside an unsaved first site. Each
    /// keeps its page, tile and default-port choice.
    #[test]
    fn another_site_opens_alongside_unsaved_work() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:replace", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(temp.path().join("first").to_string_lossy());
        state.enter_site(true);
        let first = state.scroll.current_key().unwrap();
        let first_page = state.focused_key().unwrap();
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "Draft ".into(),
            )))
            .unwrap();
        state.scroll.folder = TextInput::new(temp.path().join("second").to_string_lossy());
        state.enter_site(true);
        let second = state.scroll.current_key().unwrap();
        assert_ne!(second, first);
        assert!(state.docs.site_tile(first).is_some());
        assert!(state.docs.site_tile(second).is_some());
        assert_eq!(state.scroll.site_count(), 2);
        assert_eq!(state.docs.len(), 3, "scratch and both index pages stay");
        assert!(state.entry_for(first_page).document.snapshot().dirty);
        assert_eq!(state.scroll.site_port(first).unwrap().text(), "5699");
        assert_eq!(
            state.scroll.site_port(second).unwrap().text(),
            "0",
            "the second Scroll site defers its colliding default to the OS"
        );
        assert_eq!(page_name(&state), Some("index.scroll"));
        assert_eq!(state.site_page().unwrap().site, second);
    }

    #[test]
    fn two_sites_keep_independent_servers_and_submission_state() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:two-sites", ""),
            WindowCommands::new(),
        );

        state.scroll.folder = TextInput::new(temp.path().join("first").to_string_lossy());
        state.enter_site(true);
        let first = state.site_page().unwrap().site;
        let first_doc = state.focused_key().unwrap();
        state.entry_mut_for(first_doc).site.submission.target =
            TextInput::new("spartan://first.test/submit");

        state.scroll.folder = TextInput::new(temp.path().join("second").to_string_lossy());
        state.enter_site(true);
        let second = state.site_page().unwrap().site;
        let second_doc = state.focused_key().unwrap();
        state.entry_mut_for(second_doc).site.submission.target =
            TextInput::new("spartan://second.test/submit");

        *state.scroll.site_port_mut(first).unwrap() = TextInput::new("0");
        *state.scroll.site_port_mut(second).unwrap() = TextInput::new("0");
        state.publish_site(first);
        state.publish_site(second);

        let first_entry = state.scroll.site(first).unwrap();
        let second_entry = state.scroll.site(second).unwrap();
        let first_port = first_entry.server.as_ref().unwrap().address().port();
        let second_port = second_entry.server.as_ref().unwrap().address().port();
        assert_ne!(first_port, 0);
        assert_ne!(second_port, 0);
        assert_ne!(first_port, second_port);
        assert_eq!(first_entry.publication_number(), 1);
        assert_eq!(second_entry.publication_number(), 1);
        assert_eq!(state.scroll.serving_count(), 2);
        assert_eq!(
            state.entry_for(first_doc).site.submission.target.text(),
            "spartan://first.test/submit"
        );
        assert_eq!(
            state.entry_for(second_doc).site.submission.target.text(),
            "spartan://second.test/submit"
        );

        let mut host = site_harness(state);
        let serving_label = |host: &DesktopHarness| {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let chip = attr_nodes(&dom, dom.document(), "data-status-key", "serving")
                .into_iter()
                .next()
                .expect("the focused page's serving chip");
            text_content(&dom, chip)
        };
        assert_eq!(
            serving_label(&host),
            format!("second · serving :{second_port}")
        );
        host.update(|state| state.docs.focus(first_doc));
        host.relayout();
        assert_eq!(
            serving_label(&host),
            format!("first · serving :{first_port}")
        );

        let (chip_x, chip_y) = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let chip = attr_nodes(&dom, dom.document(), "data-status-key", "serving")[0];
            let (x, y, width, height) = host.visible_rect(chip).unwrap();
            (x + width / 2.0, y + height / 2.0)
        };
        host.click_at(chip_x, chip_y);
        host.relayout();
        fn button_named(
            dom: &genet_scripted_dom::ScriptedDom,
            node: genet_scripted_dom::NodeId,
            label: &str,
        ) -> Option<genet_scripted_dom::NodeId> {
            if dom
                .element_name(node)
                .is_some_and(|name| name.local.as_ref() == "button")
                && text_content(dom, node) == label
            {
                return Some(node);
            }
            dom.dom_children(node)
                .find_map(|child| button_named(dom, child, label))
        }
        let (stop_x, stop_y) = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let detail = class_nodes(&dom, dom.document(), "knot-status-detail")[0];
            let stop = button_named(&dom, detail, "Stop serving").unwrap();
            let (x, y, width, height) = host.visible_rect(stop).unwrap();
            (x + width / 2.0, y + height / 2.0)
        };
        host.click_at(stop_x, stop_y);
        host.relayout();
        assert!(host.state().scroll.site(first).unwrap().server.is_none());
        assert!(host.state().scroll.site(second).unwrap().server.is_some());
        assert_eq!(host.state().scroll.serving_count(), 1);
    }

    /// Step 7c: a site tile's port field takes typing for its own site.
    #[test]
    fn a_site_tiles_port_field_takes_typing_for_its_site() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:port", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(temp.path().join("site").to_string_lossy());
        state.enter_site(true);
        let site = state.scroll.current_key().unwrap();
        set_port(&mut state, "");
        let mut host = site_harness(state);
        let field = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let label = class_nodes(&dom, dom.document(), "knot-site-port")[0];
            input_node(&dom, label).expect("the port input")
        };
        let (x, y, width, height) = host.visible_rect(field).expect("the port input shows");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.key_injected("8123");
        assert_eq!(host.state().scroll.site_port(site).unwrap().text(), "8123");
        assert_eq!(
            host.state().scroll.port.text(),
            "5699",
            "the next site's port stays"
        );
    }

    /// Step 7b: closing the window over unsaved metadata asks once, naming the
    /// metadata tile, and Save all writes the manifest before the window goes.
    #[test]
    fn the_quit_prompt_names_unsaved_metadata_and_save_all_writes_it() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("site");
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:quit", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(root.to_string_lossy());
        state.enter_site(true);
        state.open_page_metadata();
        let site = state.site_page().unwrap().site;
        let tile = state.docs.metadata_tile(site, "index.scroll").unwrap();
        draft_fields(&mut state, tile)[0] = TextInput::new("Quitter");
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        let commands = host.commands();
        host.update(|state| state.set_window(commands.clone()));
        host.layout_at(1100.0, 730.0);
        host.request_close(CloseRequest::Native);
        host.relayout();
        assert!(
            !host.close_requested(),
            "the window closed over unsaved metadata"
        );
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let prompt = class_nodes(&dom, dom.document(), "knot-confirm");
            assert_eq!(prompt.len(), 1, "one prompt");
            assert!(
                text_content(&dom, prompt[0])
                    .contains("Metadata · index.scroll has unsaved changes.")
            );
        }
        assert!(host.click_on(&Selector::role("button").with_attr("id", "knot-confirm-save")));
        assert!(
            host.close_requested(),
            "the window stayed open: {:?}",
            host.state().message
        );
        let manifest = std::fs::read_to_string(root.join(knot_site::CONFIG)).unwrap();
        assert!(manifest.contains("Quitter"), "{manifest}");
    }

    /// Step 7b: a Submit tile pinned to one document takes typing for that
    /// document, whichever document has the focus, and a click places the
    /// caret in that document's field.
    #[test]
    fn a_pinned_submit_tile_writes_its_own_documents_composer() {
        let temp = tempfile::tempdir().unwrap();
        let one = temp.path().join("one.gmi");
        let two = temp.path().join("two.gmi");
        std::fs::write(&one, "# One\n").unwrap();
        std::fs::write(&two, "# Two\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&one).unwrap(),
            WindowCommands::new(),
        );
        let one_key = state.focused_key().unwrap();
        state.toggle_reading(ReadingKind::Submit);
        let tile = state.docs.following_reading(ReadingKind::Submit).unwrap();
        state.toggle_pin(tile);
        state.entry_mut().site.submission.target = TextInput::new("one.test/");
        assert!(state.open_path(two));
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        let field = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let labels = class_nodes(&dom, dom.document(), "knot-submission-target");
            assert_eq!(labels.len(), 1, "one Submit tile");
            input_node(&dom, labels[0]).expect("the target input")
        };
        // A click at the field's start puts the caret there, ahead of the
        // text already in it.
        let (x, y, _, height) = host.visible_rect(field).expect("the target input shows");
        host.click_at(x + 1.0, y + height / 2.0);
        host.key_injected("spartan://");
        let state = host.state();
        assert_eq!(state.document().snapshot().display_label, "two.gmi");
        assert_eq!(
            state.entry_for(one_key).site.submission.target.text(),
            "spartan://one.test/"
        );
        assert_eq!(
            state.entry().site.submission.target.text(),
            "",
            "the focused document's composer took the pinned tile's typing"
        );
    }

    /// Step 7b: a page's metadata lives with its site, so its tile stays open,
    /// draft and all, when the page's source tab closes.
    #[test]
    fn a_metadata_tile_outlives_its_pages_source_tab() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:outlive", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(temp.path().join("site").to_string_lossy());
        state.enter_site(true);
        state.open_page_metadata();
        let site = state.site_page().unwrap().site;
        let tile = state.docs.metadata_tile(site, "index.scroll").unwrap();
        draft_fields(&mut state, tile)[0] = TextInput::new("Kept");
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        // Bring the page's own tab up, so its close shows, then close it.
        let page_tab = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            attr_nodes(&dom, dom.document(), "role", "tab")
                .into_iter()
                .find(|tab| text_content(&dom, *tab).starts_with("index.scroll"))
                .expect("index.scroll's tab")
        };
        let (x, y, width, height) = host.visible_rect(page_tab).expect("the tab shows");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.relayout();
        let documents = host.state().docs.len();
        assert!(
            host.click_on(&Selector::role("button").with_attr("aria-label", "Close index.scroll"))
        );
        assert_eq!(
            host.state().docs.len(),
            documents - 1,
            "the page's tab closed"
        );
        assert_eq!(
            host.state().docs.metadata_tile(site, "index.scroll"),
            Some(tile)
        );
        assert_eq!(draft_field(host.state(), tile, 0).text(), "Kept");
    }

    /// Step 7a: a Micron reply belongs to the page it was sent from, and lands
    /// there even after the focus has moved to another document.
    #[test]
    fn a_micron_reply_lands_on_its_page_after_the_focus_moves() {
        let temp = tempfile::tempdir().unwrap();
        let page = temp.path().join("form.mu");
        let other = temp.path().join("other.mu");
        std::fs::write(&page, "form page\n").unwrap();
        std::fs::write(&other, "other page\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&page).unwrap(),
            WindowCommands::new(),
        );
        let sent_from = state.document().snapshot();
        let (sender, receiver) = mpsc::channel();
        let micron = &mut state.entry_mut().site.micron;
        micron.receiver = Some(receiver);
        micron.active = Some((3, sent_from.text, sent_from.source.address));
        assert!(state.open_path(other));
        sender
            .send((
                3,
                Ok(MicronResponse {
                    body: b"accepted".to_vec(),
                }),
            ))
            .unwrap();
        assert!(state.submissions_busy());
        state.drain_submissions();
        assert!(!state.submissions_busy());
        assert!(
            state.entry().site.micron.result.is_none(),
            "the reply showed on the focused document"
        );
        assert!(state.open_path(page));
        assert_eq!(
            state.entry().site.micron.result.as_deref(),
            Some("Micron reply (8 bytes)")
        );
        assert_eq!(
            state.entry().site.micron.response.as_deref(),
            Some("accepted")
        );
    }

    /// Step 7a: a site page is known by its file, whatever spelling of the
    /// path opened it. The session opens the canonical path, which the page
    /// binding relies on; this held before per-document state too.
    #[test]
    fn a_page_opened_through_another_spelling_of_its_path_is_its_site_page() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("site");
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:spelling", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(root.to_string_lossy());
        state.enter_site(true);
        assert!(state.open_path(root.join(".").join("about.scroll")));
        let host = desktop_harness_with(state);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        assert_eq!(
            attr_nodes(&dom, dom.document(), "data-status-key", "serving").len(),
            1,
            "about.scroll was not taken for its site's page"
        );
    }

    /// Step 7a: a Spartan site presents its own pages as Spartan; a Gemtext
    /// file outside it keeps Gemtext's presentation while the site is open.
    #[test]
    fn a_gemtext_file_outside_a_spartan_site_keeps_its_presentation() {
        const PROMPT: &str = "=: spartan://localhost:65025/upload Submit locally\n";
        let temp = tempfile::tempdir().unwrap();
        let loose = temp.path().join("loose.gmi");
        std::fs::write(&loose, PROMPT).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:loose", ""),
            WindowCommands::new(),
        );
        state.scroll.format = SiteFormat::Spartan;
        state.scroll.folder = TextInput::new(temp.path().join("spartan-site").to_string_lossy());
        state.enter_site(true);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
            .unwrap();
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                PROMPT.into(),
            )))
            .unwrap();
        assert!(state.open_path(loose));
        show_preview(&mut state);
        let mut host = desktop_harness_with(state);
        let prompt = Selector::role("button").containing("Submit locally");
        assert!(
            host.resolve(&prompt).is_none(),
            "a loose Gemtext file took the Spartan site's presentation"
        );
        assert!(host.click_on(&Selector::role("tab").containing("index.gmi")));
        assert!(
            host.resolve(&prompt).is_some(),
            "the site's own page lost its Spartan presentation"
        );
        assert!(host.click_on(&Selector::role("button").containing("Pin")));
        assert!(host.click_on(&Selector::role("tab").containing("loose.gmi")));
        assert!(
            host.resolve(&prompt).is_some(),
            "a pinned Preview stopped presenting its Spartan page"
        );
    }

    #[test]
    fn a_pinned_micron_preview_keeps_its_pages_form() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.mu");
        let second = temp.path().join("second.mu");
        let source = "`[Submit`0123456789abcdef0123456789abcdef:/capture`name]\n`<name`first>\n";
        std::fs::write(&first, source).unwrap();
        std::fs::write(&second, ">Second\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&first).unwrap(),
            WindowCommands::new(),
        );
        let first_key = state.focused_key().unwrap();
        state.open_micron_form_for(first_key);
        show_preview(&mut state);
        let preview = state.docs.following_reading(ReadingKind::Preview).unwrap();
        state.toggle_pin(preview);
        assert!(state.open_path(second));

        let host = desktop_harness_with(state);
        assert_eq!(host.state().docs.document_for(preview), Some(first_key));
        assert!(
            host.resolve(&Selector::role("button").containing("Close Micron form"))
                .is_some()
        );
        assert!(host.state().entry_for(first_key).site.micron.form.is_some());
        assert!(host.state().entry().site.micron.form.is_none());
    }

    /// Step 5d: a site page shows the serving chip. Its popover says why
    /// nothing is served and offers Publish locally; once published, it
    /// shows the address and offers Stop serving.
    #[test]
    fn a_site_page_shows_the_serving_chip_and_its_popover_publishes_and_stops() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:serving", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(temp.path().join("site").to_string_lossy());
        state.enter_site(true);
        set_port(&mut state, "0");
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        let chip = |host: &DesktopHarness| {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let chip = attr_nodes(&dom, dom.document(), "data-status-key", "serving")
                .into_iter()
                .next()
                .expect("the serving chip");
            let (x, y, width, height) = host.painted_rect(chip).expect("the chip paints");
            (
                text_content(&dom, chip),
                (x + width / 2.0, y + height / 2.0),
            )
        };
        let detail = |host: &DesktopHarness| {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let detail = class_nodes(&dom, dom.document(), "knot-status-detail");
            assert_eq!(detail.len(), 1, "one popover open");
            text_content(&dom, detail[0])
        };
        // The site tile offers Publish and Stop too; press the popover's own.
        fn buttons(
            dom: &genet_scripted_dom::ScriptedDom,
            node: genet_scripted_dom::NodeId,
            found: &mut Vec<genet_scripted_dom::NodeId>,
        ) {
            if dom
                .element_name(node)
                .is_some_and(|name| name.local.as_ref() == "button")
            {
                found.push(node);
            }
            for child in dom.dom_children(node) {
                buttons(dom, child, found);
            }
        }
        let press_in_detail = |host: &mut DesktopHarness, label: &str| {
            let (x, y) = {
                let dom = host.runner().dom();
                let dom = dom.borrow();
                let detail = class_nodes(&dom, dom.document(), "knot-status-detail")[0];
                let mut found = Vec::new();
                buttons(&dom, detail, &mut found);
                let button = found
                    .into_iter()
                    .find(|node| text_content(&dom, *node) == label)
                    .expect("the popover's button");
                let (x, y, width, height) = host.visible_rect(button).expect("the button shows");
                (x + width / 2.0, y + height / 2.0)
            };
            host.click_at(x, y);
            host.relayout();
        };

        let (label, (x, y)) = chip(&host);
        assert_eq!(label, "site · not serving");
        host.click_at(x, y);
        host.relayout();
        assert_eq!(
            host.state().status_bar.open.as_deref(),
            Some(crate::status::SERVING)
        );
        assert!(detail(&host).contains("Not published."));

        press_in_detail(&mut host, "Publish locally");
        let site = host.state().site_page().unwrap().site;
        let server = host
            .state()
            .scroll
            .site(site)
            .unwrap()
            .server
            .as_ref()
            .unwrap();
        let url = server.url().to_string();
        assert_eq!(
            chip(&host).0,
            format!("site · serving :{}", server.address().port())
        );
        let served = detail(&host);
        assert!(
            served.contains(&url) && served.contains("revision 1"),
            "{served}"
        );
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let footer = class_nodes(&dom, dom.document(), "knot-site-footer");
            let serving = class_nodes(&dom, dom.document(), "knot-site-serving");
            assert_eq!(
                footer.len(),
                1,
                "serving status and close action share a footer"
            );
            assert_eq!(serving.len(), 1, "one saved-snapshot status line");
            let mut buttons = Vec::new();
            buttons_under(&dom, footer[0], &mut buttons);
            let close = buttons
                .into_iter()
                .find(|node| text_content(&dom, *node) == "Close site")
                .expect("close button is inside the footer");
            assert_eq!(dom.parent(close), Some(footer[0]));
            let (status_x, status_y, status_w, status_h) =
                host.painted_rect(serving[0]).expect("status paints");
            let (close_x, close_y, _close_w, close_h) =
                host.painted_rect(close).expect("close action paints");
            let same_row = close_y < status_y + status_h && status_y < close_y + close_h;
            assert!(
                if same_row {
                    close_x >= status_x + status_w
                } else {
                    close_y >= status_y + status_h
                },
                "the saved-snapshot text and Close site control do not overlap"
            );
        }

        press_in_detail(&mut host, "Stop serving");
        assert!(host.state().scroll.site(site).unwrap().server.is_none());
        assert_eq!(
            host.state().message.as_deref(),
            Some("Local serving stopped.")
        );
    }

    fn attr_nodes(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        name: &str,
        value: &str,
    ) -> Vec<genet_scripted_dom::NodeId> {
        let mut found = Vec::new();
        if dom
            .attribute(node, &Namespace::from(""), &LocalName::from(name))
            .as_deref()
            == Some(value)
        {
            found.push(node);
        }
        for child in dom.dom_children(node) {
            found.extend(attr_nodes(dom, child, name, value));
        }
        found
    }

    #[test]
    fn launch_documents_open_behind_the_first() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.djot");
        let second = temp.path().join("second.djot");
        std::fs::write(&first, "first\n").unwrap();
        std::fs::write(&second, "second\n").unwrap();
        let folder = temp.path().join("site-one");
        let site = Site::create_for(&folder, SiteFormat::Scroll).unwrap();
        let index = site.page_path(site.config.format.index_file()).unwrap();
        let other_folder = temp.path().join("site-two");
        let other = Site::create_for(&other_folder, SiteFormat::Scroll).unwrap();
        let other_index = other.page_path(other.config.format.index_file()).unwrap();
        let mut state = DesktopState::with_path(
            KnotDocumentSession::open(&first).unwrap(),
            WindowCommands::new(),
            Some(first.clone()),
        );
        let front = state.focused_key();
        assert!(state.attach_site(&folder).is_some());
        assert!(state.attach_site(&other_folder).is_some());
        state.open_behind(
            vec![
                KnotDocumentSession::open(&second).unwrap(),
                KnotDocumentSession::open(&index).unwrap(),
                KnotDocumentSession::open(&other_index).unwrap(),
            ],
            &["missing.djot: not found".to_owned()],
        );
        assert_eq!(state.docs.len(), 4);
        assert_eq!(state.focused_key(), front);
        assert_eq!(state.path.text(), first.to_string_lossy().as_ref());
        assert_eq!(state.scroll.site_count(), 2);
        for site in state.scroll.sites.keys() {
            assert!(
                state.docs.site_tile(*site).is_some(),
                "each launch site has a tile"
            );
        }
        assert_eq!(
            page_name(&state),
            None,
            "the panel follows the file in front"
        );
        assert_eq!(
            state.message.as_deref(),
            Some("Not opened: missing.djot: not found")
        );
    }

    #[test]
    fn micron_preview_links_require_site_manifest_and_matching_authority() {
        let temp = tempfile::tempdir().unwrap();
        let site = Site::create_for(&temp.path().join("micron"), SiteFormat::Micron).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:micron-links", ""),
            WindowCommands::new(),
        );
        let index = site.page_path("index.mu").unwrap();
        let key = state.hold_site(site);

        assert_eq!(preview_manifest_page(&state, ":/page/about.mu"), None);
        assert!(state.open_path(index));
        assert_eq!(
            preview_manifest_page(&state, ":/page/about.mu"),
            Some((key, "about.mu"))
        );
        assert_eq!(preview_manifest_page(&state, ":/page/missing.mu"), None);
        assert_eq!(preview_manifest_page(&state, ":/page/../index.mu"), None);
        assert_eq!(
            preview_manifest_page(&state, ":/page/about\\notes.mu"),
            None
        );
        assert_eq!(
            preview_manifest_page(&state, "othernode:/page/about.mu"),
            None
        );
        assert_eq!(
            preview_page_name(
                "0123456789abcdef0123456789abcdef:/page/about.mu",
                Some("0123456789abcdef0123456789abcdef")
            ),
            Some("about.mu")
        );
        assert_eq!(
            preview_page_name(
                "0123456789ABCDEF0123456789ABCDEF:/page/about.mu",
                Some("0123456789abcdef0123456789abcdef")
            ),
            Some("about.mu")
        );
    }

    #[test]
    fn micron_preview_preserves_styled_link_children_and_section_layout() {
        let temp = tempfile::tempdir().unwrap();
        let site = Site::create_for(&temp.path().join("styled"), SiteFormat::Micron).unwrap();
        let path = site.page_path("index.mu").unwrap();
        let source = ">Title\n>>Section\n`c`Ff00`B123`_`[About`:/page/about.mu]\n";
        std::fs::write(&path, source).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:styled", ""),
            WindowCommands::new(),
        );
        state.hold_site(site);
        state.scroll_open_page("index.mu");
        show_preview(&mut state);
        let host = desktop_harness_with(state);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        fn inspect(
            dom: &genet_scripted_dom::ScriptedDom,
            node: genet_scripted_dom::NodeId,
            styles: &mut Vec<String>,
            styled_button: &mut bool,
        ) {
            if let Some(style) =
                dom.attribute(node, &Namespace::from(""), &LocalName::from("style"))
            {
                styles.push(style.to_owned());
            }
            if dom
                .element_name(node)
                .is_some_and(|name| name.local.as_ref() == "button")
                && text_content(dom, node) == "About"
            {
                *styled_button = dom
                    .dom_children(node)
                    .any(|child| text_content(dom, child) == "About");
            }
            for child in dom.dom_children(node) {
                inspect(dom, child, styles, styled_button);
            }
        }
        let mut styles = Vec::new();
        let mut styled_button = false;
        inspect(&dom, dom.document(), &mut styles, &mut styled_button);
        assert!(
            styles
                .iter()
                .any(|style| style.contains("color:rgb(255,0,0)")
                    && style.contains("background-color:rgb(17,34,51)")
                    && style.contains("text-decoration:underline"))
        );
        assert!(
            styles
                .iter()
                .any(|style| style.contains("text-align:center")
                    && style.contains("padding-inline-start:calc(1 *"))
        );
        assert!(styled_button);
        assert_eq!(host.state().document().snapshot().text, source);
        assert!(!host.state().document().snapshot().dirty);
    }

    type DesktopHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

    /// A saved `.mu` file open in the editor with only its preview shown.
    fn micron_preview_harness(source: &str) -> (tempfile::TempDir, DesktopHarness) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("page.mu");
        std::fs::write(&path, source).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
        );
        show_preview(&mut state);
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: format!("{DESKTOP_CSS}{CSS}"),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        (temp, host)
    }

    fn preview_text(host: &DesktopHarness) -> String {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let preview = class_nodes(&dom, dom.document(), "knot-scroll-preview");
        assert_eq!(preview.len(), 1, "one Micron preview pane");
        text_content(&dom, preview[0])
    }

    fn class_nodes(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        class: &str,
    ) -> Vec<genet_scripted_dom::NodeId> {
        let mut found = Vec::new();
        if dom.has_class(node, class) {
            found.push(node);
        }
        for child in dom.dom_children(node) {
            found.extend(class_nodes(dom, child, class));
        }
        found
    }

    // Probe 03's two links, inline: both in-page links render as their label
    // inside an activatable link, present target or not.
    #[test]
    fn micron_preview_renders_in_page_links_as_activatable_links() {
        let source = "`[jump to a missing anchor`#no-such-anchor-here]\n`[jump to a present anchor`#present-control]\n`:present-control\nMARKER CONTROL: line bound by the present-control anchor.\n";
        let (_temp, host) = micron_preview_harness(source);
        let text = preview_text(&host);
        assert!(text.contains("jump to a missing anchor"), "{text:?}");
        assert!(text.contains("jump to a present anchor"), "{text:?}");
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let links = class_nodes(&dom, dom.document(), "knot-preview-in-page");
        assert_eq!(links.len(), 2);
        for link in links {
            assert!(
                dom.element_name(link)
                    .is_some_and(|name| name.local.as_ref() == "button"),
                "an in-page link is an activatable control"
            );
            assert!(host.runner().focusables().contains(&link));
        }
    }

    /// `pad {series}001` to `pad {series}060`, the filler the navigation probes
    /// use to push targets below the fold.
    fn pads(series: char, count: usize) -> String {
        (1..=count)
            .map(|line| format!("pad {series}{line:03}\n"))
            .collect()
    }

    // Mere `crates/nematic/nematic/tests/fixtures/micron/nomadnet-1.4.2/navigation/`
    // at 5dff2f93, rebuilt byte for byte (checked with `cmp` when written).
    fn probe_03() -> String {
        format!(
            "# Probe 03: a link to an anchor that is never declared.\n`!PROBE 03 TOP`!\n`[jump to a missing anchor`#no-such-anchor-here]\n`[jump to a present anchor`#present-control]\nThe second link is the positive control for the same gesture.\n{}`:present-control\nMARKER CONTROL: line bound by the present-control anchor.\n{}>Probe 03 end\nEnd of probe 03.\n",
            pads('a', 60),
            pads('b', 60)
        )
    }

    fn probe_04() -> String {
        format!(
            "# Probe 04: the next-heading jump activated from below the last heading.\n`!PROBE 04 TOP`!\n`[next heading from the top`#]\n{}>First Heading\nMARKER FIRST HEADING.\n{}>Last Heading\nMARKER LAST HEADING: no heading follows this one.\n{}`[next heading from the tail`#]\nMARKER TAIL: the link above sits below every heading on this page.\n{}",
            pads('a', 60),
            pads('b', 60),
            pads('c', 60),
            pads('d', 6)
        )
    }

    fn probe_05() -> String {
        format!(
            "# Probe 05: an anchor whose target lies inside an initially closed section.\n`!PROBE 05 TOP`!\n`[jump to the hidden heading`#hidden-target]\n`[jump to the hidden explicit anchor`#hidden-explicit]\n{}`->Closed Outer\nMARKER OUTER BODY: first line inside the closed section.\n>>Hidden Target\nMARKER HIDDEN: body under the hidden heading.\n`:hidden-explicit\nMARKER HIDDEN EXPLICIT: line bound inside the closed section.\n>Sentinel After\nMARKER SENTINEL: this heading ends the closed section's fold.\n{}>Probe 05 end\nEnd of probe 05.\n",
            pads('a', 60),
            pads('b', 60)
        )
    }

    fn probe_17() -> String {
        format!(
            "# Probe 17: an anchor inside a closed fold that is itself inside a closed fold.\n`!PROBE 17 TOP`!\n`[jump into two closed sections`#nested-deep]\n{}`->Outer Closed\nMARKER OUTER BODY: inside the outer fold only.\n`->>Inner Closed\nMARKER INNER BODY: inside the inner fold.\n`:nested-deep\nMARKER NESTED DEEP TARGET: bound inside both closed folds.\n>Sentinel After\nMARKER SENTINEL: ends both folds.\n{}>Probe 17 end\nEnd of probe 17.\n",
            pads('a', 60),
            pads('b', 60)
        )
    }

    /// Everything an in-page jump must leave alone.
    #[derive(Debug, PartialEq)]
    struct Authored {
        text: String,
        dirty: bool,
        address: String,
        saved_page: Vec<u8>,
        manifest: Vec<u8>,
        manifest_page: Option<String>,
    }

    /// A Micron site whose `about.mu` holds `source`, open in the editor with
    /// its preview shown and its manifest page selected.
    fn micron_site_preview_harness(source: &str) -> (tempfile::TempDir, DesktopHarness) {
        let temp = tempfile::tempdir().unwrap();
        let site = Site::create_for(&temp.path().join("micron"), SiteFormat::Micron).unwrap();
        let path = site.page_path("about.mu").unwrap();
        std::fs::write(&path, source).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
        );
        state.hold_site(site);
        show_preview(&mut state);
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        assert_eq!(authored(&host).manifest_page.as_deref(), Some("about.mu"));
        (temp, host)
    }

    fn authored(host: &DesktopHarness) -> Authored {
        let state = host.state();
        let snapshot = state.document().snapshot();
        let site = &state
            .scroll
            .current_site()
            .expect("a site-backed harness")
            .site;
        Authored {
            saved_page: std::fs::read(site.page_path("about.mu").unwrap()).unwrap(),
            manifest: std::fs::read(site.root().join(knot_site::CONFIG)).unwrap(),
            manifest_page: state.site_page().map(|page| page.name.clone()),
            text: snapshot.text,
            dirty: snapshot.dirty,
            address: snapshot.source.address,
        }
    }

    /// The preview element of the block `anchor` names in `source`.
    fn anchor_block(
        host: &DesktopHarness,
        source: &str,
        anchor: &str,
    ) -> genet_scripted_dom::NodeId {
        let address = host.state().document().snapshot().source.address;
        let document = lower_micron(&address, source).unwrap();
        let block = document
            .navigation
            .resolve(anchor)
            .unwrap_or_else(|| panic!("#{anchor} resolves"));
        let dom = host.runner().dom();
        let dom = dom.borrow();
        preview_block_node(&*dom, dom.document(), &block.to_string())
            .unwrap_or_else(|| panic!("block {block} for #{anchor} is rendered"))
    }

    /// The in-page link labelled `label`. Its label sits in nested spans, which
    /// a taproot selector's shallow text match does not read.
    fn in_page_link(host: &DesktopHarness, label: &str) -> genet_scripted_dom::NodeId {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        class_nodes(&dom, dom.document(), "knot-preview-in-page")
            .into_iter()
            .find(|node| text_content(&dom, *node) == label)
            .unwrap_or_else(|| panic!("no in-page link {label:?}"))
    }

    /// Click the centre of the in-page link labelled `label`, then lay out.
    fn click_in_page_link(host: &mut DesktopHarness, label: &str) {
        let (x, y, width, height) = host
            .painted_rect(in_page_link(host, label))
            .expect("the in-page link paints");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.relayout();
    }

    /// The area the preview scrolls in: its document tile's content.
    fn pane(host: &DesktopHarness) -> genet_scripted_dom::NodeId {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let preview = class_nodes(&dom, dom.document(), "knot-scroll-preview")
            .into_iter()
            .next()
            .expect("the preview renders");
        let mut node = dom.parent(preview);
        while let Some(current) = node {
            if dom.has_class(current, "frisket-content") {
                return current;
            }
            node = dom.parent(current);
        }
        panic!("the preview sits in a tile")
    }

    /// How far the preview's tile has scrolled.
    fn pane_scroll(host: &DesktopHarness) -> f32 {
        host.element_scroll(pane(host)).1
    }

    #[track_caller]
    fn at_pane_top(host: &DesktopHarness, node: genet_scripted_dom::NodeId, what: &str) {
        let (_, top, _, _) = host.painted_rect(node).expect("the target paints");
        let (_, pane_top, _, _) = host.painted_rect(pane(host)).expect("the tile paints");
        assert!(
            (top - pane_top).abs() < 0.5,
            "{what}: painted top {top}, tile top {pane_top}, tile scroll {}",
            pane_scroll(host)
        );
        assert!(pane_scroll(host) > 0.0, "{what}: the tile scrolled");
        assert_eq!(
            host.viewport_scroll(),
            (0.0, 0.0),
            "{what}: the window stayed"
        );
    }

    // Probe 05: stock opens the closed section and puts the target at the top
    // of the viewport, for a heading and for an explicit anchor alike.
    #[test]
    fn micron_in_page_link_opens_the_closed_section_and_scrolls_its_target_to_the_top() {
        let source = probe_05();
        let (_temp, mut host) = micron_site_preview_harness(&source);
        let before = authored(&host);
        let text = preview_text(&host);
        assert!(!text.contains("MARKER HIDDEN"), "authored closed: {text:?}");
        assert!(text.contains(&fold_label(false, "Closed Outer")));
        assert_eq!(pane_scroll(&host), 0.0);

        click_in_page_link(&mut host, "jump to the hidden heading");
        let text = preview_text(&host);
        assert!(
            text.contains("MARKER HIDDEN: body under the hidden heading."),
            "the closed ancestor opened: {text:?}"
        );
        assert!(text.contains(&fold_label(true, "Closed Outer")));
        let target = anchor_block(&host, &source, "hidden-target");
        at_pane_top(&host, target, "#hidden-target");
        assert!(
            host.state().entry().micron_folds.jump.is_none(),
            "the jump is spent"
        );

        // Back to the top, then the explicit anchor in the now-open section.
        host.wheel(0.0, -100_000.0);
        assert_eq!(pane_scroll(&host), 0.0);
        click_in_page_link(&mut host, "jump to the hidden explicit anchor");
        let target = anchor_block(&host, &source, "hidden-explicit");
        at_pane_top(&host, target, "#hidden-explicit");

        assert_eq!(
            authored(&host),
            before,
            "an in-page jump is preview state only"
        );
    }

    // Probe 17: both closed folds around the target open before the scroll.
    #[test]
    fn micron_in_page_link_into_two_closed_sections_opens_both_then_scrolls() {
        let source = probe_17();
        let (_temp, mut host) = micron_site_preview_harness(&source);
        let before = authored(&host);
        let text = preview_text(&host);
        assert!(!text.contains("MARKER OUTER BODY") && !text.contains("MARKER NESTED DEEP"));
        assert!(
            !text.contains("Inner Closed"),
            "the inner heading is hidden too"
        );

        click_in_page_link(&mut host, "jump into two closed sections");
        let text = preview_text(&host);
        assert!(text.contains(&fold_label(true, "Outer Closed")), "{text:?}");
        assert!(text.contains(&fold_label(true, "Inner Closed")), "{text:?}");
        assert!(text.contains("MARKER NESTED DEEP TARGET"), "{text:?}");
        let target = anchor_block(&host, &source, "nested-deep");
        at_pane_top(&host, target, "#nested-deep");
        assert_eq!(authored(&host), before);
    }

    // Probes 03 and 04: a missing anchor and a `#` below the last heading are
    // inert. Each page's working link is the control for the same gesture.
    #[test]
    fn micron_in_page_link_to_a_missing_target_is_inert() {
        let source = probe_03();
        let (_temp, mut host) = micron_site_preview_harness(&source);
        let before = authored(&host);
        let (x, y, width, height) = host
            .painted_rect(in_page_link(&host, "jump to a missing anchor"))
            .expect("the link paints");
        host.move_to(x + width / 2.0, y + height / 2.0);
        host.wheel(0.0, 20.0);
        let scrolled = pane_scroll(&host);
        assert!(scrolled > 0.0, "a starting offset the jump could disturb");
        let text = preview_text(&host);
        let folds = host.state().entry().micron_folds.folds.clone();

        click_in_page_link(&mut host, "jump to a missing anchor");
        assert_eq!(pane_scroll(&host), scrolled, "no scroll");
        assert_eq!(
            host.state().entry().micron_folds.folds,
            folds,
            "no fold change"
        );
        assert!(host.state().entry().micron_folds.jump.is_none());
        assert_eq!(preview_text(&host), text);
        assert_eq!(authored(&host), before);

        click_in_page_link(&mut host, "jump to a present anchor");
        let target = anchor_block(&host, &source, "present-control");
        at_pane_top(&host, target, "probe 03 control");

        let source = probe_04();
        let (_temp, mut host) = micron_site_preview_harness(&source);
        let before = authored(&host);
        let (x, y, width, height) = host
            .painted_rect(in_page_link(&host, "next heading from the top"))
            .expect("the link paints");
        host.move_to(x + width / 2.0, y + height / 2.0);
        host.wheel(0.0, 100_000.0);
        let bottom = pane_scroll(&host);
        assert!(bottom > 0.0);
        click_in_page_link(&mut host, "next heading from the tail");
        assert_eq!(
            pane_scroll(&host),
            bottom,
            "`#` below every heading is inert"
        );
        assert!(host.state().entry().micron_folds.jump.is_none());
        assert_eq!(authored(&host), before);
        host.wheel(0.0, -100_000.0);
        click_in_page_link(&mut host, "next heading from the top");
        let target = anchor_block(&host, &source, "first-heading");
        at_pane_top(&host, target, "probe 04 control");

        // Neither probe has a collapsible section, so this composed page (not a
        // stock capture) puts probe 03's missing link above a closed one.
        let source = format!(
            "`[jump to a missing anchor`#no-such-anchor-here]\n{}`->Closed Outer\nMARKER OUTER BODY.\n",
            pads('a', 60)
        );
        let (_temp, mut host) = micron_site_preview_harness(&source);
        click_in_page_link(&mut host, "jump to a missing anchor");
        assert_eq!(
            host.state().entry().micron_folds.folds,
            FoldState::default(),
            "no fold opens"
        );
        assert!(!preview_text(&host).contains("MARKER OUTER BODY"));
        assert_eq!(pane_scroll(&host), 0.0);
    }

    // Enter on a focused in-page link lands exactly where a click does.
    #[test]
    fn micron_in_page_link_activates_with_enter_like_a_click() {
        let source = probe_05();
        let (_clicked_temp, mut clicked) = micron_site_preview_harness(&source);
        click_in_page_link(&mut clicked, "jump to the hidden heading");

        let (_temp, mut host) = micron_site_preview_harness(&source);
        let before = authored(&host);
        let link = in_page_link(&host, "jump to the hidden heading");
        for _ in 0..host.runner().focusables().len() {
            if host.focus() == Some(link) {
                break;
            }
            host.tab(true);
        }
        assert_eq!(host.focus(), Some(link), "Tab reaches the in-page link");
        assert!(
            !preview_text(&host).contains("MARKER HIDDEN"),
            "focus alone does nothing"
        );

        host.press_key(&KeyPress::named(NamedKey::Enter));
        let target = anchor_block(&host, &source, "hidden-target");
        at_pane_top(&host, target, "Enter");
        assert_eq!(preview_text(&host), preview_text(&clicked));
        assert_eq!(pane_scroll(&host), pane_scroll(&clicked));
        assert_eq!(
            host.state().entry().micron_folds.folds,
            clicked.state().entry().micron_folds.folds
        );
        assert_eq!(authored(&host), before);
    }

    // Mere `crates/nematic/nematic/tests/fixtures/micron/nomadnet-1.4.2/`,
    // inlined: `guide-structure.mu` and the navigation probes 08 and 06b.
    const GUIDE_STRUCTURE: &str = "# Reference fixture: heading, collapse, divider, and table forms displayed in Guide.\n>Top heading\nTop body.\n---\n>>Nested heading\nNested body.\n\n>Fold examples\n`+>Open fold\nOpen fold body.\n`->Closed fold\nClosed fold body.\n\n>Table example\n`t\n| Name | Price | Qty |\n| ---- | :---: | --: |\n| `F3a3Apple`f | Free | `!5`! |\n| Orange | Ask nicely | 3 |\n`t\n\n>>>>\nAn unnamed depth-four section.\n";
    const PROBE_08: &str = "# Probe 08: Enter and Space on a focused collapsible heading.\n`!PROBE 08 TOP`!\n`->Enter Target\nMARKER ENTER BODY: revealed only when Enter Target is open.\n>Sentinel One\nMARKER SENTINEL ONE.\n`->Space Target\nMARKER SPACE BODY: revealed only when Space Target is open.\n>Sentinel Two\nMARKER SENTINEL TWO.\n`+>Already Open\nMARKER ALREADY OPEN BODY.\n>Probe 08 end\nEnd of probe 08.\n";
    const PROBE_06B: &str = "# Probe 06b: a closed section containing depth-two collapsible headings.\n`!PROBE 06B TOP`!\n`->Outer Closed\nMARKER OUTER: body directly under the closed outer heading.\n`+>>Inner Authored Open\nMARKER INNER OPEN: body under the depth-two heading authored open.\n`->>Inner Authored Closed\nMARKER INNER CLOSED: body under the depth-two heading authored closed.\n>Sentinel After\nMARKER SENTINEL: always visible, ends the outer fold.\n";

    fn fold_label(open: bool, heading: &str) -> String {
        format!("{}{heading}", FoldMarkers::default().marker(open))
    }

    /// Replace the whole source, then run the dispatch tail as a real edit does.
    fn replace_source(host: &mut DesktopHarness, text: &str) {
        host.update(|state| {
            state
                .document_mut()
                .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
                .unwrap();
            state
                .document_mut()
                .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(text.into())))
                .unwrap();
        });
        host.after_dispatch();
        host.relayout();
    }

    #[test]
    fn micron_preview_skips_closed_extents_and_marks_collapsible_headings() {
        let (_temp, mut host) = micron_preview_harness(GUIDE_STRUCTURE);
        let text = preview_text(&host);
        for shown in [
            "Top body.",
            "Nested body.",
            "Open fold body.",
            "An unnamed depth-four section.",
        ] {
            assert!(text.contains(shown), "{shown:?} missing from {text:?}");
        }
        assert!(!text.contains("Closed fold body."), "{text:?}");
        assert!(text.contains(&fold_label(true, "Open fold")));
        assert!(text.contains(&fold_label(false, "Closed fold")));
        {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            assert_eq!(
                class_nodes(&dom, dom.document(), "knot-micron-fold").len(),
                2,
                "only collapsible headings toggle"
            );
        }

        replace_source(&mut host, PROBE_06B);
        let text = preview_text(&host);
        assert!(text.contains("MARKER SENTINEL"));
        for hidden in [
            "MARKER OUTER",
            "Inner Authored Open",
            "MARKER INNER OPEN",
            "MARKER INNER CLOSED",
        ] {
            assert!(!text.contains(hidden), "{hidden:?} shown in {text:?}");
        }
        assert!(host.click_on(&Selector::role("button").containing("Outer Closed")));
        let text = preview_text(&host);
        assert!(text.contains("MARKER OUTER") && text.contains("MARKER INNER OPEN"));
        assert!(!text.contains("MARKER INNER CLOSED"));
        assert!(host.click_on(&Selector::role("button").containing("Inner Authored Open")));
        assert!(!preview_text(&host).contains("MARKER INNER OPEN"));
        assert!(host.click_on(&Selector::role("button").containing("Outer Closed")));
        assert!(!preview_text(&host).contains("MARKER OUTER"));
        assert!(host.click_on(&Selector::role("button").containing("Outer Closed")));
        let text = preview_text(&host);
        assert!(text.contains("MARKER OUTER"));
        assert!(
            !text.contains("MARKER INNER OPEN"),
            "a nested fold keeps its own state across the outer fold closing"
        );
    }

    #[test]
    fn micron_preview_folds_toggle_by_click_and_keyboard_without_writing_source() {
        let (temp, mut host) = micron_preview_harness(PROBE_08);
        let saved = std::fs::read(temp.path().join("page.mu")).unwrap();
        let address = host.state().document().snapshot().source.address;
        let text = preview_text(&host);
        assert!(!text.contains("MARKER ENTER BODY") && !text.contains("MARKER SPACE BODY"));
        assert!(text.contains("MARKER ALREADY OPEN BODY"));
        assert!(text.contains(&fold_label(false, "Enter Target")));

        assert!(host.click_on(&Selector::role("button").containing("Enter Target")));
        let text = preview_text(&host);
        assert!(
            text.contains("MARKER ENTER BODY"),
            "pointer opens: {text:?}"
        );
        assert!(text.contains(&fold_label(true, "Enter Target")));
        host.press_key(&KeyPress::named(NamedKey::Enter));
        assert!(
            !preview_text(&host).contains("MARKER ENTER BODY"),
            "Enter on the focused heading closes it"
        );

        assert!(host.click_on(&Selector::role("button").containing("Space Target")));
        assert!(preview_text(&host).contains("MARKER SPACE BODY"));
        host.press_key(&KeyPress::named(NamedKey::Space));
        assert!(
            !preview_text(&host).contains("MARKER SPACE BODY"),
            "Space on the focused heading closes it"
        );
        host.press_key(&KeyPress::named(NamedKey::Space));
        assert!(preview_text(&host).contains("MARKER SPACE BODY"));

        let snapshot = host.state().document().snapshot();
        assert_eq!(snapshot.text, PROBE_08, "toggling never writes source");
        assert!(!snapshot.dirty);
        assert_eq!(snapshot.source.address, address);
        assert_eq!(std::fs::read(temp.path().join("page.mu")).unwrap(), saved);
    }

    #[test]
    fn micron_preview_fold_state_survives_unrelated_edits_and_drops_for_an_edited_heading() {
        let (_temp, mut host) = micron_preview_harness(GUIDE_STRUCTURE);
        assert!(host.click_on(&Selector::role("button").containing("Closed fold")));
        assert!(preview_text(&host).contains("Closed fold body."));

        // Lines above shift every source line and block index; the key holds.
        let unrelated = format!(
            "Inserted first line.\n{}",
            GUIDE_STRUCTURE.replace("Top body.", "Top body, edited.")
        );
        replace_source(&mut host, &unrelated);
        let text = preview_text(&host);
        assert!(text.contains("Top body, edited."));
        assert!(
            text.contains("Closed fold body."),
            "state kept across an unrelated edit"
        );

        // Editing the heading drops its state, so undoing the edit shows the
        // authored closed fold rather than the reader's open one.
        replace_source(
            &mut host,
            &unrelated.replace("`->Closed fold", "`->Closed folds"),
        );
        replace_source(&mut host, &unrelated);
        assert!(
            !preview_text(&host).contains("Closed fold body."),
            "an edited heading drops its fold state"
        );

        // A marker flip is a heading edit too (decision 11).
        assert!(host.click_on(&Selector::role("button").containing("Closed fold")));
        assert!(preview_text(&host).contains("Closed fold body."));
        replace_source(
            &mut host,
            &unrelated.replace("`->Closed fold", "`+>Closed fold"),
        );
        replace_source(&mut host, &unrelated);
        assert!(
            !preview_text(&host).contains("Closed fold body."),
            "a flipped marker drops its fold state"
        );
        assert_eq!(host.state().document().snapshot().text, unrelated);
    }

    #[test]
    fn micron_preview_link_inside_a_collapsible_heading_does_not_toggle_it() {
        let (_temp, mut host) = micron_preview_harness(
            "`->Closed `[About`:/page/about.mu]\nHidden body.\n>After\nAfter body.\n",
        );
        assert!(!preview_text(&host).contains("Hidden body."));
        let link = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            class_nodes(&dom, dom.document(), "knot-scroll-link")
                .into_iter()
                .find(|node| text_content(&dom, *node) == "About")
                .expect("heading link")
        };
        assert!(host.painted_rect(link).is_some(), "the nested link paints");
        host.update(|state| {
            let document = state.focused_key().unwrap();
            activate_preview_link(state, document, ":/page/about.mu");
        });
        assert!(
            host.state()
                .message
                .as_deref()
                .is_some_and(|message| message.starts_with("Preview link: :/page/about.mu")),
            "message was {:?}; link was {:?}",
            host.state().message,
            link,
        );
        assert!(
            !preview_text(&host).contains("Hidden body."),
            "the link click did not reach the fold toggle"
        );
        // The same heading row outside the link does toggle.
        let marker = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            class_nodes(&dom, dom.document(), "knot-micron-fold-marker")[0]
        };
        assert!(host.painted_rect(marker).is_some(), "marker layout");
        host.update(|state| {
            let document = state.focused_key().unwrap();
            let source = state.surface_for(document).snapshot();
            let rendered = lower_micron(&source.source.address, &source.text).unwrap();
            let key = rendered.navigation.fold_keys()[0].clone();
            state.toggle_micron_fold_for(document, &key);
        });
        assert!(preview_text(&host).contains("Hidden body."));
    }

    // Decision 19: one focus rule for every control. The harness exposes no
    // computed style, so beside the shipped rule a probe rule on the same
    // selector changes geometry: it proves the selector parses and matches
    // wherever keyboard focus lands, and outranks the pressed-toggle rule even
    // when that comes later in the sheet.
    #[test]
    fn keyboard_focus_matches_the_one_focus_rule_on_every_kind_of_control() {
        let selector = crate::appearance::focus_selector(".knot-theme-light");
        let sheet = crate::desktop_sheet();
        assert_eq!(
            sheet
                .matches(&format!("{selector} {{ outline:2px solid "))
                .count(),
            1,
            "the shipped sheet carries the focus rule"
        );
        let probe = format!(
            "{sheet}{selector} {{ min-height:123px; }} .knot-workspace button[aria-pressed=true] {{ min-height:40px; }}"
        );
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("page.mu");
        std::fs::write(
            &path,
            "`[jump to the heading below`#below]\n`->Closed fold\nHidden body.\n>Below\nBody.\n",
        )
        .unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
        );
        show_preview(&mut state);
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: probe,
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        assert!(host.click_on(&Selector::role("button").containing("Appearance")));
        // A text input to Tab to that leaves the frame's layout alone: the
        // Site popover's folder field.
        host.update(|state| state.site_popover_event(PopoverEvent::Toggle));
        host.relayout();

        let controls = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let button = |label: &str| {
                taproot::matching(&dom, &Selector::role("button").containing(label))[0]
            };
            let folder_field = node_with_id(&dom, dom.document(), "knot-scroll-folder").unwrap();
            vec![
                (
                    "fold toggle",
                    class_nodes(&dom, dom.document(), "knot-micron-fold")[0],
                ),
                ("ordinary button", button("Show Outline")),
                ("pressed toggle", button("Light")),
                ("text input", input_node(&dom, folder_field).unwrap()),
            ]
        };
        let mut controls = controls
            .into_iter()
            .chain([(
                "in-page link",
                in_page_link(&host, "jump to the heading below"),
            )])
            .collect::<Vec<_>>();
        // One forward Tab pass visits them all.
        let order = host.runner().focusables();
        controls.sort_by_key(|(_, node)| order.iter().position(|each| each == node));
        let height = |host: &DesktopHarness, node| host.painted_rect(node).expect("paints").3;
        let mut previous = None;
        for (what, node) in controls {
            assert!(height(&host, node) < 100.0, "{what} unfocused");
            for _ in 0..order.len() {
                if host.focus() == Some(node) {
                    break;
                }
                host.tab(true);
            }
            assert_eq!(host.focus(), Some(node), "Tab reaches the {what}");
            assert!(
                height(&host, node) > 122.5,
                "the focus rule matches the focused {what}: {}",
                height(&host, node)
            );
            if let Some((what, node)) = previous {
                assert!(height(&host, node) < 100.0, "{what} after focus leaves");
            }
            previous = Some((what, node));
        }
        let (what, node) = previous.unwrap();
        host.tab(true);
        assert!(height(&host, node) < 100.0, "{what} after focus leaves");
    }

    #[test]
    fn selecting_a_spartan_prompt_only_fills_the_local_composer() {
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:prompt", ""),
            WindowCommands::new(),
        );
        assert!(state.docs.following_reading(ReadingKind::Submit).is_none());
        state.entry_mut().site.submission.token = TextInput::new("stale-token");
        state.select_spartan_prompt("spartan://example.test:3000/submit".into());

        assert!(state.docs.following_reading(ReadingKind::Submit).is_some());
        let submission = &state.entry().site.submission;
        assert_eq!(
            submission.target.text(),
            "spartan://example.test:3000/submit"
        );
        assert_eq!(submission.mime.text(), "text/plain");
        assert!(submission.prepared.is_none());
        assert!(submission.receiver.is_none());
        assert!(submission.token.text().is_empty());
    }

    #[test]
    fn reviewed_submission_is_consumed_before_send_and_cancel_has_no_effect() {
        let mut submission = Submission::default();
        submission.prepared = Some(
            PreparedSubmission::from_body(
                "titan://example.test/upload",
                "text/gemini",
                b"reviewed source".to_vec(),
            )
            .unwrap(),
        );
        submission.token = TextInput::new("single-use-token");

        let (prepared, token) = submission.take_for_send().unwrap();
        assert_eq!(prepared.body(), b"reviewed source");
        assert_eq!(token.as_deref(), Some("single-use-token"));
        assert!(submission.prepared.is_none());
        assert!(submission.token.text().is_empty());
        assert!(submission.receiver.is_none());

        submission.prepared = Some(
            PreparedSubmission::from_body(
                "spartan://example.test:3000/submit",
                "text/plain",
                b"cancel me".to_vec(),
            )
            .unwrap(),
        );
        submission.token = TextInput::new("must-not-survive");
        submission.discard();
        assert!(submission.prepared.is_none());
        assert!(submission.token.text().is_empty());
        assert!(submission.receiver.is_none());
    }

    #[test]
    fn response_display_is_inert_text_and_explicitly_caps_large_bodies() {
        assert_eq!(response_display(b"accepted\n"), Some("accepted\n".into()));
        let body = vec![b'x'; RESPONSE_DISPLAY_LIMIT + 1];
        let displayed = response_display(&body).unwrap();
        assert!(displayed.starts_with(&"x".repeat(RESPONSE_DISPLAY_LIMIT)));
        assert!(displayed.contains("Response truncated after 8192 bytes; 8193 bytes received."));
    }

    fn submission_harness_with(
        state: DesktopState,
    ) -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
        let mut host = desktop_harness_with(state);
        host.update(|state| state.toggle_reading(ReadingKind::Submit));
        host.layout_at(1100.0, 730.0);
        host
    }

    fn desktop_harness_with(
        state: DesktopState,
    ) -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        host
    }

    fn submission_harness() -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>
    {
        submission_harness_with(DesktopState::new(
            KnotDocumentSession::scratch("scratch:submission", "document source"),
            WindowCommands::new(),
        ))
    }

    fn node_with_id(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
        id: &str,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom.attribute(node, &Namespace::from(""), &LocalName::from("id")) == Some(id) {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| node_with_id(dom, child, id))
    }

    fn textarea_node(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
    ) -> Option<genet_scripted_dom::NodeId> {
        if dom
            .element_name(node)
            .is_some_and(|name| name.local.as_ref() == "textarea")
        {
            return Some(node);
        }
        dom.dom_children(node)
            .find_map(|child| textarea_node(dom, child))
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

    fn text_content(
        dom: &genet_scripted_dom::ScriptedDom,
        node: genet_scripted_dom::NodeId,
    ) -> String {
        format!(
            "{}{}",
            dom.text(node).unwrap_or_default(),
            dom.dom_children(node)
                .map(|child| text_content(dom, child))
                .collect::<String>()
        )
    }

    // The preview pane is not its own scroll container: `.knot-scroll-preview`
    // grows to content height, so the window viewport carries the offset. The
    // form panel switching to its sending view must not move that offset.
    #[test]
    fn a_micron_submission_redraw_keeps_the_scrolled_preview_where_it_was() {
        let mut source = String::from(
            "`[Submit`0123456789abcdef0123456789abcdef:/capture`name]
`<name`seed>
",
        );
        for index in 0..400 {
            source.push_str(&format!(
                "Line {index} of a long Micron page.
"
            ));
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("long.mu");
        std::fs::write(&path, &source).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
        );
        show_preview(&mut state);
        let address = state.document().snapshot().source.address;
        state
            .entry_mut()
            .site
            .micron
            .open_form(source.clone(), address)
            .unwrap();
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: format!("{DESKTOP_CSS}{CSS}"),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        let (x, y) = host
            .resolve(&Selector::role("button").containing("Close Micron form"))
            .expect("the form panel is rendered in the preview");
        host.move_to(x, y);
        host.wheel(0.0, 300.0);
        let scrolled = host.element_scroll_total();
        assert!(scrolled > 0.0, "the preview did not scroll");
        let (_sender, receiver) = mpsc::channel();
        host.update(|state| {
            let micron = &mut state.entry_mut().site.micron;
            micron.receiver = Some(receiver);
            micron.result = Some("Sending reviewed Micron request".into());
        });
        host.relayout();
        assert_eq!(
            scrolled,
            host.element_scroll_total(),
            "the sending redraw moved the preview"
        );
        assert!(
            host.resolve(&Selector::role("button").containing("Cancel Micron request"))
                .is_some(),
            "the cancel control is not reachable while sending"
        );
    }

    #[test]
    fn spartan_body_injected_text_uses_its_own_clicked_caret_slot() {
        let mut host = submission_harness();
        host.update(|state| state.entry_mut().site.submission.body = TextInput::new("body"));
        host.layout_at(1100.0, 730.0);
        let textarea = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let body = class_nodes(&dom, dom.document(), "knot-spartan-body")[0];
            textarea_node(&dom, body).unwrap()
        };
        let (x, y, _, _) = host.painted_rect(textarea).unwrap();
        host.click_at(x + 1.0, y + 24.0);
        host.key_injected("署名");
        assert_eq!(host.state().entry().site.submission.body.text(), "署名body");
        assert_eq!(host.state().document().snapshot().text, "document source");
    }

    #[test]
    fn loaded_spartan_site_prompt_opens_composer_without_sending() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("spartan-site");
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:spartan", ""),
            WindowCommands::new(),
        );
        state.scroll.format = SiteFormat::Spartan;
        state.scroll.folder = TextInput::new(root.to_string_lossy());
        state.enter_site(true);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
            .unwrap();
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "=: spartan://localhost:65025/upload Submit locally\n".into(),
            )))
            .unwrap();
        show_preview(&mut state);
        let mut host = desktop_harness_with(state);

        assert!(host.click_on(&Selector::role("button").containing("Submit locally")));
        assert!(
            host.state()
                .docs
                .following_reading(ReadingKind::Submit)
                .is_some()
        );
        assert_eq!(
            host.state().entry().site.submission.target.text(),
            "spartan://localhost:65025/upload"
        );
        assert!(host.state().entry().site.submission.prepared.is_none());
        assert!(host.state().entry().site.submission.receiver.is_none());
    }

    #[test]
    fn titan_token_field_is_password_typed_and_does_not_paint_its_value() {
        let mut host = submission_harness();
        host.update(|state| {
            state.entry_mut().site.submission.prepared = Some(
                PreparedSubmission::from_body(
                    "titan://example.test/upload",
                    "text/gemini",
                    b"saved".to_vec(),
                )
                .unwrap(),
            );
            state.entry_mut().site.submission.token = TextInput::new("dummytokenonly");
        });
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let label = class_nodes(&dom, dom.document(), "knot-submission-token")[0];
        let input = input_node(&dom, label).unwrap();
        assert_eq!(
            dom.attribute(input, &Namespace::from(""), &LocalName::from("type")),
            Some("password")
        );
        assert!(!text_content(&dom, label).contains("dummytokenonly"));
    }
}
