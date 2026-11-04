// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium::{PointerEvent, PointerPhase};
use cambium_genet_winit_host::WindowCommands;
use knot_document::KnotDocumentSession;
fn fixture() -> (DesktopState, DocKey) {
    let text = "the night\nthe light";
    let mut state = DesktopState::new(
        KnotDocumentSession::scratch("scratch:metre", text),
        WindowCommands::new(),
    );
    let key = state.focused_key().unwrap();
    let snapshot = state.document().snapshot();
    state
        .document_mut()
        .session_mut()
        .select_source_span(&snapshot.source.address, text, 0, text.len())
        .unwrap();
    state.composition.cmudict_enabled = true;
    state.composition.layers.perfect_rhyme = true;
    state.composition.metre.paired_rhyme = true;
    super::super::analyze_sound(&mut state, key, false);
    start(&mut state, key);
    assert!(
        state.composition.metre.draft.is_some(),
        "{:?}",
        state.composition.metre.notice
    );
    (state, key)
}
#[test]
fn native_metre_body_and_edges_preview_cancel_commit_and_preserve_source() {
    let k = "bytes-0-9/syllable/1";
    for edit in [Edit::Move, Edit::Start, Edit::End] {
        let (mut state, key) = fixture();
        let before = state
            .composition
            .metre
            .draft
            .as_ref()
            .unwrap()
            .scene
            .clone();
        let source = state.document().snapshot().text;
        let (point, dx) = {
            let p = state.composition.metre.surfaces.borrow();
            let s = p.surface.as_ref().unwrap();
            let r = s.occurrence_rect(k, 800, 280).unwrap();
            let x = match edit {
                Edit::Move => r.x + r.width / 2.0,
                Edit::Start => r.x + 1.0,
                Edit::End => r.x + r.width - 1.0,
            };
            let dx = s
                .viewport_point(sceno::Vec2::new(95.0, 50.0), 800, 280)
                .unwrap()
                .x
                - s.viewport_point(sceno::Vec2::new(48.0, 50.0), 800, 280)
                    .unwrap()
                    .x;
            ((x, r.y + r.height / 2.0), dx)
        };
        let target = (
            point.0 + if edit == Edit::Start { -dx } else { dx },
            point.1,
        );
        let send = |s: &mut DesktopState, phase, p| {
            pointer(s, key, PointerEvent::new(phase, p, (800.0, 280.0)))
        };
        send(&mut state, PointerPhase::Down, point);
        send(&mut state, PointerPhase::Move, target);
        assert!(state.composition.metre.preview_active());
        assert_eq!(
            state.composition.metre.draft.as_ref().unwrap().scene,
            before
        );
        state.composition.metre.cancel();
        assert_eq!(
            state.composition.metre.surfaces.borrow().input.as_ref(),
            Some(&before)
        );
        send(&mut state, PointerPhase::Down, point);
        send(&mut state, PointerPhase::Move, target);
        send(&mut state, PointerPhase::Up, target);
        assert!(!state.composition.metre.preview_active());
        let m = &mut state.composition.metre;
        let after = m.draft.as_ref().unwrap().scene.clone();
        assert_ne!(after, before);
        m.mark_retained(&before);
        assert!(m.dirty());
        m.mark_retained(&after);
        assert!(!m.dirty());
        m.draft.as_mut().unwrap().undo();
        assert_eq!(m.draft.as_ref().unwrap().scene, before);
        m.draft.as_mut().unwrap().redo();
        assert_eq!(m.draft.as_ref().unwrap().scene, after);
        assert_eq!(state.document().snapshot().text, source);
        assert!(!state.document().snapshot().dirty);
    }
}
#[test]
fn native_metre_stale_source_selection_and_resized_gestures_are_atomic() {
    let (mut state, key) = fixture();
    let occurrence = "bytes-0-9/syllable/1";
    let before = state
        .composition
        .metre
        .draft
        .as_ref()
        .unwrap()
        .scene
        .clone();
    let point = {
        let p = state.composition.metre.surfaces.borrow();
        let r = p
            .surface
            .as_ref()
            .unwrap()
            .occurrence_rect(occurrence, 800, 280)
            .unwrap();
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    };
    pointer(
        &mut state,
        key,
        PointerEvent::new(PointerPhase::Down, point, (800.0, 280.0)),
    );
    pointer(
        &mut state,
        key,
        PointerEvent::new(
            PointerPhase::Move,
            (point.0 + 60.0, point.1),
            (801.0, 280.0),
        ),
    );
    assert!(!state.composition.metre.preview_active());
    assert_eq!(
        state.composition.metre.draft.as_ref().unwrap().scene,
        before
    );
    pointer(
        &mut state,
        key,
        PointerEvent::new(PointerPhase::Down, point, (800.0, 280.0)),
    );
    let snapshot = state.document().snapshot();
    state
        .document_mut()
        .session_mut()
        .select_source_span(&snapshot.source.address, &snapshot.text, 0, 3)
        .unwrap();
    pointer(
        &mut state,
        key,
        PointerEvent::new(PointerPhase::Up, (point.0 + 60.0, point.1), (800.0, 280.0)),
    );
    change(&mut state, key, Edit::Move, 1.0);
    assert!(!state.composition.metre.preview_active());
    assert_eq!(
        state.composition.metre.draft.as_ref().unwrap().scene,
        before
    );
    assert!(state.composition.metre.open_copy(before, None).is_err());
}
#[test]
fn changed_pronunciation_reading_cannot_receive_an_old_alignment_gesture() {
    let (mut state, key) = fixture();
    let before = state
        .composition
        .metre
        .draft
        .as_ref()
        .unwrap()
        .scene
        .clone();
    selection(
        &mut state,
        BackdropSceneEvent::Select("bytes-0-9/syllable/1".into()),
    );
    state.composition.sound.as_mut().unwrap().tokens[1].selected = None;
    change(&mut state, key, Edit::Move, 1.0);
    assert_eq!(
        state.composition.metre.draft.as_ref().unwrap().scene,
        before
    );
    assert!(!state.composition.metre.current(&state, key));
}
#[test]
fn production_panel_exposes_a_painted_surface_within_the_reading_width() {
    use cambium_genet_winit_host::{Harness, Init};
    fn panel(s: &DesktopState) -> DesktopView {
        super::super::view(s, s.focused_key().unwrap())
    }
    let (mut state, _) = fixture();
    state.composition.section = 1;
    let observed = Rc::new(std::cell::Cell::new(false));
    let observe = observed.clone();
    let mut hooks = crate::host_hooks();
    let mut frame = hooks.frame;
    hooks.frame = Box::new(move |ctx| {
        let result = frame(ctx);
        observe.set(ctx.producers.contains(KEY));
        result
    });
    let mut host = Harness::with_hooks(
        Init {
            state,
            logic: panel as fn(&DesktopState) -> DesktopView,
            sheet: crate::desktop_sheet(),
            fonts: crate::fonts::bundled_fonts(),
            images: vec![],
        },
        hooks,
    );
    host.layout_at(400.0, 1000.0);
    host.prepare_frame();
    host.relayout();
    let node = host.with_dom(|dom| {
        taproot::matching_classes(dom, &taproot::Selector::class("knot-metre-scene"))[0]
    });
    let rect = host.painted_rect(node).unwrap();
    assert!(rect.0 >= -1.0 && rect.0 < 100.0, "metre geometry {rect:?}");
    assert!(
        (rect.2 - 600.0).abs() < 1.0 && rect.3 > 280.0,
        "metre geometry {rect:?}"
    );
    assert!(observed.get(), "native metre producer is registered");
    let (point, delta) = {
        let p = host.state().composition.metre.surfaces.borrow();
        let s = p.surface.as_ref().unwrap();
        let r = s
            .occurrence_rect(
                "bytes-0-9/syllable/1",
                rect.2.round() as u32,
                rect.3.round() as u32,
            )
            .unwrap();
        let a = s
            .viewport_point(
                sceno::Vec2::new(48.0, 50.0),
                rect.2.round() as u32,
                rect.3.round() as u32,
            )
            .unwrap();
        let b = s
            .viewport_point(
                sceno::Vec2::new(95.0, 50.0),
                rect.2.round() as u32,
                rect.3.round() as u32,
            )
            .unwrap();
        (
            (rect.0 + r.x + r.width / 2.0, rect.1 + r.y + r.height / 2.0),
            b.x - a.x,
        )
    };
    assert!(
        point.0 >= 0.0 && point.0 < 400.0 && point.1 >= 0.0 && point.1 < 1000.0,
        "actual pointer target {point:?}"
    );
    host.press_at(point.0, point.1);
    host.after_dispatch();
    host.prepare_frame();
    host.relayout();
    host.move_to(point.0 + delta, point.1);
    host.after_dispatch();
    host.prepare_frame();
    host.relayout();
    assert!(
        host.state().composition.metre.preview_active(),
        "host-routed preview"
    );
    host.release_at(point.0 + delta, point.1);
    host.after_dispatch();
    host.prepare_frame();
    host.relayout();
    assert_eq!(
        host.state()
            .composition
            .metre
            .draft
            .as_ref()
            .unwrap()
            .span("bytes-0-9/syllable/1")
            .unwrap()
            .start,
        2
    );
    host.request_close(cambium_genet_winit_host::CloseRequest::Native);
    assert!(!host.close_requested());
    assert!(host.state().composition.metre.close_pending);
    host.update(|state| {
        let scene = state
            .composition
            .metre
            .draft
            .as_ref()
            .unwrap()
            .scene
            .clone();
        state.composition.metre.mark_retained(&scene);
    });
    host.request_close(cambium_genet_winit_host::CloseRequest::Native);
    assert!(
        host.close_requested(),
        "a confirmed retained draft permits normal close"
    );
}
#[test]
fn retained_copy_is_explicit_and_cannot_replace_an_unsaved_draft() {
    let (mut state, key) = fixture();
    let before = state
        .composition
        .metre
        .draft
        .as_ref()
        .unwrap()
        .scene
        .clone();
    assert!(
        state
            .composition
            .metre
            .open_copy(before.clone(), None)
            .is_err()
    );
    discard(&mut state);
    state
        .composition
        .metre
        .open_copy(before.clone(), None)
        .unwrap();
    selection(
        &mut state,
        BackdropSceneEvent::Select("bytes-0-9/syllable/1".into()),
    );
    change(&mut state, key, Edit::Move, 1.0);
    assert!(state.composition.metre.dirty());
    let mut wrong = before.clone();
    wrong.revision.push('x');
    state.composition.metre.mark_retained(&wrong);
    assert!(state.composition.metre.dirty());
}
