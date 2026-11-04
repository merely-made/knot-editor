// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<D> DocumentWorkspace<D> {
    pub(super) fn forme_scope(&self) -> crate::workspace_forme::Scope {
        let mut scope: Vec<_> = self
            .roles
            .iter()
            .map(|(tile, role)| {
                let binding = match role {
                    TileRole::Document(key) => match self.identity(*key) {
                        Some(DocIdentity::Scratch) => {
                            format!("scratch:{}:{}", self.scratch_scope, key.0)
                        },
                        identity => format!("document:{identity:?}"),
                    },
                    TileRole::Reading { kind, pinned } => {
                        let binding = pinned.map(|key| match self.identity(key) {
                            Some(DocIdentity::Scratch) => {
                                format!("scratch:{}:{}", self.scratch_scope, key.0)
                            },
                            identity => format!("document:{identity:?}"),
                        });
                        format!("reading:{kind:?}:{binding:?}")
                    },
                    TileRole::Site(_) | TileRole::Metadata { .. } => {
                        format!("{}:{role:?}", self.scratch_scope)
                    },
                    _ => format!("{role:?}"),
                };
                (tile.0, binding)
            })
            .collect();
        // Reading pins refer to runtime document keys; include every document's
        // identity so identical numeric keys in another launch cannot rebind them.
        scope.sort_by_key(|(tile, _)| *tile);
        scope
    }

    pub fn forme_editing(&self) -> bool {
        self.forme.is_some()
    }

    pub fn forme_resizable(&self) -> bool {
        self.forme_editing()
            && self
                .workspace()
                .tiled()
                .fractions_at(&workbench::TilePath(vec![]))
                .is_some_and(|fractions| fractions.len() >= 2)
    }

    /// A keyboard/menu route until the shared Frisket pointer producer exists.
    pub fn resize_forme_first(&mut self, more: bool) -> Result<(), String> {
        if !self.forme_editing() {
            return Err("Unlock the workspace layout first".into());
        }
        let split = workbench::TilePath(vec![]);
        let mut fractions = self
            .workspace()
            .tiled()
            .fractions_at(&split)
            .filter(|f| f.len() >= 2)
            .ok_or("This workspace has no split to resize")?;
        let pair = fractions[0] + fractions[1];
        let first = (fractions[0] + if more { 0.05 } else { -0.05 }).clamp(pair * 0.1, pair * 0.9);
        fractions[0] = first;
        fractions[1] = pair - first;
        if self.apply_layout(&WorkspaceEvent::Tile(TileEvent::DividerMoved {
            split,
            fractions,
        })) {
            Ok(())
        } else {
            Err("The workspace draft could not be resized; its open tiles may have changed".into())
        }
    }

    pub fn unlock_forme(&mut self, saved: Option<serde_json::Value>) -> Result<(), String> {
        if self.forme.is_some() {
            return Err("Apply or discard the open workspace draft first".into());
        }
        let mut forme = crate::workspace_forme::WorkspaceForme::begin(
            self.workspace.tiled(),
            self.forme_scope(),
            saved,
        )?;
        // Restoring geometry must not hide the document that still receives
        // the writer's editing, Save and Close commands.
        if let Some(tile) = self.focused.and_then(|key| self.tile_of(key)) {
            forme.edit(&WorkspaceEvent::Tile(TileEvent::Activated(tile)))?;
        }
        self.forme = Some(forme);
        Ok(())
    }

    pub fn discard_forme(&mut self) -> Result<(), String> {
        let forme = self.forme.as_mut().ok_or("No workspace draft is open")?;
        forme.discard()?;
        self.forme = None;
        Ok(())
    }

    pub fn apply_forme(
        &mut self,
        write: impl FnOnce(Option<&serde_json::Value>, &serde_json::Value) -> Result<(), String>,
    ) -> Result<(), String> {
        let scope = self.forme_scope();
        let forme = self.forme.as_mut().ok_or("No workspace draft is open")?;
        if forme.scope != scope {
            return Err("Open tiles changed; discard this workspace draft and unlock again".into());
        }
        forme.save(write)?;
        self.workspace = forme.preview.clone();
        self.forme = None;
        Ok(())
    }
}
