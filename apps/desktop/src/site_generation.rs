// SPDX-License-Identifier: MPL-2.0
//! Explicit saved-snapshot generation. Building never starts a publisher.
use crate::{
    documents::SiteKey,
    workspace::{DesktopState, DesktopView},
};
use cambium::{GenetElement, Keyed, TextInput, button, el, lens, span, text_field_typed};
use site_host::generate::{BuildOptions, BuildRequest, ExportReceipt, OutputFormat};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, TryRecvError},
};
#[derive(Default)]
pub(crate) struct GenerationState {
    pub drafts: BTreeMap<SiteKey, GenerationDraft>,
    pending: Option<Receiver<Result<(ExportReceipt, Vec<String>), String>>>,
}
impl GenerationState {
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}
pub(crate) struct GenerationDraft {
    pub destination: TextInput,
    pub target: OutputFormat,
    pub allow_loss: bool,
    pub preserve_native: bool,
}
impl Default for GenerationDraft {
    fn default() -> Self {
        Self {
            destination: TextInput::default(),
            target: OutputFormat::Html,
            allow_loss: false,
            preserve_native: false,
        }
    }
}
impl DesktopState {
    pub(crate) fn build_site_artifact(&mut self, site: SiteKey) {
        if self.scroll.generation.pending.is_some() {
            self.message = Some("Wait for the current site build.".into());
            return;
        }
        if !self.site_dirty_pages(site).is_empty() || !self.site_dirty_drafts(site).is_empty() {
            self.message = Some("Save source and metadata before building.".into());
            return;
        }
        let request = (|| {
            let draft = self
                .scroll
                .generation
                .drafts
                .get(&site)
                .ok_or("Choose a new export folder first.")?;
            if draft.destination.text().trim().is_empty() {
                return Err("Choose a new export folder first.".to_string());
            }
            let destination = std::path::PathBuf::from(draft.destination.text());
            let entry = self.scroll.site(site).ok_or("Open a site first.")?;
            let snapshot = entry.site.publication()?.to_snapshot_v1()?;
            let mut options = BuildOptions::new(draft.target);
            options.preserve_native_bytes = draft.preserve_native;
            options.reject_loss = !draft.allow_loss;
            Ok((BuildRequest::from_native(&snapshot, options)?, destination))
        })();
        let (request, destination) = match request {
            Ok(v) => v,
            Err(e) => {
                self.message = Some(e);
                return;
            },
        };
        let (sender, receiver) = mpsc::channel();
        self.scroll.generation.pending = Some(receiver);
        let wake = self.scroll.history.wake.clone();
        self.message = Some("Building the saved site…".into());
        std::thread::spawn(move || {
            let result = request.build().and_then(|artifact| {
                let notes = artifact
                    .diagnostics
                    .iter()
                    .map(|d| format!("{}: {}", d.page, d.message))
                    .collect();
                site_host::generate::export(&artifact, &destination).map(|receipt| (receipt, notes))
            });
            let _ = sender.send(result);
            if let Some(wake) = wake {
                wake.wake();
            }
        });
    }
    pub(crate) fn drain_site_generation(&mut self) {
        let Some(receiver) = self.scroll.generation.pending.as_ref() else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Site build worker stopped.".into()),
        };
        self.scroll.generation.pending = None;
        self.message = Some(match result {
            Ok((r, notes)) => format!(
                "Built {} pages in {}. {}",
                r.page_count,
                r.directory.display(),
                if notes.is_empty() {
                    "The checked artifact is ready to publish.".into()
                } else {
                    format!(
                        "{} conversion notes: {}",
                        notes.len(),
                        notes.into_iter().take(3).collect::<Vec<_>>().join("; ")
                    )
                }
            ),
            Err(e) => format!("Site build refused: {e}"),
        });
    }
}
pub(crate) fn controls(state: &DesktopState, site: SiteKey) -> DesktopView {
    let current = state.scroll.generation.drafts.get(&site);
    let target = current.map(|d| d.target).unwrap_or(OutputFormat::Html);
    let allow = current.is_some_and(|d| d.allow_loss);
    let preserve = current.is_some_and(|d| d.preserve_native);
    let choices = vec![
        ("Gemtext", OutputFormat::Gemtext),
        ("Scroll", OutputFormat::Scroll),
        ("Markdown", OutputFormat::Markdown),
        ("HTML", OutputFormat::Html),
    ]
    .into_iter()
    .enumerate()
    .map(|(n, (label, format))| {
        (
            n,
            button(label, move |s: &mut DesktopState, _| {
                let draft = s.scroll.generation.drafts.entry(site).or_default();
                draft.target = format;
                draft.preserve_native = false;
            })
            .attr(
                "aria-pressed",
                if target == format { "true" } else { "false" },
            ),
        )
    })
    .collect::<Vec<_>>();
    Box::new(
        el("section", (
            el("h3", "Build saved site"),
            el("div", Keyed::new(choices)).attr("aria-label", "Output format"),
            el("label", (
                "New export folder",
                lens(|input: &mut TextInput| text_field_typed(input), move |s: &mut DesktopState| {
                    &mut s.scroll.generation.drafts.entry(site).or_default().destination
                }),
            )),
            button(if allow { "Conversion changes allowed" } else { "Require lossless conversion" }, move |s: &mut DesktopState, _| {
                let draft = s.scroll.generation.drafts.entry(site).or_default();
                draft.allow_loss = !draft.allow_loss;
            }).attr("aria-pressed", if allow { "true" } else { "false" }),
            button(if preserve { "Preserve native bytes: on" } else { "Preserve native bytes: off" }, move |s: &mut DesktopState, _| {
                let draft = s.scroll.generation.drafts.entry(site).or_default();
                draft.preserve_native = !draft.preserve_native;
            }).attr("aria-pressed", if preserve { "true" } else { "false" }),
            button("Build artifact", move |s: &mut DesktopState, _| s.build_site_artifact(site)),
            span("Build uses saved pages. Choose a new folder; existing files are never replaced."),
        )).attr("aria-label", "Site generation"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::WindowCommands;
    use knot_document::KnotDocumentSession;
    use site_host::{Site, SiteFormat};
    #[test]
    fn build_control_exports_saved_snapshot_without_starting_a_server_or_overwriting() {
        let temp = tempfile::tempdir().unwrap();
        let site = Site::create_for(&temp.path().join("source"), SiteFormat::Gemini).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&site.page_path("index.gmi").unwrap()).unwrap(),
            WindowCommands::new(),
        );
        let key = state.hold_site(site);
        let destination = temp.path().join("html");
        state
            .scroll
            .generation
            .drafts
            .entry(key)
            .or_default()
            .destination = TextInput::new(destination.to_str().unwrap());
        state.build_site_artifact(key);
        assert!(
            state.submissions_busy(),
            "host wake must drain a site build"
        );
        wait(&mut state);
        assert!(!state.submissions_busy());
        assert!(destination.join("index.html").is_file());
        assert!(site_host::generate::open_export(&destination).is_ok());
        assert!(state.scroll.site(key).unwrap().server.is_none());
        let original = std::fs::read(destination.join("index.html")).unwrap();
        state.build_site_artifact(key);
        wait(&mut state);
        assert!(state.message.as_ref().unwrap().contains("refused"));
        assert_eq!(
            std::fs::read(destination.join("index.html")).unwrap(),
            original
        );
    }
    fn wait(state: &mut DesktopState) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.scroll.generation.pending.is_some() {
            assert!(std::time::Instant::now() < end);
            state.drain_site_generation();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn native_generation_preserves_bytes_and_dirty_source_refuses() {
        use cambium::TextCommand;
        use knot_document::KnotDocumentIntentV1;
        let temp = tempfile::tempdir().unwrap();
        let site = Site::create_for(&temp.path().join("source"), SiteFormat::Gemini).unwrap();
        let path = site.page_path("index.gmi").unwrap();
        let bytes = b"# Native\n\nLiteral & source *stays*.\n";
        std::fs::write(&path, bytes).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&path).unwrap(),
            WindowCommands::new(),
        );
        let key = state.hold_site(site);
        let destination = temp.path().join("native");
        let draft = state.scroll.generation.drafts.entry(key).or_default();
        draft.target = OutputFormat::Gemtext;
        draft.preserve_native = true;
        draft.destination = TextInput::new(destination.to_str().unwrap());
        state.build_site_artifact(key);
        wait(&mut state);
        assert_eq!(std::fs::read(destination.join("index.gmi")).unwrap(), bytes);
        state
            .document_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "draft".into(),
            )))
            .unwrap();
        let refused = temp.path().join("unsaved");
        state
            .scroll
            .generation
            .drafts
            .get_mut(&key)
            .unwrap()
            .destination = TextInput::new(refused.to_str().unwrap());
        state.build_site_artifact(key);
        assert!(!refused.exists());
        assert!(state.message.as_ref().unwrap().contains("Save source"));
    }
    fn widget_view(state: &DesktopState) -> DesktopView {
        crate::scroll_site::site_view(state, state.scroll.current_key().unwrap())
    }
    #[test]
    fn full_site_widget_changes_format_and_builds_through_its_button() {
        use cambium_genet_winit_host::{Harness, Init};
        use taproot::Selector;
        let temp = tempfile::tempdir().unwrap();
        let site = Site::create_for(&temp.path().join("source"), SiteFormat::Gemini).unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(site.page_path("index.gmi").unwrap()).unwrap(),
            WindowCommands::new(),
        );
        let key = state.hold_site(site);
        let destination = temp.path().join("markdown");
        state
            .scroll
            .generation
            .drafts
            .entry(key)
            .or_default()
            .destination = TextInput::new(destination.to_str().unwrap());
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: widget_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            crate::host_hooks(),
        );
        host.layout_at(1000.0, 700.0);
        assert!(host.click_on(&Selector::role("button").containing("Markdown")));
        assert_eq!(
            host.state().scroll.generation.drafts[&key].target,
            OutputFormat::Markdown
        );
        assert!(host.click_on(&Selector::role("button").containing("Build artifact")));
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while host.state().scroll.generation.pending.is_some() {
            assert!(std::time::Instant::now() < end);
            host.update(|s| s.drain_site_generation());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(destination.join("index.md").exists());
        assert!(host.state().scroll.site(key).unwrap().server.is_none());
    }
}
