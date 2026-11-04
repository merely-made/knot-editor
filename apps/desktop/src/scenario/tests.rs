// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn graph_capture_requires_active_content_and_an_open_collapsed_stack() {
    use workbench::{CollapsedStack, DrawerGeometry, StackPresentation};
    let mut state = DesktopState::new(
        knot_document::KnotDocumentSession::scratch("scratch:graph-capture", "# Source"),
        cambium_genet_winit_host::WindowCommands::new(),
    );
    let document = state.docs.tile_of(state.docs.focused().unwrap()).unwrap();
    let graph = state.docs.open_graph("Graph");
    let canonical = state.docs.workspace().tiled().clone();
    assert!(graph_is_presented(&state), "active Graph is presented");
    state.docs.activate(document);
    assert!(
        state.docs.graph().is_some(),
        "the background Graph remains retained"
    );
    assert!(
        !graph_is_presented(&state),
        "a background Graph is not presented"
    );
    state.docs.activate(graph);
    state.presentation.stacks.push(StackPresentation {
        anchor: graph,
        min_width: 0.0,
        collapsed: Some(CollapsedStack {
            label: "Readings".into(),
            rail_width: 28.0,
            open: false,
            drawer: None,
        }),
    });
    assert!(
        !graph_is_presented(&state),
        "a closed rail hides its active Graph"
    );
    state.presentation.stacks[0]
        .collapsed
        .as_mut()
        .unwrap()
        .drawer = Some(DrawerGeometry {
        trigger: (0.0, 0.0, 28.0, 0.0),
        panel_size: (280.0, 600.0),
        bounds: (0.0, 0.0, 320.0, 600.0),
    });
    assert!(
        !graph_is_presented(&state),
        "a closed drawer hides its active Graph"
    );
    state.presentation.stacks[0]
        .collapsed
        .as_mut()
        .unwrap()
        .open = true;
    assert!(
        graph_is_presented(&state),
        "an open drawer presents its active Graph"
    );
    state.docs.activate(document);
    assert!(
        !graph_is_presented(&state),
        "opening a drawer does not present an inactive Graph tab"
    );
    state.docs.activate(graph);
    assert_eq!(
        state.docs.workspace().tiled(),
        &canonical,
        "presentation did not mutate the canonical tree"
    );
}

#[test]
fn hidden_graph_keeps_owner_facts_but_suppresses_the_presentation_cause() {
    use crate::graph::diagnostics::{CaptureFacts, Installed};
    let accepted = apparatus_core::RecordRef {
        run: "capture-test".into(),
        source: "knot.catalog".into(),
        sequence: 6,
    };
    let installed = Installed {
        generation: 2,
        request: None,
        outcome: None,
        acceptance: Some(accepted.clone()),
    };
    let facts = CaptureFacts {
        configured: true,
        busy: false,
        queued: false,
        desired_generation: 2,
        generation_exhausted: false,
        installed: Some(&installed),
        catalog_nodes: 3,
        catalog_relations: 2,
        read_errors: 0,
        failure_present: false,
        diagnostic_admission: None,
    };
    let visible = graph_capture_projection(&facts, true);
    assert_eq!(visible["operation"], "catalog:2");
    assert_eq!(visible["cause"], serde_json::to_value(&accepted).unwrap());
    let hidden = graph_capture_projection(&facts, false);
    assert_eq!(hidden["visible"], false);
    assert!(hidden["operation"].is_null());
    assert!(hidden["cause"].is_null());
    assert_eq!(
        hidden["facts"], visible["facts"],
        "owner facts remain observable without a presentation claim"
    );
}
#[test]
fn capture_projection_reads_current_bounded_facts_without_document_material() {
    let mut state = DesktopState::new(
        knot_document::KnotDocumentSession::scratch("secret-address", "secret source"),
        cambium_genet_winit_host::WindowCommands::new(),
    );
    state.message = Some("secret error".into());
    let fields = capture_fields(&state);
    assert_eq!(fields["source_bytes"], "13");
    assert_eq!(fields["theme_dark"], "false");
    assert_eq!(fields["theme_id"], crate::theme_session::DEFAULT_THEME);
    assert_eq!(fields["theme_mode"], "light");
    assert_eq!(fields["source_face"], "ibm-plex-mono");
    assert_eq!(fields["requested_fonts"], "{}");
    let draft: std::collections::BTreeMap<String, String> =
        serde_json::from_str(&fields["font_roles_draft_fields"]).unwrap();
    assert_eq!(draft["reading"], "");
    assert_eq!(fields["font_roles_diagnostics"], "");
    assert_eq!(
        fields["requested_fonts_blake3"],
        blake3::hash(b"{}").to_hex().to_string()
    );
    assert_eq!(fields["measure"], "medium");
    assert!(!fields.contains_key("wide"));
    assert!(!serde_json::to_string(&fields).unwrap().contains("secret"));
    let mut choice =
        tabard::theme::choice::ThemeChoice::new(crate::theme_session::DEFAULT_THEME, None);
    choice.fonts = tabard::typography::FontPreferences(std::collections::BTreeMap::from([(
        tabard::typography::FontRole::Reading,
        vec![tabard::typography::FontFamily::new("IBM Plex Mono").unwrap()],
    )]));
    state.appearance.theme_choice = Some(choice);
    let fields = capture_fields(&state);
    assert_eq!(
        fields["requested_fonts"],
        r#"{"reading":["IBM Plex Mono"]}"#
    );
    assert_eq!(
        fields["requested_fonts_blake3"],
        blake3::hash(fields["requested_fonts"].as_bytes())
            .to_hex()
            .to_string()
    );
    state.appearance.dark = true;
    state.appearance.measure = crate::appearance::Measure::Full;
    assert_eq!(capture_fields(&state)["theme_dark"], "true");
    assert_eq!(capture_fields(&state)["measure"], "full");
    assert_eq!(fields["measure"], "medium");
    assert_eq!(
        fields["theme_dark"], "false",
        "the earlier reading stays frozen"
    );
}
