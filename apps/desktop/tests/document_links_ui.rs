// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Exercise source-authored links through the desktop command palette and the
//! one retained document buffer.

use cambium::{CaretAffinity, CaretPosition, CaretSelection, TextCommand};
use cambium_genet_winit_host::{Harness, Init, KeyPress, Modifiers, NamedKey, WindowCommands};
use knot_desktop::{
    desktop_sheet, host_hooks,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::{KnotDocumentIntentV1, KnotDocumentSession};
use std::path::PathBuf;
use taproot::Selector;
use tempfile::{TempDir, tempdir};

type Logic = fn(&DesktopState) -> DesktopView;
type DesktopHarness = Harness<DesktopState, Logic, DesktopView>;

struct Fixture {
    _root: TempDir,
    source: PathBuf,
    target: PathBuf,
    harness: DesktopHarness,
}

const SOURCE: &str = "# Source\n\nA selected α [passage] remains.\n";
const SELECTED: &str = "selected α [passage]";

fn palette_shortcut() -> KeyPress {
    KeyPress::character("p").with_modifiers(Modifiers {
        meta: cfg!(target_os = "macos"),
        ctrl: !cfg!(target_os = "macos"),
        shift: true,
        ..Modifiers::NONE
    })
}

fn command_shortcut(key: &str, shift: bool) -> KeyPress {
    KeyPress::character(key).with_modifiers(Modifiers {
        meta: cfg!(target_os = "macos"),
        ctrl: !cfg!(target_os = "macos"),
        shift,
        ..Modifiers::NONE
    })
}

fn fixture() -> Fixture {
    let root = tempdir().unwrap();
    let source = root.path().join("source α.djot");
    let target = root.path().join("target 東京 # (notes).djot");
    std::fs::write(&source, SOURCE).unwrap();
    std::fs::write(&target, "# Read only target\n").unwrap();
    let mut permissions = std::fs::metadata(&target).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&target, permissions).unwrap();

    let start = SOURCE.find(SELECTED).unwrap();
    let end = start + SELECTED.len();
    let mut source_session = KnotDocumentSession::open(&source).unwrap();
    source_session
        .apply(KnotDocumentIntentV1::Edit(TextCommand::SetSelection(
            CaretSelection {
                anchor: CaretPosition {
                    byte: start,
                    affinity: CaretAffinity::Downstream,
                },
                focus: CaretPosition {
                    byte: end,
                    affinity: CaretAffinity::Upstream,
                },
            },
        )))
        .unwrap();

    let mut state =
        DesktopState::with_path(source_session, WindowCommands::new(), Some(source.clone()));
    let target_session = KnotDocumentSession::open_read_only(&target).unwrap();
    state.open_behind(vec![target_session], &[]);

    let mut harness = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as Logic,
            sheet: desktop_sheet(),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    harness.layout_at(1100.0, 800.0);
    harness.prepare_frame();
    harness.layout_at(1100.0, 800.0);
    Fixture {
        _root: root,
        source,
        target,
        harness,
    }
}

fn open_link_form(harness: &mut DesktopHarness) {
    assert!(harness.click_on(&Selector::role("textbox").with_attr("aria-label", "Document text")));
    harness.press_key(&palette_shortcut());
    harness.key_char("Link to document");
    harness.press_key(&KeyPress::named(NamedKey::Enter));
    harness.after_dispatch();
    assert!(
        harness
            .resolve(&Selector::role("dialog").with_attr("aria-label", "Link to document"))
            .is_some(),
        "the document command did not open its link form"
    );
}

