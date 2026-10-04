// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::process::Command;

use knot_composition::wordnet_import::{
    ImportOptions, MAX_ESTIMATED_BYTES, import_wordnet, import_wordnet_with_budget,
};
use reference_data::{Limits, RelationKind, ValidatedPack};

const FIXTURE: &str = include_str!("fixtures/wordnet.xml");

fn options(lemma: &str) -> ImportOptions {
    ImportOptions {
        source: "oewn".into(),
        version: "2025".into(),
        lemmas: vec![lemma.into()],
    }
}

#[test]
fn real_lmf_structure_preserves_sense_targets_concepts_license_and_exact_text() {
    let imported = import_wordnet(FIXTURE.as_bytes(), &options("tide")).unwrap();
    assert_eq!(imported.pack.entries.len(), 4);
    assert_eq!(imported.report.input_entries, 6);
    assert_eq!(imported.report.excluded_entries, 2);
    assert_eq!(imported.report.ignored_elements["Form"], 1);
    assert_eq!(imported.report.excluded_relations["unsupported:similar"], 1);
    assert_eq!(
        imported.report.excluded_relations["unsupported:instance_hypernym"],
        1
    );
    assert_eq!(
        imported.pack.manifest.license.url.as_deref(),
        Some("https://creativecommons.org/licenses/by/4.0/")
    );
    let tide = &imported.pack.entries[0];
    assert_eq!(tide.id.entry, "oewn-tide-n");
    assert_eq!(tide.pronunciations[0].value, "taɪd");
    assert_eq!(tide.pronunciations[0].notation, "unspecified"); // no silent IPA inference
    assert_eq!(tide.pronunciations[0].variant.as_deref(), Some("GB"));
    let sense = &tide.senses[0];
    assert_eq!(sense.id, "oewn-tide-water");
    assert_eq!(sense.concept_id.as_deref(), Some("oewn-water-motion"));
    assert_eq!(sense.definition, "Water's periodic movement.");
    assert_eq!(
        sense.examples,
        ["A tide & a current.", "The tide moved <gently>."]
    );
    let antonym = sense
        .relations
        .iter()
        .find(|relation| relation.kind == RelationKind::Antonym)
        .unwrap();
    assert_eq!(antonym.target.entry, "oewn-ebb-n");
    assert_eq!(antonym.target_sense.as_deref(), Some("oewn-ebb-water"));
    assert!(
        sense
            .relations
            .iter()
            .any(|relation| relation.kind == RelationKind::Synonym
                && relation.target_sense.as_deref() == Some("oewn-current-water"))
    );
    assert!(tide.senses[1].relations.is_empty()); // water relations never leak to trend sense
    assert_eq!(
        imported.report.sense_synsets["oewn-tide-trend"],
        "oewn-trend"
    );
    let bytes = serde_json::to_vec(&imported.pack).unwrap();
    ValidatedPack::from_json(&bytes, None, &Limits::default()).unwrap();
}

#[test]
fn oewn_retains_both_upstream_notices_and_attributions_and_refuses_unknown_license() {
    let imported = import_wordnet(FIXTURE.as_bytes(), &options("tide")).unwrap();
    let notice = imported.pack.manifest.license.text.as_deref().unwrap();
    assert!(notice.contains("WordNet 3.1 Copyright 2011 by Princeton University"));
    assert!(notice.contains("Copyright (c) 2019-present, The Open English WordNet Team"));
    assert!(notice.contains("ALL copies"));
    assert!(notice.contains("AS IS"));
    assert!(imported.pack.manifest.attribution.contains("Princeton"));
    assert!(
        imported
            .pack
            .manifest
            .attribution
            .contains("Open English WordNet Team")
    );
    let unreviewed = FIXTURE.replace(
        "https://creativecommons.org/licenses/by/4.0/",
        "https://example.org/unreviewed-license",
    );
    assert!(
        import_wordnet(unreviewed.as_bytes(), &options("tide"))
            .unwrap_err()
            .to_string()
            .contains("unreviewed license")
    );
}

#[test]
fn lowered_retained_record_budget_is_enforced_before_growth() {
    assert!(
        import_wordnet_with_budget(
            FIXTURE.as_bytes(),
            &options("tide"),
            1024 * 1024 + FIXTURE.len()
        )
        .unwrap_err()
        .to_string()
        .contains("memory budget")
    );
    assert!(
        import_wordnet_with_budget(FIXTURE.as_bytes(), &options("tide"), 2 * 1024 * 1024).is_ok()
    );
    assert!(
        import_wordnet_with_budget(
            FIXTURE.as_bytes(),
            &options("tide"),
            MAX_ESTIMATED_BYTES + 1
        )
        .is_err()
    );
}

