// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The desktop's open documents and the tiles that show them.
//!
//! An entry is keyed by a runtime [`DocKey`] that no Save As changes, and a
//! tile names what it shows by its [`TileRole`]. The Workbench tree is only
//! presentation: it holds tiles, never documents, so closing a document's
//! tile and dropping its entry happen together here, never through the tree
//! alone. A duplicate open resolves by [`DocIdentity`] (the catalog id when
//! the file is bound, else its canonical path) and activates the open entry
//! instead of making a second writable session. A scratch document has no
//! identity to collide on.
//!
//! The payload `D` is whatever the application keeps per document; the
//! bookkeeping here does not look inside it, which is what lets it be tested
//! alone.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use workbench::{ContentSource, Tile, TileEvent, TileId, TileTree, Workspace, WorkspaceEvent};

/// The runtime key of one open document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocKey(pub u64);

/// What makes two opens the same document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocIdentity {
    Catalog(String),
    Path(PathBuf),
    Scratch,
}

/// Which reading a reading tile shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadingKind {
    Outline,
    Preview,
    Folded,
    Readings,
    Changes,
}

/// What a tile shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TileRole {
    Document(DocKey),
    /// A reading follows the focused document unless pinned to one.
    Reading {
        kind: ReadingKind,
        pinned: Option<DocKey>,
    },
    Navigator,
}

/// The Workbench lane every Knot tile rides in.
pub const TILE_KIND: &str = "knot";

struct Entry<D> {
    identity: DocIdentity,
    tile: TileId,
    doc: D,
}

/// Open documents, their tiles, and which document has the focus.
pub struct DocumentWorkspace<D> {
    entries: BTreeMap<DocKey, Entry<D>>,
    roles: HashMap<TileId, TileRole>,
    workspace: Workspace,
    focused: Option<DocKey>,
    next_doc: u64,
    next_tile: u64,
}

