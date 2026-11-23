// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The reading surface on Lua: host values and the functions that make them.
//!
//! Every host value is a piccolo userdata holding a [`Host`], with one shared
//! metatable whose `__index` reads a field and whose `__newindex` refuses. A
//! script cannot reach that metatable: piccolo's `getmetatable` and
//! `setmetatable` take tables only and `rawset` needs a table, so a write to a
//! host value always fails. Readers hand back fresh values on every call, so a
//! script holds copies and nothing it does reaches the host's input.
//!
//! Byte offsets are 0-based, as spans are; lists are the Lua way, 1-based.

use knot_document::KnotFoldKindV1;
use crate::lua_host::{Endpoint, Host, Range, check_range, field, host_of, make, refuse, span};
use crate::{NOTE_FORMATS, ReadingInput, ReadingItemKind, ReadingNoteV1, ReadingRowV1, hex32};
use piccolo::{Callback, CallbackReturn, Context, Error, MetaMethod, Stack, Table, Value};
use std::sync::Arc;

/// The names a Lua reading may call. `text_in` is a method of a document
/// (`document():text_in(span)`); the rest are globals.
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

/// Charge fuel for host work: one unit per item, at least one.
fn charge(exec: &mut piccolo::Execution<'_, '_>, units: usize) {
    exec.fuel()
        .consume(i32::try_from(units.max(1)).unwrap_or(i32::MAX));
}

fn list<'gc>(
    ctx: Context<'gc>,
    values: impl Iterator<Item = Value<'gc>>,
) -> Result<Value<'gc>, Error<'gc>> {
    let table = Table::new(&ctx);
    for (index, value) in values.enumerate() {
        table.set(
            ctx,
            i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
            value,
        )?;
    }
    Ok(table.into())
}

/// Install the reading surface in a state's globals.
pub(crate) fn install<'gc>(ctx: Context<'gc>, input: &Arc<ReadingInput>) -> Result<(), Error<'gc>> {
    let meta = Table::new(&ctx);
    let index = Callback::from_fn_with(&ctx, meta, |meta, ctx, _, mut stack| {
        let (target, key): (Value, Value) = stack.consume(ctx)?;
        let host = host_of(target).ok_or_else(|| refuse("not a reading value"))?;
        let found = match key {
            Value::String(name) => name.to_str().ok().and_then(|n| field(host, ctx, *meta, n)),
            _ => None,
        };
        let value = found.ok_or_else(|| {
            refuse(format!(
                "{} has no field {}",
                host.type_name(),
                key.display()
            ))
        })?;
        stack.replace(ctx, value);
        Ok(CallbackReturn::Return)
    });
    let refuse_write = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (target, key): (Value, Value) = stack.consume(ctx)?;
        let kind = host_of(target).map_or("value", Host::type_name);
        Err(refuse(format!(
            "a {kind} is read-only: cannot set {}",
            key.display()
        )))
    });
    meta.set(ctx, MetaMethod::Index, index)?;
    meta.set(ctx, MetaMethod::NewIndex, refuse_write)?;
    readers(ctx, meta, input)?;
    constructors(ctx, meta, input)
}

fn readers<'gc>(
    ctx: Context<'gc>,
    meta: Table<'gc>,
    input: &Arc<ReadingInput>,
) -> Result<(), Error<'gc>> {
    let global = |name: &'static str, call: Callback<'gc>| ctx.globals().set(ctx, name, call);
    let doc = Arc::clone(input);
    global(
        "document",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, _, mut stack| {
            stack.replace(ctx, make(ctx, *meta, Host::Document(Arc::clone(&doc))));
            Ok(CallbackReturn::Return)
        }),
    )?;
    let outline = Arc::clone(input);
    global(
        "outline",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, mut exec, mut stack| {
            charge(&mut exec, outline.outline.len());
            let items = outline.outline.iter().map(|item| Host::Heading {
                label: item.label.clone(),
                level: i64::from(item.level),
                range: Range {
                    start: item.start,
                    end: item.end,
                },
            });
            stack.replace(ctx, list(ctx, items.map(|host| make(ctx, *meta, host)))?);
            Ok(CallbackReturn::Return)
        }),
    )?;
    let sections = Arc::clone(input);
    global(
        "sections",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, mut exec, mut stack| {
            // Each section looks ahead through the later headings.
            charge(&mut exec, sections.outline.len().saturating_mul(2));
            let items = section_hosts(&sections);
            stack.replace(ctx, list(ctx, items.map(|host| make(ctx, *meta, host)))?);
            Ok(CallbackReturn::Return)
        }),
    )?;
    readers_of_records(ctx, meta, input)
}

/// A section runs to the first later heading at or above its own level;
/// deeper headings are nested inside it.
fn section_hosts(input: &ReadingInput) -> impl Iterator<Item = Host> + '_ {
    let items = &input.outline;
    items.iter().enumerate().map(move |(index, item)| {
        let next_start = items[index + 1..]
            .iter()
            .find(|later| later.level <= item.level)
            .map_or(input.text.len(), |later| later.start);
        let end = next_start.max(item.end);
        Host::Section {
            label: item.label.clone(),
            level: i64::from(item.level),
            heading: Range {
                start: item.start,
                end: item.end,
            },
            body: Range {
                start: item.end,
                end,
            },
            span: Range {
                start: item.start,
                end,
            },
        }
    })
}

