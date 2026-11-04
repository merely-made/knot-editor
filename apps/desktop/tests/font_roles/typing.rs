// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn mounted_typing_picker_keyboard_apply_and_reopen_preserve_writer_authority() {
    let dir = tempdir().unwrap();
    let library = dir.path().join("themes.json");
    let preference = dir.path().join("preferences.json");
    let mut host = launch(&library, &preference, desktop_view);
    assert!(
        host.state()
            .theme_session
            .available_fonts
            .iter()
            .any(|family| family == "IBM Plex Mono")
    );
    assert!(
        host.state()
            .theme_session
            .available_fonts
            .iter()
            .any(|family| family == "Source Serif 4")
    );
    let writer = host.state().document().snapshot();
    focus(&mut host, &field("reading"));
    host.key_injected("IBM Plex Mono");
    assert_eq!(
        host.state()
            .theme_session
            .font_editor
            .field(FontRole::Reading)
            .text(),
        "IBM Plex Mono"
    );
    assert!(requested(&host).is_empty());
    assert!(!preference.exists());
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-font-choose", "heading"),
    );
    focus(
        &mut host,
        &Selector::role("textbox").with_attr("data-font-search", "true"),
    );
    host.key_injected("Source Serif 4");
    host.after_dispatch();
    host.relayout();
    let projection = host.a11y_projection();
    assert!(host.with_dom(|dom| {
        taproot::matching_with_projection(
            dom,
            &Selector::role("button").with_attr("data-font-family", "IBM Plex Mono"),
            &projection,
        )
        .is_empty()
    }));
    enter(
        &mut host,
        &Selector::role("button").with_attr("data-font-family", "Source Serif 4"),
    );
    assert!(requested(&host).is_empty());
    assert!(!preference.exists());
    apply(&mut host);
    assert_eq!(
        requested(&host).0[&FontRole::Reading][0].name(),
        "IBM Plex Mono"
    );
    assert_eq!(
        requested(&host).0[&FontRole::Heading][0].name(),
        "Source Serif 4"
    );
    assert_eq!(host.state().appearance.source_face, SourceFace::IbmPlexMono);
    assert_eq!(host.state().document().snapshot(), writer);
    let mut reopened = launch(&library, &preference, desktop_view);
    assert_eq!(requested(&reopened), requested(&host));
    assert_eq!(
        reopened
            .state()
            .theme_session
            .font_editor
            .field(FontRole::Reading)
            .text(),
        "IBM Plex Mono"
    );
    let saved = std::fs::read(&preference).unwrap();
    enter(
        &mut reopened,
        &Selector::role("button").with_attr("data-font-default", "reading"),
    );
    assert_eq!(requested(&reopened), requested(&host));
    assert_eq!(
        std::fs::read(&preference).unwrap(),
        saved,
        "Default stages until explicit Apply"
    );
    apply(&mut reopened);
    assert!(!requested(&reopened).0.contains_key(&FontRole::Reading));
    assert_eq!(
        requested(&reopened).0[&FontRole::Heading][0].name(),
        "Source Serif 4"
    );
}

#[test]
fn shared_slots_keep_preedit_out_of_saved_intent_and_route_commit() {
    let dir = tempdir().unwrap();
    let mut host = launch(
        &dir.path().join("themes.json"),
        &dir.path().join("preferences.json"),
        desktop_view,
    );
    focus(&mut host, &field("reading"));
    let slot = knot_desktop::workspace::focused_text(host.runner()).expect("shared font text slot");
    host.update(|state| (slot.get_mut)(state).set_preedit("仮入力"));
    host.after_dispatch();
    assert_eq!(
        host.state()
            .theme_session
            .font_editor
            .field(FontRole::Reading)
            .text(),
        ""
    );
    assert!(requested(&host).is_empty());
    host.update(|state| (slot.get_mut)(state).clear_preedit());
    host.key_injected("Source Serif 4");
    apply(&mut host);
    assert_eq!(
        requested(&host),
        prefs(FontRole::Reading, &["Source Serif 4"])
    );
}