#[test]
fn attacker_chosen_diagnostic_names_are_bounded() {
    let mut names = String::new();
    for index in 0..1025 {
        names.push_str(&format!("<Unknown{index}/>"));
    }
    let hostile = FIXTURE.replace("</Lexicon>", &format!("{names}</Lexicon>"));
    assert!(
        import_wordnet(hostile.as_bytes(), &options("tide"))
            .unwrap_err()
            .to_string()
            .contains("diagnostic names")
    );
    let oversized = FIXTURE.replace(
        "<Form writtenForm=\"tides\"/>",
        &format!("<{} />", "x".repeat(129)),
    );
    assert!(
        import_wordnet(oversized.as_bytes(), &options("tide"))
            .unwrap_err()
            .to_string()
            .contains("name exceeds")
    );
}

#[test]
fn neighbors_do_not_recursively_expand_but_all_output_targets_resolve() {
    let imported = import_wordnet(FIXTURE.as_bytes(), &options("tide")).unwrap();
    assert!(
        !imported
            .pack
            .entries
            .iter()
            .any(|entry| entry.lemma == "change")
    );
    assert_eq!(
        imported.report.excluded_relations["outside_one_hop_subset"],
        1
    );
    for entry in &imported.pack.entries {
        for relation in entry.senses.iter().flat_map(|sense| &sense.relations) {
            let target = imported
                .pack
                .entries
                .iter()
                .find(|entry| entry.id == relation.target)
                .unwrap();
            assert!(
                target
                    .senses
                    .iter()
                    .any(|sense| Some(&sense.id) == relation.target_sense.as_ref())
            );
        }
    }
}

#[test]
fn unicode_lemma_lookup_is_exact_and_preserved() {
    let imported = import_wordnet(FIXTURE.as_bytes(), &options("café")).unwrap();
    assert_eq!(imported.pack.entries[0].lemma, "café");
    assert_eq!(
        imported.pack.entries[0].senses[0].definition,
        "A café serving drinks."
    );
    assert!(import_wordnet(FIXTURE.as_bytes(), &options("cafe\u{301}")).is_err());
    assert!(import_wordnet(FIXTURE.as_bytes(), &options("Tide")).is_err());
}

#[test]
fn explicit_identity_and_nonempty_lemma_selection_are_required() {
    let mut wrong = options("tide");
    wrong.source = "other".into();
    assert!(
        import_wordnet(FIXTURE.as_bytes(), &wrong)
            .unwrap_err()
            .to_string()
            .contains("identity")
    );
    wrong = options("tide");
    wrong.version = "2024".into();
    assert!(import_wordnet(FIXTURE.as_bytes(), &wrong).is_err());
    wrong = options("tide");
    wrong.lemmas.clear();
    assert!(import_wordnet(FIXTURE.as_bytes(), &wrong).is_err());
    wrong.lemmas = vec!["tide".into(); 65];
    assert!(import_wordnet(FIXTURE.as_bytes(), &wrong).is_err());
}

#[test]
fn malformed_external_entities_extensions_and_nested_important_markup_are_refused() {
    let malformed = &FIXTURE[..FIXTURE.len() / 2];
    assert!(import_wordnet(malformed.as_bytes(), &options("tide")).is_err());
    let internal_entity = FIXTURE.replace(
        "SYSTEM \"https://globalwordnet.github.io/schemas/WN-LMF-1.4.dtd\"",
        "[<!ENTITY secret SYSTEM \"file:///private/test-only\">]",
    );
    assert!(
        import_wordnet(internal_entity.as_bytes(), &options("tide"))
            .unwrap_err()
            .to_string()
            .contains("DTD")
    );
    let extension = FIXTURE.replace(
        "</LexicalResource>",
        "<LexiconExtension id=\"external\"/></LexicalResource>",
    );
    assert!(
        import_wordnet(extension.as_bytes(), &options("tide"))
            .unwrap_err()
            .to_string()
            .contains("external")
    );
    let markup = FIXTURE.replace(
        "Water&apos;s periodic movement.",
        "Water <b>periodic</b> movement.",
    );
    assert!(import_wordnet(markup.as_bytes(), &options("tide")).is_err());
    let unknown_entity = FIXTURE.replace("&amp;", "&unknown;");
    assert!(import_wordnet(unknown_entity.as_bytes(), &options("tide")).is_err());
}

