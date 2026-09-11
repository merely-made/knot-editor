// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::workspace::{DesktopState, DesktopView, PendingAction};
use cambium::{Keyed, TextInput, button, el, lens, span, text_field_typed, textarea_typed};
use inker::{Block, Engine, EngineInput, InlineSpan};
use knot_scroll_site::{LocalServer, Page, Site};

const LABELS: [&str; 6] = [
    "Author",
    "Language",
    "Classification (0–9)",
    "Published (UTC or blank)",
    "Modified (UTC or blank)",
    "Abstract (native Scrolltext)",
];

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
        }
    }
}

impl ScrollWorkspace {
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
}

impl DesktopState {
    fn enter_site(&mut self, create: bool) {
        if self.document.snapshot().dirty || self.scroll.metadata_dirty() {
            self.message =
                Some("Save or discard document and metadata changes before changing sites.".into());
            return;
        }
        let root = std::path::PathBuf::from(self.scroll.folder.text());
        let result = if create {
            Site::create(&root)
        } else {
            Site::open(&root)
        };
        match result.and_then(|site| {
            let path = site.page_path("index.scroll")?;
            Ok((site, path))
        }) {
            Ok((site, path)) => {
                self.scroll.server = None;
                self.scroll.page = None;
                self.scroll.site = Some(site);
                self.request(PendingAction::Open(path));
            },
            Err(error) => self.message = Some(format!("Site: {error}")),
        }
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

    fn publish_scroll(&mut self) {
        if self.document.snapshot().dirty || self.scroll.metadata_dirty() {
            self.message = Some("Save source and metadata before publishing locally.".into());
            return;
        }
        let result = (|| {
            let site = self.scroll.site.as_ref().ok_or("Open a site first")?;
            let publication = site.publication()?;
            let count = publication.page_count();
            if let Some(server) = &self.scroll.server {
                server.replace(publication);
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
        let fields = LABELS
            .iter()
            .enumerate()
            .map(|(i, label)| {
                (
                    i,
                    el(
                        "label",
                        (
                            *label,
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
        Box::new(el(
            "section",
            (
                span(
                    "Publication metadata for the selected page. The abstract is separate native Scrolltext and needs a # Title.",
                ),
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
    Box::new(el("section", (
        el("div", (
            input("Site folder", "knot-scroll-folder", |s| &mut s.scroll.folder),
            button("Create Scroll site", |s: &mut DesktopState,_| s.enter_site(true)),
            button("Open site", |s: &mut DesktopState,_| s.enter_site(false)),
            button("Metadata", |s: &mut DesktopState,_| s.scroll.metadata_visible = !s.scroll.metadata_visible),
            button("Toggle preview", |s: &mut DesktopState,_| s.scroll.preview_visible = !s.scroll.preview_visible),
        )).attr("class", "knot-scroll-controls"),
        pages,
        metadata,
        el("div", (
            input("Local port", "knot-scroll-port", |s| &mut s.scroll.port),
            button("Publish locally", |s: &mut DesktopState,_| s.publish_scroll()),
            button("Stop serving", |s: &mut DesktopState,_| {
                s.scroll.server = None;
                s.message = Some("Local serving stopped.".into());
            }),
            span(state.scroll.server.as_ref().map(|server| format!("{} · revision {} · saved snapshot", server.url(), state.scroll.publication_number))
                .unwrap_or_else(|| "Not published. Save writes drafts; Publish locally serves saved pages over loopback TLS.".into())),
        )).attr("class", "knot-scroll-controls"),
    )).attr("class", "knot-scroll-site"))
}

fn inline(items: &[InlineSpan]) -> DesktopView {
    let children = items.iter().enumerate().map(|(i,item)| {
        let view: DesktopView = match item {
            InlineSpan::Text(text) => Box::new(span(text.clone())),
            InlineSpan::Code(text) => Box::new(el("code", text.clone())),
            InlineSpan::Emphasis(items) => Box::new(el("em", inline(items))),
            InlineSpan::Strong(items) => Box::new(el("strong", inline(items))),
            InlineSpan::Link { url, spans, predicate, .. } => {
                let destination = url.clone();
                let label = inker::inline_text(spans);
                Box::new(button(label, move |state: &mut DesktopState,_| {
                    let name = destination.trim_start_matches('/');
                    if !name.contains('/') && !name.contains(':') && !name.contains('#') {
                        state.scroll_open_page(name);
                    } else { state.message = Some(format!("Preview link: {destination}. Open with an independent client; this preview only navigates local site pages.")); }
                }).attr("title", format!("{url} {}", predicate.as_deref().unwrap_or(""))).attr("class", "knot-scroll-link"))
            },
            InlineSpan::LineBreak => Box::new(el("br", ())),
            InlineSpan::SoftBreak => Box::new(span(" ")),
            _ => Box::new(span("Unsupported inline content")),
        };
        (i, view)
    }).collect::<Vec<_>>();
    Box::new(el("span", Keyed::new(children)))
}

fn blocks(items: &[Block]) -> DesktopView {
    let children = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let view: DesktopView = match item {
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
                _ => Box::new(span("Unsupported preview block")),
            };
            (i, view)
        })
        .collect::<Vec<_>>();
    Box::new(el("div", Keyed::new(children)))
}

pub fn preview(state: &DesktopState) -> DesktopView {
    let source = state.document.snapshot();
    if source.format != knot_document::DocumentFormat::Scroll || !state.scroll.preview_visible {
        return Box::new(el("div", ()));
    }
    let rendered = nematic::ScrollEngine::new().render(
        &EngineInput::new(&source.source.address, &source.text).with_content_type("text/scroll"),
    );
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
        el("aside", (el("h2", "Scroll preview · current source"), body))
            .attr("class", "knot-scroll-preview"),
    )
}

pub const CSS: &str = r#"
.knot-workspace { overflow:auto; }
.knot-scroll-mode .knot-source-wrapper { flex:1 1 50%; width:0; }
.knot-scroll-mode .knot-document-body textarea { display:block; width:auto; min-width:0; min-height:260px; }
.knot-scroll-site input { min-height:32px; box-sizing:border-box; }
.knot-scroll-fields textarea { white-space:pre-wrap; min-height:80px; padding:8px; border:1px solid; background:transparent; color:inherit; }
.knot-scroll-site { padding: 8px; border-bottom: 1px solid #888; flex-shrink: 0; }
.knot-scroll-controls, .knot-scroll-pages { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
.knot-scroll-fields { display: flex; flex-wrap: wrap; gap: 8px; }
.knot-scroll-fields label { display: flex; flex-direction: column; width: 230px; }
.knot-scroll-preview { flex: 1 1 50%; width:0; min-width:0; box-sizing:border-box; padding: 16px; overflow: auto; }
.knot-scroll-preview p { margin: 8px 0; }
.knot-scroll-preview pre { white-space: pre-wrap; }
.knot-scroll-link { text-decoration: underline; }
#knot-scroll-folder input { width: 350px; }
#knot-scroll-port input { width: 70px; }
@media (max-width:700px) { .knot-scroll-mode .knot-source-wrapper, .knot-scroll-preview { width:100%; flex-basis:auto; } #knot-scroll-folder input { width:220px; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::TextCommand;
    use cambium_genet_winit_host::WindowCommands;
    use knot_document::{KnotDocumentIntentV1, KnotDocumentSession};

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
        state.publish_scroll();
        assert!(state.scroll.server.is_none());
        state.scroll.save_metadata().unwrap();
        state.scroll_open_page("about.scroll");
        assert_eq!(state.scroll.page.as_deref(), Some("about.scroll"));
        assert_eq!(state.scroll.fields[0].text(), "");
        state.scroll_open_page("index.scroll");
        assert_eq!(state.scroll.fields[0].text(), "Writer");
        state.scroll.port = TextInput::new("0");
        state.publish_scroll();
        assert!(state.scroll.server.is_some());
        assert_eq!(state.scroll.publication_number, 1);
        state
            .document
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
                "Unsaved ".into(),
            )))
            .unwrap();
        state.publish_scroll();
        assert_eq!(state.scroll.publication_number, 1);
        state.document.apply(KnotDocumentIntentV1::Save).unwrap();
        assert_eq!(state.scroll.publication_number, 1);
        state.publish_scroll();
        assert_eq!(state.scroll.publication_number, 2);
    }
}
