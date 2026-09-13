// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use cambium::{Keyed, button, el, span};
use inker::{Block, DocumentDiagnostic, InlineSpan};
use knot_document::KnotOutlineItemV1;
use std::sync::Arc;

use crate::workspace::{DesktopState, DesktopView};

pub const CSS: &str = ".knot-document-preview-mode .knot-writing-area { display:flex; align-items:flex-start; gap:12px; } .knot-document-preview { flex:1 1 50%; width:0; min-width:0; box-sizing:border-box; padding:16px; overflow:auto; } .knot-document-preview h1,.knot-document-preview h2,.knot-document-preview h3,.knot-document-preview h4,.knot-document-preview h5 { margin:8px 0; } .knot-document-preview-heading { display:block; width:100%; text-align:left; } @media (max-width:700px) { .knot-document-preview-mode .knot-document-preview { width:100%; flex-basis:auto; } }";

fn inline(items: &[InlineSpan]) -> DesktopView {
    let children = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let view: DesktopView = match item {
                InlineSpan::Text(text) => Box::new(span(text.clone())),
                InlineSpan::Code(text) => Box::new(el("code", text.clone())),
                InlineSpan::Emphasis(items) => Box::new(el("em", inline(items))),
                InlineSpan::Strong(items) => Box::new(el("strong", inline(items))),
                InlineSpan::Link { url, spans, .. } => {
                    let destination = url.clone();
                    let accessible_destination = destination.clone();
                    Box::new(
                        button(
                            inker::inline_text(spans),
                            move |state: &mut DesktopState, _| {
                                state.message = Some(format!("Preview link: {destination}"));
                            },
                        )
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
                InlineSpan::LineBreak => Box::new(el("br", ())),
                InlineSpan::SoftBreak => Box::new(span(" ")),
            };
            (index, view)
        })
        .collect::<Vec<_>>();
    Box::new(el("span", Keyed::new(children)))
}

fn table_row(cells: &[Vec<InlineSpan>], cell_element: &'static str) -> DesktopView {
    Box::new(el(
        "tr",
        Keyed::new(
            cells
                .iter()
                .enumerate()
                .map(|(index, cell)| (index, el(cell_element, inline(cell))))
                .collect::<Vec<_>>(),
        ),
    ))
}

fn blocks(
    items: &[Block],
    headings: &[KnotOutlineItemV1],
    address: &Arc<str>,
    source_text: &Arc<str>,
    next_heading: &mut usize,
) -> DesktopView {
    let children = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let view: DesktopView = match item {
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
                Block::Paragraph { spans } => Box::new(el("p", inline(spans))),
                Block::CodeBlock { text, .. } | Block::Preformatted { text } => {
                    Box::new(el("pre", text.clone()))
                },
                Block::Quote { blocks: items } => Box::new(el(
                    "blockquote",
                    blocks(items, headings, address, source_text, next_heading),
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
                                        blocks(item, headings, address, source_text, next_heading),
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
                        Box::new(el("thead", table_row(header, "th")))
                    };
                    let body = rows
                        .iter()
                        .enumerate()
                        .map(|(row, cells)| (row, table_row(cells, "td")))
                        .collect::<Vec<_>>();
                    Box::new(el("table", (head, el("tbody", Keyed::new(body)))))
                },
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

pub fn view(state: &DesktopState) -> DesktopView {
    if !state.document_preview_visible
        || !matches!(
            state.document.snapshot().format,
            knot_document::DocumentFormat::Djot | knot_document::DocumentFormat::Knot
        )
    {
        return Box::new(el("div", ()));
    }
    let preview: DesktopView = match state.document.session().preview_snapshot() {
        Ok(snapshot) => {
            let address: Arc<str> = Arc::from(snapshot.address.clone());
            let source_text: Arc<str> = Arc::from(snapshot.source_text.clone());
            let diagnostics = if snapshot.document.diagnostics.is_empty() {
                "Preview diagnostics: none".to_owned()
            } else {
                format!(
                    "Preview diagnostics: {}",
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
                &mut 0,
            );
            Box::new(el(
                "div",
                (
                    span(format!("Source: {address}")),
                    span(diagnostics),
                    rendered,
                ),
            ))
        },
        Err(error) => Box::new(el("div", span(format!("Preview unavailable: {error}")))),
    };
    Box::new(
        el("aside", (el("h2", "Preview · current source"), preview))
            .attr("id", "knot-document-preview")
            .attr("class", "knot-document-preview")
            .attr("role", "complementary")
            .attr("aria-label", "Djot or Knot document preview"),
    )
}
