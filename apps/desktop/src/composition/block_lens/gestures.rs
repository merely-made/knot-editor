// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Preview against current block facts; commit once through document history.
use super::*;
use cambium::{Key, NamedKey, PointerButton, PointerEvent, PointerPhase};
use scenomise::{
    backdrops::SceneInput,
    span_edit::{SpanEdit, SpanEditIntent},
};

pub(super) struct Drag {
    pub owner: DocKey,
    pub id: String,
    baseline: SceneInput,
    proposal: SpanEditIntent,
    edit: SpanEdit,
    x: f32,
    scale: f32,
    size: (f32, f32),
    pointer: i32,
}
fn input(state: &DesktopState, key: DocKey, id: &str) -> Result<SceneInput, String> {
    if state.focused_key() != Some(key) {
        return Err("Composition owner changed".into());
    }
    let snapshot = state.surface_for(key).session().div_snapshot()?;
    let div = find(&snapshot.divs, id).ok_or("Block is missing or its id is ambiguous")?;
    Ok(metre_block::scene(
        &snapshot.address,
        id,
        &snapshot.source_text[div.body.clone()],
        &div.attributes,
    )?
    .0)
}
pub(super) fn preview(
    state: &DesktopState,
    key: DocKey,
    id: &str,
    current: &SceneInput,
) -> SceneInput {
    let Some(drag) = &state.composition.block_lens.drag else {
        return current.clone();
    };
    if drag.owner != key || drag.id != id {
        return current.clone();
    }
    let Ok(facts) = drag.proposal.proposed_facts(current) else {
        return current.clone();
    };
    let mut next = current.clone();
    next.facts = facts;
    next.revision = format!(
        "preview:{}:{}:{}",
        current.revision, drag.proposal.after.start, drag.proposal.after.length
    );
    next
}
fn accept(
    state: &mut DesktopState,
    key: DocKey,
    id: &str,
    intent: &SpanEditIntent,
) -> Result<(), String> {
    if state.focused_key() != Some(key) {
        return Err("Composition owner changed".into());
    }
    if intent.before == intent.after {
        return Ok(());
    }
    let snapshot = state.surface_for(key).session().div_snapshot()?;
    let div = find(&snapshot.divs, id).ok_or("Block is missing or its id is ambiguous")?;
    let mut attrs = div.attributes.clone();
    metre_block::apply_span(
        &snapshot.address,
        id,
        &snapshot.source_text[div.body.clone()],
        &mut attrs,
        intent,
    )?;
    state
        .surface_mut_for(key)
        .session_mut()
        .set_div_attributes(&snapshot, div, attrs)
}
pub(super) fn pointer(state: &mut DesktopState, key: DocKey, id: &str, event: PointerEvent) {
    if event.button != PointerButton::Primary {
        return;
    }
    if event.phase == PointerPhase::Cancel
        || !event.local.0.is_finite()
        || !event.local.1.is_finite()
        || !event.size.0.is_finite()
        || !event.size.1.is_finite()
        || event.size.0 <= 0.0
        || event.size.1 <= 0.0
    {
        state.composition.block_lens.drag = None;
        return;
    }
    let Ok(current) = input(state, key, id) else {
        state.composition.block_lens.drag = None;
        return;
    };
    if event.phase == PointerPhase::Down {
        state.composition.block_lens.drag = None;
        let surface = state.composition.block_lens.surface.borrow();
        if surface.input.as_ref() != Some(&current) {
            return;
        }
        let Some(s) = &surface.surface else { return };
        let (w, h) = (event.size.0.round() as u32, event.size.1.round() as u32);
        let found = s
            .pick(event.local.0, event.local.1, w, h)
            .map(str::to_owned);
        let geometry = found
            .as_ref()
            .and_then(|key| s.occurrence_rect(key, w, h))
            .and_then(|r| {
                Some((
                    r,
                    (s.scene_point(1.0, 0.0, w, h)?.x - s.scene_point(0.0, 0.0, w, h)?.x) / 47.0,
                ))
            });
        drop(surface);
        if let Some(s) = state
            .composition
            .block_lens
            .surface
            .borrow_mut()
            .surface
            .as_mut()
        {
            let _ = s.select(found.as_deref());
        }
        if let (Some(found), Some((r, scale))) = (found, geometry) {
            let edge = 8.0_f32.min(r.width / 3.0);
            let edit = if event.local.0 <= r.x + edge {
                SpanEdit::Start
            } else if event.local.0 >= r.x + r.width - edge {
                SpanEdit::End
            } else {
                SpanEdit::Move
            };
            if let Ok(proposal) = SpanEditIntent::propose(&current, &found, edit, 0.0, 1) {
                state.composition.block_lens.drag = Some(Drag {
                    owner: key,
                    id: id.into(),
                    baseline: current,
                    proposal,
                    edit,
                    x: event.local.0,
                    scale,
                    size: event.size,
                    pointer: event.pointer_id,
                });
            }
        }
    } else if let Some(mut drag) = state.composition.block_lens.drag.take() {
        if drag.pointer != event.pointer_id {
            state.composition.block_lens.drag = Some(drag);
            return;
        }
        if drag.owner == key && drag.id == id && drag.size == event.size && drag.baseline == current
        {
            let dx = event.local.0 - drag.x;
            if let Ok(proposal) = SpanEditIntent::propose(
                &current,
                &drag.proposal.occurrence,
                drag.edit,
                if dx.abs() < 3.0 { 0.0 } else { dx * drag.scale },
                1,
            ) {
                drag.proposal = proposal;
                if event.phase == PointerPhase::Up {
                    let result = accept(state, key, id, &drag.proposal);
                    report(state, result);
                } else {
                    state.composition.block_lens.drag = Some(drag);
                }
            }
        }
    }
    state.composition.block_lens.surface.borrow_mut().revision += 1;
    event.prop.prevent_default();
}
pub(super) fn keypress(state: &mut DesktopState, key: DocKey, id: &str, event: cambium::KeyEvent) {
    if state.focused_key() != Some(key) {
        state.composition.block_lens.drag = None;
        return;
    }
    match event.key {
        Key::Character(ref c)
            if (event.mods.ctrl || event.mods.meta)
                && (c.to_string().eq_ignore_ascii_case("z")
                    || c.to_string().eq_ignore_ascii_case("y")) =>
        {
            state.composition.block_lens.drag = None;
            let redo = event.mods.shift || c.to_string().eq_ignore_ascii_case("y");
            let result = state
                .surface_mut_for(key)
                .apply(knot_document::KnotDocumentIntentV1::Edit(if redo {
                    cambium::TextCommand::Redo
                } else {
                    cambium::TextCommand::Undo
                }))
                .map(|_| ())
                .map_err(|e| format!("History action refused: {e:?}"));
            report(state, result);
        },
        Key::Named(NamedKey::Escape) => state.composition.block_lens.drag = None,
        Key::Named(NamedKey::Enter) => {
            if let Some(drag) = state.composition.block_lens.drag.take() {
                if drag.owner == key && drag.id == id {
                    let result = accept(state, key, id, &drag.proposal);
                    report(state, result);
                }
            }
        },
        Key::Named(k @ (NamedKey::ArrowLeft | NamedKey::ArrowRight))
            if !event.mods.ctrl && !event.mods.meta =>
        {
            state.composition.block_lens.drag = None;
            let selected = state
                .composition
                .block_lens
                .surface
                .borrow()
                .surface
                .as_ref()
                .and_then(|s| s.selected())
                .map(str::to_owned);
            if let Some(selected) = selected {
                let edit = if event.mods.alt {
                    SpanEdit::Start
                } else if event.mods.shift {
                    SpanEdit::End
                } else {
                    SpanEdit::Move
                };
                let result = input(state, key, id)
                    .and_then(|input| {
                        SpanEditIntent::propose(
                            &input,
                            &selected,
                            edit,
                            if k == NamedKey::ArrowLeft { -1.0 } else { 1.0 },
                            1,
                        )
                    })
                    .and_then(|intent| accept(state, key, id, &intent));
                report(state, result);
            }
        },
        _ => return,
    }
    event.prevent_default();
    event.stop_propagation();
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::TextCommand;
    use cambium_genet_winit_host::{Harness, Init, WindowCommands};
    use knot_document::{KnotDocumentIntentV1 as Intent, KnotDocumentSession};
    fn fixture() -> (DesktopState, DocKey, String) {
        let mut attrs = BTreeMap::from([
            ("id".into(), "verse".into()),
            ("class".into(), "metre".into()),
        ]);
        metre_block::set_line(&mut attrs, "quux light", "quux | light", "? /").unwrap();
        let source = nematic::knot::djot::divs::wrap(&attrs, "quux light\n");
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("memory:gesture", source),
            WindowCommands::new(),
        );
        let key = state.focused_key().unwrap();
        state.composition.block_lens.overview = Some((key, "verse".into()));
        let _ = view(&state, key);
        (state, key, "verse/line/0/syllable/0".into())
    }
    #[test]
    fn gesture_previews_cancel_and_commit_into_one_document_undo_step() {
        let (mut state, key, occurrence) = fixture();
        let original = state.surface_for(key).snapshot().text;
        let (point, dx) = {
            let p = state.composition.block_lens.surface.borrow();
            let s = p.surface.as_ref().unwrap();
            let r = s.occurrence_rect(&occurrence, 600, 320).unwrap();
            (
                (r.x + r.width / 2.0, r.y + r.height / 2.0),
                s.viewport_point(sceno::Vec2::new(95.0, 50.0), 600, 320)
                    .unwrap()
                    .x
                    - s.viewport_point(sceno::Vec2::new(48.0, 50.0), 600, 320)
                        .unwrap()
                        .x,
            )
        };
        pointer(
            &mut state,
            key,
            "verse",
            PointerEvent::new(PointerPhase::Down, point, (600.0, 320.0)),
        );
        pointer(
            &mut state,
            key,
            "verse",
            PointerEvent::new(PointerPhase::Move, (point.0 + dx, point.1), (600.0, 320.0)),
        );
        assert!(state.composition.block_lens.drag.is_some());
        assert_eq!(state.surface_for(key).snapshot().text, original);
        pointer(
            &mut state,
            key,
            "verse",
            PointerEvent::new(PointerPhase::Cancel, point, (600.0, 320.0)),
        );
        assert_eq!(state.surface_for(key).snapshot().text, original);
        let _ = view(&state, key);
        pointer(
            &mut state,
            key,
            "verse",
            PointerEvent::new(PointerPhase::Down, point, (600.0, 320.0)),
        );
        pointer(
            &mut state,
            key,
            "verse",
            PointerEvent::new(PointerPhase::Up, (point.0 + dx, point.1), (600.0, 320.0)),
        );
        assert!(state.message.is_none(), "{:?}", state.message);
        let changed = state.surface_for(key).snapshot().text;
        assert_ne!(changed, original);
        state
            .surface_mut_for(key)
            .apply(Intent::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(state.surface_for(key).snapshot().text, original);
        state
            .surface_mut_for(key)
            .apply(Intent::Edit(TextCommand::Redo))
            .unwrap();
        assert_eq!(state.surface_for(key).snapshot().text, changed);
    }
    #[test]
    fn resized_or_changed_source_gestures_cannot_overwrite_current_authorship() {
        for changed_source in [false, true] {
            let (mut state, key, occurrence) = fixture();
            let point = {
                let p = state.composition.block_lens.surface.borrow();
                let r = p
                    .surface
                    .as_ref()
                    .unwrap()
                    .occurrence_rect(&occurrence, 600, 320)
                    .unwrap();
                (r.x + r.width / 2.0, r.y + r.height / 2.0)
            };
            pointer(
                &mut state,
                key,
                "verse",
                PointerEvent::new(PointerPhase::Down, point, (600.0, 320.0)),
            );
            if changed_source {
                let snap = state.surface_for(key).snapshot();
                let start = snap.text.rfind("quux light").unwrap();
                state
                    .surface_mut_for(key)
                    .session_mut()
                    .replace_source_span(
                        &snap.source.address,
                        &snap.text,
                        start,
                        start + 10,
                        "quux changed".into(),
                    )
                    .unwrap();
            }
            let before = state.surface_for(key).snapshot().text;
            pointer(
                &mut state,
                key,
                "verse",
                PointerEvent::new(
                    PointerPhase::Up,
                    (point.0 + 100.0, point.1),
                    if changed_source {
                        (600.0, 320.0)
                    } else {
                        (601.0, 320.0)
                    },
                ),
            );
            assert_eq!(state.surface_for(key).snapshot().text, before);
            assert!(state.composition.block_lens.drag.is_none());
        }
    }
    #[test]
    fn production_document_surface_paints_the_same_shared_scene_with_fonts() {
        fn panel(s: &DesktopState) -> DesktopView {
            crate::workspace::document_tile(s, s.focused_key().unwrap())
        }
        let (state, _, _) = fixture();
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: panel as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: crate::fonts::bundled_fonts(),
                images: vec![],
            },
            crate::host_hooks(),
        );
        host.layout_at(420.0, 1600.0);
        host.prepare_frame();
        host.relayout();
        host.capture_portable()
            .expect("document lens exports its actual host paint resources");
        let stage = host.with_dom(|dom| {
            taproot::matching(dom, &taproot::Selector::class("knot-block-metre-overview"))[0]
        });
        let rect = host.painted_rect(stage).unwrap();
        assert!((rect.2 - 600.0).abs() < 1.0 && (rect.3 - 320.0).abs() < 1.0);
        let p = host.state().composition.block_lens.surface.borrow();
        assert_eq!(p.rendered_size, Some((600, 320)));
        assert!(p.error.is_none(), "{:?}", p.error);
    }
}
