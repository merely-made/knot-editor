// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium_genet_winit_host::WindowCommands;
use knot_document::KnotDocumentSession;

fn fixture(source: &str) -> (tempfile::TempDir, DesktopState, DocKey, DocKey) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("source.djot");
    let b = dir.path().join("target.djot");
    std::fs::write(&a, source).unwrap();
    std::fs::write(&b, "# Target\n").unwrap();
    let mut state = DesktopState::new(
        KnotDocumentSession::open(&a).unwrap(),
        WindowCommands::new(),
    );
    let source = state.focused_key().unwrap();
    state.open_path(b);
    let target = state.focused_key().unwrap();
    state.focus_document(source);
    (dir, state, source, target)
}

fn select(state: &mut DesktopState, key: DocKey, start: usize, end: usize) {
    let session = state.surface_for(key).session();
    let address = session.snapshot().source.address;
    let text = session.input().text().to_owned();
    state
        .surface_mut_for(key)
        .session_mut()
        .select_source_span(&address, &text, start, end)
        .unwrap();
}

fn arm(state: &mut DesktopState) {
    begin(state);
    state.link_form.as_mut().unwrap().selected = Some(0);
}

#[test]
fn unicode_selection_link_is_one_undo_step_and_only_save_persists() {
    let (_dir, mut state, source, target) = fixture("α 日本語 tail\n");
    let path = state
        .surface_for(source)
        .session()
        .source_path()
        .unwrap()
        .to_owned();
    select(&mut state, source, "α ".len(), "α 日本語".len());
    arm(&mut state);
    state.focus_document(target);
    insert(&mut state);
    assert!(state.link_form.is_none());
    assert_eq!(state.focused_key(), Some(source));
    let inserted = state
        .surface_for(source)
        .session()
        .input()
        .text()
        .to_owned();
    assert_eq!(inserted, "α [日本語](./target.djot) tail\n");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "α 日本語 tail\n");
    state
        .surface_mut_for(source)
        .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
        .unwrap();
    assert_eq!(
        state.surface_for(source).session().input().text(),
        "α 日本語 tail\n"
    );
    state
        .surface_mut_for(source)
        .apply(KnotDocumentIntentV1::Edit(TextCommand::Redo))
        .unwrap();
    state
        .surface_mut_for(source)
        .apply(KnotDocumentIntentV1::Save)
        .unwrap();
    let reopened = KnotDocumentSession::open(path).unwrap();
    assert_eq!(reopened.input().text(), inserted);
    assert_eq!(
        document_links::parsed_links(&reopened).unwrap()[0].target,
        "./target.djot"
    );
}

#[test]
fn forged_snapshot_with_current_stamp_cannot_redirect_selected_text() {
    let (_dir, mut state, source, _) = fixture("source");
    select(&mut state, source, 0, 6);
    arm(&mut state);
    let stamp = state.surface_for(source).session().source_stamp();
    state.link_form.as_mut().unwrap().base =
        knot_document::SourceSnapshot::capture(stamp.clone(), "forged");
    insert(&mut state);
    assert_eq!(state.surface_for(source).session().input().text(), "source");
    assert_eq!(state.surface_for(source).session().source_stamp(), stamp);
    assert!(
        state
            .link_form
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("source changed")
    );
}

#[test]
fn edit_then_undo_does_not_revive_link_form() {
    let (_dir, mut state, source, _) = fixture("source");
    arm(&mut state);
    for command in [TextCommand::Insert("later".into()), TextCommand::Undo] {
        state
            .surface_mut_for(source)
            .apply(KnotDocumentIntentV1::Edit(command))
            .unwrap();
    }
    assert_eq!(state.surface_for(source).session().input().text(), "source");
    insert(&mut state);
    assert!(
        state
            .link_form
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("source changed")
    );
    assert_eq!(state.surface_for(source).session().input().text(), "source");
}

#[test]
fn dirty_source_allowed_but_changed_source_refuses_stale_form() {
    let (_dir, mut state, source, _) = fixture("draft");
    state
        .surface_mut_for(source)
        .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
            "dirty ".into(),
        )))
        .unwrap();
    assert_eq!(unavailable(&state), None);
    arm(&mut state);
    state
        .surface_mut_for(source)
        .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
            "later".into(),
        )))
        .unwrap();
    let before = state
        .surface_for(source)
        .session()
        .input()
        .text()
        .to_owned();
    insert(&mut state);
    assert_eq!(state.surface_for(source).session().input().text(), before);
    assert!(
        state
            .link_form
            .as_ref()
            .unwrap()
            .error
            .as_deref()
            .unwrap()
            .contains("source changed")
    );
}

#[test]
fn moved_selection_or_pending_composition_refuses_stale_form() {
    for composition in [false, true] {
        let (_dir, mut state, source, _) = fixture("source");
        select(&mut state, source, 0, 2);
        arm(&mut state);
        if composition {
            state
                .surface_mut_for(source)
                .apply(KnotDocumentIntentV1::Edit(TextCommand::SetComposition {
                    text: "未".into(),
                    selection: Some((0, 3)),
                }))
                .unwrap();
        } else {
            select(&mut state, source, 2, 4);
        }
        insert(&mut state);
        assert_eq!(state.surface_for(source).session().input().text(), "source");
        assert!(state.link_form.as_ref().unwrap().error.is_some());
    }
}

