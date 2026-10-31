// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Knot's adapter for Platen's scoped Forme session. Members name presentation
//! tiles, never documents or resources. The host keeps all document payloads.
use kernel::graph::{Graph, apply::add_node};
use platen::{
    TileLayout,
    projection_geometry::{TreeBranch, TreeGeometry},
    workspace::{FormeSession, FormeWorkspace, NoPreferences, SaveKind},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use workbench::{Tile, TileBranch, TileId, TileTree, Workspace, WorkspaceEvent};

const OWNER: Uuid = Uuid::from_u128(0x6b6e6f74_776f726b_73706163_65666f72);
pub(crate) type Scope = Vec<(u64, String)>;

#[derive(Serialize, Deserialize)]
struct StoredForme {
    scope: Scope,
    record: Vec<u8>,
}

pub(crate) struct WorkspaceForme {
    session: FormeSession,
    pub scope: Scope,
    pub preview: Workspace,
    /// Includes the envelope: a changed slot for another tile set also refuses a save.
    baseline: Option<Value>,
}

impl WorkspaceForme {
    pub fn begin(tree: &TileTree, scope: Scope, baseline: Option<Value>) -> Result<Self, String> {
        let mut graph = Graph::new();
        let tiles = tiles(tree);
        let mut workspace = FormeWorkspace::<NoPreferences>::new(OWNER);
        for tile in &tiles {
            let member = member(tile.id, &scope);
            add_node(
                &mut graph,
                Some(member),
                format!("knot:workspace-tile/{}", tile.id.0),
                Default::default(),
            );
            workspace.open(member, &graph)?;
        }
        keep_tree(&mut workspace, tree, &scope);
        workspace.validate(OWNER)?;
        // Restore a saved layout only for exactly the same host-owned bindings.
        // Scratch bindings carry a process-local nonce supplied by DocumentWorkspace.
        let mut session = if let Some(value) = &baseline {
            let saved: StoredForme = serde_json::from_value(value.clone())
                .map_err(|e| format!("Invalid saved workspace: {e}"))?;
            if compatible_scope(&saved.scope, &scope) {
                FormeSession::from_bytes(Some(&saved.record), OWNER, &graph)?
            } else {
                FormeSession::new(workspace)
            }
        } else {
            FormeSession::new(workspace)
        };
        // An external record may omit/rebind a tile despite matching its envelope.
        let mut actual = session.committed().members();
        let mut expected: Vec<_> = tiles.iter().map(|tile| member(tile.id, &scope)).collect();
        actual.sort();
        expected.sort();
        if actual != expected {
            return Err("Saved workspace does not match its tile bindings".into());
        }
        session.unlock()?;
        let preview = Workspace::new(project(session.view().geometry.as_ref(), &tiles, &scope)?);
        Ok(Self {
            session,
            scope,
            preview,
            baseline,
        })
    }

    pub fn edit(&mut self, event: &WorkspaceEvent) -> Result<bool, String> {
        let mut next = self.preview.clone();
        let changed = next.apply(event).changed();
        if changed {
            let key = match event {
                WorkspaceEvent::Tile(workbench::TileEvent::DividerMoved { split, .. }) => {
                    Some(format!("divider:{split:?}"))
                },
                _ => None,
            };
            self.session.edit(key, now_ms(), |workspace| {
                keep_tree(workspace, next.tiled(), &self.scope);
                Ok(())
            })?;
            self.preview = next;
        }
        Ok(changed)
    }

    pub fn save(
        &mut self,
        write: impl FnOnce(Option<&Value>, &Value) -> Result<(), String>,
    ) -> Result<(), String> {
        let proposal = self.session.prepare_save(SaveKind::Apply, now_ms())?;
        let result = (|| {
            let value = serde_json::to_value(StoredForme {
                scope: self.scope.clone(),
                record: proposal.bytes()?,
            })
            .map_err(|e| e.to_string())?;
            write(self.baseline.as_ref(), &value)?;
            Ok(())
        })();
        self.session.finish_save(proposal, result)
    }

    pub fn discard(&mut self) -> Result<(), String> {
        self.session.discard()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn member(tile: TileId, scope: &Scope) -> Uuid {
    let binding = &scope
        .iter()
        .find(|(id, _)| *id == tile.0)
        .expect("tile has a host binding")
        .1;
    let mut hash = blake3::Hasher::new();
    hash.update(b"knot.workspace-member/v1\0");
    hash.update(binding.as_bytes());
    // Multiple identical reading surfaces have no stable independent binding.
    // Keep them distinct in-process and refuse cross-allocation restoration.
    if scope.iter().filter(|(_, value)| value == binding).count() > 1 {
        hash.update(&tile.0.to_le_bytes());
    }
    Uuid::from_bytes(hash.finalize().as_bytes()[..16].try_into().unwrap())
}
fn compatible_scope(saved: &Scope, current: &Scope) -> bool {
    if saved == current {
        return true;
    }
    let mut old: Vec<_> = saved.iter().map(|(_, binding)| binding).collect();
    let mut new: Vec<_> = current.iter().map(|(_, binding)| binding).collect();
    old.sort();
    new.sort();
    old == new && old.windows(2).all(|pair| pair[0] != pair[1])
}

pub(crate) fn tiles(tree: &TileTree) -> Vec<Tile> {
    match tree {
        TileTree::Stack(stack) => stack.tabs.clone(),
        TileTree::Split { children, .. } => children
            .iter()
            .flat_map(|child| tiles(&child.tree))
            .collect(),
    }
}
fn geometry(tree: &TileTree, scope: &Scope) -> TreeGeometry {
    match tree {
        TileTree::Stack(stack) => TreeGeometry::Stack {
            members: stack
                .tabs
                .iter()
                .map(|tile| member(tile.id, scope))
                .collect(),
            active: stack.active,
        },
        TileTree::Split { axis, children } => TreeGeometry::Split {
            axis: (*axis).into(),
            children: children
                .iter()
                .map(|child| TreeBranch {
                    fraction: child.fraction,
                    node: geometry(&child.tree, scope),
                })
                .collect(),
        },
    }
}
fn keep_tree(workspace: &mut FormeWorkspace, tree: &TileTree, scope: &Scope) {
    let layout = TileLayout::from_arrangement(
        &workspace.document.arrangement,
        Some(&geometry(tree, scope)),
    );
    workspace.keep_layout(&layout);
}
fn project(
    geometry: Option<&TreeGeometry>,
    tiles: &[Tile],
    scope: &Scope,
) -> Result<TileTree, String> {
    match geometry {
        Some(TreeGeometry::Stack { members, active }) => {
            let tabs = members
                .iter()
                .map(|id| {
                    tiles
                        .iter()
                        .find(|tile| member(tile.id, scope) == *id)
                        .cloned()
                        .ok_or_else(|| "Workspace tile is no longer open".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TileTree::stack(tabs, *active))
        },
        Some(TreeGeometry::Split { axis, children }) => Ok(TileTree::split(
            (*axis).into(),
            children
                .iter()
                .map(|child| {
                    Ok(TileBranch {
                        fraction: child.fraction,
                        tree: project(Some(&child.node), tiles, scope)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        )),
        None => Ok(TileTree::stack(Vec::new(), 0)),
    }
}
