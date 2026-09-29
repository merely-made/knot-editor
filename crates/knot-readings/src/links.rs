// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Inline links with byte spans.
//!
//! The parsed preview is authoritative for links, labels and predicates. It
//! carries no source ranges, so a bounded scan supplies candidate ranges for
//! simple inline links. Candidates are joined by URL and literal label in
//! document order. Syntax the scan cannot place keeps `span: None`; a scan hit
//! without a parsed link is never emitted as a link fact.

use crate::ReadingLinkV1;
use inker::{Block, EngineDocument, InlineSpan, inline_text};

/// Upper bound on scanned links, so a pathological source cannot make the scan
/// the expensive part of a bounded reading.
const MAX_SCANNED_LINKS: usize = 4_096;

struct Scanned {
    target: String,
    text: String,
    span: (usize, usize),
}

pub fn extract(source: &str, preview: &EngineDocument) -> Vec<ReadingLinkV1> {
    let mut scanned: Vec<_> = scan(source).into_iter().map(Some).collect();
    let mut out = Vec::new();
    for block in &preview.blocks {
        collect_block(block, &mut out);
    }
    for link in &mut out {
        if let Some(candidate) = scanned.iter_mut().find(|candidate| {
            candidate
                .as_ref()
                .is_some_and(|hit| hit.target == link.target && hit.text == link.text)
        }) {
            link.span = candidate.take().map(|hit| hit.span);
        }
    }
    out
}

fn collect_block(block: &Block, out: &mut Vec<ReadingLinkV1>) {
    match block {
        Block::Presented { block, .. } => collect_block(block, out),
        Block::Heading { spans, .. } | Block::Paragraph { spans } => collect_spans(spans, out),
        Block::Quote { blocks } => {
            for block in blocks {
                collect_block(block, out);
            }
        },
        Block::List { items, .. } => {
            for block in items.iter().flatten() {
                collect_block(block, out);
            }
        },
        Block::Table { header, rows, .. } => {
            for cell in header.iter().chain(rows.iter().flatten()) {
                collect_spans(cell, out);
            }
        },
        _ => {},
    }
}

fn collect_spans(spans: &[InlineSpan], out: &mut Vec<ReadingLinkV1>) {
    for span in spans {
        match span {
            InlineSpan::Link {
                url,
                predicate,
                spans,
                ..
            } => {
                out.push(ReadingLinkV1 {
                    target: url.clone(),
                    rel: predicate.clone(),
                    text: inline_text(spans),
                    span: None,
                });
                collect_spans(spans, out);
            },
            InlineSpan::Presented { spans, .. }
            | InlineSpan::Emphasis(spans)
            | InlineSpan::Strong(spans)
            | InlineSpan::Submit { spans, .. }
            | InlineSpan::InPage { spans, .. } => collect_spans(spans, out),
            _ => {},
        }
    }
}

/// Scan `[text](url)` inline links, with the trailing `{...}` attribute block
/// folded into the span when present. Fenced code is skipped so a link inside a
/// code block is not offered as a selectable range.
fn scan(source: &str) -> Vec<Scanned> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut inline_code = None;
    let mut line_start = 0;
    while line_start <= bytes.len() {
        let line_end = source[line_start..]
            .find('\n')
            .map_or(bytes.len(), |offset| line_start + offset);
        let line = &source[line_start..line_end];
        let trimmed = line.trim_start();
        let marker = trimmed.as_bytes().first().copied();
        let run = trimmed
            .bytes()
            .take_while(|byte| Some(*byte) == marker)
            .count();
        if matches!(marker, Some(b'`' | b'~')) && run >= 3 {
            match fence {
                None => fence = marker.map(|marker| (marker, run)),
                Some((opening, length)) if marker == Some(opening) && run >= length => fence = None,
                _ => {},
            }
            inline_code = None;
        } else if fence.is_none() {
            if trimmed.is_empty() {
                inline_code = None;
            }
            scan_line(source, line_start, line_end, &mut inline_code, &mut out);
        }
        if out.len() >= MAX_SCANNED_LINKS {
            break;
        }
        line_start = line_end + 1;
    }
    out
}