#[test]
fn command_inserts_undoes_saves_and_reopens_a_source_authored_link() {
    let mut fixture = fixture();
    let target_session = KnotDocumentSession::open_read_only(&fixture.target).unwrap();
    assert_eq!(
        target_session.snapshot().write_posture,
        knot_document::KnotDocumentWritePostureV1::ReadOnly
    );

    let stamp = fixture.harness.state().document().session().source_stamp();
    open_link_form(&mut fixture.harness);
    fixture.harness.relayout();
    assert_eq!(
        fixture.harness.state().document().session().source_stamp(),
        stamp
    );
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").with_attr("data-link-target", "0"))
    );
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").containing("supports"))
    );
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").with_attr("id", "knot-link-insert"))
    );
    fixture.harness.after_dispatch();

    let linked = fixture.harness.state().document().snapshot();
    assert_eq!(
        fixture
            .harness
            .state()
            .document()
            .session()
            .source_stamp()
            .revision,
        stamp.revision + 1
    );
    assert_eq!(std::fs::read_to_string(&fixture.source).unwrap(), SOURCE);
    assert!(
        linked.dirty,
        "insertion should dirty the existing source buffer"
    );
    assert!(linked.text.contains("[selected α \\[passage\\]](./target%20%E6%9D%B1%E4%BA%AC%20%23%20%28notes%29.djot){rel=\"https://mere.computer/ns/rel#supports\"}"), "{}", linked.text);
    let parsed =
        knot_readings::ReadingInput::from_session(fixture.harness.state().document().session())
            .unwrap();
    assert_eq!(parsed.links.len(), 1, "{:?}", parsed.links);
    assert_eq!(parsed.links[0].text, SELECTED);
    assert_eq!(
        parsed.links[0].target,
        "./target%20%E6%9D%B1%E4%BA%AC%20%23%20%28notes%29.djot"
    );
    assert_eq!(
        parsed.links[0].rel.as_deref(),
        Some("https://mere.computer/ns/rel#supports")
    );
    let range = parsed.links[0].span.unwrap();
    assert!(linked.text[range.0..range.1].starts_with("[selected α"));

    fixture.harness.update(|state| {
        state
            .document_mut()
            .session_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Undo))
            .unwrap();
    });
    assert_eq!(fixture.harness.state().document().snapshot().text, SOURCE);
    fixture.harness.update(|state| {
        state
            .document_mut()
            .session_mut()
            .apply(KnotDocumentIntentV1::Edit(TextCommand::Redo))
            .unwrap();
    });
    assert_eq!(
        fixture.harness.state().document().snapshot().text,
        linked.text
    );

    fixture.harness.press_key(&command_shortcut("s", false));
    fixture.harness.after_dispatch();
    assert!(!fixture.harness.state().document().snapshot().dirty);
    assert_eq!(
        std::fs::read_to_string(&fixture.source).unwrap(),
        linked.text
    );
    drop(fixture.harness);

    let reopened = KnotDocumentSession::open(&fixture.source).unwrap();
    let parsed = knot_readings::ReadingInput::from_session(&reopened).unwrap();
    assert_eq!(parsed.links.len(), 1);
    assert_eq!(parsed.links[0].text, SELECTED);
    assert_eq!(
        parsed.links[0].target,
        "./target%20%E6%9D%B1%E4%BA%AC%20%23%20%28notes%29.djot"
    );
    assert_eq!(
        parsed.links[0].rel.as_deref(),
        Some("https://mere.computer/ns/rel#supports")
    );
    fresh_process_reopen(&fixture.source, &linked.text, 1);
}

#[test]
fn cancelling_or_escaping_restores_source_focus_without_editing() {
    let mut fixture = fixture();
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("textbox").with_attr("aria-label", "Document text"))
    );
    let source_focus = fixture.harness.focus();
    open_link_form(&mut fixture.harness);
    assert_ne!(fixture.harness.focus(), source_focus);
    fixture
        .harness
        .press_key(&KeyPress::named(NamedKey::Escape));
    fixture.harness.after_dispatch();
    assert!(
        fixture
            .harness
            .resolve(&Selector::role("dialog").with_attr("aria-label", "Link to document"))
            .is_none(),
        "Escape should dismiss the link form"
    );
    assert_eq!(fixture.harness.state().document().snapshot().text, SOURCE);
    assert!(!fixture.harness.state().document().snapshot().dirty);
    assert_eq!(
        fixture.harness.focus(),
        source_focus,
        "focus should return to the source editor"
    );

    open_link_form(&mut fixture.harness);
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").with_attr("id", "knot-link-cancel"))
    );
    fixture.harness.after_dispatch();
    assert!(
        fixture
            .harness
            .resolve(&Selector::role("dialog").with_attr("aria-label", "Link to document"))
            .is_none(),
        "Cancel should dismiss the link form"
    );
    assert_eq!(fixture.harness.state().document().snapshot().text, SOURCE);
    assert!(!fixture.harness.state().document().snapshot().dirty);
    assert_eq!(
        fixture.harness.focus(),
        source_focus,
        "Cancel should return focus to the source editor"
    );
}

