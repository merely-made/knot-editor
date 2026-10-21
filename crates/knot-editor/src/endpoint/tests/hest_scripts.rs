// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Receive command-producing fences through the actual consented Knot endpoint.

use super::*;
use cambium::{CommandSet, Invocation, catalogue};
use script_rhai::command::{CommandLimits, RhaiCommandEvaluator};

const SCHEMA: &str = "graphshell.viewer/v1";
const LANGUAGE: &str = "rhai-command";

fn command_endpoint(root: &std::path::Path, allowed: bool, max_ops: u64) -> KnotEndpoint {
    let mut offered = CommandSet::new();
    for id in [catalogue::ids::VIEW_ZOOM_IN, catalogue::ids::PHYSICS_TOGGLE] {
        offered.register(catalogue::command(id).unwrap());
    }
    let policy = KnotEffectPolicy {
        run: KnotEffectMode::Ask,
        allowed_languages: if allowed {
            vec![LANGUAGE.into()]
        } else {
            vec!["rhai".into()]
        },
        max_ops,
        ..Default::default()
    };
    let effects = KnotEffectAuthority::new(policy).register_evaluator(RhaiCommandEvaluator::new(
        offered,
        SCHEMA,
        CommandLimits::default(),
    ));
    let mut endpoint = KnotEndpoint::open_writable(root, KnotWriteGrant::new(4096)).unwrap();
    endpoint.grant_effects(effects);
    endpoint
}

fn run(
    endpoint: &mut KnotEndpoint,
    snapshot: &ProjectionSnapshot,
    target: InstanceId,
    base_token: Vec<u8>,
    confirmed: bool,
) -> IntentResult {
    endpoint
        .invoke(effect_invocation(
            snapshot,
            target,
            KNOT_BLOCK_RUN_INTENT,
            &KnotEffectV1 {
                base_token,
                confirmed,
            },
        ))
        .unwrap()
}

fn resumed(endpoint: &mut KnotEndpoint, prior: &ProjectionSnapshot) -> ProjectionSnapshot {
    let ResumeReply::Snapshot(current) = endpoint
        .resume(ResumeRequest {
            session: prior.session.clone(),
            epoch: prior.scene.epoch,
            revision: prior.scene.revision,
        })
        .unwrap()
    else {
        panic!("evaluation must refresh the derived presentation");
    };
    *current
}

#[test]
fn hest_command_fence_requires_consent_and_only_authors_request_data() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("commands.knot");
    let authored = "```rhai-command eval\ncommand(\"view:zoom_in\", \"\");\ncommand(\"physics:toggle\", \"\");\n```\n";
    fs::write(&path, authored).unwrap();
    let mut endpoint = command_endpoint(temp.path(), true, 10_000);
    let request = endpoint.describe().projections.remove(0).request;
    let snapshot = endpoint.snapshot(request).unwrap();
    let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "commands.knot");

    let rejected = run(
        &mut endpoint,
        &snapshot,
        target,
        editable.base_token.clone(),
        false,
    );
    assert!(matches!(rejected, IntentResult::Rejected { .. }));
    assert!(endpoint.derived.is_empty());
    assert_eq!(endpoint.poll_notice().unwrap(), None);

    let admitted = run(&mut endpoint, &snapshot, target, editable.base_token, true);
    assert_eq!(admitted, IntentResult::Accepted);
    assert_eq!(
        admitted.command_outcome(),
        None,
        "request data is not enactment"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), authored);
    let current = resumed(&mut endpoint, &snapshot);
    let (_, editable, _) = editable_resource(&mut endpoint, &current, "commands.knot");
    assert_eq!(
        editable.derived.as_ref().unwrap().summary,
        "ran 1; denied 0; failed 0"
    );

    let derived = endpoint.derived.values().next().unwrap();
    let [inker::Block::Paragraph { spans }] = derived.document.blocks.as_slice() else {
        panic!("the opt-in evaluator must produce plain request data");
    };
    let json: String = spans
        .iter()
        .map(|span| match span {
            inker::InlineSpan::Text(text) => text.as_str(),
            _ => panic!("request data must remain plain text"),
        })
        .collect();
    let requests: Vec<Invocation> = serde_json::from_str(&json).unwrap();
    let expected: Vec<_> = [catalogue::ids::VIEW_ZOOM_IN, catalogue::ids::PHYSICS_TOGGLE]
        .into_iter()
        .map(|id| catalogue::command(id).unwrap().invocation(SCHEMA, vec![]))
        .collect();
    assert_eq!(requests, expected);
    assert!(
        serde_json::from_str::<serde_json::Value>(&json)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .all(|request| request.get("outcome").is_none())
    );

    fs::write(&path, "Changed externally.\n").unwrap();
    let stale = run(&mut endpoint, &current, target, editable.base_token, true);
    assert!(matches!(stale, IntentResult::Stale { .. }));
    assert_eq!(fs::read_to_string(&path).unwrap(), "Changed externally.\n");
}

#[test]
fn hest_failed_command_fences_preserve_source_without_partial_request_output() {
    for script in [
        "command(\"view:zoom_in\", \"\"); throw \"late failure\";",
        "command(\"view:zoom_in\", \"\"); loop {}",
    ] {
        let temp = tempdir().unwrap();
        let path = temp.path().join("commands.knot");
        let authored = format!("```rhai-command eval\n{script}\n```\n");
        fs::write(&path, &authored).unwrap();
        let mut endpoint = command_endpoint(temp.path(), true, 1_000);
        let request = endpoint.describe().projections.remove(0).request;
        let snapshot = endpoint.snapshot(request).unwrap();
        let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "commands.knot");
        let admitted = run(&mut endpoint, &snapshot, target, editable.base_token, true);
        assert_eq!(admitted, IntentResult::Accepted);
        assert_eq!(admitted.command_outcome(), None);
        let current = resumed(&mut endpoint, &snapshot);
        let (_, editable, _) = editable_resource(&mut endpoint, &current, "commands.knot");
        let preview = editable.derived.unwrap();
        assert_eq!(preview.summary, "ran 0; denied 0; failed 1");
        assert!(preview.source.contains(script));
        let derived = endpoint.derived.values().next().unwrap();
        assert!(matches!(
            derived.document.blocks.as_slice(),
            [inker::Block::CodeBlock { .. }]
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), authored);
    }
}

#[test]
fn hest_command_language_still_requires_the_hosts_allowlist() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("commands.knot");
    let authored = "```rhai-command eval\ncommand(\"view:zoom_in\", \"\");\n```\n";
    fs::write(&path, authored).unwrap();
    let mut endpoint = command_endpoint(temp.path(), false, 10_000);
    let request = endpoint.describe().projections.remove(0).request;
    let snapshot = endpoint.snapshot(request).unwrap();
    let (target, editable, _) = editable_resource(&mut endpoint, &snapshot, "commands.knot");
    assert!(matches!(
        run(&mut endpoint, &snapshot, target, editable.base_token, true),
        IntentResult::Rejected { reason } if reason == "intent was not advertised for this target"
    ));
    assert!(
        endpoint.derived.is_empty(),
        "denied languages never evaluate"
    );
    assert_eq!(endpoint.poll_notice().unwrap(), None);
    assert_eq!(fs::read_to_string(path).unwrap(), authored);
}