fn scan_line(
    source: &str,
    line_start: usize,
    line_end: usize,
    inline_code: &mut Option<usize>,
    out: &mut Vec<Scanned>,
) {
    let bytes = source.as_bytes();
    let mut index = line_start;
    while index < line_end {
        if let Some(run) = *inline_code {
            if bytes[index] != b'`' {
                index += 1;
                continue;
            }
            let end = (index..line_end)
                .find(|i| bytes[*i] != b'`')
                .unwrap_or(line_end);
            if end - index == run {
                *inline_code = None;
            }
            index = end;
            continue;
        }
        if bytes[index] == b'\\' {
            // Escaped punctuation cannot start either a link or inline code.
            index += 1;
            if index < line_end && bytes[index].is_ascii_punctuation() {
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'`' {
            let run_end = (index..line_end)
                .find(|i| bytes[*i] != b'`')
                .unwrap_or(line_end);
            *inline_code = Some(run_end - index);
            index = run_end;
            continue;
        }
        if bytes[index] != b'[' {
            index += 1;
            continue;
        }
        // Image syntax is not a navigation link.
        if index > line_start && bytes[index - 1] == b'!' {
            index += 1;
            continue;
        }
        let Some(label_end) = find_unescaped(bytes, index + 1, line_end, b']') else {
            return;
        };
        if label_end + 1 >= line_end || bytes[label_end + 1] != b'(' {
            index = label_end + 1;
            continue;
        }
        let Some(url_end) = find_unescaped(bytes, label_end + 2, line_end, b')') else {
            return;
        };
        let mut span_end = url_end + 1;
        if span_end < line_end
            && bytes[span_end] == b'{'
            && let Some(attributes_end) = find_unescaped(bytes, span_end + 1, line_end, b'}')
        {
            span_end = attributes_end + 1;
        }
        out.push(Scanned {
            target: source[label_end + 2..url_end].to_owned(),
            text: literal_label(&source[index + 1..label_end]),
            span: (index, span_end),
        });
        if out.len() >= MAX_SCANNED_LINKS {
            return;
        }
        index = span_end;
    }
}

fn find_unescaped(bytes: &[u8], from: usize, to: usize, needle: u8) -> Option<usize> {
    let mut index = from;
    while index < to {
        if bytes[index] == b'\\' && index + 1 < to && bytes[index + 1].is_ascii_punctuation() {
            index += 2;
        } else if bytes[index] == needle {
            return Some(index);
        } else {
            index += 1;
        }
    }
    None
}

fn literal_label(label: &str) -> String {
    let mut characters = label.chars().peekable();
    let mut text = String::new();
    while let Some(character) = characters.next() {
        if character == '\\' && characters.peek().is_some_and(char::is_ascii_punctuation) {
            text.push(characters.next().unwrap());
        } else {
            text.push(character);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use knot_document::{DocumentFormat, KnotDocumentSession};

    fn links(source: &str) -> Vec<ReadingLinkV1> {
        let session = KnotDocumentSession::read_only_with_format(
            "memory:links",
            source,
            DocumentFormat::Djot,
        );
        crate::ReadingInput::from_session(&session).unwrap().links
    }

    #[test]
    fn escaped_labels_have_literal_text_and_exact_source_spans() {
        let source = r#"Prefix [\[α\] \\ slash](./b%20c.djot){rel="https://mere.computer/ns/rel#supports"} suffix"#;
        let links = links(source);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].text, r"[α] \ slash");
        let span = links[0].span.unwrap();
        assert_eq!(
            &source[span.0..span.1],
            r#"[\[α\] \\ slash](./b%20c.djot){rel="https://mere.computer/ns/rel#supports"}"#
        );
        assert_eq!(
            links[0].rel.as_deref(),
            Some("https://mere.computer/ns/rel#supports")
        );
    }

    #[test]
    fn fake_links_in_code_images_and_escaped_syntax_do_not_become_facts() {
        let source = "`[same](b.djot)` \\[same](b.djot) ![image](image.png) [same](b.djot)\n\n```\n[fenced](c.djot)\n```\n";
        let links = links(source);
        assert_eq!(links.len(), 1, "{links:?}");
        assert_eq!(links[0].text, "same");
        assert_eq!(links[0].target, "b.djot");
        assert_eq!(
            links[0].span,
            source
                .rfind("[same](b.djot)")
                .map(|start| (start, start + "[same](b.djot)".len()))
        );
    }

    #[test]
    fn duplicate_destinations_keep_parser_labels_and_predicates_in_order() {
        let links =
            links("[first](b.djot) [second](b.djot){rel=cites} [third](b.djot){rel=supports}");
        assert_eq!(
            links
                .iter()
                .map(|link| link.text.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert_eq!(
            links
                .iter()
                .map(|link| link.rel.as_deref())
                .collect::<Vec<_>>(),
            [None, Some("cites"), Some("supports")]
        );
        assert!(links.iter().all(|link| link.span.is_some()));
    }

    #[test]
    fn multiline_inline_code_cannot_claim_a_real_links_span() {
        let source = "`code\n[same](b.djot)\ncode`\n\n[same](b.djot)\n";
        let links = links(source);
        assert_eq!(links.len(), 1, "{links:?}");
        let start = source.rfind("[same](b.djot)").unwrap();
        assert_eq!(links[0].span, Some((start, start + "[same](b.djot)".len())));
    }
}