#[test]
fn external_edit_refuses_save_and_reopening_keeps_the_external_source() {
    let mut fixture = fixture();
    open_link_form(&mut fixture.harness);
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").with_attr("data-link-target", "0"))
    );
    fixture.harness.relayout();
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("button").with_attr("id", "knot-link-insert"))
    );
    let draft = fixture.harness.state().document().snapshot().text;
    assert_ne!(draft, SOURCE);
    std::fs::write(
        &fixture.source,
        "# External author\n\nTheir edit survives.\n",
    )
    .unwrap();
    fixture.harness.press_key(&command_shortcut("s", false));
    fixture.harness.after_dispatch();
    let snapshot = fixture.harness.state().document().snapshot();
    assert_eq!(snapshot.text, draft);
    assert!(snapshot.dirty);
    assert_eq!(
        snapshot.refusal,
        Some(knot_document::KnotDocumentRefusalV1::ExternalChange)
    );
    drop(fixture.harness);
    let reopened = KnotDocumentSession::open(&fixture.source).unwrap();
    assert_eq!(
        reopened.input().text(),
        "# External author\n\nTheir edit survives.\n"
    );
    assert!(!reopened.snapshot().dirty);
    fresh_process_reopen(
        &fixture.source,
        "# External author\n\nTheir edit survives.\n",
        0,
    );
}

fn fresh_process_reopen(path: &std::path::Path, expected: &str, links: usize) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fresh_process_reopen_child",
            "--ignored",
            "--nocapture",
        ])
        .env("KNOT_PROPOSAL_REOPEN_PATH", path)
        .env("KNOT_PROPOSAL_REOPEN_TEXT", expected)
        .env("KNOT_PROPOSAL_REOPEN_LINKS", links.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!(
        "fresh-process reopen ({} links): {}",
        links,
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
#[ignore = "child-process helper invoked with a specific saved fixture"]
fn fresh_process_reopen_child() {
    let path = std::env::var_os("KNOT_PROPOSAL_REOPEN_PATH").expect("child path");
    let expected = std::env::var("KNOT_PROPOSAL_REOPEN_TEXT").unwrap();
    let links = std::env::var("KNOT_PROPOSAL_REOPEN_LINKS")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let reopened = KnotDocumentSession::open(PathBuf::from(path)).unwrap();
    assert_eq!(reopened.input().text(), expected);
    assert!(!reopened.snapshot().dirty);
    let parsed = knot_readings::ReadingInput::from_session(&reopened).unwrap();
    assert_eq!(parsed.links.len(), links);
    if links == 1 {
        assert_eq!(parsed.links[0].text, SELECTED);
        assert_eq!(
            parsed.links[0].rel.as_deref(),
            Some("https://mere.computer/ns/rel#supports")
        );
    }
}

#[test]
fn native_textarea_edits_undo_redo_advance_revision_but_layout_does_not() {
    let mut fixture = fixture();
    assert!(
        fixture
            .harness
            .click_on(&Selector::role("textbox").with_attr("aria-label", "Document text"))
    );
    let stamp = fixture.harness.state().document().session().source_stamp();
    fixture.harness.relayout();
    assert_eq!(
        fixture.harness.state().document().session().source_stamp(),
        stamp
    );
    fixture
        .harness
        .press_key(&KeyPress::named(NamedKey::ArrowRight));
    // The desktop focused-text-slot integration uses the public raw mutable
    // escape hatch. It conservatively invalidates even selection-only borrows;
    // plain session selection intents are separately tested as stamp-neutral.
    let after_motion = fixture.harness.state().document().session().source_stamp();
    assert!(after_motion.revision >= stamp.revision);
    let before = fixture.harness.state().document().snapshot().text;
    fixture.harness.press_key(&KeyPress::character("x"));
    let changed = fixture.harness.state().document().session().source_stamp();
    assert!(changed.revision > after_motion.revision);
    fixture.harness.press_key(&command_shortcut("z", false));
    assert_eq!(fixture.harness.state().document().snapshot().text, before);
    assert_eq!(
        fixture
            .harness
            .state()
            .document()
            .session()
            .source_stamp()
            .revision,
        changed.revision + 1
    );
    fixture.harness.press_key(&command_shortcut("z", true));
    assert_eq!(
        fixture
            .harness
            .state()
            .document()
            .session()
            .source_stamp()
            .revision,
        changed.revision + 2
    );
}
