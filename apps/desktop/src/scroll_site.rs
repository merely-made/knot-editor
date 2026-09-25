// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::workspace::{DesktopState, DesktopView};
use cambium::{
    El, GenetCtx, GenetElement, KeyEvent, Keyed, TextFieldMode, TextInput, View, button,
    button_with, el, lens, on_key, span, text_field_typed, textarea_typed,
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
use std::sync::mpsc::{self, Receiver, TryRecvError};

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

pub struct ScrollWorkspace {
    pub folder: TextInput,
    pub port: TextInput,
    pub fields: [TextInput; 6],
    baseline: [String; 6],
    pub site: Option<Site>,
    page: Option<String>,
    pub server: Option<LocalServer>,
    pub visible: bool,
    metadata_visible: bool,
    pub preview_visible: bool,
    publication_number: usize,
    pub format: SiteFormat,
    submission_visible: bool,
    pub(crate) submission_target: TextInput,
    pub(crate) submission_mime: TextInput,
    pub(crate) submission_body: TextInput,
    pub(crate) submission_token: TextInput,
    prepared: Option<PreparedSubmission>,
    pub(crate) submission_receiver: Option<Receiver<Result<SubmissionReceipt, String>>>,
    submission_wake: Option<HostWake>,
    submission_result: Option<String>,
    submission_response: Option<String>,
    titan_submission_error: Option<String>,
    micron_form: Option<MicronFormEditor>,
    micron_submission_receiver: Option<Receiver<(u64, Result<MicronResponse, String>)>>,
    next_micron_submission: u64,
    active_micron_submission: Option<(u64, String, String)>,
    micron_cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Default for ScrollWorkspace {
    fn default() -> Self {
        Self {
            folder: TextInput::default(),
            port: TextInput::new("5699"),
            fields: std::array::from_fn(|_| TextInput::default()),
            baseline: Default::default(),
            site: None,
            page: None,
            server: None,
            visible: false,
            metadata_visible: false,
            preview_visible: true,
            publication_number: 0,
            format: SiteFormat::Scroll,
            submission_visible: false,
            submission_target: TextInput::default(),
            submission_mime: TextInput::new("text/gemini"),
            submission_body: TextInput::default(),
            submission_token: TextInput::default(),
            prepared: None,
            submission_receiver: None,
            submission_wake: None,
            submission_result: None,
            submission_response: None,
            titan_submission_error: None,
            micron_form: None,
            micron_submission_receiver: None,
            next_micron_submission: 0,
            active_micron_submission: None,
            micron_cancel: None,
        }
    }
}

impl ScrollWorkspace {
    pub fn set_titan_submission_error(&mut self, error: Option<String>) {
        self.titan_submission_error = error;
    }

    pub(crate) fn submission_busy(&self) -> bool {
        self.submission_receiver.is_some() || self.micron_submission_receiver.is_some()
    }

    fn select_spartan_prompt(&mut self, target: String) {
        self.submission_visible = true;
        self.submission_target = TextInput::new(target);
        self.submission_mime = TextInput::new("text/plain");
        self.prepared = None;
        self.submission_token = TextInput::default();
        self.submission_result = None;
        self.submission_response = None;
    }

    fn discard_submission(&mut self) {
        self.prepared = None;
        self.submission_token = TextInput::default();
        self.submission_result = None;
        self.submission_response = None;
    }

    fn take_submission_for_send(&mut self) -> Result<(PreparedSubmission, Option<String>), String> {
        let prepared = self
            .prepared
            .take()
            .ok_or("Prepare reviewed bytes before sending.")?;
        let token = if prepared.target().starts_with("titan://") {
            let token = std::mem::take(&mut self.submission_token).text().to_owned();
            (!token.is_empty()).then_some(token)
        } else {
            self.submission_token = TextInput::default();
            None
        };
        Ok((prepared, token))
    }

    pub fn set_submission_wake(&mut self, wake: HostWake) {
        self.submission_wake = Some(wake);
    }
    pub fn drain_submission(&mut self, current_source: &str, current_address: &str) {
        if let Some(rx) = self.submission_receiver.as_ref() {
            match rx.try_recv() {
                Ok(Ok(r)) => {
                    self.submission_result = Some(format!(
                        "Reply {} {} ({} bytes)",
                        r.code,
                        r.meta,
                        r.body.len()
                    ));
                    self.submission_response = response_display(&r.body);
                    self.submission_receiver = None
                },
                Ok(Err(e)) => {
                    self.submission_result = Some(format!("Send failed: {e}"));
                    self.submission_response = None;
                    self.submission_receiver = None
                },
                Err(TryRecvError::Disconnected) => {
                    self.submission_result =
                        Some("Send outcome unavailable; check target before retrying.".into());
                    self.submission_response = None;
                    self.submission_receiver = None
                },
                Err(TryRecvError::Empty) => {},
            }
        }
        let Some(rx) = self.micron_submission_receiver.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok((id, Ok(response))) => {
                let current = self.active_micron_submission.as_ref().is_some_and(
                    |(active, source, address)| {
                        *active == id && source == current_source && address == current_address
                    },
                );
                self.micron_submission_receiver = None;
                self.active_micron_submission = None;
                self.micron_cancel = None;
                if !current {
                    return;
                }
                self.submission_result =
                    Some(format!("Micron reply ({} bytes)", response.body.len()));
                self.submission_response = response_display(&response.body);
            },
            Ok((id, Err(error))) => {
                let current = self.active_micron_submission.as_ref().is_some_and(
                    |(active, source, address)| {
                        *active == id && source == current_source && address == current_address
                    },
                );
                self.micron_submission_receiver = None;
                self.active_micron_submission = None;
                self.micron_cancel = None;
                if !current {
                    return;
                }
                self.submission_result = Some(format!("Micron request failed: {error}"));
                self.submission_response = None;
            },
            Err(TryRecvError::Disconnected) => {
                let current =
                    self.active_micron_submission
                        .as_ref()
                        .is_some_and(|(_, source, address)| {
                            source == current_source && address == current_address
                        });
                self.micron_submission_receiver = None;
                self.micron_cancel = None;
                self.active_micron_submission = None;
                if current {
                    self.submission_result =
                        Some("Micron request outcome unavailable; review before retrying.".into());
                    self.submission_response = None;
                }
            },
            Err(TryRecvError::Empty) => {},
        }
    }
    fn metadata_indices(&self) -> &'static [usize] {
        match self
            .site
            .as_ref()
            .map(|site| site.config.format)
            .unwrap_or(self.format)
        {
            SiteFormat::Scroll => &[0, 1, 2, 3, 4, 5],
            // Gemini has a native language parameter. The other Scroll-only
            // header/abstract fields are not silently projected into Gemtext.
            SiteFormat::Gemini => &[1],
            SiteFormat::Spartan | SiteFormat::Micron => &[],
        }
    }

    pub fn metadata_dirty(&self) -> bool {
        self.page.is_some()
            && self
                .fields
                .iter()
                .zip(&self.baseline)
                .any(|(a, b)| a.text() != b)
    }

    pub fn sync_page(&mut self, path: Option<&std::path::Path>) {
        let selected = self.site.as_ref().and_then(|site| {
            site.config
                .pages
                .iter()
                .find(|page| site.page_path(&page.path).ok().as_deref() == path && path.is_some())
        });
        let name = selected.map(|page| page.path.clone());
        if name == self.page {
            return;
        }
        self.page = name;
        if let Some(page) = selected {
            // Preserve the entire abstract in the text field when opening an
            // existing site; metadata saving never reformats its source.
            self.baseline = [
                page.author.clone(),
                page.language.clone(),
                page.classification.to_string(),
                page.published.clone(),
                page.modified.clone(),
                page.abstract_source.clone(),
            ];
        } else {
            self.baseline = Default::default();
        }
        self.fields = self.baseline.clone().map(TextInput::new);
    }

    fn save_metadata(&mut self) -> Result<(), String> {
        let site = self.site.as_mut().ok_or("Open a site first")?;
        let name = self.page.as_ref().ok_or("Open a page in this site first")?;
        let values: [String; 6] = std::array::from_fn(|i| self.fields[i].text().to_owned());
        let abstract_source = values[5].clone();
        let page = Page {
            path: name.clone(),
            author: values[0].clone(),
            language: values[1].clone(),
            classification: values[2]
                .parse()
                .map_err(|_| "Classification must be 0–9")?,
            published: values[3].clone(),
            modified: values[4].clone(),
            abstract_source,
        };
        let index = site
            .config
            .pages
            .iter()
            .position(|page| &page.path == name)
            .ok_or("Page missing")?;
        let previous = std::mem::replace(&mut site.config.pages[index], page);
        if let Err(error) = site.save_config_with(|path, before, after| {
            knot_document::write_if_distinct(path, before, after).map(|_| ())
        }) {
            site.config.pages[index] = previous;
            return Err(error);
        }
        self.baseline = values;
        Ok(())
    }

    fn open_micron_form(&mut self, source: String, address: String) -> Result<(), String> {
        let form = FormState::from_source(&source, FormLimits::default())?;
        if form.actions().is_empty() {
            return Err("This Micron page has no request action.".into());
        }
        let inputs = form
            .fields()
            .iter()
            .map(|field| TextInput::new(field.value()))
            .collect();
        self.micron_form = Some(MicronFormEditor {
            source,
            address,
            form,
            inputs,
            prepared: None,
        });
        Ok(())
    }

    fn close_micron_form(&mut self) {
        self.micron_form = None;
        // A reopened form must not show the previous reply. An in-flight send
        // keeps its own status, which its completion replaces.
        if !self.submission_busy() {
            self.submission_result = None;
            self.submission_response = None;
        }
    }

    fn cancel_micron_submission(&mut self) {
        if let Some(cancel) = self.micron_cancel.take() {
            let _ = cancel.send(());
            self.active_micron_submission = None;
            self.submission_result = Some(
                "Micron request cancelled locally. Its remote outcome may be unknown; do not retry automatically."
                    .into(),
            );
            self.submission_response = None;
        }
    }

    fn micron_set_checked(&mut self, index: usize, checked: bool) -> Result<(), String> {
        let editor = self
            .micron_form
            .as_mut()
            .ok_or("Open the Micron form first.")?;
        editor.form.set_checked(index, checked)?;
        editor.prepared = None;
        Ok(())
    }

    fn prepare_micron_form(
        &mut self,
        source: &str,
        address: &str,
        action: usize,
    ) -> Result<(), String> {
        let editor = self
            .micron_form
            .as_mut()
            .ok_or("Open the Micron form first.")?;
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
        let current = self.document().snapshot();
        let folds = &mut self.entry_mut().micron_folds;
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
    fn toggle_micron_fold(&mut self, key: &FoldKey) {
        self.sync_micron_folds();
        let current = self.document().snapshot();
        let Ok(document) = lower_micron(&current.source.address, &current.text) else {
            return;
        };
        let navigation = &document.navigation;
        let fold = navigation
            .is_current(&document.blocks)
            .then(|| navigation.fold_keys().iter().position(|each| each == key))
            .flatten();
        match fold {
            Some(fold) => {
                self.entry_mut().micron_folds.folds.toggle(navigation, fold);
            },
            None => {
                self.message = Some("That heading is no longer in the current source.".into());
            },
        }
    }

    /// Follow an in-page link (decision 18): open the folds hiding its target,
    /// so this dispatch renders it, and queue the scroll for `after_dispatch`.
    /// A target with no block is inert.
    fn follow_micron_in_page(&mut self, target: &InPageTarget) {
        let Some(block) = target.block else {
            return;
        };
        let current = self.document().snapshot();
        if current.format != knot_document::DocumentFormat::Micron {
            return;
        }
        self.sync_micron_folds();
        let Ok(document) = lower_micron(&current.source.address, &current.text) else {
            return;
        };
        let navigation = &document.navigation;
        if !navigation.is_current(&document.blocks) || block >= document.blocks.len() {
            return;
        }
        self.entry_mut()
            .micron_folds
            .folds
            .open_ancestors(navigation, block);
        self.entry_mut().micron_folds.jump = Some(target.clone());
    }

    fn open_micron_form(&mut self) {
        let source = self.document().snapshot();
        if source.format != knot_document::DocumentFormat::Micron {
            self.message = Some("Micron forms are available only for a Micron document.".into());
            return;
        }
        match self
            .scroll
            .open_micron_form(source.text, source.source.address)
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

    fn prepare_micron_form(&mut self, action: usize) {
        let source = self.document().snapshot();
        match self
            .scroll
            .prepare_micron_form(&source.text, &source.source.address, action)
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

    fn send_micron_form(&mut self) {
        if self.scroll.submission_busy() {
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
        let current_document = self.document().snapshot();
        let Some(editor) = self.scroll.micron_form.as_mut() else {
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
        let _ = editor;
        self.scroll.submission_result = Some("Sending reviewed Micron request…".into());
        self.scroll.submission_response = None;
        let (tx, rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        self.scroll.micron_submission_receiver = Some(rx);
        self.scroll.micron_cancel = Some(cancel_tx);
        let id = self.scroll.next_micron_submission;
        self.scroll.next_micron_submission = self.scroll.next_micron_submission.wrapping_add(1);
        self.scroll.active_micron_submission = Some((id, source_binding, address_binding));
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

    fn prepare_titan(&mut self) {
        if self.scroll.submission_busy() {
            self.message = Some("A submission is already sending.".into());
            return;
        }
        if let Some(error) = &self.scroll.titan_submission_error {
            self.message = Some(format!("Titan upload is unavailable: {error}"));
            return;
        }
        if self.document().snapshot().dirty || self.scroll.metadata_dirty() {
            self.message = Some("Save source and metadata before preparing Titan upload.".into());
            return;
        }
        let Some(path) = self.document().session().source_path() else {
            self.message = Some("Save the source file before preparing Titan upload.".into());
            return;
        };
        match PreparedSubmission::from_saved_file(
            path,
            self.scroll.submission_target.text(),
            self.scroll.submission_mime.text(),
        ) {
            Ok(p) if p.target().starts_with("titan://") => {
                self.scroll.prepared = Some(p);
                self.scroll.submission_result = None;
                self.scroll.submission_response = None
            },
            Ok(_) => self.message = Some("Titan preparation requires a titan:// target.".into()),
            Err(e) => self.message = Some(format!("Prepare failed: {e}")),
        }
    }
    fn prepare_spartan(&mut self) {
        if self.scroll.submission_busy() {
            self.message = Some("A submission is already sending.".into());
            return;
        }
        self.scroll.submission_token = TextInput::default();
        match PreparedSubmission::from_body(
            self.scroll.submission_target.text(),
            self.scroll.submission_mime.text(),
            self.scroll.submission_body.text().as_bytes().to_vec(),
        ) {
            Ok(p) if p.target().starts_with("spartan://") => {
                self.scroll.prepared = Some(p);
                self.scroll.submission_result = None;
                self.scroll.submission_response = None
            },
            Ok(_) => {
                self.message = Some("Spartan preparation requires a spartan:// target.".into())
            },
            Err(e) => self.message = Some(format!("Prepare failed: {e}")),
        }
    }
    fn send_submission(&mut self) {
        if self.scroll.submission_busy() {
            self.message = Some("A submission is already sending.".into());
            return;
        }
        let Some(wake) = self.scroll.submission_wake.clone() else {
            self.message = Some("Submission worker is unavailable.".into());
            return;
        };
        let (prepared, token) = match self.scroll.take_submission_for_send() {
            Ok(send) => send,
            Err(error) => {
                self.message = Some(error);
                return;
            },
        };
        self.scroll.submission_result = Some("Sending reviewed bytes…".into());
        let (tx, rx) = mpsc::channel();
        self.scroll.submission_receiver = Some(rx);
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
    fn enter_site(&mut self, create: bool) {
        if self.document().snapshot().dirty || self.scroll.metadata_dirty() {
            self.message =
                Some("Save or discard document and metadata changes before changing sites.".into());
            return;
        }
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
                self.scroll.close_micron_form();
                self.scroll.server = None;
                self.scroll.page = None;
                self.scroll.site = Some(site);
                self.scroll.format = self.scroll.site.as_ref().unwrap().config.format;
                let _ = self.open_path(path);
            },
            Err(error) => self.message = Some(format!("Site: {error}")),
        }
    }

    fn close_site(&mut self) {
        if self.document().snapshot().dirty || self.scroll.metadata_dirty() {
            self.message = Some(
                "Save or discard document and metadata changes before closing this site.".into(),
            );
            return;
        }
        self.scroll.server = None;
        self.scroll.close_micron_form();
        self.scroll.site = None;
        self.scroll.page = None;
        self.scroll.fields = std::array::from_fn(|_| TextInput::default());
        self.scroll.baseline = Default::default();
        self.message = Some("Site closed. The current source remains open.".into());
    }

    pub(crate) fn scroll_open_page(&mut self, name: &str) {
        let path = self
            .scroll
            .site
            .as_ref()
            .ok_or("Open a site first".to_owned())
            .and_then(|site| site.page_path(name));
        match path {
            Ok(path) => {
                self.open_path(path);
            },
            Err(error) => self.message = Some(error),
        }
    }

    fn publish_site(&mut self) {
        if self.document().snapshot().dirty || self.scroll.metadata_dirty() {
            self.message = Some("Save source and metadata before publishing locally.".into());
            return;
        }
        let result = (|| {
            let site = self.scroll.site.as_ref().ok_or("Open a site first")?;
            let publication = site.publication()?;
            let count = publication.page_count();
            if let Some(server) = &self.scroll.server {
                server.replace(publication)?;
            } else {
                let port = self
                    .scroll
                    .port
                    .text()
                    .parse::<u16>()
                    .map_err(|_| "Port must be 0–65535 (0 chooses a free port)")?;
                self.scroll.server = Some(LocalServer::start(publication, port)?);
            }
            self.scroll.publication_number += 1;
            Ok::<_, String>(format!(
                "Published {count} saved pages locally, revision {}. {}",
                self.scroll.publication_number,
                self.scroll.server.as_ref().unwrap().url()
            ))
        })();
        self.message = Some(result.unwrap_or_else(|e| format!("Publication failed: {e}")));
    }
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

fn password_input(
    label: &'static str,
    id: &'static str,
    get: fn(&mut DesktopState) -> &mut TextInput,
) -> DesktopView {
    Box::new(
        el(
            "label",
            (
                label,
                lens(|input: &mut TextInput| password_field(input), get),
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

pub fn site_panel(state: &DesktopState) -> DesktopView {
    if !state.scroll.visible {
        return Box::new(el("div", ()));
    }
    let pages: DesktopView = if let Some(site) = &state.scroll.site {
        let rows = site
            .config
            .pages
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let name = page.path.clone();
                (
                    i,
                    button(page.path.clone(), move |state: &mut DesktopState, _| {
                        state.scroll_open_page(&name)
                    }),
                )
            })
            .collect::<Vec<_>>();
        Box::new(el("nav", Keyed::new(rows)).attr("class", "knot-scroll-pages"))
    } else {
        Box::new(span(
            "Enter a new folder path to create a three-page site, or an existing folder to open it.",
        ))
    };
    let metadata: DesktopView = if state.scroll.metadata_visible && state.scroll.page.is_some() {
        let fields = state
            .scroll
            .metadata_indices()
            .iter()
            .map(|&i| {
                let label = LABELS[i];
                (
                    i,
                    el(
                        "label",
                        (
                            label,
                            lens(
                                move |input: &mut TextInput| {
                                    if i == 5 {
                                        textarea_typed(input)
                                    } else {
                                        text_field_typed(input)
                                    }
                                },
                                move |state: &mut DesktopState| &mut state.scroll.fields[i],
                            ),
                        ),
                    )
                    .attr("id", format!("knot-scroll-meta-{i}")),
                )
            })
            .collect::<Vec<_>>();
        let explanation = match state
            .scroll
            .site
            .as_ref()
            .map(|site| site.config.format)
            .unwrap_or(state.scroll.format)
        {
            SiteFormat::Scroll => {
                "Publication metadata for the selected page. The abstract is separate native Scrolltext and needs a # Title."
            },
            SiteFormat::Gemini => {
                "Gemini publication metadata for the selected page. Language is separate from native Gemtext source."
            },
            SiteFormat::Spartan | SiteFormat::Micron => {
                "This site format has no supported page metadata controls."
            },
        };
        Box::new(el(
            "section",
            (
                span(explanation),
                el("div", Keyed::new(fields)).attr("class", "knot-scroll-fields"),
                button("Save metadata", |state: &mut DesktopState, _| {
                    state.message = Some(
                        state
                            .scroll
                            .save_metadata()
                            .map(|_| "Metadata saved. Existing publication is unchanged.".into())
                            .unwrap_or_else(|e| e),
                    );
                }),
                button("Discard metadata edits", |state: &mut DesktopState, _| {
                    state.scroll.fields = state.scroll.baseline.clone().map(TextInput::new);
                }),
                span(if state.scroll.metadata_dirty() {
                    "Unsaved metadata"
                } else {
                    "Metadata saved"
                }),
            ),
        ))
    } else {
        Box::new(el("div", ()))
    };
    let format = state.scroll.format;
    let submission_busy = state.scroll.submission_busy();
    let review: DesktopView = if let Some(p) = state.scroll.prepared.as_ref() {
        let is_titan = p.target().starts_with("titan://");
        let send_action: DesktopView = if submission_busy {
            Box::new(span("Sending reviewed bytes…"))
        } else {
            Box::new(button("Send reviewed bytes", |s: &mut DesktopState, _| {
                s.send_submission()
            }))
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
                        password_input("Titan token", "knot-submission-token", |s| {
                            &mut s.scroll.submission_token
                        })
                    } else {
                        Box::new(span(
                            "Spartan sends this reviewed body without a Titan token.",
                        ))
                    },
                    send_action,
                    button("Cancel reviewed submission", |s: &mut DesktopState, _| {
                        s.scroll.discard_submission()
                    }),
                ),
            )
            .attr("class", "knot-submission-review"),
        )
    } else if submission_busy {
        Box::new(
            el("section", span("Sending reviewed bytes…")).attr("class", "knot-submission-review"),
        )
    } else {
        Box::new(el("div", ()))
    };
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
                if s.scroll.site.is_none() {
                    s.scroll.format = candidate;
                    if let Some(port) = candidate.default_port() {
                        s.scroll.port = TextInput::new(port.to_string());
                    }
                } else {
                    s.message =
                        Some("Close or open another site before changing its format.".into());
                }
            })
            .attr("aria-pressed", (format == candidate).to_string()),
        )
    })
    .collect::<Vec<_>>();
    let titan_prepare: DesktopView = if state.scroll.submission_busy() {
        Box::new(span("Sending reviewed bytes…"))
    } else if let Some(error) = &state.scroll.titan_submission_error {
        Box::new(span(format!("Titan upload disabled: {error}")))
    } else {
        Box::new(button(
            "Prepare saved source for Titan",
            |s: &mut DesktopState, _| s.prepare_titan(),
        ))
    };
    let spartan_prepare: DesktopView = if state.scroll.submission_busy() {
        Box::new(span("Spartan preparation is unavailable while sending."))
    } else {
        Box::new(button("Prepare Spartan body", |s: &mut DesktopState, _| {
            s.prepare_spartan()
        }))
    };
    let submission_composer: DesktopView = if state.scroll.submission_visible {
        Box::new(
            el(
                "section",
                (
                    input("Submission target", "knot-submission-target", |s| {
                        &mut s.scroll.submission_target
                    }),
                    input("MIME", "knot-submission-mime", |s| {
                        &mut s.scroll.submission_mime
                    }),
                    titan_prepare,
                    el(
                        "label",
                        (
                            "Spartan body",
                            lens(
                                |input: &mut TextInput| textarea_typed(input),
                                |s: &mut DesktopState| &mut s.scroll.submission_body,
                            ),
                        ),
                    )
                    .attr("id", "knot-spartan-body"),
                    spartan_prepare,
                    span("Preparing is local only. Sending is a separate action over the reviewed bytes."),
                ),
            )
            .attr("class", "knot-submission"),
        )
    } else {
        Box::new(el("div", ()))
    };
    Box::new(el("section", (
        el("div", (
            input("Site folder", "knot-scroll-folder", |s| &mut s.scroll.folder),
            el("span", Keyed::new(format_picker)).attr("class", "knot-site-format-picker"),
            button("Create site", |s: &mut DesktopState,_| s.enter_site(true)),
            button("Open site", |s: &mut DesktopState,_| s.enter_site(false)),
            button("Close site", |s: &mut DesktopState,_| s.close_site()),
            button("Metadata", |s: &mut DesktopState,_| s.scroll.metadata_visible = !s.scroll.metadata_visible),
            button("Toggle preview", |s: &mut DesktopState,_| s.scroll.preview_visible = !s.scroll.preview_visible),
            button(
                if state.scroll.submission_visible {
                    "Hide upload / submit"
                } else {
                    "Upload / submit"
                },
                |s: &mut DesktopState, _| s.scroll.submission_visible = !s.scroll.submission_visible,
            ),
        )).attr("class", "knot-scroll-controls"),
        pages,
        metadata,
        submission_composer,
        review,
        span(state.scroll.submission_result.clone().unwrap_or_default())
            .attr("class", "knot-submission-status"),
        state
            .scroll
            .submission_response
            .as_ref()
            .map(|body| Box::new(el("pre", body.clone()).attr("class", "knot-submission-response")) as DesktopView)
            .unwrap_or_else(|| Box::new(el("div", ()))),
        el("div", (
            input("Local port", "knot-scroll-port", |s| &mut s.scroll.port),
            button("Publish locally", |s: &mut DesktopState,_| s.publish_site()),
            button("Stop serving", |s: &mut DesktopState,_| {
                s.scroll.server = None;
                s.message = Some("Local serving stopped.".into());
            }),
            span(state.scroll.server.as_ref().map(|server| format!("{} · revision {} · saved snapshot", server.url(), state.scroll.publication_number))
                .unwrap_or_else(|| "Not published. Save writes drafts; Publish locally serves a saved snapshot over loopback when this site format has a local server.".into())),
        )).attr("class", "knot-scroll-controls"),
    )).attr("class", "knot-scroll-site"))
}

fn inline(items: &[InlineSpan]) -> DesktopView {
    let children = items.iter().enumerate().map(|(i,item)| {
        let view: DesktopView = match item {
            InlineSpan::Text(text) => Box::new(span(text.clone())),
            InlineSpan::Presented { presentation, spans } => Box::new(
                el("span", inline(spans)).attr("style", crate::document_preview::inline_presentation_css(presentation)),
            ),
            InlineSpan::Code(text) => Box::new(el("code", text.clone())),
            InlineSpan::Emphasis(items) => Box::new(el("em", inline(items)).attr("class", "knot-preview-emphasis")),
            InlineSpan::Strong(items) => Box::new(el("strong", inline(items)).attr("class", "knot-preview-strong")),
            InlineSpan::Link { url, spans, predicate, .. } => {
                let destination = url.clone();
                Box::new(button_with(inline(spans), move |state: &mut DesktopState, click| {
                    // A link inside a collapsible heading wins over its fold toggle.
                    click.stop_propagation();
                    if let Some(local_name) = preview_manifest_page(state, &destination) {
                        state.scroll_open_page(local_name);
                    } else {
                        state.message = Some(format!("Preview link: {destination}. Open with an independent client; this preview only navigates local site pages."));
                    }
                }).attr("title", format!("{url} {}", predicate.as_deref().unwrap_or(""))).attr("class", "knot-scroll-link"))
            },
            InlineSpan::Submit { target, spans } => {
                let label = inker::inline_text(spans);
                let target = target.clone();
                Box::new(button(label, move |state: &mut DesktopState, _| {
                    state.scroll.select_spartan_prompt(target.clone());
                    state.message = Some("Enter a Spartan body, review it, then send explicitly.".into());
                }).attr("class", "knot-spartan-submit"))
            },
            // Preview state only: opens folds and scrolls, never navigates.
            InlineSpan::InPage { target, spans } => {
                let target = target.clone();
                Box::new(button_with(inline(spans), move |state: &mut DesktopState, click| {
                    click.stop_propagation();
                    state.follow_micron_in_page(&target);
                }).attr("class", "knot-scroll-link knot-preview-in-page"))
            },
            InlineSpan::LineBreak => Box::new(el("br", ())),
            InlineSpan::SoftBreak => Box::new(span(" ")),
        };
        (i, view)
    }).collect::<Vec<_>>();
    Box::new(el("span", Keyed::new(children)))
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

fn preview_manifest_page<'a>(state: &DesktopState, destination: &'a str) -> Option<&'a str> {
    // A site can remain selected while the editor displays an unrelated local
    // file. The manifest is an authority only while the current document is
    // one of that site's pages.
    state.scroll.page.as_ref()?;
    let active_destination = state
        .scroll
        .server
        .as_ref()
        .and_then(|server| server.nomadnet_destination())
        .map(|destination| destination.to_string());
    let name = preview_page_name(destination, active_destination.as_deref())?;
    state
        .scroll
        .site
        .as_ref()?
        .page_path(name)
        .ok()
        .map(|_| name)
}

