// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use scenomise::projection::{ProjectionOccurrence, ProjectionValue};

fn fixture_material() -> ProjectionRecipeMaterial {
    let (source, reading) = super::super::tests::fixture();
    super::super::material(
        &source,
        &reading,
        &knot_readings::sound::SoundLayers {
            perfect_rhyme: true,
            ..Default::default()
        },
    )
    .unwrap()
}

fn with_spring_slot() -> ProjectionRecipeMaterial {
    let mut material = fixture_material();
    material.snapshot.recipe.definition.dynamics =
        Some(cartography::canvas::dynamics_recipe::preset_slot("spring.rapier").unwrap());
    material
}

#[test]
fn real_sound_consumer_preserves_occurrences_and_disclosed_edges_change_motion() {
    let mut material = with_spring_slot();
    let occurrences = &material.dataset.dataset.occurrences;
    let pinned = occurrences[0].occurrence_id.clone();
    let moving = occurrences[1].occurrence_id.clone();
    assert_eq!(occurrences[0].source, occurrences[1].source);
    let presentation = material.presentation.get_or_insert_with(Default::default);
    presentation
        .motion_roles
        .insert(pinned.clone(), seiche::Role::Pinned);
    presentation
        .motion_roles
        .insert(moving.clone(), seiche::Role::Seeded);
    presentation.refresh_version();
    material.validate().unwrap();
    let original = material.clone();
    let mut linked = RuntimeState::default();
    linked.start(&material, VALIDATION_CARD).unwrap();
    let initial = linked.positions().clone();

    // The authored Knot recipe requires explained relationships. Remove
    // only compiled edge input at the shared binding seam as the negative
    // control, without weakening that source/recipe acceptance contract.
    let mut bare = validation_compiler()
        .compile_relationship_snapshot(&material.snapshot, &material.dataset)
        .unwrap();
    bare.relationships.clear();
    bare.projection.scene.relations.clear();
    let slot = material
        .snapshot
        .recipe
        .definition
        .dynamics
        .as_ref()
        .unwrap();
    let mut control = RuntimeState::default();
    control.bind_compiled(&material, bare, slot).unwrap();
    let mut repeated = RuntimeState::default();
    repeated.start(&material, VALIDATION_CARD).unwrap();
    assert_eq!(linked.positions(), control.positions());
    assert_eq!(
        linked.positions().len(),
        material.dataset.dataset.occurrences.len()
    );
    for _ in 0..12 {
        linked.tick_once().unwrap();
        control.tick_once().unwrap();
        repeated.tick_once().unwrap();
    }
    assert_eq!(linked.positions()[&pinned], initial[&pinned]);
    assert_ne!(linked.positions()[&moving], initial[&moving]);
    assert_ne!(
        linked.controller.as_ref().unwrap().scene().items,
        control.controller.as_ref().unwrap().scene().items
    );
    assert_eq!(
        linked.controller.as_ref().unwrap().scene(),
        repeated.controller.as_ref().unwrap().scene()
    );
    assert_eq!(
        linked.controller.as_ref().unwrap().relationship_ids().len(),
        material.dataset.relationships.len()
    );
    assert_eq!(
        material, original,
        "motion cannot rewrite copied source or authoring"
    );
}

