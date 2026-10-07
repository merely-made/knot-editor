// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use cambium::{Keyed, button, button_with, el, span};
use inker::{Block, DocumentDiagnostic, InlineSpan};
use knot_document::KnotOutlineItemV1;
use std::sync::Arc;

use crate::documents::DocKey;
use crate::workspace::{DesktopState, DesktopView};

pub const CSS: &str = concat!(
    ".knot-preview-frame { box-sizing:border-box; padding:16px; min-width:0; }",
    ".knot-document-preview { box-sizing:border-box; }",
    ".knot-document-preview h1,.knot-document-preview h2,.knot-document-preview h3,.knot-document-preview h4,.knot-document-preview h5,.knot-document-preview h6 { margin:8px 0; }",
    ".knot-document-preview h1 { font-size:1.6em; }",
    ".knot-document-preview h2 { font-size:1.25em; }",
    ".knot-document-preview h3,.knot-document-preview h4,.knot-document-preview h5,.knot-document-preview h6 { font-size:1.05em; }",
    ".knot-document-preview-diagnostics { display:block; font-family:system-ui,sans-serif; font-size:13px; line-height:normal; opacity:0.8; }",
    ".knot-workspace .knot-document-preview-heading { display:block; width:100%; text-align:left; font:inherit; font-weight:inherit; padding:0; border:none; border-radius:0; background:transparent; color:inherit; cursor:pointer; }",
);

pub(crate) fn inline_presentation_css(presentation: &inker::InlinePresentation) -> String {
    let mut css = String::new();
    if let Some([r, g, b]) = presentation.foreground {
        css.push_str(&format!("color:rgb({r},{g},{b});"));
    }
    if let Some([r, g, b]) = presentation.background {
        css.push_str(&format!("background-color:rgb({r},{g},{b});"));
    }
    if presentation.underline {
        css.push_str("text-decoration:underline;");
    }
    css
}

pub(crate) fn block_presentation_css(presentation: &inker::BlockPresentation) -> String {
    let alignment = match presentation.alignment {
        inker::BlockAlignment::Start => "start",
        inker::BlockAlignment::Center => "center",
        inker::BlockAlignment::End => "end",
    };
    format!(
        "text-align:{alignment};padding-inline-start:calc({} * var(--knot-reader-indent,24px));",
        presentation.indent_level
    )
}

#[derive(Clone)]
struct LinkSource {
    document: DocKey,
    address: Arc<str>,
    text: Arc<str>,
}

fn inline(items: &[InlineSpan], source: Option<&LinkSource>) -> DesktopView {
    let children = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let view: DesktopView = match item {
                InlineSpan::Presented {
                    presentation,
                    spans,
                } => Box::new(
                    el("span", inline(spans, source))
                        .attr("style", inline_presentation_css(presentation)),
                ),
                InlineSpan::Text(text) => Box::new(span(text.clone())),
                InlineSpan::Code(text) => Box::new(el("code", text.clone())),
                InlineSpan::Emphasis(items) => Box::new(el("em", inline(items, source))),
                InlineSpan::Strong(items) => Box::new(el("strong", inline(items, source))),
                InlineSpan::Link { url, spans, .. } => {
                    let destination = url.clone();
                    let accessible_destination = destination.clone();
                    let origin = source.cloned();
                    Box::new(
                        button_with(inline(spans, source), move |state: &mut DesktopState, _| {
                            if let Some(origin) = &origin {
                                crate::link_workflow::follow_link(
                                    state,
                                    origin.document,
                                    &origin.address,
                                    &origin.text,
                                    &destination,
                                );
                            } else {
                                state.message = Some(format!("Preview link: {destination}"));
                            }
                        })
                        .attr(
                            "aria-label",
                            format!("Preview link: {accessible_destination}"),
                        ),
                    )
                },
                InlineSpan::Submit { target, spans } => {
                    let destination = target.clone();
                    let accessible_destination = destination.clone();
                    Box::new(
                        button(
                            inker::inline_text(spans),
                            move |state: &mut DesktopState, _| {
                                state.message =
                                    Some(format!("Preview submission target: {destination}"));
                            },
                        )
                        .attr(
                            "aria-label",
                            format!("Preview submission target: {accessible_destination}"),
                        ),
                    )
                },
                // An in-page link is its label here; this preview has no anchors.
                InlineSpan::InPage { spans, .. } => Box::new(el("span", inline(spans, source))),
                InlineSpan::LineBreak => Box::new(el("br", ())),
                InlineSpan::SoftBreak => Box::new(span(" ")),
            };
            (index, view)
        })
        .collect::<Vec<_>>();
    Box::new(el("span", Keyed::new(children)))
}