fn blocks(items: &[Block]) -> DesktopView {
    let children = items
        .iter()
        .enumerate()
        .map(|(i, item)| (i, block(item, None, None)))
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
    if ctx.runner.state().entry().micron_folds.jump.is_none() {
        return;
    }
    let mut jump = None;
    ctx.runner
        .update(|state| jump = state.entry_mut().micron_folds.jump.take());
    let Some(block) = jump.and_then(|target| target.block) else {
        return;
    };
    let node = {
        let dom = ctx.runner.dom();
        let dom = dom.borrow();
        preview_block_node(&*dom, dom.document(), &block.to_string())
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
fn document_blocks(document: &EngineDocument, folds: Option<&MicronPreviewFolds>) -> DesktopView {
    let navigation = &document.navigation;
    let folds = folds.filter(|_| navigation.is_current(&document.blocks));
    let hidden = folds.map_or_else(Vec::new, |folds| folds.folds.hidden(navigation));
    let keys = navigation.fold_keys();
    let children = document
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
            (index, block(item, toggle.as_ref(), Some(index)))
        })
        .collect::<Vec<_>>();
    Box::new(el("div", Keyed::new(children)))
}

fn block(item: &Block, fold: Option<&FoldToggle>, index: Option<usize>) -> DesktopView {
    match item {
        Block::Presented {
            presentation,
            block: inner,
        } => Box::new(indexed(
            el(
                "div",
                el("div", Keyed::new(vec![(0, block(inner, fold, None))])),
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
                None => Box::new(indexed(el(tag, inline(spans)), index)),
                Some(toggle) => {
                    let key = toggle.key.clone();
                    Box::new(indexed(
                        el(
                            tag,
                            button_with(
                                (
                                    span(toggle.marker.clone())
                                        .attr("class", "knot-micron-fold-marker"),
                                    inline(spans),
                                ),
                                move |state: &mut DesktopState, _| state.toggle_micron_fold(&key),
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
        Block::Paragraph { spans } => Box::new(indexed(el("p", inline(spans)), index)),
        Block::CodeBlock { text, .. } | Block::Preformatted { text } => {
            Box::new(indexed(el("pre", text.clone()), index))
        },
        Block::Quote { blocks: items } => Box::new(indexed(el("blockquote", blocks(items)), index)),
        // Nematic retains ordered markers in the text; use one bullet list
        // to avoid assigning a second, invented set of ordered numbers.
        Block::List { items, .. } => Box::new(indexed(
            el(
                "ul",
                Keyed::new(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| (i, el("li", blocks(item))))
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
        } => table_block(alignments, header, rows, index),
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
) -> DesktopView {
    let tag = if header { "th" } else { "td" };
    let mut cell = el(tag, inline(spans));
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
                                table_cell(spans, true, alignments.get(index).copied()),
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
                                    table_cell(spans, false, alignments.get(column).copied()),
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

fn micron_form_panel(state: &DesktopState, source: &str, address: &str) -> DesktopView {
    let Some(editor) = state.scroll.micron_form.as_ref() else {
        return Box::new(
            el(
                "section",
                button(
                    "Open Micron form controls",
                    |state: &mut DesktopState, _| state.open_micron_form(),
                ),
            )
            .attr("class", "knot-micron-form"),
        );
    };

    if state.scroll.micron_submission_receiver.is_some() {
        return Box::new(
            el(
                "section",
                (
                    span("Sending reviewed Micron request…"),
                    button("Cancel Micron request", |state: &mut DesktopState, _| {
                        state.scroll.cancel_micron_submission()
                    }),
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
                    button("Discard stale form", |state: &mut DesktopState, _| {
                        state.scroll.close_micron_form()
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
                                    .scroll
                                    .micron_form
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
                                    .scroll
                                    .micron_form
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
                            let result = state.scroll.micron_set_checked(index, !checked);
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
                            if let Err(error) = state.scroll.micron_set_checked(index, true) {
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
                    move |state: &mut DesktopState, _| state.prepare_micron_form(index),
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
                    button("Send reviewed Micron request", |state: &mut DesktopState, _| {
                        state.send_micron_form()
                    }),
                    button("Discard prepared request", |state: &mut DesktopState, _| {
                        if let Some(editor) = state.scroll.micron_form.as_mut() {
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
        .scroll
        .submission_result
        .as_ref()
        .map(|message| Box::new(span(message.clone())) as DesktopView)
        .unwrap_or_else(|| Box::new(el("div", ())));
    let response: DesktopView = state
        .scroll
        .submission_response
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
                button("Close Micron form", |state: &mut DesktopState, _| {
                    state.scroll.close_micron_form()
                }),
            ),
        )
        .attr("class", "knot-micron-form"),
    )
}

pub fn preview(state: &DesktopState) -> DesktopView {
    let source = state.document().snapshot();
    if !matches!(
        source.format,
        knot_document::DocumentFormat::Scroll
            | knot_document::DocumentFormat::Gemtext
            | knot_document::DocumentFormat::Micron
    ) || !state.scroll.preview_visible
    {
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
                .scroll
                .site
                .as_ref()
                .is_some_and(|site| site.config.format == SiteFormat::Spartan)
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
                        .then_some(&state.entry().micron_folds),
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
            "aside",
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
                    micron_form_panel(state, &source.text, &source.source.address)
                } else {
                    Box::new(el("div", ()))
                },
            ),
        )
        .attr("class", "knot-scroll-preview"),
    )
}

pub const CSS: &str = r#"
.knot-workspace { overflow:auto; }
.knot-native-site-mode .knot-source-wrapper { flex:1 1 50%; width:0; }
.knot-native-site-mode .knot-document-body textarea { display:block; width:auto; min-width:0; min-height:260px; }
.knot-scroll-site input { min-height:32px; box-sizing:border-box; }
.knot-scroll-fields textarea { white-space:pre-wrap; min-height:80px; padding:8px; border:1px solid; background:transparent; color:inherit; }
.knot-scroll-site { padding: 8px; border-bottom: 1px solid #888; flex-shrink: 0; }
.knot-scroll-controls, .knot-scroll-pages, .knot-site-format-picker { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
.knot-scroll-fields { display: flex; flex-wrap: wrap; gap: 8px; }
.knot-scroll-fields label { display: flex; flex-direction: column; width: 230px; }
.knot-submission { display: flex; gap: 8px; flex-wrap: wrap; align-items: flex-start; margin-top: 8px; }
.knot-submission label { display: flex; flex-direction: column; min-width: 180px; }
#knot-spartan-body { display: flex; flex: 1 0 100%; flex-direction: column; min-width: 0; }
#knot-spartan-body textarea { display: block; box-sizing: border-box; width: 100%; min-width: 0; min-height: 120px; max-height: 180px; overflow: auto; padding: 8px; border: 1px solid; background: transparent; color: inherit; white-space: pre-wrap; }
.knot-submission-review { max-width: 100%; margin-top: 8px; }
.knot-submission-review pre { box-sizing: border-box; width: 100%; max-height: 220px; overflow: auto; white-space: pre-wrap; }
.knot-submission-status { display: block; min-height: 1.2em; margin-top: 4px; }
.knot-submission-response { box-sizing: border-box; width: 100%; max-height: 220px; overflow: auto; white-space: pre-wrap; }
.knot-micron-form { display: flex; flex-direction: column; gap: 8px; margin-top: 12px; padding: 8px; border: 1px solid currentColor; }
.knot-micron-fields { display: flex; flex-wrap: wrap; gap: 8px; align-items: flex-end; }
.knot-micron-fields label { display: flex; flex-direction: column; min-width: 180px; }
.knot-micron-form pre { box-sizing: border-box; width: 100%; max-height: 180px; overflow: auto; white-space: pre-wrap; }
.knot-scroll-preview { flex: 1 1 50%; width:0; min-width:0; box-sizing:border-box; padding: 16px; overflow: auto; }
.knot-writing-area, .knot-scroll-preview { background: inherit; }
.knot-scroll-preview p { margin: 8px 0; }
.knot-scroll-preview pre { white-space: pre-wrap; }
.knot-preview-badge { display: block; margin: 8px 0; padding: 4px 8px; border: 1px solid currentColor; border-radius: 4px; }
.knot-preview-strong { font-weight: 700; }
.knot-preview-emphasis { font-style: italic; }
.knot-scroll-link { text-decoration: underline; }
.knot-micron-fold { display: block; width: 100%; text-align: left; }
#knot-scroll-folder input { width: 350px; }
#knot-scroll-port input { width: 70px; }
@media (max-width:700px) { .knot-native-site-mode .knot-source-wrapper, .knot-scroll-preview { width:100%; flex-basis:auto; } #knot-scroll-folder input { width:220px; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DESKTOP_CSS, host_hooks, workspace::desktop_view};
    use cambium::TextCommand;
    use cambium_genet_winit_host::{Harness, Init, KeyPress, NamedKey, WindowCommands};
    use knot_document::{KnotDocumentIntentV1, KnotDocumentSession};
    use layout_dom_api::{LayoutDom, LocalName, Namespace};
    use taproot::Selector;

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
        assert_eq!(state.scroll.page.as_deref(), Some("index.scroll"));
        state.scroll.fields[0] = TextInput::new("Writer");
        state.scroll_open_page("about.scroll");
        assert_eq!(state.scroll.page.as_deref(), Some("index.scroll"));
        state.publish_site();
        assert!(state.scroll.server.is_none());
        state.scroll.save_metadata().unwrap();
        state.scroll_open_page("about.scroll");
        assert_eq!(state.scroll.page.as_deref(), Some("about.scroll"));
        assert_eq!(state.scroll.fields[0].text(), "");
        state.scroll_open_page("index.scroll");
        assert_eq!(state.scroll.fields[0].text(), "Writer");
        state.scroll.port = TextInput::new("0");
        state.publish_site();
        assert!(state.scroll.server.is_some());
        assert_eq!(state.scroll.publication_number, 1);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "Unsaved ".into(),
            )))
            .unwrap();
        state.publish_site();
        assert_eq!(state.scroll.publication_number, 1);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Save)
            .unwrap();
        assert_eq!(state.scroll.publication_number, 1);
        state.publish_site();
        assert_eq!(state.scroll.publication_number, 2);
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
        assert_eq!(state.scroll.page.as_deref(), Some("index.gmi"));
        assert_eq!(
            state.scroll.site.as_ref().unwrap().config.format,
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
        assert_eq!(state.scroll.page.as_deref(), Some("index.mu"));
        assert_eq!(
            state.scroll.site.as_ref().unwrap().config.format,
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
        state.scroll.visible = true;
        state.scroll.preview_visible = true;
        let host = submission_harness_with(state);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let text = text_content(&dom, dom.document());
        assert!(text.contains("Micron preview · current source"));
        assert!(text.contains("Heading"));
        assert!(text.contains("Local"));
        assert!(text.contains("Micron preview: some presentation or controls"));
        assert!(text.contains("Rendering notes:"));
    }

    #[test]
    fn micron_form_review_is_ephemeral_and_uses_the_declared_action_map() {
        let source = "`[Submit selected`0123456789abcdef0123456789abcdef:/capture`name|checks|fixed=ready]\nText: `<name`seed>\nChecks: `<?|checks|red|*`> Red\n`<?|checks|blue`> Blue\n";
        let mut workspace = ScrollWorkspace::default();
        workspace
            .open_micron_form(source.into(), "scratch:micron-form".into())
            .unwrap();
        workspace.micron_form.as_mut().unwrap().inputs[0] = TextInput::new("edited");
        workspace.micron_set_checked(1, false).unwrap();
        workspace.micron_set_checked(2, true).unwrap();
        assert!(
            workspace
                .prepare_micron_form("changed", "scratch:micron-form", 0)
                .is_err()
        );
        assert!(
            workspace
                .prepare_micron_form(source, "scratch:other-address", 0)
                .is_err()
        );
        workspace
            .prepare_micron_form(source, "scratch:micron-form", 0)
            .unwrap();
        let prepared = workspace
            .micron_form
            .as_ref()
            .unwrap()
            .prepared
            .as_ref()
            .unwrap();
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
        let mut workspace = ScrollWorkspace::default();
        workspace.micron_submission_receiver = Some(receiver);
        workspace.active_micron_submission = Some((7, "old source".into(), "old:page".into()));
        sender
            .send((
                7,
                Ok(MicronResponse {
                    body: b"old reply".to_vec(),
                }),
            ))
            .unwrap();
        workspace.drain_submission("new source", "new:page");
        assert!(workspace.submission_result.is_none());
        assert!(workspace.submission_response.is_none());
        assert!(workspace.active_micron_submission.is_none());
    }

    #[test]
    fn cancelling_micron_request_keeps_its_local_unknown_outcome_status() {
        let (sender, receiver) = mpsc::channel();
        let (cancel, _cancelled) = tokio::sync::oneshot::channel();
        let mut workspace = ScrollWorkspace::default();
        workspace.micron_submission_receiver = Some(receiver);
        workspace.active_micron_submission = Some((8, "source".into(), "address".into()));
        workspace.micron_cancel = Some(cancel);
        workspace.cancel_micron_submission();
        drop(sender);
        workspace.drain_submission("source", "address");
        assert_eq!(
            workspace.submission_result.as_deref(),
            Some(
                "Micron request cancelled locally. Its remote outcome may be unknown; do not retry automatically."
            )
        );
        assert!(workspace.active_micron_submission.is_none());
    }

    #[test]
    fn closing_a_micron_form_drops_the_previous_reply_unless_a_send_is_in_flight() {
        let source = "`[Submit`0123456789abcdef0123456789abcdef:/capture`name]\n`<name`seed>\n";
        let mut workspace = ScrollWorkspace::default();
        workspace
            .open_micron_form(source.into(), "scratch:reopen".into())
            .unwrap();
        workspace.submission_result = Some("Micron reply (8 bytes)".into());
        workspace.submission_response = Some("accepted".into());
        workspace.close_micron_form();
        assert!(workspace.submission_result.is_none());
        assert!(workspace.submission_response.is_none());
        workspace
            .open_micron_form(source.into(), "scratch:reopen".into())
            .unwrap();
        assert!(workspace.submission_response.is_none());

        let (_sender, receiver) = mpsc::channel();
        workspace.micron_submission_receiver = Some(receiver);
        workspace.submission_result = Some("Sending reviewed Micron request…".into());
        workspace.close_micron_form();
        assert_eq!(
            workspace.submission_result.as_deref(),
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
        state.scroll.preview_visible = true;
        let address = state.document().snapshot().source.address;
        state
            .scroll
            .open_micron_form(source.into(), address)
            .unwrap();
        state.scroll.submission_result = Some("Micron reply (8 bytes)".into());
        state.scroll.submission_response = Some("accepted".into());
        assert!(!state.scroll.visible);
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

    #[test]
    fn the_site_page_follows_the_focused_tab_and_holds_unsaved_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:site-tabs", ""),
            WindowCommands::new(),
        );
        state.scroll.folder = TextInput::new(temp.path().join("site").to_string_lossy());
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
        assert_eq!(host.state().scroll.page.as_deref(), Some("about.scroll"));
        assert!(host.click_on(&Selector::role("tab").containing("index.scroll")));
        assert_eq!(host.state().scroll.page.as_deref(), Some("index.scroll"));
        assert!(host.click_on(&Selector::role("tab").containing("scratch:site-tabs")));
        assert_eq!(host.state().scroll.page, None);

        // An unsaved metadata edit keeps the focus on its page: neither
        // another tab nor closing this one takes it away.
        assert!(host.click_on(&Selector::role("tab").containing("about.scroll")));
        host.update(|state| state.scroll.fields[0] = TextInput::new("Writer"));
        assert!(host.click_on(&Selector::role("tab").containing("index.scroll")));
        assert_eq!(host.state().scroll.page.as_deref(), Some("about.scroll"));
        assert_eq!(
            host.state().document().snapshot().display_label,
            "about.scroll"
        );
        assert_eq!(host.state().scroll.fields[0].text(), "Writer");
        assert!(
            host.state()
                .message
                .as_deref()
                .is_some_and(|message| message.contains("metadata edits"))
        );
        assert!(
            host.click_on(&Selector::role("button").with_attr("aria-label", "Close about.scroll"))
        );
        assert_eq!(host.state().docs.len(), 3);
        assert_eq!(host.state().scroll.fields[0].text(), "Writer");
    }

    #[test]
    fn launch_documents_open_behind_the_first() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.djot");
        let second = temp.path().join("second.djot");
        std::fs::write(&first, "first\n").unwrap();
        std::fs::write(&second, "second\n").unwrap();
        let folder = temp.path().join("site");
        let site = Site::create_for(&folder, SiteFormat::Scroll).unwrap();
        let index = site.page_path(site.config.format.index_file()).unwrap();
        let mut state = DesktopState::with_path(
            KnotDocumentSession::open(&first).unwrap(),
            WindowCommands::new(),
            Some(first.clone()),
        );
        let front = state.focused_key();
        assert!(state.attach_site(&folder).is_some());
        state.open_behind(
            vec![
                KnotDocumentSession::open(&second).unwrap(),
                KnotDocumentSession::open(&index).unwrap(),
            ],
            &["missing.djot: not found".to_owned()],
        );
        assert_eq!(state.docs.len(), 3);
        assert_eq!(state.focused_key(), front);
        assert_eq!(state.path.text(), first.to_string_lossy().as_ref());
        assert!(state.scroll.site.is_some());
        assert_eq!(
            state.scroll.page, None,
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
        state.scroll.site = Some(site);

        assert_eq!(preview_manifest_page(&state, ":/page/about.mu"), None);
        let index = state
            .scroll
            .site
            .as_ref()
            .unwrap()
            .page_path("index.mu")
            .unwrap();
        state.scroll.sync_page(Some(&index));
        assert_eq!(
            preview_manifest_page(&state, ":/page/about.mu"),
            Some("about.mu")
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
        state.scroll.site = Some(site);
        state.scroll_open_page("index.mu");
        state.scroll.preview_visible = true;
        let host = submission_harness_with(state);
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
        state.scroll.preview_visible = true;
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
        state.scroll.site = Some(site);
        state.scroll.sync_page(Some(&path));
        state.scroll.preview_visible = true;
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
        assert_eq!(authored(&host).manifest_page.as_deref(), Some("about.mu"));
        (temp, host)
    }

    fn authored(host: &DesktopHarness) -> Authored {
        let state = host.state();
        let snapshot = state.document().snapshot();
        let site = state.scroll.site.as_ref().expect("a site-backed harness");
        Authored {
            saved_page: std::fs::read(site.page_path("about.mu").unwrap()).unwrap(),
            manifest: std::fs::read(site.root().join(knot_site::CONFIG)).unwrap(),
            manifest_page: state.scroll.page.clone(),
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
        let (x, y, width, height) = host.painted_rect(link).expect("link layout");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.relayout();
        assert!(
            host.state()
                .message
                .as_deref()
                .is_some_and(|message| message.starts_with("Preview link: :/page/about.mu"))
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
        let (x, y, width, height) = host.painted_rect(marker).expect("marker layout");
        host.click_at(x + width / 2.0, y + height / 2.0);
        host.relayout();
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
        state.scroll.preview_visible = true;
        state.scroll.visible = true;
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
        let mut workspace = ScrollWorkspace::default();
        assert!(!workspace.submission_visible);
        workspace.submission_token = TextInput::new("stale-token");
        workspace.select_spartan_prompt("spartan://example.test:3000/submit".into());

        assert!(workspace.submission_visible);
        assert_eq!(
            workspace.submission_target.text(),
            "spartan://example.test:3000/submit"
        );
        assert_eq!(workspace.submission_mime.text(), "text/plain");
        assert!(workspace.prepared.is_none());
        assert!(workspace.submission_receiver.is_none());
        assert!(workspace.submission_token.text().is_empty());
    }

    #[test]
    fn reviewed_submission_is_consumed_before_send_and_cancel_has_no_effect() {
        let mut workspace = ScrollWorkspace::default();
        workspace.prepared = Some(
            PreparedSubmission::from_body(
                "titan://example.test/upload",
                "text/gemini",
                b"reviewed source".to_vec(),
            )
            .unwrap(),
        );
        workspace.submission_token = TextInput::new("single-use-token");

        let (prepared, token) = workspace.take_submission_for_send().unwrap();
        assert_eq!(prepared.body(), b"reviewed source");
        assert_eq!(token.as_deref(), Some("single-use-token"));
        assert!(workspace.prepared.is_none());
        assert!(workspace.submission_token.text().is_empty());
        assert!(workspace.submission_receiver.is_none());

        workspace.prepared = Some(
            PreparedSubmission::from_body(
                "spartan://example.test:3000/submit",
                "text/plain",
                b"cancel me".to_vec(),
            )
            .unwrap(),
        );
        workspace.submission_token = TextInput::new("must-not-survive");
        workspace.discard_submission();
        assert!(workspace.prepared.is_none());
        assert!(workspace.submission_token.text().is_empty());
        assert!(workspace.submission_receiver.is_none());
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
        host.update(|state| {
            state.scroll.visible = true;
            state.scroll.submission_visible = true;
        });
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
        state.scroll.preview_visible = true;
        let address = state.document().snapshot().source.address;
        state
            .scroll
            .open_micron_form(source.clone(), address)
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
            state.scroll.micron_submission_receiver = Some(receiver);
            state.scroll.submission_result = Some("Sending reviewed Micron request".into());
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
        host.update(|state| state.scroll.submission_body = TextInput::new("body"));
        host.layout_at(1100.0, 730.0);
        let textarea = {
            let dom = host.runner().dom();
            let dom = dom.borrow();
            let body = node_with_id(&dom, dom.document(), "knot-spartan-body").unwrap();
            textarea_node(&dom, body).unwrap()
        };
        let (x, y, _, _) = host.painted_rect(textarea).unwrap();
        host.click_at(x + 1.0, y + 24.0);
        host.key_injected("署名");
        assert_eq!(host.state().scroll.submission_body.text(), "署名body");
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
        state.scroll.visible = true;
        state.scroll.preview_visible = true;
        let mut host = submission_harness_with(state);

        assert!(host.click_on(&Selector::role("button").containing("Submit locally")));
        assert!(host.state().scroll.submission_visible);
        assert_eq!(
            host.state().scroll.submission_target.text(),
            "spartan://localhost:65025/upload"
        );
        assert!(host.state().scroll.prepared.is_none());
        assert!(host.state().scroll.submission_receiver.is_none());
    }

    #[test]
    fn titan_token_field_is_password_typed_and_does_not_paint_its_value() {
        let mut host = submission_harness();
        host.update(|state| {
            state.scroll.prepared = Some(
                PreparedSubmission::from_body(
                    "titan://example.test/upload",
                    "text/gemini",
                    b"saved".to_vec(),
                )
                .unwrap(),
            );
            state.scroll.submission_token = TextInput::new("dummytokenonly");
        });
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let label = node_with_id(&dom, dom.document(), "knot-submission-token").unwrap();
        let input = input_node(&dom, label).unwrap();
        assert_eq!(
            dom.attribute(input, &Namespace::from(""), &LocalName::from("type")),
            Some("password")
        );
        assert!(!text_content(&dom, label).contains("dummytokenonly"));
    }
}
