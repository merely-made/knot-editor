// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Arrangement controls backed by Scenograph's shared option declarations.

use super::RecipeState;
use crate::workspace::{DesktopState, DesktopView};
use cambium::{Keyed, TextInput, button, el, lens, select, span, text_field_typed};
use sceno::Size2;
use scenograph::options::{OptionDefault, OptionKind, OptionSpec};
use scenomise::catalog::{FAMILIES, Family, Measure};
use std::collections::BTreeMap;

const FAMILY_LABELS: [&str; 11] = [
    "Spiral",
    "Grid",
    "Geographic",
    "Hulls",
    "Stack",
    "Penrose",
    "L-system",
    "Timeline",
    "Kanban",
    "Embedded",
    "Radial",
];

/// Clear only session UI drafts and point the family picker at the accepted
/// arrangement. Call after a new/rebound/reopened recipe or a history move.
pub(super) fn reset_controls(recipe: &mut RecipeState) {
    recipe.editor_option_drafts.clear();
    let arrangement_kind = recipe.material.as_ref().map(|material| {
        material
            .snapshot
            .recipe
            .definition
            .arrangement
            .kind
            .as_str()
    });
    recipe.editor_family_select.selected = arrangement_kind
        .and_then(Family::resolve)
        .and_then(|family| FAMILIES.iter().position(|candidate| candidate == &family))
        .unwrap_or(0);
    recipe.editor_family_select.open = false;
}

fn measure(state: &DesktopState) -> Measure {
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return Measure {
            largest: Size2::new(1.0, 1.0),
            count: 1,
            spacing: 16.0,
            coordinates: None,
        };
    };
    let card = state
        .composition
        .recipe
        .measured
        .as_ref()
        .map_or(knot_composition::retention::VALIDATION_CARD, |measured| {
            measured.card
        });
    Measure {
        largest: card,
        count: material.dataset.dataset.occurrences.len(),
        spacing: material.snapshot.recipe.definition.arrangement.spacing as f32,
        coordinates: None,
    }
}

fn option_default(spec: &OptionSpec, resolved: &BTreeMap<String, String>) -> String {
    resolved
        .get(&spec.key)
        .cloned()
        .unwrap_or_else(|| match &spec.default {
            OptionDefault::Value(value) => value.clone(),
            OptionDefault::Measured(words) => words.clone(),
            OptionDefault::Auto => "automatic".into(),
            OptionDefault::Empty => String::new(),
        })
}

fn declared_options(family: Family, measure: &Measure) -> Vec<(OptionSpec, String)> {
    let resolved = family.clone().resolved_defaults(measure);
    family
        .options()
        .into_iter()
        .map(|spec| {
            let default = option_default(&spec, &resolved);
            (spec, default)
        })
        .collect()
}

fn default_description(spec: &OptionSpec) -> Option<String> {
    match &spec.default {
        OptionDefault::Value(value) => Some(format!("Default: {value}")),
        OptionDefault::Measured(words) => Some(format!("Measured default: {words}")),
        OptionDefault::Auto => Some("Default: automatic".into()),
        OptionDefault::Empty => Some("Default: empty".into()),
    }
}

fn apply_family(state: &mut DesktopState, family: Family) {
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return;
    };
    let mut arrangement = material.snapshot.recipe.definition.arrangement.clone();
    arrangement.kind = family.id().to_owned();
    arrangement.options.clear();
    super::commit_arrangement(state, arrangement);
}

fn apply_option(state: &mut DesktopState, family: Family, key: &str) {
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return;
    };
    if Family::resolve(&material.snapshot.recipe.definition.arrangement.kind)
        != Some(family.clone())
    {
        state.composition.notice =
            Some("Apply this arrangement family before editing its options.".into());
        return;
    }
    let specs = family.options();
    if !specs.iter().any(|spec| spec.key == key) {
        return;
    }
    let draft_key = option_draft_key(&family, key);
    let value = state
        .composition
        .recipe
        .editor_option_drafts
        .get(&draft_key)
        .map(|input| input.text().to_owned())
        .filter(|value| !value.is_empty());
    let mut arrangement = material.snapshot.recipe.definition.arrangement.clone();
    if let Some(value) = value {
        arrangement.options.insert(key.to_owned(), value);
    } else {
        arrangement.options.remove(key);
    }
    super::commit_arrangement(state, arrangement);
}

fn option_draft_key(family: &Family, key: &str) -> String {
    format!("{}:{key}", family.id())
}

