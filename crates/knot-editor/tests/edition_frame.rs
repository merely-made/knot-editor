// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Knot opens a clip edition by address and republishes it as a fork.

use knot_editor::edition_frame::{EditionFrame, FrameError};
use site_model::clip::{ClipPayloadV1, clip_address, clip_body};
use site_model::editions::{
    EditionKind, EditionLimits, EditionWriter, SigningKey, verify_edition_chain,
};

fn public(key: &SigningKey) -> [u8; 32] {
    *key.verifying_key().as_bytes()
}

#[test]
fn opens_by_address_and_republishes_with_fork_lineage() {
    let author = SigningKey::from_bytes(&[1; 32]);
    let clip = ClipPayloadV1::new("https://example.test/a", Some("A"), None, "Passage.\n");
    let first = EditionWriter::new(author.clone())
        .append(&clip_body(&clip, "turnstone", 5).unwrap())
        .unwrap();
    let ops = [first];
    let source = verify_edition_chain(&public(&author), &ops, EditionLimits::default()).unwrap();
    let link = clip_address(public(&author), source.head().id).to_string();

    let frame = EditionFrame::open(&link, &ops, &clip.encode().unwrap()).unwrap();
    assert_eq!(frame.knot_body(), "Passage.\n");
    assert_eq!(frame.source_url(), "https://example.test/a");
    assert_eq!(frame.address().to_string(), link);

    let me = SigningKey::from_bytes(&[2; 32]);
    let mut writer = EditionWriter::new(me.clone());
    let fork = frame
        .republish(&mut writer, public(&me), Some("Passage.\nMine.\n"), 9)
        .unwrap();
    let chain = verify_edition_chain(
        &public(&me),
        std::slice::from_ref(&fork.operation),
        EditionLimits::default(),
    )
    .unwrap();
    assert_eq!(chain.fork_source(), Some(frame.edition_id()));
    assert_eq!(chain.head().body.kind, EditionKind::Clip);
    assert_eq!(chain.head().body.provenance.source, "knot");
    // Copy link for the new edition opens it again.
    let reopened = EditionFrame::open(
        &fork.address.to_string(),
        std::slice::from_ref(&fork.operation),
        &fork.payload,
    )
    .unwrap();
    assert_eq!(reopened.source_url(), "https://example.test/a");
    assert_eq!(reopened.knot_body(), "Passage.\nMine.\n");
}

#[test]
fn refuses_bad_addresses_and_vault_notes() {
    for text in ["knot://vault/field-note", "mere://x/y", "knot://zz/p"] {
        assert!(matches!(
            EditionFrame::open(text, &[], b""),
            Err(FrameError::Address(_))
        ));
    }
}
