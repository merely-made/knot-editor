// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn provider_mode_requires_root_and_a_non_builtin_route() {
    let args = [
        "projection-provider".into(),
        "/tmp/knot-documents".into(),
        "knot-documents".into(),
    ];
    let parsed = parse_provider_args(&args).expect("valid provider arguments");
    assert_eq!(parsed.root, PathBuf::from("/tmp/knot-documents"));
    assert_eq!(parsed.route.as_str(), "knot-documents");
    assert_eq!(parsed.endpoint, None);
    assert_eq!(parsed.max_sessions, 8);

    let colliding = [
        "projection-provider".into(),
        "/tmp/knot-documents".into(),
        "knot".into(),
    ];
    assert!(parse_provider_args(&colliding).is_err());
}

#[test]
fn provider_mode_accepts_an_explicit_app_endpoint() {
    let args = [
        "projection-provider".into(),
        "/tmp/knot-documents".into(),
        "knot-documents".into(),
        "--endpoint".into(),
        "/tmp/djinn-app.sock".into(),
        "--max-sessions".into(),
        "12".into(),
    ];
    let parsed = parse_provider_args(&args).expect("valid provider arguments");
    assert_eq!(parsed.endpoint.as_deref(), Some("/tmp/djinn-app.sock"));
    assert_eq!(parsed.max_sessions, 12);
}

#[test]
fn provider_mode_rejects_session_limits_outside_the_bounded_range() {
    for count in ["0", "65"] {
        let args = [
            "projection-provider".into(),
            "/tmp/knot-documents".into(),
            "knot-documents".into(),
            "--max-sessions".into(),
            count.into(),
        ];
        assert!(parse_provider_args(&args).is_err());
    }
}

#[test]
fn provider_factory_opens_a_real_read_only_directory_endpoint() {
    use graphshell_endpoint::{ProjectionCatalog, ProjectionSource};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("field.knot"), "# Field\n").unwrap();
    let mut first = open_directory_projection(root.path()).unwrap();

    // The directory canonicalizes its root (`/tmp` is `/private/tmp` on macOS).
    assert_eq!(
        first.directory().unwrap().root(),
        root.path().canonicalize().unwrap()
    );
    let descriptor = first.describe();
    assert_eq!(descriptor.label, "Knot");
    assert_eq!(descriptor.projections[0].label, "Files in place");
    let snapshot = first
        .snapshot(descriptor.projections[0].request.clone())
        .unwrap();
    assert!(
        snapshot
            .presentation
            .offers
            .values()
            .flatten()
            .all(|offer| offer.codec != chirograph::PresentationCodec::EditableTextV1)
    );
}
