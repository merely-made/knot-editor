// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<D> DocumentWorkspace<D> {
    /// Open the metadata tile of `page` in `site`, or activate the open one.
    /// It joins the focused document's stack, as a document would.
    pub fn open_metadata(&mut self, site: SiteKey, page: &str, title: impl Into<String>) -> TileId {
        if let Some(tile) = self.metadata_tile(site, page) {
            self.activate(tile);
            return tile;
        }
        let role = TileRole::Metadata {
            site,
            page: page.to_owned(),
        };
        let tile = self.mint_tile(title.into(), role);
        let id = tile.id;
        self.place_document_tile(tile);
        self.activate(id);
        id
    }

    /// The open metadata tile of `page` in `site`, if any.
    pub fn metadata_tile(&self, site: SiteKey, page: &str) -> Option<TileId> {
        self.metadata_tiles(site).into_iter().find(|tile| {
            matches!(self.roles.get(tile), Some(TileRole::Metadata { page: open, .. }) if open == page)
        })
    }

    /// Every open metadata tile of `site`, in tile order.
    pub fn metadata_tiles(&self, site: SiteKey) -> Vec<TileId> {
        let mut tiles: Vec<TileId> = self
            .roles
            .iter()
            .filter(
                |(_, role)| matches!(role, TileRole::Metadata { site: open, .. } if *open == site),
            )
            .map(|(tile, _)| *tile)
            .collect();
        tiles.sort_by_key(|tile| tile.0);
        tiles
    }

    /// Open a reading tile of `kind`, following the focused document or
    /// pinned to one. It joins the stack of the reading activated last, else
    /// takes a new stack right of the focused document's.
    pub fn open_reading(
        &mut self,
        kind: ReadingKind,
        pinned: Option<DocKey>,
        title: impl Into<String>,
    ) -> TileId {
        let beside_reading = self
            .last_reading
            .filter(|tile| self.workspace.tiled().find(*tile).is_some());
        let beside_document = self
            .focused
            .or_else(|| self.entries.keys().next().copied())
            .and_then(|key| self.tile_of(key));
        let tile = self.mint_tile(title.into(), TileRole::Reading { kind, pinned });
        let id = tile.id;
        let tree = self.workspace.tiled_mut();
        let placed = beside_reading.is_some_and(|last| tree.insert_tab_after(last, tile.clone()))
            || beside_document
                .is_some_and(|document| tree.split_beside(document, Edge::Right, tile.clone()));
        if !placed {
            match tree {
                TileTree::Stack(stack) => {
                    stack.tabs.push(tile);
                    stack.active = stack.tabs.len() - 1;
                },
                _ => *tree = TileTree::stack(vec![tile], 0),
            }
        }
        self.workspace
            .apply(&WorkspaceEvent::Tile(TileEvent::Activated(id)));
        self.last_reading = Some(id);
        id
    }

    /// The open Navigator tile, if any.
    pub fn navigator(&self) -> Option<TileId> {
        self.roles
            .iter()
            .find(|(_, role)| **role == TileRole::Navigator)
            .map(|(tile, _)| *tile)
    }

    /// Open the Navigator, once, in a new stack left of the documents taking
    /// [`NAVIGATOR_SHARE`] of the split; with no document open it joins the
    /// frame's stack.
    pub fn open_navigator(&mut self, title: impl Into<String>) -> TileId {
        if let Some(tile) = self.navigator() {
            return tile;
        }
        self.open_left(title.into(), TileRole::Navigator)
    }

    pub fn graph(&self) -> Option<TileId> {
        self.roles
            .iter()
            .find(|(_, role)| **role == TileRole::Graph)
            .map(|(tile, _)| *tile)
    }

    /// Open the singleton graph in the document stack.
    pub fn open_graph(&mut self, title: impl Into<String>) -> TileId {
        if let Some(tile) = self.graph() {
            self.activate(tile);
            return tile;
        }
        let tile = self.mint_tile(title.into(), TileRole::Graph);
        let id = tile.id;
        self.place_document_tile(tile);
        self.activate(id);
        id
    }

    /// The open site tile of `site`, if any.
    pub fn site_tile(&self, site: SiteKey) -> Option<TileId> {
        self.roles
            .iter()
            .find(|(_, role)| **role == TileRole::Site(site))
            .map(|(tile, _)| *tile)
    }

    /// Open `site`'s tile, or activate the open one. It joins the Navigator's
    /// stack or another site's, else takes a new stack left of the documents
    /// at the Navigator's share.
    pub fn open_site_tile(&mut self, site: SiteKey, title: impl Into<String>) -> TileId {
        if let Some(tile) = self.site_tile(site) {
            self.activate(tile);
            return tile;
        }
        let tile = self.open_left(title.into(), TileRole::Site(site));
        self.activate(tile);
        tile
    }

    /// The open reading tile of `kind` that follows the focus, if any.
    pub fn following_reading(&self, kind: ReadingKind) -> Option<TileId> {
        self.readings()
            .find(|(_, open, pinned)| *open == kind && pinned.is_none())
            .map(|(tile, _, _)| tile)
    }

    /// Every open reading tile: its id, kind and pin, in tile order.
    pub fn readings(&self) -> impl Iterator<Item = (TileId, ReadingKind, Option<DocKey>)> + '_ {
        let mut tiles: Vec<_> = self
            .roles
            .iter()
            .filter_map(|(tile, role)| match role {
                TileRole::Reading { kind, pinned } => Some((*tile, *kind, *pinned)),
                _ => None,
            })
            .collect();
        tiles.sort_by_key(|(tile, _, _)| tile.0);
        tiles.into_iter()
    }
}
