// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn path(name: &str) -> DocIdentity {
    DocIdentity::Path(PathBuf::from(name))
}

fn tabs(space: &DocumentWorkspace<&'static str>) -> Vec<String> {
    space
        .workspace()
        .tiled()
        .tiles()
        .into_iter()
        .map(|tile| tile.title.clone())
        .collect()
}

#[test]
fn two_files_and_a_scratch_coexist_in_one_stack() {
    let mut space = DocumentWorkspace::new();
    let (a, new_a) = space.open(path("a.djot"), "a.djot", "A");
    let (b, new_b) = space.open(path("b.djot"), "b.djot", "B");
    let (s, new_s) = space.open(DocIdentity::Scratch, "Untitled", "S");
    assert!(new_a && new_b && new_s);
    assert_eq!(space.len(), 3);
    assert_eq!(tabs(&space), ["a.djot", "b.djot", "Untitled"]);
    assert_eq!(space.focused(), Some(s));
    assert_eq!(space.doc(a), Some(&"A"));
    assert_eq!(space.doc(b), Some(&"B"));
    assert!(matches!(space.workspace().tiled(), TileTree::Stack(_)));
}

#[test]
fn the_navigator_opens_once_in_a_stack_left_of_the_documents() {
    let mut space = DocumentWorkspace::new();
    space.open(path("a.djot"), "a.djot", "A");
    assert_eq!(space.navigator(), None);
    let navigator = space.open_navigator("Navigator");
    assert_eq!(
        space.open_navigator("Navigator"),
        navigator,
        "one Navigator"
    );
    assert_eq!(space.navigator(), Some(navigator));
    assert_eq!(space.document_for(navigator), None);
    assert_eq!(
        tabs(&space),
        ["Navigator", "a.djot"],
        "left of the documents"
    );
    let TileTree::Split { children, .. } = space.workspace().tiled() else {
        panic!("the Navigator splits the frame");
    };
    let shares: Vec<f32> = children.iter().map(|branch| branch.fraction).collect();
    assert_eq!(shares, [NAVIGATOR_SHARE, 1.0 - NAVIGATOR_SHARE]);
    space.close(navigator);
    assert_eq!(space.navigator(), None);
    assert_eq!(tabs(&space), ["a.djot"]);
}

#[test]
fn a_duplicate_open_activates_the_existing_entry() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "first");
    space.open(path("b.djot"), "b.djot", "B");
    let (again, opened) = space.open(path("a.djot"), "a.djot", "second");
    assert_eq!(again, a);
    assert!(!opened);
    assert_eq!(space.doc(a), Some(&"first"), "the open session is kept");
    assert_eq!(space.len(), 2);
    assert_eq!(space.focused(), Some(a));
    let active = match space.workspace().tiled() {
        TileTree::Stack(stack) => stack.tabs[stack.active].id,
        _ => panic!("one stack"),
    };
    assert_eq!(Some(active), space.tile_of(a));
}

#[test]
fn scratch_documents_never_collide() {
    let mut space = DocumentWorkspace::new();
    let (one, _) = space.open(DocIdentity::Scratch, "Untitled", "1");
    let (two, opened) = space.open(DocIdentity::Scratch, "Untitled", "2");
    assert!(opened);
    assert_ne!(one, two);
}

#[test]
fn a_catalog_identity_outlives_a_rename() {
    let mut space = DocumentWorkspace::new();
    let (key, _) = space.open(
        DocIdentity::Catalog("knot:document:1".into()),
        "a.djot",
        "A",
    );
    space.set_identity(key, DocIdentity::Catalog("knot:document:1".into()));
    space.set_title(key, "b.djot");
    let (again, opened) = space.open(
        DocIdentity::Catalog("knot:document:1".into()),
        "b.djot",
        "dup",
    );
    assert_eq!((again, opened), (key, false));
    assert_eq!(tabs(&space), ["b.djot"]);
}

#[test]
fn closing_a_tile_drops_its_entry_and_moves_the_focus() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    let (b, _) = space.open(path("b.djot"), "b.djot", "B");
    let tile = space.tile_of(b).unwrap();
    assert_eq!(space.close(tile), Some((b, "B")));
    assert_eq!(space.doc(b), None);
    assert_eq!(space.role(tile), None);
    assert_eq!(space.focused(), Some(a));
    assert_eq!(tabs(&space), ["a.djot"]);
    // The key is never reused.
    let (c, _) = space.open(path("c.djot"), "c.djot", "C");
    assert_ne!(c, b);
}

#[test]
fn closing_the_last_document_leaves_an_empty_frame() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    space.close(space.tile_of(a).unwrap());
    assert!(space.is_empty());
    assert_eq!(space.focused(), None);
    assert!(tabs(&space).is_empty());
    let (b, _) = space.open(path("b.djot"), "b.djot", "B");
    assert_eq!(space.focused(), Some(b));
    assert_eq!(tabs(&space), ["b.djot"]);
}