#[test]
fn missing_targets_duplicate_senses_and_ambiguous_definitions_are_refused() {
    for invalid in [
        FIXTURE.replace("target=\"oewn-ebb-water\"", "target=\"missing-sense\""),
        FIXTURE.replace("target=\"oewn-motion\"", "target=\"missing-synset\""),
        FIXTURE.replace("id=\"oewn-tide-trend\"", "id=\"oewn-tide-water\""),
        FIXTURE.replace("synset=\"oewn-trend\"", "synset=\"missing-synset\""),
        FIXTURE.replace(
            "<Definition>A tendency in events.</Definition>",
            "<Definition>one</Definition><Definition>two</Definition>",
        ),
    ] {
        assert!(import_wordnet(invalid.as_bytes(), &options("tide")).is_err());
    }
}

#[test]
fn self_closing_senses_and_explicitly_closed_relations_are_equivalent() {
    let alternate = FIXTURE
        .replace(
            "<Sense id=\"oewn-tide-trend\" synset=\"oewn-trend\"/>",
            "<Sense id=\"oewn-tide-trend\" synset=\"oewn-trend\"></Sense>",
        )
        .replace(
            "<SenseRelation relType=\"antonym\" target=\"oewn-ebb-water\"/>",
            "<SenseRelation relType=\"antonym\" target=\"oewn-ebb-water\"></SenseRelation>",
        );
    let normal = import_wordnet(FIXTURE.as_bytes(), &options("tide")).unwrap();
    let other = import_wordnet(alternate.as_bytes(), &options("tide")).unwrap();
    assert_eq!(normal.pack, other.pack);
}

#[test]
fn cli_emits_valid_pack_and_report_and_never_overwrites_output() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("fixture.xml");
    let output = root.path().join("subset.json");
    fs::write(&input, FIXTURE).unwrap();
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_knot-wordnet-import"))
            .args(["--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .args(["--source", "oewn", "--version", "2025", "--lemma", "tide"])
            .output()
            .unwrap()
    };
    let result = invoke();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["output_entries"], 4);
    let before = fs::read(&output).unwrap();
    ValidatedPack::from_json(&before, None, &Limits::default()).unwrap();
    assert!(!invoke().status.success());
    assert_eq!(fs::read(&output).unwrap(), before);
    assert_eq!(fs::read_to_string(&input).unwrap(), FIXTURE);
}

#[test]
fn cli_rejects_bad_input_without_creating_output() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("bad.xml");
    let output = root.path().join("should-not-exist.json");
    fs::write(&input, "author's non-XML data").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_knot-wordnet-import"))
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .args(["--source", "oewn", "--version", "2025", "--lemma", "tide"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
}

/// An explicit acceptance lane, not a test that quietly fetches or installs data.
#[test]
#[ignore = "requires KNOT_TEST_OEWN_PACK pointing to an explicitly converted local upstream OEWN2025 pack"]
fn local_upstream_oewn_pack_installs_disabled_then_looks_up_after_enable() {
    use reference_data::{LookupQuery, SourceKey, SourceRegistry};
    let path =
        std::env::var_os("KNOT_TEST_OEWN_PACK").expect("provide explicit local converted pack");
    let bytes = fs::read(path).unwrap();
    let validated = ValidatedPack::from_json(&bytes, None, &Limits::default()).unwrap();
    assert_eq!(validated.manifest().id, "oewn");
    assert_eq!(validated.manifest().version, "2025");
    assert!(
        validated
            .manifest()
            .license
            .text
            .as_deref()
            .unwrap()
            .contains("Princeton University")
    );
    let root = tempfile::tempdir().unwrap();
    let mut registry = SourceRegistry::open(root.path()).unwrap();
    let status = registry
        .import_json(&bytes, validated.manifest().digest.as_deref())
        .unwrap();
    assert!(status.installed);
    assert!(!status.enabled);
    let query = LookupQuery {
        lemma: "tide".into(),
        language: Some("en".into()),
        sources: vec![SourceKey {
            source: "oewn".into(),
            version: "2025".into(),
        }],
    };
    assert!(registry.lookup(&query, 32).unwrap().is_empty());
    registry.set_enabled("oewn", "2025", true).unwrap();
    let entries = registry.lookup(&query, 32).unwrap();
    assert!(!entries.is_empty());
    assert!(
        entries
            .iter()
            .flat_map(|entry| &entry.senses)
            .all(|sense| sense.id.contains('.') && sense.concept_id.is_some())
    );
    eprintln!(
        "real OEWN2025 acceptance: {} subset entries; {} exact tide entries; {} tide senses; disabled->enabled lookup and reopened persistence",
        validated.entries().len(),
        entries.len(),
        entries
            .iter()
            .map(|entry| entry.senses.len())
            .sum::<usize>()
    );
    drop(registry);
    let reopened = SourceRegistry::open(root.path()).unwrap();
    assert_eq!(reopened.lookup(&query, 32).unwrap(), entries);
}
