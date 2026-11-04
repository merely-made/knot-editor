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

use workbench::{
    ContentSource, Edge, Tile, TileEvent, TileId, TileTree, Workspace, WorkspaceEvent,
};

/// The runtime key of one open document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocKey(pub u64);

/// The runtime key of one open site. It names the site's entry, not its
/// folder, so a folder opened again gets a new key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SiteKey(pub u64);

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
    /// Outgoing links and backlinks from documents currently open in this window.
    Links,
    /// The Titan or Spartan composer of its document.
    Submit,
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
    /// The shared Mere graph view over this host's catalog and open documents.
    Graph,
    /// A site page's metadata form. Its draft lives with the site, so the
    /// tile needs no tab of the page's source.
    Metadata {
        site: SiteKey,
        page: String,
    },
    /// An open site: its pages, publishing and serving.
    Site(SiteKey),
}

/// The Workbench lane every Knot tile rides in.
pub const TILE_KIND: &str = "knot";

/// The singleton Graph's reserved tile identity. Knot uses its stable
/// `data-tile` value to exempt only that Workbench content slot from the
/// renderer-hostile overflow topology around the shared Mere view.
const GRAPH_TILE_ID: TileId = TileId(u64::MAX);
pub(crate) const GRAPH_TILE_ID_ATTR: &str = "18446744073709551615";

/// The share of its split the Navigator opens at: about a quarter, the rest
/// going to the documents (Mark's ruling, 2026-09-26). The divider moves it.
pub const NAVIGATOR_SHARE: f32 = 0.25;

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
    forme: Option<crate::workspace_forme::WorkspaceForme>,
    scratch_scope: uuid::Uuid,
    focused: Option<DocKey>,
    /// The reading tile activated or opened last, whose stack the next
    /// reading joins.
    last_reading: Option<TileId>,
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
            forme: None,
            scratch_scope: uuid::Uuid::new_v4(),
            focused: None,
            last_reading: None,
            next_doc: 1,
            next_tile: 1,
        }
    }

    /// The tree a frame renders.
    pub fn workspace(&self) -> &Workspace {
        self.forme
            .as_ref()
            .filter(|forme| forme.scope == self.forme_scope())
            .map_or(&self.workspace, |forme| &forme.preview)
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
            if let Some(forme) = self.forme.as_mut() {
                let _ = forme.edit(&WorkspaceEvent::Tile(TileEvent::Activated(entry.tile)));
            }
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
        let title = title.into();
        if let Some(id) = self.tile_of(key) {
            if let Some(forme) = self.forme.as_mut() {
                if let Some(tile) = forme.preview.tiled_mut().tile_mut(id) {
                    tile.title = title.clone();
                }
            }
        }
        if let Some(tile) = self.tile_of(key)
            && let Some(tile) = self.workspace.tiled_mut().tile_mut(tile)
        {
            tile.title = title.into();
        }
    }

    pub fn tile_of(&self, key: DocKey) -> Option<TileId> {
        self.entries.get(&key).map(|entry| entry.tile)
    }

    /// The selected tile in the stack containing this document. A selected
    /// Outline or Preview in another pane is independent and is left alone.
    pub fn active_tile_in_stack(&self, key: DocKey) -> Option<TileId> {
        fn active(tree: &TileTree, target: TileId) -> Option<TileId> {
            match tree {
                TileTree::Stack(stack) if stack.tabs.iter().any(|tile| tile.id == target) => {
                    stack.tabs.get(stack.active).map(|tile| tile.id)
                },
                TileTree::Stack(_) => None,
                TileTree::Split { children, .. } => children
                    .iter()
                    .find_map(|branch| active(&branch.tree, target)),
            }
        }
        active(self.workspace.tiled(), self.tile_of(key)?)
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
            TileRole::Navigator
            | TileRole::Graph
            | TileRole::Metadata { .. }
            | TileRole::Site(_) => None,
        }
    }

    /// Activate a tile, focusing its document when it has one.
    pub fn activate(&mut self, tile: TileId) {
        if let Some(forme) = self.forme.as_mut() {
            let _ = forme.edit(&WorkspaceEvent::Tile(TileEvent::Activated(tile)));
        }
        self.workspace
            .apply(&WorkspaceEvent::Tile(TileEvent::Activated(tile)));
        match self.roles.get(&tile) {
            Some(TileRole::Document(key)) => self.focused = Some(*key),
            Some(TileRole::Reading { .. }) => self.last_reading = Some(tile),
            _ => {},
        }
    }
}

/// Give the stack holding `tile` `share` of its split, the rest going to its
/// siblings in proportion to what they had. `false` when no split holds it.
fn set_stack_share(tree: &mut TileTree, tile: TileId, share: f32) -> bool {
    let TileTree::Split { children, .. } = tree else {
        return false;
    };
    let holds = |branch: &workbench::TileBranch| matches!(&branch.tree, TileTree::Stack(stack) if stack.tabs.iter().any(|t| t.id == tile));
    let Some(index) = children.iter().position(holds) else {
        return children
            .iter_mut()
            .any(|branch| set_stack_share(&mut branch.tree, tile, share));
    };
    let rest: f32 = children
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, branch)| branch.fraction)
        .sum();
    let siblings = (children.len() - 1).max(1) as f32;
    for (i, branch) in children.iter_mut().enumerate() {
        branch.fraction = if i == index {
            share
        } else if rest > f32::EPSILON {
            branch.fraction / rest * (1.0 - share)
        } else {
            (1.0 - share) / siblings
        };
    }
    true
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod forme_adoption_tests;

mod forme;

mod projections;

mod tiles;