fn option_row(
    state: &DesktopState,
    family: Family,
    spec: &OptionSpec,
    default: &str,
) -> DesktopView {
    let key = spec.key.clone();
    let label = spec.label.clone();
    let initial = state
        .composition
        .recipe
        .material
        .as_ref()
        .and_then(|material| {
            (Family::resolve(&material.snapshot.recipe.definition.arrangement.kind)
                == Some(family.clone()))
            .then(|| {
                material
                    .snapshot
                    .recipe
                    .definition
                    .arrangement
                    .options
                    .get(&key)
                    .cloned()
            })
            .flatten()
        })
        .unwrap_or_default();
    let input_draft_key = option_draft_key(&family, &key);
    let input_id = format!("knot-recipe-option-input-{}", spec.key);
    let mut children: Vec<DesktopView> = vec![Box::new(el(
        "label",
        (
            span(label.clone()),
            el(
                "div",
                lens(
                    move |input: &mut TextInput| {
                        text_field_typed(input)
                            .attr("aria-label", label.clone())
                            .attr("id", input_id.clone())
                            .attr("style", format!("{}box-sizing:border-box;max-width:100%;width:18em;min-height:1.2em;", cambium::SINGLE_LINE_FIELD_STYLE))
                    },
                    move |state: &mut DesktopState| {
                        state
                            .composition
                            .recipe
                            .editor_option_drafts
                            .entry(input_draft_key.clone())
                            .or_insert_with(|| TextInput::new(initial.clone()))
                    },
                ),
            )
            .attr("style", "max-width:100%;"),
        ),
    )) as DesktopView];
    if let Some(description) = default_description(spec) {
        children.push(
            Box::new(span(format!("Resolved default: {default} · {description}"))) as DesktopView,
        );
    }
    if let Some(value) = state
        .composition
        .recipe
        .editor_option_drafts
        .get(&option_draft_key(&family, &spec.key))
        .map(|input| input.text())
        .filter(|value| !value.is_empty())
        && let Some(reason) = spec.kind.refusal(value)
    {
        children.push(Box::new(span(format!(
            "Draft {reason}; Apply to validate this option."
        ))) as DesktopView);
    }
    if let OptionKind::Choice { names } = &spec.kind {
        let choice_draft_key = option_draft_key(&family, &spec.key);
        let selected = state
            .composition
            .recipe
            .editor_option_drafts
            .get(&choice_draft_key)
            .map(|input| input.text())
            .or_else(|| {
                state
                    .composition
                    .recipe
                    .material
                    .as_ref()
                    .and_then(|material| {
                        material
                            .snapshot
                            .recipe
                            .definition
                            .arrangement
                            .options
                            .get(&spec.key)
                            .map(String::as_str)
                    })
            })
            .unwrap_or(default);
        let buttons: Vec<(String, DesktopView)> = names
            .iter()
            .map(|name| {
                let target = name.clone();
                let choice_key = choice_draft_key.clone();
                (
                    name.clone(),
                    Box::new(
                        button(name.clone(), move |state: &mut DesktopState, _| {
                            state
                                .composition
                                .recipe
                                .editor_option_drafts
                                .insert(choice_key.clone(), TextInput::new(target.clone()));
                        })
                        .attr("aria-pressed", (selected == name.as_str()).to_string()),
                    ) as DesktopView,
                )
            })
            .collect();
        children.push(Box::new(el("div", Keyed::new(buttons))) as DesktopView);
    }
    let option_family = family.clone();
    children.push(Box::new(
        button(
            format!("Apply recipe option: {}", spec.key),
            move |state: &mut DesktopState, _| apply_option(state, option_family.clone(), &key),
        )
        .attr("id", format!("knot-recipe-option-{}", spec.key))
        .attr("style", "max-width:100%;"),
    ) as DesktopView);
    let default_key = spec.key.clone();
    let default_family = family.clone();
    children.push(Box::new(
        button(
            format!("Use default for recipe option: {}", spec.key),
            move |state: &mut DesktopState, _| {
                apply_default(state, default_family.clone(), &default_key)
            },
        )
        .attr("id", format!("knot-recipe-option-default-{}", spec.key)),
    ) as DesktopView);
    Box::new(el(
        "div",
        el("div", children).attr(
            "style",
            "display:flex;flex-wrap:wrap;align-items:center;gap:6px;max-width:100%;",
        ),
    ))
}

fn apply_default(state: &mut DesktopState, family: Family, key: &str) {
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return;
    };
    if Family::resolve(&material.snapshot.recipe.definition.arrangement.kind)
        != Some(family.clone())
    {
        state.composition.notice =
            Some("Apply this arrangement family before editing its options.".into());
        return;
    }
    let mut arrangement = material.snapshot.recipe.definition.arrangement.clone();
    arrangement.options.remove(key);
    super::commit_arrangement(state, arrangement);
}

