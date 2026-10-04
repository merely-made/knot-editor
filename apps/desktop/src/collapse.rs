// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Automatic, host-side presentation of narrow workspaces. The canonical
//! Workbench tree is never changed: its divider ratios return when room does.

use crate::appearance::{Appearance, SourceFace};
use crate::documents::TileRole;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use workbench::{
    CollapsedStack, DrawerGeometry, SplitAxis, SplitPresentation, StackPresentation, TileId,
    TilePath, TileTree, WorkbenchPresentation,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CollapsePreferences {
    pub reading_min_width: u16,
    pub rail_width: u16,
    pub menu_below: u16,
    pub drawer_below: u16,
    pub status_below: u16,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Default for CollapsePreferences {
    fn default() -> Self {
        Self {
            reading_min_width: 280,
            rail_width: 28,
            menu_below: 700,
            drawer_below: 500,
            status_below: 700,
            other: Map::new(),
        }
    }
}

impl CollapsePreferences {
    pub fn normalize(&mut self) {
        self.reading_min_width = self.reading_min_width.clamp(120, 1200);
        self.rail_width = self.rail_width.clamp(24, 64);
        self.menu_below = self.menu_below.clamp(320, 2400);
        self.drawer_below = self.drawer_below.clamp(320, 2400);
        self.status_below = self.status_below.clamp(320, 2400);
    }

    pub fn chip_limit(&self, viewport: f32) -> usize {
        if viewport < f32::from(self.status_below) {
            1
        } else {
            usize::MAX
        }
    }
}

/// Source text width plus Workbench's 12px content insets and the writing
/// area's 16px insets and border (58px total).
/// Plex Mono's bundled advance is exactly 0.6em. System monospace uses the
/// host's laid-out 1ch probe; before that is available, 0.65em is a conservative
/// estimate. Full measure has no fixed requirement and uses the reading minimum.
pub fn source_width(appearance: &Appearance, source: &str, system_ch: Option<f32>) -> f32 {
    let size = f32::from(
        appearance
            .font_size
            .clamp(Appearance::MIN_FONT_SIZE, Appearance::MAX_FONT_SIZE),
    );
    let advance = match appearance.source_face {
        SourceFace::IbmPlexMono => size * 0.6,
        SourceFace::SystemMonospace => system_ch
            .filter(|width| width.is_finite() && *width > 0.0)
            .unwrap_or(size * 0.65),
    };
    appearance
        .source_columns(source)
        .map_or(0.0, |columns| columns as f32 * advance + 58.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Source,
    Navigator,
    Reading,
}

struct Leaf {
    anchor: TileId,
    kind: Kind,
    label: String,
    measure: f32,
    canonical: f32,
    collapsed: bool,
    open: bool,
}

/// Derive rails and constrained fractions from today's viewport and today's
/// source measure. `role` and `measure` remain application facts, not Workbench
/// vocabulary. Previous open rails survive a resize by their stable tile anchor.
#[allow(clippy::too_many_arguments)]
pub fn presentation(
    tree: &TileTree,
    preferences: &CollapsePreferences,
    viewport: (f32, f32),
    frame: (f32, f32, f32, f32),
    previous: &WorkbenchPresentation,
    source: Option<TileId>,
    role: impl Fn(TileId) -> Option<TileRole>,
    measure: impl Fn(TileId) -> f32,
) -> WorkbenchPresentation {
    let mut leaves = Vec::new();
    collect(tree, frame.2, &role, &measure, previous, &mut leaves);
    let rail = f32::from(preferences.rail_width);
    let minimum = f32::from(preferences.reading_min_width);
    let drawer = viewport.0 < f32::from(preferences.drawer_below);
    let squeezed = leaves
        .iter()
        .any(|leaf| leaf.kind == Kind::Source && leaf.canonical + 0.5 < leaf.measure);
    if squeezed || drawer {
        for leaf in &mut leaves {
            leaf.collapsed = leaf.kind == Kind::Navigator || (drawer && leaf.kind != Kind::Source);
        }
    }
    if (squeezed || drawer) && required(tree, &leaves, rail, minimum) > frame.2 {
        for leaf in &mut leaves {
            if leaf.kind == Kind::Reading {
                leaf.collapsed = true;
            }
        }
    }
    let mut result = WorkbenchPresentation {
        return_focus: source,
        ..WorkbenchPresentation::default()
    };
    // An open rail at desktop sizes gets its minimum as an inline stack. Under
    // the drawer threshold it remains a rail and overlays the source instead.
    for leaf in &leaves {
        result.stacks.push(StackPresentation {
            anchor: leaf.anchor,
            min_width: 0.0,
            collapsed: leaf.collapsed.then(|| CollapsedStack {
                label: leaf.label.clone(),
                rail_width: rail,
                open: leaf.open,
                drawer: drawer.then(|| DrawerGeometry {
                    // Overlay coordinates resolve inside the workspace frame.
                    trigger: (0.0, 0.0, rail, 0.0),
                    panel_size: (minimum.min(frame.2.max(1.0)), frame.3.max(1.0)),
                    bounds: (0.0, 0.0, frame.2.max(1.0), frame.3.max(1.0)),
                }),
            }),
        });
    }
    if squeezed || drawer {
        constrain(
            tree,
            &leaves,
            rail,
            minimum,
            drawer,
            frame.2,
            &mut Vec::new(),
            &mut result.splits,
        );
    }
    result
}

fn collect(
    tree: &TileTree,
    width: f32,
    role: &impl Fn(TileId) -> Option<TileRole>,
    measure: &impl Fn(TileId) -> f32,
    previous: &WorkbenchPresentation,
    leaves: &mut Vec<Leaf>,
) {
    match tree {
        TileTree::Stack(stack) => {
            let Some(first) = stack.tabs.first() else {
                return;
            };
            let kind =
                if stack
                    .tabs
                    .iter()
                    .any(|tile| matches!(role(tile.id), Some(TileRole::Document(_))))
                {
                    Kind::Source
                } else if stack.tabs.iter().any(|tile| {
                    matches!(role(tile.id), Some(TileRole::Navigator | TileRole::Site(_)))
                }) {
                    Kind::Navigator
                } else {
                    Kind::Reading
                };
            let measure = stack
                .tabs
                .iter()
                .map(|tile| measure(tile.id))
                .fold(0.0, f32::max);
            let open = previous
                .stacks
                .iter()
                .find(|p| p.anchor == first.id)
                .and_then(|p| p.collapsed.as_ref())
                .is_some_and(|p| p.open);
            leaves.push(Leaf {
                anchor: first.id,
                kind,
                label: if kind == Kind::Navigator {
                    "Navigator".into()
                } else {
                    "Readings".into()
                },
                measure,
                canonical: width,
                collapsed: false,
                open,
            });
        },
        TileTree::Split { axis, children } => {
            let available = (width - children.len().saturating_sub(1) as f32).max(1.0);
            let total: f32 = children.iter().map(|b| b.fraction.max(0.0)).sum();
            for child in children {
                let child_width = if *axis == SplitAxis::Row {
                    available * child.fraction.max(0.0) / total.max(f32::EPSILON)
                } else {
                    width
                };
                collect(&child.tree, child_width, role, measure, previous, leaves);
            }
        },
    }
}

fn leaf<'a>(tree: &TileTree, leaves: &'a [Leaf]) -> Option<&'a Leaf> {
    let TileTree::Stack(stack) = tree else {
        return None;
    };
    let first = stack.tabs.first()?;
    leaves.iter().find(|leaf| leaf.anchor == first.id)
}

fn required(tree: &TileTree, leaves: &[Leaf], rail: f32, minimum: f32) -> f32 {
    if let Some(leaf) = leaf(tree, leaves) {
        return if leaf.collapsed {
            rail
        } else if leaf.kind == Kind::Source {
            leaf.measure
        } else {
            minimum
        };
    }
    match tree {
        TileTree::Split {
            axis: SplitAxis::Row,
            children,
        } => {
            children
                .iter()
                .map(|b| required(&b.tree, leaves, rail, minimum))
                .sum::<f32>()
                + children.len().saturating_sub(1) as f32
        },
        TileTree::Split { children, .. } => children
            .iter()
            .map(|b| required(&b.tree, leaves, rail, minimum))
            .fold(0.0, f32::max),
        _ => 0.0,
    }
}

fn minimum_width(tree: &TileTree, leaves: &[Leaf], rail: f32, minimum: f32, drawer: bool) -> f32 {
    if let Some(leaf) = leaf(tree, leaves) {
        return if leaf.collapsed && (!leaf.open || drawer) {
            rail
        } else if leaf.kind == Kind::Source {
            leaf.measure
        } else {
            minimum
        };
    }
    match tree {
        TileTree::Split {
            axis: SplitAxis::Row,
            children,
        } => {
            children
                .iter()
                .map(|b| minimum_width(&b.tree, leaves, rail, minimum, drawer))
                .sum::<f32>()
                + children.len().saturating_sub(1) as f32
        },
        TileTree::Split { children, .. } => children
            .iter()
            .map(|b| minimum_width(&b.tree, leaves, rail, minimum, drawer))
            .fold(0.0, f32::max),
        _ => 0.0,
    }
}

fn has_source(tree: &TileTree, leaves: &[Leaf]) -> bool {
    tree.tiles().iter().any(|tile| {
        leaves
            .iter()
            .any(|leaf| leaf.anchor == tile.id && leaf.kind == Kind::Source)
    })
}

#[allow(clippy::too_many_arguments)]
fn constrain(
    tree: &TileTree,
    leaves: &[Leaf],
    rail: f32,
    minimum: f32,
    drawer: bool,
    width: f32,
    path: &mut Vec<usize>,
    splits: &mut Vec<SplitPresentation>,
) {
    let TileTree::Split { axis, children } = tree else {
        return;
    };
    let mut widths = vec![width; children.len()];
    if *axis == SplitAxis::Row {
        let available = (width - children.len().saturating_sub(1) as f32).max(1.0);
        let total: f32 = children.iter().map(|b| b.fraction.max(0.0)).sum();
        let minimums: Vec<_> = children
            .iter()
            .map(|b| minimum_width(&b.tree, leaves, rail, minimum, drawer))
            .collect();
        widths = children
            .iter()
            .zip(&minimums)
            .map(|(b, min)| {
                let canonical = available * b.fraction.max(0.0) / total.max(f32::EPSILON);
                if let Some(leaf) = leaf(&b.tree, leaves)
                    && leaf.collapsed
                    && (!leaf.open || drawer)
                {
                    rail
                } else {
                    canonical.max(*min)
                }
            })
            .collect();
        let mut excess = widths.iter().sum::<f32>() - available;
        // Side readings narrow to the minimum before source measure is lost.
        for (index, child) in children.iter().enumerate() {
            if !has_source(&child.tree, leaves) {
                let reduction = excess
                    .max(0.0)
                    .min((widths[index] - minimums[index]).max(0.0));
                widths[index] -= reduction;
                excess -= reduction;
            }
        }
        if excess > 0.0 {
            let source_total: f32 = children
                .iter()
                .enumerate()
                .filter(|(_, b)| has_source(&b.tree, leaves))
                .map(|(i, _)| widths[i])
                .sum();
            for (index, child) in children.iter().enumerate() {
                if has_source(&child.tree, leaves) {
                    widths[index] =
                        (widths[index] - excess * widths[index] / source_total.max(1.0)).max(1.0);
                }
            }
        }
        let spare = available - widths.iter().sum::<f32>();
        if spare > 0.0 {
            let mut recipients: Vec<_> = children
                .iter()
                .enumerate()
                .filter(|(_, branch)| {
                    leaf(&branch.tree, leaves)
                        .is_some_and(|leaf| leaf.kind == Kind::Reading && !leaf.collapsed)
                })
                .map(|(i, _)| i)
                .collect();
            if recipients.is_empty() {
                recipients = children
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| has_source(&b.tree, leaves))
                    .map(|(i, _)| i)
                    .collect();
            }
            if !recipients.is_empty() {
                for index in &recipients {
                    widths[*index] += spare / recipients.len() as f32;
                }
            }
        }
        // Rails already occupy a fixed flex basis in the shared surface. Only
        // expanded branches participate in flex-grow. Including rail pixels in
        // their denominator would leave the grow sum below one; CSS then keeps
        // some free space unallocated and the source loses roughly another rail.
        let fixed: Vec<_> = children
            .iter()
            .map(|branch| {
                leaf(&branch.tree, leaves)
                    .is_some_and(|leaf| leaf.collapsed && (!leaf.open || drawer))
            })
            .collect();
        let sum = widths
            .iter()
            .zip(&fixed)
            .filter(|(_, fixed)| !**fixed)
            .map(|(width, _)| *width)
            .sum::<f32>()
            .max(1.0);
        let fractions: Vec<_> = widths
            .iter()
            .zip(&fixed)
            .map(|(width, fixed)| if *fixed { 0.0 } else { *width / sum })
            .collect();
        if fractions
            .iter()
            .zip(children)
            .any(|(f, b)| (*f - b.fraction).abs() > 0.0001)
        {
            splits.push(SplitPresentation {
                path: TilePath(path.clone()),
                fractions,
            });
        }
    }
    for (index, child) in children.iter().enumerate() {
        path.push(index);
        constrain(
            &child.tree,
            leaves,
            rail,
            minimum,
            drawer,
            widths[index],
            path,
            splits,
        );
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workbench::{ContentSource, Tile, TileBranch};
    fn tile(id: u64) -> Tile {
        Tile {
            id: TileId(id),
            title: format!("Tile {id}"),
            content: ContentSource::Open {
                kind: "test".into(),
                id: id.to_string(),
            },
            accent: None,
        }
    }
    fn tree() -> TileTree {
        TileTree::split(
            SplitAxis::Row,
            vec![
                TileBranch::new(0.25, TileTree::single(tile(1))),
                TileBranch::new(0.5, TileTree::single(tile(2))),
                TileBranch::new(0.25, TileTree::single(tile(3))),
            ],
        )
    }
    fn project(width: f32, measure: f32) -> WorkbenchPresentation {
        presentation(
            &tree(),
            &CollapsePreferences::default(),
            (width, 700.0),
            (0.0, 40.0, width, 600.0),
            &WorkbenchPresentation::default(),
            Some(TileId(2)),
            |id| {
                Some(match id.0 {
                    1 => TileRole::Navigator,
                    2 => TileRole::Document(crate::documents::DocKey(1)),
                    _ => TileRole::Reading {
                        kind: crate::documents::ReadingKind::Preview,
                        pinned: None,
                    },
                })
            },
            |id| if id.0 == 2 { measure } else { 0.0 },
        )
    }
    #[test]
    fn navigation_gives_way_reading_narrows_then_folds_and_ratios_restore() {
        let at_1100 = project(1100.0, 786.0);
        assert!(at_1100.stacks[0].collapsed.is_some());
        assert!(at_1100.stacks[2].collapsed.is_none());
        let fractions = &at_1100.splits[0].fractions;
        assert!((fractions[1] * 1070.0 - 786.0).abs() < 0.1);
        assert!((fractions[2] * 1070.0 - 284.0).abs() < 0.1);
        assert_eq!(fractions[0], 0.0);
        let narrow = project(640.0, 786.0);
        assert!(narrow.stacks[0].collapsed.is_some());
        assert!(narrow.stacks[2].collapsed.is_some());
        assert!(
            narrow.stacks[2]
                .collapsed
                .as_ref()
                .unwrap()
                .drawer
                .is_none()
        );
        let drawer = project(320.0, 786.0);
        assert!(
            drawer.stacks[2]
                .collapsed
                .as_ref()
                .unwrap()
                .drawer
                .is_some()
        );
        assert!(project(1800.0, 786.0).splits.is_empty());
        assert!(
            project(1800.0, 786.0)
                .stacks
                .iter()
                .all(|stack| stack.collapsed.is_none())
        );
    }
    #[test]
    fn plex_measure_uses_real_advance_and_current_frame_insets() {
        let appearance = Appearance::default();
        assert!((source_width(&appearance, "", None) - (72.0 * 9.6 + 58.0)).abs() < 0.01);
        let mut system = appearance;
        system.source_face = SourceFace::SystemMonospace;
        assert!((source_width(&system, "", Some(10.0)) - 778.0).abs() < 0.01);
    }

    #[test]
    fn system_measure_uses_host_ch_and_documents_the_pre_layout_fallback() {
        let appearance = Appearance {
            source_face: SourceFace::SystemMonospace,
            ..Appearance::default()
        };
        assert!((source_width(&appearance, "", Some(8.0)) - 634.0).abs() < 0.01);
        assert!((source_width(&appearance, "", None) - 806.8).abs() < 0.01);
        assert_eq!(
            source_width(&appearance, "", Some(f32::NAN)),
            source_width(&appearance, "", None)
        );
    }

    #[test]
    fn full_measure_keeps_ratios_until_the_configured_drawer_threshold() {
        let appearance = Appearance {
            measure: crate::appearance::Measure::Full,
            follow_hard_wrap: false,
            ..Appearance::default()
        };
        assert_eq!(source_width(&appearance, "source", None), 0.0);
        let inline = project(640.0, 0.0);
        assert!(inline.splits.is_empty());
        assert!(inline.stacks.iter().all(|stack| stack.collapsed.is_none()));
        let narrow = project(320.0, 0.0);
        assert!(
            narrow.stacks[0]
                .collapsed
                .as_ref()
                .unwrap()
                .drawer
                .is_some()
        );
        assert!(
            narrow.stacks[2]
                .collapsed
                .as_ref()
                .unwrap()
                .drawer
                .is_some()
        );
    }
}