#[test]
fn keeping_a_paused_pose_is_one_undoable_edit_and_running_motion_cannot_be_kept() {
    let mut material = with_spring_slot();
    material.snapshot.recipe.definition.dynamics = Some(bounded_spring_slot().unwrap());
    material
        .presentation
        .get_or_insert_with(Default::default)
        .overview_visible = true;
    let mut state = DesktopState::with_path(
        knot_document::KnotDocumentSession::scratch("scratch:keep", "night night light"),
        cambium_genet_winit_host::WindowCommands::new(),
        None,
    );
    super::super::reopen(&mut state, material.clone());
    keep_placement(&mut state);
    assert!(!state.composition.recipe.history.can_undo());
    state
        .composition
        .recipe
        .dynamics
        .start(&material, VALIDATION_CARD)
        .unwrap();
    for _ in 0..12 {
        state.composition.recipe.dynamics.tick_once().unwrap();
    }
    assert!(state.composition.recipe.dynamics.running());
    keep_placement(&mut state);
    assert_eq!(state.composition.recipe.material.as_ref(), Some(&material));
    assert!(!state.composition.recipe.history.can_undo());
    state.composition.recipe.dynamics.pause();
    let pose = state.composition.recipe.dynamics.positions().clone();
    keep_placement(&mut state);
    let kept = state.composition.recipe.material.clone().unwrap();
    assert_eq!(kept.scene_positions().unwrap(), pose);
    assert_eq!(kept.dataset, material.dataset);
    assert_eq!(kept.anchors, material.anchors);
    assert_eq!(kept.snapshot, material.snapshot);
    assert!(state.composition.recipe.history.is_dirty());
    assert!(state.composition.recipe.dynamics.positions().is_empty());
    assert_eq!(state.composition.recipe.dynamics.ticks(), 0);
    super::super::step_history(&mut state, false);
    assert_eq!(state.composition.recipe.material.as_ref(), Some(&material));
    assert!(
        !state.composition.recipe.history.can_undo(),
        "one Undo restores the entire pose"
    );
    super::super::step_history(&mut state, true);
    assert_eq!(state.composition.recipe.material.as_ref(), Some(&kept));
    super::super::reopen(&mut state, material.clone());
    state
        .composition
        .recipe
        .dynamics
        .start(&material, VALIDATION_CARD)
        .unwrap();
    for _ in 0..121 {
        if !state.composition.recipe.dynamics.tick_once().unwrap() {
            break;
        }
    }
    assert!(!state.composition.recipe.dynamics.running());
    let finished = state.composition.recipe.dynamics.positions().clone();
    keep_placement(&mut state);
    assert_eq!(
        state
            .composition
            .recipe
            .material
            .as_ref()
            .unwrap()
            .scene_positions()
            .unwrap(),
        finished
    );
    super::super::step_history(&mut state, false);
    assert_eq!(state.composition.recipe.material.as_ref(), Some(&material));
    assert_eq!(state.document().snapshot().text, "night night light");
    assert!(!state.document().snapshot().dirty);
}

#[test]
fn a_failed_tick_preserves_the_last_valid_pose_and_play_rebinds_from_authored() {
    let material = with_spring_slot();
    let original = material.clone();
    let mut runtime = RuntimeState::default();
    runtime.start(&material, VALIDATION_CARD).unwrap();
    let initial = runtime.positions().clone();
    assert!(runtime.advance_frame(true));
    let last_valid = runtime.positions().clone();
    // Lose a required host mapping after binding. The supplier may advance,
    // but an unvalidated pose must never replace the displayed snapshot.
    runtime.plane = None;
    assert!(!runtime.advance_frame(true));
    assert_eq!(runtime.status_key(), "refused");
    assert!(
        runtime
            .refusal()
            .unwrap()
            .contains("no fixed placement plane")
    );
    assert_eq!(runtime.ticks(), 1);
    assert_eq!(runtime.positions(), &last_valid);
    assert!(!runtime.can_resume());
    assert!(!runtime.can_step());
    runtime.step_once(&material, VALIDATION_CARD).unwrap();
    assert!(!runtime.advance_frame(true));
    assert_eq!(runtime.ticks(), 1);
    assert_eq!(runtime.positions(), &last_valid);
    runtime.start(&material, VALIDATION_CARD).unwrap();
    assert!(runtime.refusal().is_none());
    assert_eq!(runtime.status_key(), "running");
    assert_eq!(runtime.ticks(), 0);
    assert_eq!(runtime.positions(), &initial);
    assert!(runtime.advance_frame(true));
    assert_eq!(runtime.ticks(), 1);
    assert_eq!(material, original);
}