impl<D> Default for DocumentWorkspace<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D> DocumentWorkspace<D> {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            roles: HashMap::new(),
            workspace: Workspace::new(TileTree::stack(Vec::new(), 0)),
            focused: None,
            next_doc: 1,
            next_tile: 1,
        }
    }

    /// The tree a frame renders.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Open a document, or activate the open entry with the same identity.
    /// Returns its key and whether a new entry was made; when an existing
    /// entry is reused, `doc` is dropped.
    pub fn open(
        &mut self,
        identity: DocIdentity,
        title: impl Into<String>,
        doc: D,
    ) -> (DocKey, bool) {
        if let Some(key) = self.find(&identity) {
            self.focus(key);
            return (key, false);
        }
        let key = DocKey(self.next_doc);
        self.next_doc += 1;
        let tile = self.mint_tile(title.into(), TileRole::Document(key));
        let id = tile.id;
        self.place_document_tile(tile);
        self.entries.insert(
            key,
            Entry {
                identity,
                tile: id,
                doc,
            },
        );
        self.focus(key);
        (key, true)
    }

    /// The open entry with `identity`, if any. Scratch never matches.
    pub fn find(&self, identity: &DocIdentity) -> Option<DocKey> {
        if *identity == DocIdentity::Scratch {
            return None;
        }
        self.entries
            .iter()
            .find(|(_, entry)| entry.identity == *identity)
            .map(|(key, _)| *key)
    }

    /// Make `key` the focused document and its tile the active tab.
    pub fn focus(&mut self, key: DocKey) {
        if let Some(entry) = self.entries.get(&key) {
            self.workspace
                .apply(&WorkspaceEvent::Tile(TileEvent::Activated(entry.tile)));
            self.focused = Some(key);
        }
    }

    /// The focused document.
    pub fn focused(&self) -> Option<DocKey> {
        self.focused
    }

    pub fn doc(&self, key: DocKey) -> Option<&D> {
        self.entries.get(&key).map(|entry| &entry.doc)
    }

    pub fn doc_mut(&mut self, key: DocKey) -> Option<&mut D> {
        self.entries.get_mut(&key).map(|entry| &mut entry.doc)
    }

    /// Every open document, in the order they were opened.
    pub fn docs(&self) -> impl Iterator<Item = (DocKey, &D)> {
        self.entries.iter().map(|(key, entry)| (*key, &entry.doc))
    }

    pub fn docs_mut(&mut self) -> impl Iterator<Item = (DocKey, &mut D)> {
        self.entries
            .iter_mut()
            .map(|(key, entry)| (*key, &mut entry.doc))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn identity(&self, key: DocKey) -> Option<&DocIdentity> {
        self.entries.get(&key).map(|entry| &entry.identity)
    }

    /// Record a new identity after Save As or a catalog binding.
    pub fn set_identity(&mut self, key: DocKey, identity: DocIdentity) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.identity = identity;
        }
    }

    /// Rename a document's tab.
    pub fn set_title(&mut self, key: DocKey, title: impl Into<String>) {
        if let Some(tile) = self.tile_of(key)
            && let Some(tile) = self.workspace.tiled_mut().tile_mut(tile)
        {
            tile.title = title.into();
        }
    }

    pub fn tile_of(&self, key: DocKey) -> Option<TileId> {
        self.entries.get(&key).map(|entry| entry.tile)
    }

    pub fn role(&self, tile: TileId) -> Option<&TileRole> {
        self.roles.get(&tile)
    }

    /// The document a tile shows: its own for a document tile, the pinned or
    /// the focused one for a reading.
    pub fn document_for(&self, tile: TileId) -> Option<DocKey> {
        match self.roles.get(&tile)? {
            TileRole::Document(key) => Some(*key),
            TileRole::Reading { pinned, .. } => pinned.or(self.focused),
            TileRole::Navigator => None,
        }
    }

    /// Activate a tile, focusing its document when it has one.
    pub fn activate(&mut self, tile: TileId) {
        self.workspace
            .apply(&WorkspaceEvent::Tile(TileEvent::Activated(tile)));
        if let Some(TileRole::Document(key)) = self.roles.get(&tile) {
            self.focused = Some(*key);
        }
    }

    /// Apply a divider move. Drags, floats and tearouts are not adopted yet
    /// and return `false`, leaving the tree unchanged.
    pub fn apply_layout(&mut self, event: &WorkspaceEvent) -> bool {
        match event {
            WorkspaceEvent::Tile(TileEvent::DividerMoved { .. }) => {
                self.workspace.apply(event).changed()
            },
            _ => false,
        }
    }

    /// Remove a tile, and with a document tile its entry, returning the
    /// document it showed. The focus moves to the document now active in the
    /// same stack, else to any open document. The caller preflights a dirty
    /// document first; this never asks.
    pub fn close(&mut self, tile: TileId) -> Option<(DocKey, D)> {
        let role = self.roles.remove(&tile)?;
        self.workspace
            .apply(&WorkspaceEvent::Tile(TileEvent::Closed(tile)));
        let closed = match role {
            TileRole::Document(key) => self.entries.remove(&key).map(|entry| (key, entry.doc)),
            _ => None,
        };
        if let Some((key, _)) = &closed
            && self.focused == Some(*key)
        {
            self.focused = self
                .active_document()
                .or_else(|| self.entries.keys().next().copied());
        }
        closed
    }

    fn active_document(&self) -> Option<DocKey> {
        fn active(tree: &TileTree) -> Vec<TileId> {
            match tree {
                TileTree::Stack(stack) => stack
                    .tabs
                    .get(stack.active)
                    .map(|tile| tile.id)
                    .into_iter()
                    .collect(),
                TileTree::Split { children, .. } => children
                    .iter()
                    .flat_map(|branch| active(&branch.tree))
                    .collect(),
            }
        }
        active(self.workspace.tiled())
            .into_iter()
            .find_map(|tile| match self.roles.get(&tile) {
                Some(TileRole::Document(key)) => Some(*key),
                _ => None,
            })
    }

    fn mint_tile(&mut self, title: String, role: TileRole) -> Tile {
        let id = TileId(self.next_tile);
        self.next_tile += 1;
        self.roles.insert(id, role);
        Tile {
            id,
            title,
            content: ContentSource::Open {
                kind: TILE_KIND.to_string(),
                id: id.0.to_string(),
            },
            accent: None,
        }
    }

    /// A new document goes beside the focused document's tile, else into the
    /// first stack that holds a document, else into an empty root.
    fn place_document_tile(&mut self, tile: Tile) {
        let beside = self
            .focused
            .and_then(|key| self.entries.get(&key))
            .map(|entry| entry.tile)
            .or_else(|| self.entries.values().map(|entry| entry.tile).next());
        let tree = self.workspace.tiled_mut();
        if let Some(target) = beside
            && tree.insert_tab_after(target, tile.clone())
        {
            return;
        }
        match tree {
            TileTree::Stack(stack) if stack.tabs.is_empty() => {
                stack.tabs.push(tile);
                stack.active = 0;
            },
            _ => {
                *tree = TileTree::stack(vec![tile], 0);
            },
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn activating_a_document_tile_focuses_it() {
        let mut space = DocumentWorkspace::new();
        let (a, _) = space.open(path("a.djot"), "a.djot", "A");
        space.open(path("b.djot"), "b.djot", "B");
        space.activate(space.tile_of(a).unwrap());
        assert_eq!(space.focused(), Some(a));
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
}