#[test]
fn graph_is_singleton_in_the_document_stack() {
    let mut space = DocumentWorkspace::new();
    let (document, _) = space.open(path("a.djot"), "a.djot", "A");
    let document_tile = space.tile_of(document).unwrap();
    let graph = space.open_graph("Graph");
    assert_eq!(graph, GRAPH_TILE_ID);
    assert_eq!(space.open_graph("Graph again"), graph);
    assert_eq!(space.graph(), Some(graph));
    assert_eq!(space.role(graph), Some(&TileRole::Graph));
    assert_eq!(tabs(&space), ["a.djot", "Graph"]);
    assert_eq!(space.focused(), Some(document));
    space.close(graph);
    assert_eq!(space.graph(), None);
    let (second, _) = space.open(path("b.djot"), "b.djot", "B");
    assert_eq!(space.tile_of(second), Some(TileId(document_tile.0 + 1)));
    assert_eq!(space.open_graph("Graph reopened"), GRAPH_TILE_ID);
}

#[test]
fn activating_a_document_tile_focuses_it() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    space.open(path("b.djot"), "b.djot", "B");
    space.activate(space.tile_of(a).unwrap());
    assert_eq!(space.focused(), Some(a));
}

fn stacks(space: &DocumentWorkspace<&'static str>) -> Vec<Vec<TileId>> {
    fn collect(tree: &TileTree, stacks: &mut Vec<Vec<TileId>>) {
        match tree {
            TileTree::Stack(stack) => stacks.push(stack.tabs.iter().map(|tile| tile.id).collect()),
            TileTree::Split { children, .. } => {
                for child in children {
                    collect(&child.tree, stacks);
                }
            },
        }
    }
    let mut stacks = Vec::new();
    collect(space.workspace().tiled(), &mut stacks);
    stacks
}

#[test]
fn metadata_keeps_the_centre_stack_for_new_after_graph_closes() {
    let mut space = DocumentWorkspace::new();
    let (source, _) = space.open(path("page.djot"), "page.djot", "source");
    let navigator = space.open_navigator("Navigator");
    let reading = space.open_reading(ReadingKind::Preview, None, "Preview");
    let metadata = space.open_metadata(SiteKey(1), "page.djot", "Metadata");

    let graph = space
        .prepare_last_document_close(source, "Graph")
        .expect("last source is replaced by Graph");
    space.close(space.tile_of(source).unwrap());
    space.close(graph);
    assert_eq!(
        stacks(&space),
        [vec![navigator], vec![metadata], vec![reading]]
    );

    let (new, inserted) = space.open(DocIdentity::Scratch, "Untitled", "new");
    assert!(inserted);
    assert_eq!(
        stacks(&space),
        [
            vec![navigator],
            vec![metadata, space.tile_of(new).unwrap()],
            vec![reading]
        ]
    );
}

#[test]
fn readings_share_a_stack_right_of_the_documents() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    let outline = space.open_reading(ReadingKind::Outline, None, "Outline");
    let preview = space.open_reading(ReadingKind::Preview, None, "Preview");
    assert_eq!(
        stacks(&space),
        [vec![space.tile_of(a).unwrap()], vec![outline, preview]]
    );
    assert_eq!(space.document_for(outline), Some(a));
    assert_eq!(
        space.focused(),
        Some(a),
        "a reading takes no document focus"
    );
}

#[test]
fn a_pinned_reading_keeps_its_document_as_the_focus_moves() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    let following = space.open_reading(ReadingKind::Preview, None, "Preview");
    let pinned = space.open_reading(ReadingKind::Preview, Some(a), "Preview");
    let (b, _) = space.open(path("b.djot"), "b.djot", "B");
    assert_eq!(space.document_for(following), Some(b));
    assert_eq!(space.document_for(pinned), Some(a));
    assert_eq!(
        space.following_reading(ReadingKind::Preview),
        Some(following)
    );
    space.set_pinned(pinned, None);
    assert_eq!(space.document_for(pinned), Some(b), "unpinned, it follows");
}

#[test]
fn closing_a_document_closes_the_readings_pinned_to_it() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    space.open(path("b.djot"), "b.djot", "B");
    let pinned = space.open_reading(ReadingKind::Outline, Some(a), "Outline");
    let following = space.open_reading(ReadingKind::Preview, None, "Preview");
    space.close(space.tile_of(a).unwrap());
    assert_eq!(space.role(pinned), None);
    assert!(space.role(following).is_some());
}

#[test]
fn the_last_reading_closes_its_stack_and_the_next_opens_another() {
    let mut space = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    let outline = space.open_reading(ReadingKind::Outline, None, "Outline");
    space.close(outline);
    assert_eq!(stacks(&space), [vec![space.tile_of(a).unwrap()]]);
    let again = space.open_reading(ReadingKind::Outline, None, "Outline");
    assert_eq!(
        stacks(&space),
        [vec![space.tile_of(a).unwrap()], vec![again]]
    );
}

#[test]
fn drags_are_declined_until_adopted() {
    let mut space: DocumentWorkspace<&str> = DocumentWorkspace::new();
    let (a, _) = space.open(path("a.djot"), "a.djot", "A");
    let tile = space.tile_of(a).unwrap();
    let drag = WorkspaceEvent::Tile(TileEvent::Dragged {
        tile,
        to: workbench::DropTarget::Outside,
    });
    assert!(!space.apply_layout(&drag));
    assert_eq!(tabs(&space), ["a.djot"]);
}