#[test]
fn target_close_save_as_and_missing_file_refuse_without_edit() {
    for operation in 0..3 {
        let (dir, mut state, source, target) = fixture("source");
        arm(&mut state);
        match operation {
            0 => {
                let tile = state.docs.tile_of(target).unwrap();
                state.docs.close(tile);
            },
            1 => {
                state
                    .surface_mut_for(target)
                    .apply(KnotDocumentIntentV1::SaveAs(dir.path().join("moved.djot")))
                    .unwrap();
            },
            _ => {
                std::fs::remove_file(dir.path().join("target.djot")).unwrap();
            },
        }
        insert(&mut state);
        assert_eq!(state.surface_for(source).session().input().text(), "source");
        assert!(state.link_form.as_ref().unwrap().error.is_some());
    }
}

#[test]
fn source_save_as_or_closed_source_refuses_without_redirecting() {
    for close in [false, true] {
        let (dir, mut state, source, target) = fixture("source");
        arm(&mut state);
        if close {
            let tile = state.docs.tile_of(source).unwrap();
            state.docs.close(tile);
        } else {
            state
                .surface_mut_for(source)
                .apply(KnotDocumentIntentV1::SaveAs(
                    dir.path().join("elsewhere.djot"),
                ))
                .unwrap();
        }
        state.focus_document(target);
        insert(&mut state);
        assert_eq!(
            state.surface_for(target).session().input().text(),
            "# Target\n"
        );
        assert!(state.link_form.as_ref().unwrap().error.is_some());
    }
}

#[test]
fn unsaved_and_read_only_sources_refuse_read_only_target_is_allowed() {
    let mut scratch = DesktopState::new(
        KnotDocumentSession::scratch("scratch:test", "draft"),
        WindowCommands::new(),
    );
    begin(&mut scratch);
    assert!(scratch.link_form.is_none());
    assert!(unavailable(&scratch).unwrap().contains("Save As"));
    let (dir, mut state, source, target) = fixture("source");
    state.docs.doc_mut(target).unwrap().document = knot_document::KnotDocumentSurfaceState::new(
        KnotDocumentSession::open_read_only(dir.path().join("target.djot")).unwrap(),
    );
    arm(&mut state);
    insert(&mut state);
    assert!(
        state
            .surface_for(source)
            .session()
            .input()
            .text()
            .contains("[target\\.djot](./target.djot)")
    );
    let mut readonly = DesktopState::new(
        KnotDocumentSession::open_read_only(dir.path().join("source.djot")).unwrap(),
        WindowCommands::new(),
    );
    begin(&mut readonly);
    assert!(readonly.link_form.is_none());
    assert!(unavailable(&readonly).unwrap().contains("read-only"));
}

#[test]
fn current_file_permissions_are_checked_again_before_insert() {
    let (dir, mut state, source, _) = fixture("source");
    arm(&mut state);
    let path = dir.path().join("source.djot");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(path, permissions).unwrap();
    insert(&mut state);
    assert_eq!(state.surface_for(source).session().input().text(), "source");
    assert!(
        state
            .link_form
            .as_ref()
            .unwrap()
            .error
            .as_deref()
            .unwrap()
            .contains("read-only")
    );
}

#[test]
fn follows_exact_owner_source_and_reuses_open_target_key() {
    let (_dir, mut state, source, target) = fixture("[Target](target.djot)\n");
    let session = state.surface_for(source).session();
    let address = session.snapshot().source.address;
    let text = session.input().text().to_owned();
    follow_link(&mut state, source, &address, &text, "target.djot");
    assert_eq!(state.focused_key(), Some(target));
    assert_eq!(state.docs.len(), 2);
    follow_link(&mut state, source, &address, "stale", "target.djot");
    assert!(state.message.as_deref().unwrap().contains("stale"));
    follow_link(&mut state, source, &address, &text, "https://example.org/");
    assert!(state.message.as_deref().unwrap().contains("no network"));
}

#[test]
fn named_relationship_round_trips_and_link_span_is_source_bound() {
    let (_dir, mut state, source, _) = fixture("日本語");
    select(&mut state, source, 0, "日本語".len());
    arm(&mut state);
    let relationship = &document_links::RELATIONSHIPS[0];
    state.link_form.as_mut().unwrap().relationship = Some(relationship.slug);
    insert(&mut state);
    let session = state.surface_for(source).session();
    let links = document_links::parsed_links(session).unwrap();
    assert_eq!(links[0].rel.as_deref(), Some(relationship.iri));
    let address = session.snapshot().source.address;
    let text = session.input().text().to_owned();
    select_link(&mut state, source, &address, &text, links[0].span);
    let span = links[0].span.unwrap();
    assert_eq!(
        state.surface_for(source).snapshot().selection.focus.byte,
        span.1
    );
    state
        .surface_mut_for(source)
        .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert(
            "changed".into(),
        )))
        .unwrap();
    let before = state.surface_for(source).snapshot().selection;
    select_link(&mut state, source, &address, &text, Some(span));
    assert_eq!(state.surface_for(source).snapshot().selection, before);
    assert!(state.message.as_deref().unwrap().contains("stale"));
}
