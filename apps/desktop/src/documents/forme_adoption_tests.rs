// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::preferences::PreferencesStore;
use cambium::{CaretAffinity, CaretPosition, CaretSelection, TextCommand};
use knot_document::{KnotDocumentIntentV1, KnotDocumentSession};

fn workspace() -> (DocumentWorkspace<String>, DocKey) {
    let mut docs = DocumentWorkspace::new();
    let (key, _) = docs.open(
        DocIdentity::Path("/writing/a.djot".into()),
        "a.djot",
        "original".to_owned(),
    );
    docs.open_reading(ReadingKind::Outline, Some(key), "Outline");
    (docs, key)
}
fn resize() -> WorkspaceEvent {
    WorkspaceEvent::Tile(TileEvent::DividerMoved {
        split: workbench::TilePath(vec![]),
        fractions: vec![0.65, 0.35],
    })
}

fn active_in_stack(tree: &TileTree, target: TileId) -> Option<TileId> {
    match tree {
        TileTree::Stack(stack) if stack.tabs.iter().any(|tile| tile.id == target) => {
            stack.tabs.get(stack.active).map(|tile| tile.id)
        },
        TileTree::Stack(_) => None,
        TileTree::Split { children, .. } => children
            .iter()
            .find_map(|branch| active_in_stack(&branch.tree, target)),
    }
}

#[test]
fn forme_restoration_keeps_the_current_focused_document_visible() {
    let (mut docs, first) = workspace();
    let (second, _) = docs.open(
        DocIdentity::Path("/writing/b.djot".into()),
        "b.djot",
        "second".into(),
    );
    let first_tile = docs.tile_of(first).unwrap();
    let second_tile = docs.tile_of(second).unwrap();
    docs.focus(first);
    docs.unlock_forme(None).unwrap();
    assert!(docs.apply_layout(&resize()));
    let mut saved = None;
    docs.apply_forme(|_, value| {
        saved = Some(value.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        active_in_stack(docs.workspace().tiled(), second_tile),
        Some(first_tile)
    );

    docs.focus(second);
    docs.unlock_forme(saved).unwrap();
    assert_eq!(docs.focused(), Some(second));
    assert_eq!(
        active_in_stack(docs.workspace().tiled(), second_tile),
        Some(second_tile),
        "restored geometry cannot select a different command target"
    );
    assert_eq!(
        docs.workspace()
            .tiled()
            .fractions_at(&workbench::TilePath(vec![])),
        Some(vec![0.65, 0.35])
    );
    docs.apply_forme(|_, _| Ok(())).unwrap();
    assert_eq!(docs.focused(), Some(second));
    assert_eq!(
        active_in_stack(docs.workspace().tiled(), second_tile),
        Some(second_tile)
    );
    assert_eq!(docs.doc(first).unwrap(), "original");
    assert_eq!(docs.doc(second).unwrap(), "second");
}

#[test]
fn forme_discard_preserves_real_source_selection_and_document_undo() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("writer.djot");
    let original = "# α\n\nBody.\n";
    std::fs::write(&path, original).unwrap();
    let identity = DocIdentity::Path(path.clone());
    let mut docs = DocumentWorkspace::new();
    let (key, _) = docs.open(
        identity.clone(),
        "writer.djot",
        KnotDocumentSession::open(&path).unwrap(),
    );
    docs.open_reading(ReadingKind::Outline, Some(key), "Outline");
    let layout = docs.workspace().tiled().clone();
    docs.unlock_forme(None).unwrap();
    assert!(docs.apply_layout(&resize()));
    let selection = CaretSelection {
        anchor: CaretPosition {
            byte: 4,
            affinity: CaretAffinity::Upstream,
        },
        focus: CaretPosition {
            byte: 2,
            affinity: CaretAffinity::Downstream,
        },
    };
    let session = docs.doc_mut(key).unwrap();
    session
        .apply(KnotDocumentIntentV1::Edit(TextCommand::SetSelection(
            selection,
        )))
        .unwrap();
    session
        .apply(KnotDocumentIntentV1::Edit(TextCommand::Insert("ββ".into())))
        .unwrap();
    session
        .apply(KnotDocumentIntentV1::Edit(TextCommand::SelectAll))
        .unwrap();
    let edited = session.snapshot();
    assert_eq!(edited.text, "# ββ\n\nBody.\n");
    assert!(edited.dirty);

    docs.discard_forme().unwrap();
    assert_eq!(docs.workspace().tiled(), &layout);
    assert_eq!(docs.focused(), Some(key));
    assert_eq!(docs.identity(key), Some(&identity));
    assert_eq!(docs.doc(key).unwrap().snapshot(), edited);
    let undone = docs
        .doc_mut(key)
        .unwrap()
        .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
        .unwrap();
    assert_eq!(undone.text, original);
    assert_eq!(undone.selection, selection);
    assert!(!undone.dirty);
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn forme_discard_restores_layout_without_undoing_document_edits_or_focus() {
    let (mut docs, key) = workspace();
    let before = docs.workspace().tiled().clone();
    docs.unlock_forme(None).unwrap();
    assert!(docs.apply_layout(&resize()));
    assert_ne!(docs.workspace().tiled(), &before);
    assert_eq!(
        docs.workspace.tiled(),
        &before,
        "draft never replaces live layout"
    );
    *docs.doc_mut(key).unwrap() = "writer changed this while arranging".into();
    docs.focus(key);
    docs.discard_forme().unwrap();
    assert_eq!(docs.workspace().tiled(), &before);
    assert_eq!(
        docs.doc(key).unwrap(),
        "writer changed this while arranging"
    );
    assert_eq!(docs.focused(), Some(key));
}

#[test]
fn forme_failed_apply_retains_draft_then_saved_layout_reopens_for_same_bindings() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("preferences.json");
    let mut store = PreferencesStore::open(path.clone());
    let (mut docs, _) = workspace();
    docs.unlock_forme(store.workspace_forme()).unwrap();
    docs.apply_layout(&resize());
    let preview = docs.workspace().tiled().clone();
    assert!(
        docs.apply_forme(|_, _| Err("disk unavailable".into()))
            .is_err()
    );
    assert!(docs.forme_editing());
    assert_eq!(docs.workspace().tiled(), &preview);
    docs.apply_forme(|base, next| store.save_workspace_forme(base, next))
        .unwrap();
    assert!(!docs.forme_editing());
    assert_eq!(docs.workspace().tiled(), &preview);
    let reopened = PreferencesStore::open(path);
    let mut fresh = DocumentWorkspace::new();
    let (temporary, _) = fresh.open(DocIdentity::Scratch, "Temporary", String::new());
    fresh.close(fresh.tile_of(temporary).unwrap());
    let (key, _) = fresh.open(
        DocIdentity::Path("/writing/a.djot".into()),
        "a.djot",
        "original".to_owned(),
    );
    fresh.open_reading(ReadingKind::Outline, Some(key), "Outline");
    fresh.unlock_forme(reopened.workspace_forme()).unwrap();
    assert_eq!(
        fresh
            .workspace()
            .tiled()
            .fractions_at(&workbench::TilePath(vec![])),
        preview.fractions_at(&workbench::TilePath(vec![]))
    );
    assert_eq!(fresh.doc(key).unwrap(), "original");
}

#[test]
fn forme_changed_open_set_refuses_apply_and_discard_keeps_new_document() {
    let (mut docs, _) = workspace();
    docs.unlock_forme(None).unwrap();
    docs.apply_layout(&resize());
    let (new, _) = docs.open(DocIdentity::Scratch, "New", "new contents".into());
    assert!(
        docs.apply_forme(|_, _| panic!("stale draft must never reach storage"))
            .is_err()
    );
    docs.discard_forme().unwrap();
    assert_eq!(docs.doc(new).unwrap(), "new contents");
    assert!(
        docs.workspace()
            .tiled()
            .find(docs.tile_of(new).unwrap())
            .is_some()
    );
}

#[test]
fn forme_external_slot_change_refuses_save_without_losing_draft() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("preferences.json");
    let mut store = PreferencesStore::open(path.clone());
    let (mut docs, _) = workspace();
    docs.unlock_forme(store.workspace_forme()).unwrap();
    docs.apply_layout(&resize());
    let mut external = crate::preferences::DesktopPreferences::default();
    external.other.insert(
        "workspace_forme".into(),
        serde_json::json!({"another": "owner"}),
    );
    external.save(&path).unwrap();
    assert!(
        docs.apply_forme(|base, next| store.save_workspace_forme(base, next))
            .is_err()
    );
    assert!(docs.forme_editing());
    assert_eq!(
        crate::preferences::DesktopPreferences::load(&path).unwrap(),
        external
    );
    docs.discard_forme().unwrap();
}