fn table_row(
    cells: &[Vec<InlineSpan>],
    cell_element: &'static str,
    source: Option<&LinkSource>,
) -> DesktopView {
    Box::new(el(
        "tr",
        Keyed::new(
            cells
                .iter()
                .enumerate()
                .map(|(index, cell)| (index, el(cell_element, inline(cell, source))))
                .collect::<Vec<_>>(),
        ),
    ))
}

/// Render a document that is not the editor's source — a reading's note, say.
/// It carries its own address and no headings, so a heading button inside it
/// can never match the session and any activation is refused, not misapplied.
pub(crate) fn note_blocks(document: &inker::EngineDocument) -> DesktopView {
    let address: Arc<str> = Arc::from(document.address.clone());
    let source_text: Arc<str> = Arc::from("");
    blocks(&document.blocks, &[], &address, &source_text, None, &mut 0)
}

fn blocks(
    items: &[Block],
    headings: &[KnotOutlineItemV1],
    address: &Arc<str>,
    source_text: &Arc<str>,
    document: Option<DocKey>,
    next_heading: &mut usize,
) -> DesktopView {
    let link_source = document.map(|document| LinkSource {
        document,
        address: Arc::clone(address),
        text: Arc::clone(source_text),
    });
    let children = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let view: DesktopView = match item {
                Block::Presented {
                    presentation,
                    block,
                } => Box::new(
                    el(
                        "div",
                        blocks(
                            std::slice::from_ref(block.as_ref()),
                            headings,
                            address,
                            source_text,
                            document,
                            next_heading,
                        ),
                    )
                    .attr("style", block_presentation_css(presentation)),
                ),
                Block::Heading { level, spans } => {
                    let index = *next_heading;
                    *next_heading += 1;
                    let heading = headings.get(index).cloned().unwrap_or(KnotOutlineItemV1 {
                        label: inker::inline_text(spans),
                        level: *level,
                        start: 0,
                        end: 0,
                    });
                    let label = heading.label.clone();
                    let heading_address = Arc::clone(address);
                    let heading_source = Arc::clone(source_text);
                    Box::new(el(
                        match level {
                            1 => "h1",
                            2 => "h2",
                            3 => "h3",
                            4 => "h4",
                            _ => "h5",
                        },
                        button(label.clone(), move |state: &mut DesktopState, _| {
                            state.select_preview_heading(
                                document,
                                heading_address.as_ref(),
                                heading_source.as_ref(),
                                &heading,
                                index,
                            )
                        })
                        .attr("class", "knot-document-preview-heading")
                        .attr("aria-label", format!("Select source heading: {label}")),
                    ))
                },
                Block::Paragraph { spans } => {
                    Box::new(el("p", inline(spans, link_source.as_ref())))
                },
                Block::CodeBlock { text, .. } | Block::Preformatted { text } => {
                    Box::new(el("pre", text.clone()))
                },
                Block::Quote { blocks: items } => Box::new(el(
                    "blockquote",
                    blocks(
                        items,
                        headings,
                        address,
                        source_text,
                        document,
                        next_heading,
                    ),
                )),
                Block::List { ordered, items } => Box::new(el(
                    if *ordered { "ol" } else { "ul" },
                    Keyed::new(
                        items
                            .iter()
                            .enumerate()
                            .map(|(i, item)| {
                                (
                                    i,
                                    el(
                                        "li",
                                        blocks(
                                            item,
                                            headings,
                                            address,
                                            source_text,
                                            document,
                                            next_heading,
                                        ),
                                    ),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                )),
                Block::Rule => Box::new(el("hr", ())),
                Block::Badge { text } => Box::new(el("p", text.clone())),
                Block::Image { url, alt } => Box::new(
                    el(
                        "figure",
                        (
                            el("figcaption", format!("Image: {alt}")),
                            span(format!("Source: {url}")),
                        ),
                    )
                    .attr("class", "knot-preview-image"),
                ),
                Block::FeedHeader {
                    title,
                    subtitle,
                    summary,
                    source_url,
                } => Box::new(el(
                    "header",
                    (
                        el("h3", title.clone()),
                        span(subtitle.clone().unwrap_or_default()),
                        el("p", summary.clone().unwrap_or_default()),
                        span(source_url.clone().unwrap_or_default()),
                    ),
                )),
                Block::FeedEntry {
                    title,
                    date,
                    summary,
                    article_url,
                    source_url,
                    ..
                } => Box::new(el(
                    "article",
                    (
                        el("h3", title.clone()),
                        span(date.clone().unwrap_or_default()),
                        el("p", summary.clone().unwrap_or_default()),
                        span(article_url.clone().unwrap_or_default()),
                        span(source_url.clone().unwrap_or_default()),
                    ),
                )),
                Block::MetadataRow { label, value } => Box::new(el(
                    "div",
                    (el("strong", format!("{label}: ")), span(value.clone())),
                )),
                Block::Table { header, rows, .. } => {
                    let head: DesktopView = if header.is_empty() {
                        Box::new(el("thead", ()))
                    } else {
                        Box::new(el("thead", table_row(header, "th", link_source.as_ref())))
                    };
                    let body = rows
                        .iter()
                        .enumerate()
                        .map(|(row, cells)| (row, table_row(cells, "td", link_source.as_ref())))
                        .collect::<Vec<_>>();
                    Box::new(el("table", (head, el("tbody", Keyed::new(body)))))
                },
                other => Box::new(el(
                    "p",
                    format!("Unsupported preview block: {}", other.kind_name()),
                )),
            };
            (index, view)
        })
        .collect::<Vec<_>>();
    Box::new(el("div", Keyed::new(children)))
}

fn diagnostic_text(diagnostic: &DocumentDiagnostic) -> String {
    match diagnostic {
        DocumentDiagnostic::UnsupportedConstruct(detail) => {
            format!("Unsupported construct: {detail}")
        },
        DocumentDiagnostic::DegradedRendering(detail) => {
            format!("Reduced-fidelity rendering: {detail}")
        },
        DocumentDiagnostic::ParseWarning(detail) => format!("Parse warning: {detail}"),
        DocumentDiagnostic::RawSourceFallback => "Raw source fallback".to_owned(),
    }
}

/// Whether `format` has a preview reading.
pub fn supported(format: knot_document::DocumentFormat) -> bool {
    matches!(
        format,
        knot_document::DocumentFormat::Djot
            | knot_document::DocumentFormat::Knot
            | knot_document::DocumentFormat::Scroll
            | knot_document::DocumentFormat::Gemtext
            | knot_document::DocumentFormat::Micron
    )
}

/// The preview reading of `key`'s document, inside reading tile `tile`: its
/// id is the tile's, so two previews can be open at once.
pub fn view(state: &DesktopState, key: DocKey, tile: workbench::TileId) -> DesktopView {
    let surface = state.surface_for(key);
    if surface.snapshot().format.native_source() {
        return Box::new(
            el("div", crate::scroll_site::preview(state, key, tile))
                .attr("class", "knot-reader-body")
                .attr("style", state.appearance.preview_style()),
        );
    }
    if !supported(surface.snapshot().format) {
        return Box::new(
            el("div", span("This document's format has no preview."))
                .attr("class", "knot-reading-empty"),
        );
    }
    let preview: DesktopView = match surface.session().preview_snapshot() {
        Ok(snapshot) => {
            let address: Arc<str> = Arc::from(snapshot.address.clone());
            let source_text: Arc<str> = Arc::from(snapshot.source_text.clone());
            let diagnostics = if snapshot.document.diagnostics.is_empty() {
                "Diagnostics: none".to_owned()
            } else {
                format!(
                    "Diagnostics: {}",
                    snapshot
                        .document
                        .diagnostics
                        .iter()
                        .map(diagnostic_text)
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            };
            let rendered = blocks(
                &snapshot.document.blocks,
                &snapshot.headings,
                &address,
                &source_text,
                Some(key),
                &mut 0,
            );
            Box::new(el(
                "div",
                (
                    span(diagnostics).attr("class", "knot-document-preview-diagnostics"),
                    rendered,
                ),
            ))
        },
        Err(error) => Box::new(el("div", span(format!("Preview unavailable: {error}")))),
    };
    Box::new(
        el(
            "div",
            el("div", preview)
                .attr("id", format!("knot-document-preview-{}", tile.0))
                .attr("class", "knot-document-preview")
                .attr("style", state.appearance.preview_style())
                .attr("role", "complementary")
                .attr("aria-label", "Document preview"),
        )
        .attr("class", "knot-preview-frame"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::{Harness, WindowCommands};
    use inker::{Engine, EngineInput};
    use knot_document::KnotDocumentSession;
    use layout_dom_api::LayoutDom;
    use taproot::Selector;

    fn pinned_preview(state: &DesktopState) -> DesktopView {
        view(state, DocKey(1), workbench::TileId(900))
    }

    #[test]
    fn pinned_preview_resolves_links_from_its_source_and_activates_existing_target() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("notes");
        std::fs::create_dir(&folder).unwrap();
        let source = folder.join("essay.djot");
        let target = temp.path().join("reference.djot");
        let other = temp.path().join("other.djot");
        std::fs::write(&source, "[Reference](../reference.djot)\n").unwrap();
        std::fs::write(&target, "# Reference\n").unwrap();
        std::fs::write(&other, "# Other\n").unwrap();
        let mut state = DesktopState::new(
            KnotDocumentSession::open(&source).unwrap(),
            WindowCommands::new(),
        );
        state.open_behind(
            vec![
                KnotDocumentSession::open_read_only(&target).unwrap(),
                KnotDocumentSession::open(&other).unwrap(),
            ],
            &[],
        );
        state.docs.focus(DocKey(3));
        let mut host = Harness::new(
            crate::desktop_sheet(),
            state,
            pinned_preview as fn(&DesktopState) -> DesktopView,
        );
        host.layout_at(900.0, 600.0);
        assert!(host.click_on(
            &Selector::role("button").with_attr("aria-label", "Preview link: ../reference.djot")
        ));
        assert_eq!(host.state().focused_key(), Some(DocKey(2)));
        assert_eq!(host.state().docs.len(), 3);
        assert_eq!(
            host.state().document().snapshot().write_posture,
            knot_document::KnotDocumentWritePostureV1::ReadOnly
        );
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            "[Reference](../reference.djot)\n"
        );
    }

    fn in_page_note(_: &DesktopState) -> DesktopView {
        let document = nematic::MicronEngine::new()
            .render(&EngineInput::new(
                "scratch:note",
                "`[Jump to notes`#notes]\n`:notes\nNotes body.\n",
            ))
            .unwrap();
        note_blocks(&document)
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

    #[test]
    fn note_preview_renders_an_in_page_link_as_its_label() {
        let mut host = Harness::new(
            CSS,
            DesktopState::new(
                KnotDocumentSession::scratch("scratch:note-host", ""),
                WindowCommands::new(),
            ),
            in_page_note as fn(&DesktopState) -> DesktopView,
        );
        host.layout_at(800.0, 600.0);
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let text = text_content(&dom, dom.document());
        assert!(text.contains("Jump to notes"), "label missing: {text:?}");
        assert!(text.contains("Notes body."));
    }
}
