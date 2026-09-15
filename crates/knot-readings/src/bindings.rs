// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The reading surface and the sandbox it runs in.
//!
//! The sandbox is deliberately stricter than [`script_rhai::base_engine`]:
//! `Engine::new()` installs a `FileModuleResolver` on native targets, so an
//! unguarded engine reads `./x.rhai` off disk for `import "x" as m;`. This lane
//! installs a `DummyModuleResolver` and disables `import` and `eval` outright.
//! No fetch, file, or write binding exists to register.

use crate::{
    NOTE_FORMATS, ReadingBudget, ReadingInput, ReadingItem, ReadingItemKind, ReadingNoteV1,
    ReadingRowV1, hex32,
};
use knot_document::KnotFoldKindV1;
use script_rhai::rhai::{
    Array, Dynamic, Engine, EvalAltResult, ImmutableString, Position,
    module_resolvers::DummyModuleResolver,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn refuse(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from(message.into()),
        Position::NONE,
    ))
}

// ── Script-visible values ───────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpanValue {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct DocumentValue(Arc<ReadingInput>);

#[derive(Clone, Debug)]
pub(crate) struct HeadingValue {
    label: String,
    level: i64,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct SectionValue {
    label: String,
    level: i64,
    heading: SpanValue,
    body: SpanValue,
    span: SpanValue,
}

#[derive(Clone, Debug)]
pub(crate) struct FoldValue {
    kind: String,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct LinkValue {
    target: String,
    rel: String,
    text: String,
    span: SpanValue,
    has_span: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RelationValue {
    id: String,
    author: String,
    predicate: String,
    active: bool,
    subject_document: String,
    subject_start: i64,
    subject_end: i64,
    object_document: String,
    object_start: i64,
    object_end: i64,
}

// ── Engine ──────────────────────────────────────────────────────────────────

pub(crate) fn engine_for(
    input: Arc<ReadingInput>,
    budget: ReadingBudget,
    ops: Arc<AtomicU64>,
) -> Engine {
    let mut engine = script_rhai::base_engine();
    engine.set_module_resolver(DummyModuleResolver::new());
    engine.disable_symbol("import");
    engine.disable_symbol("eval");
    // `max_ops == 0` is refused by `run` before we get here; `.max(1)` keeps an
    // unlimited engine unreachable even if a caller reaches this directly.
    engine.set_max_operations(budget.max_ops.max(1));
    engine.set_max_string_size(budget.max_note_bytes);
    engine.set_max_array_size(budget.max_rows.saturating_mul(4));
    engine.set_max_map_size(1024);
    engine.on_progress(move |count| {
        ops.store(count, Ordering::Relaxed);
        None
    });
    register_types(&mut engine);
    register_readers(&mut engine, Arc::clone(&input));
    register_constructors(&mut engine, input);
    engine
}

fn register_types(engine: &mut Engine) {
    engine.register_type_with_name::<DocumentValue>("Document");
    engine.register_type_with_name::<HeadingValue>("Heading");
    engine.register_type_with_name::<SectionValue>("Section");
    engine.register_type_with_name::<FoldValue>("Fold");
    engine.register_type_with_name::<LinkValue>("Link");
    engine.register_type_with_name::<RelationValue>("Relation");
    engine.register_type_with_name::<SpanValue>("Span");
    engine.register_type_with_name::<ReadingItem>("ReadingItem");

    engine.register_get("start", |span: &mut SpanValue| span.start as i64);
    engine.register_get("end", |span: &mut SpanValue| span.end as i64);
    engine.register_get("len", |span: &mut SpanValue| {
        span.end.saturating_sub(span.start) as i64
    });

    engine.register_get("address", |doc: &mut DocumentValue| {
        doc.0.source.address.clone()
    });
    engine.register_get("format", |doc: &mut DocumentValue| doc.0.format.clone());
    engine.register_get("text", |doc: &mut DocumentValue| doc.0.text.clone());
    engine.register_get("selection_start", |doc: &mut DocumentValue| {
        doc.0.selection.map_or(-1, |(start, _)| start as i64)
    });
    engine.register_get("selection_end", |doc: &mut DocumentValue| {
        doc.0.selection.map_or(-1, |(_, end)| end as i64)
    });
    engine.register_get("revision", |doc: &mut DocumentValue| {
        doc.0
            .source
            .revision
            .as_ref()
            .map(hex32)
            .unwrap_or_default()
    });
    engine.register_fn(
        "text_in",
        |doc: &mut DocumentValue, span: SpanValue| -> Result<String, Box<EvalAltResult>> {
            slice(&doc.0.text, span).map(str::to_owned)
        },
    );

    engine.register_get("label", |heading: &mut HeadingValue| heading.label.clone());
    engine.register_get("level", |heading: &mut HeadingValue| heading.level);
    engine.register_get("start", |heading: &mut HeadingValue| heading.start as i64);
    engine.register_get("end", |heading: &mut HeadingValue| heading.end as i64);
    engine.register_get("span", |heading: &mut HeadingValue| SpanValue {
        start: heading.start,
        end: heading.end,
    });

    engine.register_get("label", |section: &mut SectionValue| section.label.clone());
    engine.register_get("level", |section: &mut SectionValue| section.level);
    engine.register_get("heading", |section: &mut SectionValue| section.heading);
    engine.register_get("body", |section: &mut SectionValue| section.body);
    engine.register_get("span", |section: &mut SectionValue| section.span);

    engine.register_get("kind", |fold: &mut FoldValue| fold.kind.clone());
    engine.register_get("start", |fold: &mut FoldValue| fold.start as i64);
    engine.register_get("end", |fold: &mut FoldValue| fold.end as i64);
    engine.register_get("span", |fold: &mut FoldValue| SpanValue {
        start: fold.start,
        end: fold.end,
    });

    engine.register_get("target", |link: &mut LinkValue| link.target.clone());
    engine.register_get("rel", |link: &mut LinkValue| link.rel.clone());
    engine.register_get("text", |link: &mut LinkValue| link.text.clone());
    engine.register_get("span", |link: &mut LinkValue| link.span);
    engine.register_get("has_span", |link: &mut LinkValue| link.has_span);

    engine.register_get("id", |relation: &mut RelationValue| relation.id.clone());
    engine.register_get("predicate", |relation: &mut RelationValue| {
        relation.predicate.clone()
    });
    engine.register_get("author", |relation: &mut RelationValue| {
        relation.author.clone()
    });
    engine.register_get("active", |relation: &mut RelationValue| relation.active);
    engine.register_get("subject_document", |relation: &mut RelationValue| {
        relation.subject_document.clone()
    });
    engine.register_get("subject_start", |relation: &mut RelationValue| {
        relation.subject_start
    });
    engine.register_get("subject_end", |relation: &mut RelationValue| {
        relation.subject_end
    });
    engine.register_get("object_document", |relation: &mut RelationValue| {
        relation.object_document.clone()
    });
    engine.register_get("object_start", |relation: &mut RelationValue| {
        relation.object_start
    });
    engine.register_get("object_end", |relation: &mut RelationValue| {
        relation.object_end
    });
}

/// Readers hand back freshly built values every call. A script holds copies, so
/// nothing it does can reach the host's input.
fn register_readers(engine: &mut Engine, input: Arc<ReadingInput>) {
    let document = Arc::clone(&input);
    engine.register_fn("document", move || DocumentValue(Arc::clone(&document)));

    let outline = Arc::clone(&input);
    engine.register_fn("outline", move || -> Array {
        outline
            .outline
            .iter()
            .map(|item| {
                Dynamic::from(HeadingValue {
                    label: item.label.clone(),
                    level: item.level as i64,
                    start: item.start,
                    end: item.end,
                })
            })
            .collect()
    });

    let sections = Arc::clone(&input);
    engine.register_fn("sections", move || -> Array {
        let items = &sections.outline;
        items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                // A section runs to the first later heading at or above its own
                // level; deeper headings are nested inside it.
                let next_start = items[index + 1..]
                    .iter()
                    .find(|later| later.level <= item.level)
                    .map_or(sections.text.len(), |later| later.start);
                let end = next_start.max(item.end);
                Dynamic::from(SectionValue {
                    label: item.label.clone(),
                    level: item.level as i64,
                    heading: SpanValue {
                        start: item.start,
                        end: item.end,
                    },
                    body: SpanValue {
                        start: item.end,
                        end,
                    },
                    span: SpanValue {
                        start: item.start,
                        end,
                    },
                })
            })
            .collect()
    });