#[test]
fn pinned_preview_reports_the_shared_rest_result_without_reseeding() {
    let mut material = with_spring_slot();
    material
        .presentation
        .get_or_insert_with(Default::default)
        .motion_roles = material
        .dataset
        .dataset
        .occurrences
        .iter()
        .map(|occurrence| (occurrence.occurrence_id.clone(), seiche::Role::Pinned))
        .collect();
    material.presentation.as_mut().unwrap().refresh_version();
    let mut runtime = RuntimeState::default();
    runtime.start(&material, VALIDATION_CARD).unwrap();
    let initial = runtime.positions().clone();
    for _ in 0..240 {
        if !runtime.advance_frame(true) {
            break;
        }
    }
    assert_eq!(
        runtime.status(),
        PlaybackStatus::Finished(seiche::SettleEnd::Rested)
    );
    assert_eq!(runtime.status_key(), "rested");
    assert!(runtime.status_label(true).starts_with("Motion at rest"));
    assert_eq!(runtime.positions(), &initial);
    let ticks = runtime.ticks();
    runtime.step_once(&material, VALIDATION_CARD).unwrap();
    assert_eq!(runtime.ticks(), ticks);
    assert_eq!(runtime.positions(), &initial);
}

#[test]
fn fixed_plane_inversion_does_not_follow_moving_scene_bounds() {
    let plane = FixedPlane {
        origin: [-10.0, 20.0],
        size: [100.0, 80.0],
    };
    assert_eq!(plane.scene_point([0.25, 0.75]).unwrap(), [15.0, 80.0]);
    assert_eq!(plane.normalized([15.0, 80.0]).unwrap(), [0.25, 0.75]);
    assert_eq!(plane.normalized([-50.0, 200.0]).unwrap(), [0.0, 1.0]);
}

#[test]
fn authored_positions_reject_nonfinite_or_out_of_plane_values() {
    let plane = FixedPlane {
        origin: [0.0, 0.0],
        size: [10.0, 10.0],
    };
    assert!(plane.scene_point([f32::NAN, 0.5]).is_err());
    assert!(plane.scene_point([1.1, 0.5]).is_err());
}

#[test]
fn cold_runtime_does_not_advance_and_hidden_motion_stays_paused() {
    let mut runtime = RuntimeState::default();
    assert!(!runtime.running());
    assert_eq!(runtime.ticks(), 0);
    assert!(runtime.positions().is_empty());
    assert!(!runtime.advance_frame(true));

    runtime.running = true;
    runtime.positions.insert("occurrence-a".into(), [0.4, 0.6]);
    assert!(!runtime.advance_frame(false));
    assert!(!runtime.running());
    assert_eq!(runtime.positions()["occurrence-a"], [0.4, 0.6]);
    assert!(!runtime.advance_frame(true));
    assert_eq!(runtime.ticks(), 0);
}

#[test]
fn frame_advances_one_fixed_tick_without_mutating_retained_material() {
    let material = with_spring_slot();
    let original = material.clone();
    let mut runtime = RuntimeState::default();
    runtime.start(&material, VALIDATION_CARD).unwrap();
    assert_eq!(runtime.ticks(), 0);

    assert!(runtime.advance_frame(true));
    assert_eq!(runtime.ticks(), 1);
    assert_eq!(material, original);

    runtime.pause();
    assert!(!runtime.advance_frame(true));
    assert_eq!(runtime.ticks(), 1);
}

#[test]
fn single_step_advances_exactly_once_and_never_starts_continuous_playback() {
    let material = with_spring_slot();
    let mut runtime = RuntimeState::default();
    runtime.step_once(&material, VALIDATION_CARD).unwrap();
    assert_eq!(runtime.ticks(), 1);
    assert!(!runtime.running());
    assert!(!runtime.advance_frame(true));
    assert_eq!(runtime.ticks(), 1);

    // A stale callback while already running is harmless: it neither
    // converts the playback to a one-step pause nor adds an extra tick.
    runtime.start(&material, VALIDATION_CARD).unwrap();
    assert!(runtime.running());
    runtime.step_once(&material, VALIDATION_CARD).unwrap();
    assert!(runtime.running());
    assert_eq!(runtime.ticks(), 1);
}