fn readers_of_records<'gc>(
    ctx: Context<'gc>,
    meta: Table<'gc>,
    input: &Arc<ReadingInput>,
) -> Result<(), Error<'gc>> {
    let global = |name: &'static str, call: Callback<'gc>| ctx.globals().set(ctx, name, call);
    let folds = Arc::clone(input);
    global(
        "folds",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, mut exec, mut stack| {
            charge(&mut exec, folds.folds.len());
            let items = folds.folds.iter().map(|item| Host::Fold {
                kind: fold_kind(item.kind),
                range: Range {
                    start: item.start,
                    end: item.end,
                },
            });
            stack.replace(ctx, list(ctx, items.map(|host| make(ctx, *meta, host)))?);
            Ok(CallbackReturn::Return)
        }),
    )?;
    let links = Arc::clone(input);
    global(
        "links",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, mut exec, mut stack| {
            charge(&mut exec, links.links.len());
            let items = links.links.iter().map(link_host);
            stack.replace(ctx, list(ctx, items.map(|host| make(ctx, *meta, host)))?);
            Ok(CallbackReturn::Return)
        }),
    )?;
    let relations = Arc::clone(input);
    global(
        "relations",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, mut exec, mut stack| {
            charge(&mut exec, relations.relations.len());
            let items = relations.relations.iter().map(relation_host);
            stack.replace(ctx, list(ctx, items.map(|host| make(ctx, *meta, host)))?);
            Ok(CallbackReturn::Return)
        }),
    )?;
    Ok(())
}

fn link_host(link: &crate::ReadingLinkV1) -> Host {
    Host::Link {
        target: link.target.clone(),
        rel: link.rel.clone(),
        text: link.text.clone(),
        span: link.span.map(|(start, end)| Range { start, end }),
    }
}

fn relation_host(relation: &crate::ReadingRelationV1) -> Host {
    let endpoint = |end: &crate::ReadingEndpointV1| Endpoint {
        document: end.document_id.clone(),
        range: end.position.map(|(start, end)| Range { start, end }),
    };
    Host::Relation {
        id: hex32(&relation.id),
        author: hex32(&relation.author),
        predicate: relation.predicate.clone(),
        active: relation.active,
        subject: endpoint(&relation.subject),
        object: endpoint(&relation.object),
    }
}

/// A string argument as UTF-8 text.
fn text_arg<'gc>(value: Value<'gc>, what: &str) -> Result<&'gc str, Error<'gc>> {
    match value {
        Value::String(string) => string
            .to_str()
            .map_err(|_| refuse(format!("{what} is not valid UTF-8"))),
        other => Err(refuse(format!(
            "{what} must be a string, not a {}",
            other.type_name()
        ))),
    }
}

fn constructors<'gc>(
    ctx: Context<'gc>,
    meta: Table<'gc>,
    input: &Arc<ReadingInput>,
) -> Result<(), Error<'gc>> {
    let source = Arc::clone(input);
    ctx.globals().set(
        ctx,
        "span",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, _, mut stack| {
            let (start, end): (i64, i64) = stack.consume(ctx)?;
            let not_a_range = || refuse(format!("span({start}, {end}) is not a range"));
            if end < start {
                return Err(not_a_range());
            }
            let from = usize::try_from(start).map_err(|_| not_a_range())?;
            let to = usize::try_from(end).map_err(|_| not_a_range())?;
            check_range(&source.text, from, to)?;
            stack.replace(
                ctx,
                span(
                    ctx,
                    *meta,
                    Range {
                        start: from,
                        end: to,
                    },
                ),
            );
            Ok(CallbackReturn::Return)
        }),
    )?;
    ctx.globals().set(
        ctx,
        "row",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, _, stack| {
            row(ctx, *meta, stack)
        }),
    )?;
    ctx.globals().set(
        ctx,
        "note",
        Callback::from_fn_with(&ctx, meta, move |meta, ctx, _, stack| {
            note(ctx, *meta, stack)
        }),
    )?;
    Ok(())
}

fn row<'gc>(
    ctx: Context<'gc>,
    meta: Table<'gc>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let label = text_arg(stack.get(0), "a row's label")?.to_owned();
    let span = match stack.get(1) {
        Value::Nil => None,
        other => match host_of(other) {
            Some(Host::Span(range)) => Some((range.start, range.end)),
            _ => return Err(refuse("a row's second argument must be a span")),
        },
    };
    let item = ReadingItemKind::Row(ReadingRowV1 {
        label,
        span,
        document: None,
    });
    stack.replace(ctx, make(ctx, meta, Host::Item(item)));
    Ok(CallbackReturn::Return)
}

fn note<'gc>(
    ctx: Context<'gc>,
    meta: Table<'gc>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let body = text_arg(stack.get(0), "a note's text")?.to_owned();
    let format = match stack.get(1) {
        Value::Nil => "plain",
        other => text_arg(other, "a note's format")?,
    };
    if !NOTE_FORMATS.contains(&format) {
        return Err(refuse(format!(
            "note format {format:?} is not one of {}",
            NOTE_FORMATS.join(", ")
        )));
    }
    let item = ReadingItemKind::Note(ReadingNoteV1 {
        format: format.to_owned(),
        text: body,
    });
    stack.replace(ctx, make(ctx, meta, Host::Item(item)));
    Ok(CallbackReturn::Return)
}

/// The reading item a script value holds, if it holds one.
pub(crate) fn item_of(value: Value<'_>) -> Option<ReadingItemKind> {
    match host_of(value)? {
        Host::Item(item) => Some(item.clone()),
        _ => None,
    }
}

/// The name of a script value for a shape error: a host type's own, else Lua's.
pub(crate) fn kind_of(value: Value<'_>) -> &'static str {
    host_of(value).map_or_else(|| value.type_name(), Host::type_name)
}

fn fold_kind(kind: KnotFoldKindV1) -> &'static str {
    match kind {
        KnotFoldKindV1::Section => "section",
        KnotFoldKindV1::List => "list",
        KnotFoldKindV1::Blockquote => "blockquote",
        KnotFoldKindV1::CodeBlock => "code",
        KnotFoldKindV1::Div => "div",
    }
}