    let folds = Arc::clone(&input);
    engine.register_fn("folds", move || -> Array {
        folds
            .folds
            .iter()
            .map(|item| {
                Dynamic::from(FoldValue {
                    kind: fold_kind(item.kind).to_owned(),
                    start: item.start,
                    end: item.end,
                })
            })
            .collect()
    });

    let links = Arc::clone(&input);
    engine.register_fn("links", move || -> Array {
        links
            .links
            .iter()
            .map(|link| {
                let (start, end) = link.span.unwrap_or((0, 0));
                Dynamic::from(LinkValue {
                    target: link.target.clone(),
                    rel: link.rel.clone().unwrap_or_default(),
                    text: link.text.clone(),
                    span: SpanValue { start, end },
                    has_span: link.span.is_some(),
                })
            })
            .collect()
    });

    engine.register_fn("relations", move || -> Array {
        input
            .relations
            .iter()
            .map(|relation| {
                let (subject_start, subject_end) = endpoint_range(relation.subject.position);
                let (object_start, object_end) = endpoint_range(relation.object.position);
                Dynamic::from(RelationValue {
                    id: hex32(&relation.id),
                    author: hex32(&relation.author),
                    predicate: relation.predicate.clone(),
                    active: relation.active,
                    subject_document: relation.subject.document_id.clone(),
                    subject_start,
                    subject_end,
                    object_document: relation.object.document_id.clone(),
                    object_start,
                    object_end,
                })
            })
            .collect()
    });
}

fn register_constructors(engine: &mut Engine, input: Arc<ReadingInput>) {
    let text = Arc::clone(&input);
    engine.register_fn(
        "span",
        move |start: i64, end: i64| -> Result<SpanValue, Box<EvalAltResult>> {
            if start < 0 || end < start {
                return Err(refuse(format!("span({start}, {end}) is not a range")));
            }
            let span = SpanValue {
                start: start as usize,
                end: end as usize,
            };
            slice(&text.text, span)?;
            Ok(span)
        },
    );

    engine.register_fn("row", |label: ImmutableString, span: SpanValue| {
        ReadingItem(ReadingItemKind::Row(ReadingRowV1 {
            label: label.into_owned(),
            span: Some((span.start, span.end)),
            document: None,
        }))
    });
    engine.register_fn("row", |label: ImmutableString| {
        ReadingItem(ReadingItemKind::Row(ReadingRowV1 {
            label: label.into_owned(),
            span: None,
            document: None,
        }))
    });
    engine.register_fn("note", |text: ImmutableString| {
        ReadingItem(ReadingItemKind::Note(ReadingNoteV1 {
            format: "plain".to_owned(),
            text: text.into_owned(),
        }))
    });
    engine.register_fn(
        "note",
        |text: ImmutableString,
         format: ImmutableString|
         -> Result<ReadingItem, Box<EvalAltResult>> {
            if !NOTE_FORMATS.contains(&format.as_str()) {
                return Err(refuse(format!(
                    "note format {format:?} is not one of {}",
                    NOTE_FORMATS.join(", ")
                )));
            }
            Ok(ReadingItem(ReadingItemKind::Note(ReadingNoteV1 {
                format: format.into_owned(),
                text: text.into_owned(),
            })))
        },
    );
}

/// The names a reading may call. The sandbox receipt asserts this is the whole
/// surface: everything outside it must fail to resolve.
pub const READING_SURFACE: [&str; 10] = [
    "document",
    "outline",
    "sections",
    "folds",
    "links",
    "relations",
    "span",
    "row",
    "note",
    "text_in",
];

fn fold_kind(kind: KnotFoldKindV1) -> &'static str {
    match kind {
        KnotFoldKindV1::Section => "section",
        KnotFoldKindV1::List => "list",
        KnotFoldKindV1::Blockquote => "blockquote",
        KnotFoldKindV1::CodeBlock => "code",
        KnotFoldKindV1::Div => "div",
    }
}

fn endpoint_range(position: Option<(usize, usize)>) -> (i64, i64) {
    position.map_or((-1, -1), |(start, end)| (start as i64, end as i64))
}

fn slice(text: &str, span: SpanValue) -> Result<&str, Box<EvalAltResult>> {
    if span.end > text.len() {
        return Err(refuse(format!(
            "span {}..{} runs past the {} byte source",
            span.start,
            span.end,
            text.len()
        )));
    }
    if !text.is_char_boundary(span.start) || !text.is_char_boundary(span.end) {
        return Err(refuse(format!(
            "span {}..{} does not land on character boundaries",
            span.start, span.end
        )));
    }
    Ok(&text[span.start..span.end])
}