pub(super) fn view(state: &DesktopState) -> DesktopView {
    let Some(material) = state.composition.recipe.material.as_ref() else {
        return Box::new(el(
            "section",
            span("Arrangement controls are available after a recipe is loaded."),
        ));
    };
    let current_kind = material.snapshot.recipe.definition.arrangement.kind.clone();
    let accepted_family = Family::resolve(&current_kind);
    let selected_index = state.composition.recipe.editor_family_select.selected;
    let selected_family = FAMILIES
        .get(selected_index)
        .cloned()
        .unwrap_or_else(|| accepted_family.clone().unwrap_or(Family::Grid));
    let family_picker = lens(
        |selection: &mut cambium::SelectState| select(selection, &FAMILY_LABELS),
        |state: &mut DesktopState| &mut state.composition.recipe.editor_family_select,
    );
    let toggle = Box::new(
        button(
            if state.composition.recipe.editor_controls_expanded {
                "Hide recipe arrangement controls"
            } else {
                "Show recipe arrangement controls"
            },
            |state: &mut DesktopState, _| {
                state.composition.recipe.editor_controls_expanded =
                    !state.composition.recipe.editor_controls_expanded;
            },
        )
        .attr("id", "knot-recipe-arrangement-toggle")
        .attr("aria-controls", "knot-recipe-arrangement-controls")
        .attr(
            "aria-expanded",
            state
                .composition
                .recipe
                .editor_controls_expanded
                .to_string(),
        ),
    ) as DesktopView;
    let apply_family_target = selected_family.clone();
    let apply_family_button = Box::new(
        button(
            "Apply recipe arrangement",
            move |state: &mut DesktopState, _| apply_family(state, apply_family_target.clone()),
        )
        .attr("id", "knot-recipe-arrangement-apply"),
    ) as DesktopView;
    let family_mismatch = accepted_family != Some(selected_family.clone());
    let expanded_controls: DesktopView = if state.composition.recipe.editor_controls_expanded {
        let rows: Vec<(String, DesktopView)> =
            declared_options(selected_family.clone(), &measure(state))
                .into_iter()
                .map(|(spec, default)| {
                    let row = option_row(state, selected_family.clone(), &spec, &default);
                    (spec.key, row)
                })
                .collect();
        Box::new(el(
            "div",
            (
                el("div", (family_picker, apply_family_button)).attr(
                    "style",
                    "display:flex;flex-wrap:wrap;align-items:center;gap:8px;max-width:100%;",
                ),
                family_mismatch
                    .then(|| span("Apply the selected family before applying option values.")),
                Keyed::new(rows),
            ),
        )) as DesktopView
    } else {
        Box::new(el("div", ())) as DesktopView
    };
    let controls = Box::new(
        el("div", expanded_controls)
            .attr("id", "knot-recipe-arrangement-controls")
            .attr(
                "style",
                if state.composition.recipe.editor_controls_expanded {
                    "display:flex;flex-direction:column;gap:8px;max-width:100%;"
                } else {
                    "display:none;"
                },
            ),
    ) as DesktopView;
    Box::new(el(
        "section",
        (
            el("h5", "Arrangement"),
            span(format!("Accepted arrangement: {current_kind}")),
            toggle,
            controls,
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_measure() -> Measure {
        Measure {
            largest: Size2::new(120.0, 48.0),
            count: 4,
            spacing: 16.0,
            coordinates: None,
        }
    }

    #[test]
    fn editor_rows_come_from_every_bounded_catalog_family() {
        assert_eq!(FAMILIES.len(), 11);
        for (family, label) in FAMILIES.iter().zip(FAMILY_LABELS) {
            assert_eq!(family.clone().capability().display_name, label);
            let family = family.clone();
            let rows = declared_options(family.clone(), &test_measure());
            assert_eq!(rows.len(), family.options().len());
            assert!(rows.iter().all(|(spec, _)| !spec.label.is_empty()));
        }
    }

    #[test]
    fn measured_defaults_use_the_current_card_measure() {
        let grid = declared_options(Family::Grid, &test_measure());
        let value = grid
            .iter()
            .find(|(spec, _)| spec.key == "cell_width")
            .map(|(_, value)| value.as_str());
        assert_eq!(value, Some("120"));
    }

    #[test]
    fn shared_declaration_explains_invalid_drafts() {
        let spec = Family::Grid
            .options()
            .into_iter()
            .find(|spec| spec.key == "columns")
            .unwrap();
        assert_eq!(
            spec.kind.refusal("0").as_deref(),
            Some("needs a positive whole number")
        );
        assert_eq!(spec.kind.refusal("2"), None);
    }

    #[test]
    fn choice_rows_preserve_catalog_choice_order() {
        let spec = Family::Spiral
            .options()
            .into_iter()
            .find(|spec| spec.key == "curve")
            .unwrap();
        assert!(matches!(
            spec.kind,
            OptionKind::Choice { ref names } if names == &["square_root", "linear", "quadratic", "logarithmic"]
        ));
    }
}
