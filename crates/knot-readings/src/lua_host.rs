// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The host values of the Lua reading surface: what a userdata holds and the
//! fields it answers to. See [`crate::lua_bindings`] for how they are made
//! and why a write to one fails.

use crate::{ReadingInput, ReadingItemKind, hex32};
use piccolo::{Callback, CallbackReturn, Context, Error, Stack, Table, UserData, Value};
use std::fmt;
use std::sync::Arc;

/// The error a host function or a refused write raises into the script.
#[derive(Debug)]
struct Refused(String);

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

pub(crate) fn refuse<'gc>(message: impl Into<String>) -> Error<'gc> {
    Error::from(Refused(message.into()))
}

/// A byte range of the source.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Range {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// One end of a relation: a document and a byte range, absent when unplaced.
#[derive(Clone, Debug)]
pub(crate) struct Endpoint {
    pub(crate) document: String,
    pub(crate) range: Option<Range>,
}

/// What a host value holds. Everything is owned or shared and read-only.
#[derive(Clone, Debug)]
pub(crate) enum Host {
    Span(Range),
    Document(Arc<ReadingInput>),
    Heading {
        label: String,
        level: i64,
        range: Range,
    },
    Section {
        label: String,
        level: i64,
        heading: Range,
        body: Range,
        span: Range,
    },
    Fold {
        kind: &'static str,
        range: Range,
    },
    Link {
        target: String,
        rel: Option<String>,
        text: String,
        span: Option<Range>,
    },
    Relation {
        id: String,
        author: String,
        predicate: String,
        active: bool,
        subject: Endpoint,
        object: Endpoint,
    },
    Item(ReadingItemKind),
}

impl Host {
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Self::Span(_) => "Span",
            Self::Document(_) => "Document",
            Self::Heading { .. } => "Heading",
            Self::Section { .. } => "Section",
            Self::Fold { .. } => "Fold",
            Self::Link { .. } => "Link",
            Self::Relation { .. } => "Relation",
            Self::Item(_) => "ReadingItem",
        }
    }
}

pub(crate) fn int<'gc>(value: usize) -> Value<'gc> {
    Value::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

pub(crate) fn text<'gc>(ctx: Context<'gc>, value: &str) -> Value<'gc> {
    Value::String(ctx.intern(value.as_bytes()))
}

/// Make a host value carrying the shared metatable.
pub(crate) fn make<'gc>(ctx: Context<'gc>, meta: Table<'gc>, host: Host) -> Value<'gc> {
    let value = UserData::new_static(&ctx, host);
    value.set_metatable(&ctx, Some(meta));
    value.into()
}

pub(crate) fn span<'gc>(ctx: Context<'gc>, meta: Table<'gc>, range: Range) -> Value<'gc> {
    make(ctx, meta, Host::Span(range))
}

/// The value of `key` on `host`, or `None` when the type has no such field.
/// A range's end is `stop`: `end` is a Lua keyword.
pub(crate) fn field<'gc>(
    host: &Host,
    ctx: Context<'gc>,
    meta: Table<'gc>,
    key: &str,
) -> Option<Value<'gc>> {
    let range = |range: &Range| match key {
        "start" => Some(int(range.start)),
        "stop" => Some(int(range.end)),
        "span" => Some(span(ctx, meta, *range)),
        _ => None,
    };
    match host {
        Host::Span(range) => match key {
            "start" => Some(int(range.start)),
            "stop" => Some(int(range.end)),
            "len" => Some(int(range.end.saturating_sub(range.start))),
            _ => None,
        },
        Host::Document(input) => document_field(input, ctx, key),
        Host::Heading {
            label,
            level,
            range: at,
        } => match key {
            "label" => Some(text(ctx, label)),
            "level" => Some(Value::Integer(*level)),
            _ => range(at),
        },
        Host::Section {
            label,
            level,
            heading,
            body,
            span: whole,
        } => match key {
            "label" => Some(text(ctx, label)),
            "level" => Some(Value::Integer(*level)),
            "heading" => Some(span(ctx, meta, *heading)),
            "body" => Some(span(ctx, meta, *body)),
            "span" => Some(span(ctx, meta, *whole)),
            _ => None,
        },
        Host::Fold { kind, range: at } => match key {
            "kind" => Some(text(ctx, kind)),
            _ => range(at),
        },
        Host::Link {
            target,
            rel,
            text: label,
            span: at,
        } => match key {
            "target" => Some(text(ctx, target)),
            "rel" => Some(rel.as_deref().map_or(Value::Nil, |rel| text(ctx, rel))),
            "text" => Some(text(ctx, label)),
            "span" => Some(at.map_or(Value::Nil, |at| span(ctx, meta, at))),
            _ => None,
        },
        Host::Relation { .. } => relation_field(host, ctx, key),
        Host::Item(_) => None,
    }
}