#[test]
fn forme_does_not_restore_a_layout_for_reused_tile_numbers() {
    let (mut docs, _) = workspace();
    docs.unlock_forme(None).unwrap();
    docs.apply_layout(&resize());
    let mut saved = None;
    docs.apply_forme(|_, value| {
        saved = Some(value.clone());
        Ok(())
    })
    .unwrap();
    let mut other = DocumentWorkspace::new();
    let (key, _) = other.open(
        DocIdentity::Path("/writing/b.djot".into()),
        "b.djot",
        String::new(),
    );
    other.open_reading(ReadingKind::Outline, Some(key), "Outline");
    let original = other.workspace().tiled().clone();
    other.unlock_forme(saved).unwrap();
    assert_eq!(other.workspace().tiled(), &original);
}

#[test]
fn forme_does_not_restore_a_layout_for_a_fresh_scratch_instance() {
    let mut docs = DocumentWorkspace::new();
    let (key, _) = docs.open(DocIdentity::Scratch, "Untitled", "first draft".to_owned());
    docs.open_reading(ReadingKind::Outline, Some(key), "Outline");
    docs.unlock_forme(None).unwrap();
    assert!(docs.apply_layout(&resize()));
    let mut saved = None;
    docs.apply_forme(|_, value| {
        saved = Some(value.clone());
        Ok(())
    })
    .unwrap();

    let mut fresh = DocumentWorkspace::new();
    let (other, _) = fresh.open(
        DocIdentity::Scratch,
        "Untitled",
        "different draft".to_owned(),
    );
    fresh.open_reading(ReadingKind::Outline, Some(other), "Outline");
    assert_eq!(fresh.tile_of(other), docs.tile_of(key));
    let original = fresh.workspace().tiled().clone();
    fresh.unlock_forme(saved).unwrap();
    assert_eq!(fresh.workspace().tiled(), &original);
    assert_eq!(fresh.doc(other).unwrap(), "different draft");
    assert_eq!(fresh.focused(), Some(other));
}
