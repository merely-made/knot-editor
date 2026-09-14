// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::workspace::{DesktopState, DesktopView, PendingAction};
use cambium::{
    GenetCtx, GenetElement, KeyEvent, Keyed, TextFieldMode, TextInput, View, button, button_with,
    el, lens, on_key, span, text_field_typed, textarea_typed,
};
use cambium_genet_winit_host::HostWake;
use inker::{Block, Engine, EngineInput, InlineSpan, TableAlignment};
use knot_site::micron_submission::{MicronResponse, MicronSubmissionConfig, PreparedMicronRequest};
use knot_site::submission::{PreparedSubmission, SubmissionReceipt};
use knot_site::{LocalServer, Page, Site, SiteFormat};
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
    fn open_micron_form(&mut self) {
        let source = self.document.snapshot();
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
        let source = self.document.snapshot();
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
        let current_document = self.document.snapshot();
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
        if self.document.snapshot().dirty || self.scroll.metadata_dirty() {
            self.message = Some("Save source and metadata before preparing Titan upload.".into());
            return;
        }
        let Some(path) = self.document.session().source_path() else {
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
        if self.document.snapshot().dirty || self.scroll.metadata_dirty() {
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
                self.request(PendingAction::Open(path));
            },
            Err(error) => self.message = Some(format!("Site: {error}")),
        }
    }

    fn close_site(&mut self) {
        if self.document.snapshot().dirty || self.scroll.metadata_dirty() {
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
            Ok(path) => self.request(PendingAction::Open(path)),
            Err(error) => self.message = Some(error),
        }
    }

    fn publish_site(&mut self) {
        if self.document.snapshot().dirty || self.scroll.metadata_dirty() {
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
                Box::new(button_with(inline(spans), move |state: &mut DesktopState,_| {
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
        .map(|(i, item)| {
            let view: DesktopView = match item {
                Block::Presented {
                    presentation,
                    block,
                } => Box::new(
                    el("div", blocks(std::slice::from_ref(block.as_ref()))).attr(
                        "style",
                        crate::document_preview::block_presentation_css(presentation),
                    ),
                ),
                Block::Heading { level, spans } => Box::new(el(
                    match level {
                        1 => "h1",
                        2 => "h2",
                        3 => "h3",
                        4 => "h4",
                        _ => "h5",
                    },
                    inline(spans),
                )),
                Block::Paragraph { spans } => Box::new(el("p", inline(spans))),
                Block::CodeBlock { text, .. } | Block::Preformatted { text } => {
                    Box::new(el("pre", text.clone()))
                },
                Block::Quote { blocks: items } => Box::new(el("blockquote", blocks(items))),
                // Nematic retains ordered markers in the text; use one bullet list
                // to avoid assigning a second, invented set of ordered numbers.
                Block::List { items, .. } => Box::new(el(
                    "ul",
                    Keyed::new(
                        items
                            .iter()
                            .enumerate()
                            .map(|(i, item)| (i, el("li", blocks(item))))
                            .collect::<Vec<_>>(),
                    ),
                )),
                Block::Rule => Box::new(el("hr", ())),
                Block::Table {
                    alignments,
                    header,
                    rows,
                } => table_block(alignments, header, rows),
                Block::Badge { text } => {
                    Box::new(el("p", text.clone()).attr("class", "knot-preview-badge"))
                },
                _ => Box::new(span("Unsupported preview block")),
            };
            (i, view)
        })
        .collect::<Vec<_>>();
    Box::new(el("div", Keyed::new(children)))
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
    Box::new(el(
        "table",
        (header_view, el("tbody", Keyed::new(body_rows))),
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
    let source = state.document.snapshot();
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
        knot_document::DocumentFormat::Micron => nematic::MicronEngine::new()
            .render(&EngineInput::new(&source.source.address, &source.text)),
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
                blocks(&document.blocks),
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
#knot-scroll-folder input { width: 350px; }
#knot-scroll-port input { width: 70px; }
@media (max-width:700px) { .knot-native-site-mode .knot-source-wrapper, .knot-scroll-preview { width:100%; flex-basis:auto; } #knot-scroll-folder input { width:220px; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DESKTOP_CSS, host_hooks, workspace::desktop_view};
    use cambium::TextCommand;
    use cambium_genet_winit_host::{Harness, Init, WindowCommands};
    use genet_probe::Selector;
    use knot_document::{KnotDocumentIntentV1, KnotDocumentSession};
    use layout_dom_api::{LayoutDom, LocalName, Namespace};

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
            state.document.snapshot().format,
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
            .document
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "Unsaved ".into(),
            )))
            .unwrap();
        state.publish_site();
        assert_eq!(state.scroll.publication_number, 1);
        state.document.apply(KnotDocumentIntentV1::Save).unwrap();
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
            state.document.snapshot().format,
            knot_document::DocumentFormat::Gemtext
        );
        assert_eq!(state.scroll.page.as_deref(), Some("index.gmi"));
        assert_eq!(
            state.scroll.site.as_ref().unwrap().config.format,
            SiteFormat::Gemini
        );

        state.document.apply(KnotDocumentIntentV1::Save).unwrap();
        state.scroll.format = SiteFormat::Micron;
        state.scroll.folder = TextInput::new(temp.path().join("micron").to_string_lossy());
        state.enter_site(true);
        assert_eq!(
            state.document.snapshot().format,
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
            .document
            .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
            .unwrap();
        state
            .document
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
        let address = state.document.snapshot().source.address;
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
        assert_eq!(host.state().document.snapshot().text, source);
        assert!(!host.state().document.snapshot().dirty);
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
        let mut source =
            String::from("`[Submit`0123456789abcdef0123456789abcdef:/capture`name]
`<name`seed>
");
        for index in 0..400 {
            source.push_str(&format!("Line {index} of a long Micron page.
"));
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("long.mu");
        std::fs::write(&path, &source).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
        );
        state.scroll.preview_visible = true;
        let address = state.document.snapshot().source.address;
        state
            .scroll
            .open_micron_form(source.clone(), address)
            .unwrap();
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: format!("{DESKTOP_CSS}{CSS}"),
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
        assert_eq!(host.state().document.snapshot().text, "document source");
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
            .document
            .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
            .unwrap();
        state
            .document
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