fn document_field<'gc>(input: &ReadingInput, ctx: Context<'gc>, key: &str) -> Option<Value<'gc>> {
    let selection =
        |pick: fn((usize, usize)) -> usize| input.selection.map_or(Value::Nil, |at| int(pick(at)));
    Some(match key {
        "address" => text(ctx, &input.source.address),
        "format" => text(ctx, &input.format),
        "text" => text(ctx, &input.text),
        "selection_start" => selection(|(start, _)| start),
        "selection_end" => selection(|(_, end)| end),
        "revision" => input
            .source
            .revision
            .as_ref()
            .map_or(Value::Nil, |revision| text(ctx, &hex32(revision))),
        "text_in" => Callback::from_fn(&ctx, text_in).into(),
        _ => return None,
    })
}

/// One offset of a relation endpoint, nil when the endpoint is unplaced.
fn place<'gc>(end: &Endpoint, pick: fn(&Range) -> usize) -> Value<'gc> {
    end.range.as_ref().map_or(Value::Nil, |at| int(pick(at)))
}

fn relation_field<'gc>(host: &Host, ctx: Context<'gc>, key: &str) -> Option<Value<'gc>> {
    let Host::Relation {
        id,
        author,
        predicate,
        active,
        subject,
        object,
    } = host
    else {
        return None;
    };
    Some(match key {
        "id" => text(ctx, id),
        "author" => text(ctx, author),
        "predicate" => text(ctx, predicate),
        "active" => Value::Boolean(*active),
        "subject_document" => text(ctx, &subject.document),
        "subject_start" => place(subject, |at| at.start),
        "subject_stop" => place(subject, |at| at.end),
        "object_document" => text(ctx, &object.document),
        "object_start" => place(object, |at| at.start),
        "object_stop" => place(object, |at| at.end),
        _ => return None,
    })
}

/// Refuse a range that is backwards, past the text, or inside a character.
pub(crate) fn check_range<'gc>(source: &str, start: usize, end: usize) -> Result<(), Error<'gc>> {
    if end > source.len() {
        return Err(refuse(format!(
            "span {start}..{end} runs past the {} byte source",
            source.len()
        )));
    }
    if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(refuse(format!(
            "span {start}..{end} does not land on character boundaries"
        )));
    }
    Ok(())
}

pub(crate) fn host_of<'gc>(value: Value<'gc>) -> Option<&'gc Host> {
    match value {
        Value::UserData(data) => data.downcast_static::<Host>().ok(),
        _ => None,
    }
}

/// `document:text_in(span)`: the source text under a span, a copy.
fn text_in<'gc>(
    ctx: Context<'gc>,
    _: piccolo::Execution<'gc, '_>,
    mut stack: Stack<'gc, '_>,
) -> Result<CallbackReturn<'gc>, Error<'gc>> {
    let (Some(Host::Document(input)), Some(Host::Span(range))) =
        (host_of(stack.get(0)), host_of(stack.get(1)))
    else {
        return Err(refuse("text_in takes a document and a span"));
    };
    check_range(&input.text, range.start, range.end)?;
    let piece = text(ctx, &input.text[range.start..range.end]);
    stack.replace(ctx, piece);
    Ok(CallbackReturn::Return)
}