#[test]
fn hidden_frame_freezes_real_poses_and_reopening_does_not_resume() {
    let material = with_spring_slot();
    let mut runtime = RuntimeState::default();
    runtime.start(&material, VALIDATION_CARD).unwrap();
    assert!(runtime.advance_frame(true));
    let poses = runtime.positions().clone();
    let ticks = runtime.ticks();

    assert!(!runtime.advance_frame(false));
    assert!(!runtime.running());
    assert_eq!(runtime.positions(), &poses);
    assert!(!runtime.advance_frame(true));
    assert_eq!(runtime.ticks(), ticks);
    assert_eq!(runtime.positions(), &poses);
}

#[test]
fn unknown_shared_channel_refuses_atomically_without_partial_controller() {
    let mut material = with_spring_slot();
    let mut spec: serde_json::Value = serde_json::from_str(
        &material
            .snapshot
            .recipe
            .definition
            .dynamics
            .as_ref()
            .unwrap()
            .spec,
    )
    .unwrap();
    spec["channels"] = serde_json::json!({"mass":"mass.pagerank"});
    material.snapshot.recipe.definition.dynamics =
        Some(DynamicsSlot::from_json(1, &spec.to_string()).unwrap());
    let original = material.clone();
    let mut runtime = RuntimeState::default();

    assert!(runtime.start(&material, VALIDATION_CARD).is_err());
    assert!(runtime.controller.is_none());
    assert!(runtime.positions().is_empty());
    assert_eq!(runtime.ticks(), 0);
    assert_eq!(material, original);
}

#[test]
fn authored_pinned_center_is_preserved_while_another_occurrence_moves() {
    let mut material = with_spring_slot();
    let pinned = material.dataset.dataset.occurrences[0]
        .occurrence_id
        .clone();
    let moving = material.dataset.dataset.occurrences[2]
        .occurrence_id
        .clone();
    let target = [0.05, 0.2];
    let mut presentation = knot_composition::retention::RecipeScenePresentation {
        overview_visible: true,
        foreground_occurrences: BTreeMap::from([(pinned.clone(), ()), (moving.clone(), ())])
            .into_keys()
            .collect(),
        ..Default::default()
    };
    material.presentation = Some(presentation.clone());
    material.move_scene_occurrence(&pinned, target).unwrap();
    presentation = material.presentation.take().unwrap();
    presentation
        .motion_roles
        .insert(pinned.clone(), seiche::Role::Pinned);
    presentation.refresh_version();
    material.presentation = Some(presentation);

    let mut runtime = RuntimeState::default();
    runtime.start(&material, VALIDATION_CARD).unwrap();
    let initial = runtime.positions().clone();
    assert!((initial[&pinned][0] - target[0]).abs() < 1e-5);
    assert!((initial[&pinned][1] - target[1]).abs() < 1e-5);

    runtime.pause();
    for _ in 0..12 {
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
    }
    assert_eq!(runtime.ticks(), 12);
    let final_positions = runtime.positions();
    assert!((final_positions[&pinned][0] - target[0]).abs() < 1e-5);
    assert!((final_positions[&pinned][1] - target[1]).abs() < 1e-5);
    assert_ne!(final_positions[&moving], initial[&moving]);
}

#[test]
fn stepping_a_completed_crowded_pinned_run_preserves_its_final_pose() {
    let mut material = with_spring_slot();
    let pinned = material.dataset.dataset.occurrences[0]
        .occurrence_id
        .clone();
    let neighbor = material.dataset.dataset.occurrences[2]
        .occurrence_id
        .clone();
    let target = [0.72, 0.34];
    let mut presentation = knot_composition::retention::RecipeScenePresentation {
        overview_visible: true,
        foreground_occurrences: [pinned.clone(), neighbor.clone()].into(),
        ..Default::default()
    };
    material.presentation = Some(presentation.clone());
    material.move_scene_occurrence(&pinned, target).unwrap();
    presentation = material.presentation.take().unwrap();
    presentation
        .motion_roles
        .insert(pinned.clone(), seiche::Role::Pinned);
    presentation.refresh_version();
    material.presentation = Some(presentation);

    let mut runtime = RuntimeState::default();
    runtime.step_once(&material, VALIDATION_CARD).unwrap();
    for _ in 0..128 {
        if runtime
            .controller
            .as_ref()
            .is_some_and(|controller| !controller.is_running())
        {
            break;
        }
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
    }
    assert!(
        runtime
            .controller
            .as_ref()
            .is_some_and(|controller| !controller.is_running())
    );

    let final_ticks = runtime.ticks();
    let final_positions = runtime.positions().clone();
    for _ in 0..3 {
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
        assert_eq!(runtime.ticks(), final_ticks);
        assert_eq!(runtime.positions(), &final_positions);
    }
}

