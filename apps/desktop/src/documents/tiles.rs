// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<D> DocumentWorkspace<D> {
    /// Put the singleton Graph beside the last document before that document
    /// leaves. Closing the document then preserves its stack and every other
    /// workspace branch.
    pub fn prepare_last_document_close(
        &mut self,
        document: DocKey,
        title: impl Into<String>,
    ) -> Option<TileId> {
        if self.entries.len() != 1 || !self.entries.contains_key(&document) {
            return None;
        }
        if let Some(graph) = self.graph() {
            self.activate(graph);
            return Some(graph);
        }
        let document_tile = self.tile_of(document)?;
        let tile = self.mint_tile(title.into(), TileRole::Graph);
        let graph = tile.id;
        if !self
            .workspace
            .tiled_mut()
            .insert_tab_after(document_tile, tile)
        {
            self.roles.remove(&graph);
            return None;
        }
        self.activate(graph);
        Some(graph)
    }

    /// Place a left-stack tile: beside an open Navigator or site tile, else
    /// in a new stack left of the documents taking [`NAVIGATOR_SHARE`] of the
    /// split; with no document open it joins the frame's stack.
    pub(super) fn open_left(&mut self, title: String, role: TileRole) -> TileId {
        let beside_left = self
            .roles
            .iter()
            .filter(|(_, role)| matches!(role, TileRole::Navigator | TileRole::Site(_)))
            .map(|(tile, _)| *tile)
            .min_by_key(|tile| tile.0)
            .filter(|tile| self.workspace.tiled().find(*tile).is_some());
        let beside_document = self
            .focused
            .or_else(|| self.entries.keys().next().copied())
            .and_then(|key| self.tile_of(key));
        let tile = self.mint_tile(title, role);
        let id = tile.id;
        let tree = self.workspace.tiled_mut();
        if let Some(left) = beside_left
            && tree.insert_tab_after(left, tile.clone())
        {
            return id;
        }
        let placed = beside_document
            .is_some_and(|document| tree.split_beside(document, Edge::Left, tile.clone()));
        if placed {
            set_stack_share(tree, id, NAVIGATOR_SHARE);
        } else {
            match tree {
                TileTree::Stack(stack) => {
                    stack.tabs.push(tile);
                    stack.active = stack.tabs.len() - 1;
                },
                _ => *tree = TileTree::stack(vec![tile], 0),
            }
        }
        id
    }

    /// Pin a reading tile to `pinned`, or let it follow the focus again.
    pub fn set_pinned(&mut self, tile: TileId, pinned: Option<DocKey>) {
        if let Some(TileRole::Reading {
            pinned: current, ..
        }) = self.roles.get_mut(&tile)
        {
            *current = pinned;
        }
    }

    /// Rename any tile's tab.
    pub fn set_tile_title(&mut self, tile: TileId, title: impl Into<String>) {
        let title = title.into();
        if let Some(forme) = self.forme.as_mut() {
            if let Some(preview) = forme.preview.tiled_mut().tile_mut(tile) {
                preview.title = title.clone();
            }
        }
        if let Some(tile) = self.workspace.tiled_mut().tile_mut(tile) {
            tile.title = title.into();
        }
    }

    /// Apply a divider move to the Forme preview when unlocked, otherwise
    /// to the ordinary live layout. Drags, floats and tearouts are not adopted yet
    /// and return `false`, leaving the tree unchanged.
    pub fn apply_layout(&mut self, event: &WorkspaceEvent) -> bool {
        if self
            .forme
            .as_ref()
            .is_some_and(|forme| forme.scope != self.forme_scope())
        {
            return false;
        }
        match event {
            WorkspaceEvent::Tile(TileEvent::DividerMoved { .. }) => {
                if let Some(forme) = self.forme.as_mut() {
                    forme.edit(event).unwrap_or(false)
                } else {
                    self.workspace.apply(event).changed()
                }
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
        if self.last_reading == Some(tile) {
            self.last_reading = self.readings().map(|(tile, _, _)| tile).last();
        }
        let closed = match role {
            TileRole::Document(key) => self.entries.remove(&key).map(|entry| (key, entry.doc)),
            _ => None,
        };
        // A reading pinned to a closed document has nothing left to show.
        if let Some((key, _)) = &closed {
            let pinned: Vec<TileId> = self
                .readings()
                .filter(|(_, _, pinned)| *pinned == Some(*key))
                .map(|(tile, _, _)| tile)
                .collect();
            for reading in pinned {
                self.close(reading);
            }
        }
        if let Some((key, _)) = &closed
            && self.focused == Some(*key)
        {
            self.focused = self
                .active_document()
                .or_else(|| self.entries.keys().next().copied());
        }
        closed
    }

    pub(super) fn active_document(&self) -> Option<DocKey> {
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

    pub(super) fn mint_tile(&mut self, title: String, role: TileRole) -> Tile {
        let id = if role == TileRole::Graph {
            GRAPH_TILE_ID
        } else {
            let id = TileId(self.next_tile);
            self.next_tile += 1;
            id
        };
        debug_assert_eq!(GRAPH_TILE_ID.0.to_string(), GRAPH_TILE_ID_ATTR);
        debug_assert!(!self.roles.contains_key(&id));
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

    /// A new document goes into the existing document/Graph stack. If that
    /// centre stack was explicitly closed while side stacks survived, rebuild
    /// it immediately left of a reading stack or right of the left stack;
    /// never turn a Navigator, site, or reading into a document stack.
    pub(super) fn place_document_tile(&mut self, tile: Tile) {
        let beside = self
            .focused
            .and_then(|key| self.entries.get(&key))
            .map(|entry| entry.tile)
            .or_else(|| self.entries.values().map(|entry| entry.tile).next())
            .or_else(|| self.graph())
            .or_else(|| {
                self.roles
                    .iter()
                    .filter(|(_, role)| matches!(role, TileRole::Metadata { .. }))
                    .map(|(tile, _)| *tile)
                    .filter(|tile| self.workspace.tiled().find(*tile).is_some())
                    .min_by_key(|tile| tile.0)
            });
        let reading = self
            .roles
            .iter()
            .filter(|(_, role)| matches!(role, TileRole::Reading { .. }))
            .map(|(tile, _)| *tile)
            .filter(|tile| self.workspace.tiled().find(*tile).is_some())
            .min_by_key(|tile| tile.0);
        let left = self
            .roles
            .iter()
            .filter(|(_, role)| matches!(role, TileRole::Navigator | TileRole::Site(_)))
            .map(|(tile, _)| *tile)
            .filter(|tile| self.workspace.tiled().find(*tile).is_some())
            .min_by_key(|tile| tile.0);
        let tree = self.workspace.tiled_mut();
        if let Some(target) = beside
            && tree.insert_tab_after(target, tile.clone())
        {
            return;
        }
        if let Some(target) = reading
            && tree.split_beside(target, Edge::Left, tile.clone())
        {
            return;
        }
        if let Some(target) = left
            && tree.split_beside(target, Edge::Right, tile.clone())
        {
            return;
        }
        match tree {
            TileTree::Stack(stack) if stack.tabs.is_empty() => {
                stack.tabs.push(tile);
                stack.active = 0;
            },
            _ => {
                // All retained tiles have a role, so this is only defensive:
                // preserve an unexpected survivor in a separate stack.
                let target = tree.tiles().first().map(|survivor| survivor.id);
                if !target
                    .is_some_and(|target| tree.split_beside(target, Edge::Right, tile.clone()))
                {
                    *tree = TileTree::stack(vec![tile], 0);
                }
            },
        }
    }
}