#[test]
fn bounded_preview_uses_the_shared_frame_schedule_and_preserves_its_slot() {
    let mut material = with_spring_slot();
    let slot = bounded_spring_slot().unwrap();
    material.snapshot.recipe.definition.dynamics = Some(slot.clone());
    let original = material.clone();
    let mut runtime = RuntimeState::default();
    for tick in 1..=120 {
        runtime.step_once(&material, VALIDATION_CARD).unwrap();
        assert_eq!(runtime.ticks(), tick);
        assert_eq!(runtime.can_resume(), tick < 120);
    }
    let positions = runtime.positions().clone();
    runtime.step_once(&material, VALIDATION_CARD).unwrap();
    assert_eq!(runtime.ticks(), 120);
    assert_eq!(runtime.positions(), &positions);
    assert_eq!(material, original);
    assert_eq!(
        material.snapshot.recipe.definition.dynamics.as_ref(),
        Some(&slot)
    );
}

#[test]
fn shared_slot_and_roles_roundtrip_as_durable_material_not_runtime_state() {
    let mut material = with_spring_slot();
    let occurrence = material.dataset.dataset.occurrences[0]
        .occurrence_id
        .clone();
    let presentation = material.presentation.get_or_insert_with(Default::default);
    presentation
        .motion_roles
        .insert(occurrence.clone(), seiche::Role::Pinned);
    presentation.refresh_version();
    let runtime = RuntimeState::default();
    let json = serde_json::to_vec(&material).unwrap();
    let decoded: ProjectionRecipeMaterial = serde_json::from_slice(&json).unwrap();

    assert_eq!(decoded, material);
    assert_eq!(
        decoded.presentation.as_ref().unwrap().motion_roles[&occurrence],
        seiche::Role::Pinned
    );
    assert!(runtime.positions().is_empty());
    assert_eq!(runtime.ticks(), 0);
}

#[test]
fn maximum_occurrence_scene_is_deterministic_and_ticks_only_when_presented() {
    let mut material = with_spring_slot();
    let template = material.dataset.dataset.occurrences[0].clone();
    let anchor = material.anchors[0].clone();
    while material.dataset.dataset.occurrences.len() < 256 {
        let index = material.dataset.dataset.occurrences.len();
        let id = format!("fixture-occurrence-{index}");
        let mut occurrence: ProjectionOccurrence = template.clone();
        occurrence.occurrence_id = id.clone();
        occurrence
            .values
            .insert("occurrence_id".into(), ProjectionValue::Text(id.clone()));
        occurrence
            .values
            .insert("label".into(), ProjectionValue::Text(id.clone()));
        occurrence
            .values
            .insert("order".into(), ProjectionValue::Number(index as f64));
        material.dataset.dataset.occurrences.push(occurrence);
        let mut source_anchor = anchor.clone();
        source_anchor.node_id = id;
        material.anchors.push(source_anchor);
    }
    material.validate().unwrap();
    assert_eq!(material.dataset.dataset.occurrences.len(), 256);

    let mut first = RuntimeState::default();
    let mut second = RuntimeState::default();
    first.start(&material, VALIDATION_CARD).unwrap();
    second.start(&material, VALIDATION_CARD).unwrap();
    assert_eq!(first.positions().len(), 256);
    assert_eq!(first.positions(), second.positions());
    assert!(!first.advance_frame(false));
    assert_eq!(first.ticks(), 0);
    assert!(!first.advance_frame(true));
    first.start(&material, VALIDATION_CARD).unwrap();
    assert!(first.advance_frame(true));
    assert!(second.advance_frame(true));
    assert_eq!(first.ticks(), 1);
    assert_eq!(second.ticks(), 1);
    assert_eq!(first.positions(), second.positions());
}
