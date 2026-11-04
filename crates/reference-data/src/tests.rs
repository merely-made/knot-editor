// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use crate::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn reference(entry: &str) -> ReferenceId {
    ReferenceId {
        source: "fixture".into(),
        version: "1".into(),
        entry: entry.into(),
    }
}
fn manifest() -> SourceManifest {
    SourceManifest {
        id: "fixture".into(),
        version: "1".into(),
        label: "Synthetic bilingual fixture — not an upstream dictionary".into(),
        language: "mul".into(),
        features: vec![
            ReferenceFeature::Definitions,
            ReferenceFeature::Examples,
            ReferenceFeature::Relations,
            ReferenceFeature::Pronunciations,
        ],
        license: LicenseInfo {
            name: "CC0-1.0".into(),
            text: Some("Synthetic test data dedicated to the public domain.".into()),
            url: Some("https://creativecommons.org/publicdomain/zero/1.0/".into()),
        },
        attribution: "Mere reference-data test fixture".into(),
        upstream: None,
        digest: None,
    }
}
fn fixture() -> NormalizedPack {
    let english = LexicalEntry {
        id: reference("bank-en"),
        lemma: "bank".into(),
        language: "en".into(),
        part_of_speech: Some(PartOfSpeech::Noun),
        senses: vec![LexicalSense {
            id: "edge".into(),
            concept_id: Some("river-edge".into()),
            definition: "The sloping edge of a river.".into(),
            examples: vec!["They sat on the river bank.".into()],
            relations: vec![],
        }],
        pronunciations: vec![Pronunciation {
            notation: "ipa".into(),
            value: "bæŋk".into(),
            variant: None,
        }],
        relations: vec![LexicalRelation {
            kind: RelationKind::Related,
            target: reference("rive-fr"),
            target_sense: Some("bord".into()),
        }],
    };
    let french = LexicalEntry {
        id: reference("rive-fr"),
        lemma: "rive".into(),
        language: "fr".into(),
        part_of_speech: Some(PartOfSpeech::Noun),
        senses: vec![LexicalSense {
            id: "bord".into(),
            concept_id: Some("river-edge".into()),
            definition: "Le bord d’un cours d’eau.".into(),
            examples: vec![],
            relations: vec![],
        }],
        pronunciations: vec![],
        relations: vec![],
    };
    let mut pack = NormalizedPack {
        schema_version: SCHEMA_VERSION,
        manifest: manifest(),
        entries: vec![english, french],
    };
    seal(&mut pack);
    pack
}
fn seal(pack: &mut NormalizedPack) {
    pack.manifest.digest = Some(digest_entries(&pack.entries).unwrap());
}
fn bytes(pack: &NormalizedPack) -> Vec<u8> {
    serde_json::to_vec_pretty(pack).unwrap()
}
fn query(word: &str) -> LookupQuery {
    LookupQuery {
        lemma: word.into(),
        language: None,
        sources: vec![manifest().source_key()],
    }
}
fn parse(pack: &NormalizedPack) -> Result<ValidatedPack> {
    ValidatedPack::from_json(&bytes(pack), None, &Limits::default())
}

struct TempDirectory(PathBuf);
impl TempDirectory {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "mere-reference-data-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            if fs::create_dir(&path).is_ok() {
                return Self(path);
            }
        }
        panic!("cannot create isolated test directory")
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn normalized_fixture_is_real_json_with_verifiable_payload_and_owned_entries() {
    let pack = fixture();
    let shipped = include_bytes!("../fixtures/bilingual.json");
    let shipped_pack =
        ValidatedPack::from_json(shipped, pack.manifest.digest.as_deref(), &Limits::default())
            .unwrap();
    assert_eq!(shipped_pack.entries(), pack.entries);
    let parsed = ValidatedPack::from_json(
        &bytes(&pack),
        pack.manifest.digest.as_deref(),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(parsed.entries(), pack.entries);
    assert_eq!(
        parsed.entry(&reference("bank-en")).unwrap().senses[0].definition,
        "The sloping edge of a river."
    );
    assert!(parsed.entry(&reference("absent")).is_none());
}

#[test]
fn catalog_install_enable_disable_and_reopen_are_distinct_durable_states() {
    let dir = TempDirectory::new();
    let pack = fixture();
    {
        let mut registry = SourceRegistry::open(&dir.0).unwrap();
        assert!(registry.list().is_empty());
        let available = registry.advertise(manifest()).unwrap();
        assert!(!available.installed && !available.enabled);
        assert!(matches!(
            registry.set_enabled("fixture", "1", true),
            Err(ReferenceError::NotInstalled)
        ));
        let installed = registry
            .import_json(&bytes(&pack), pack.manifest.digest.as_deref())
            .unwrap();
        assert!(installed.installed && !installed.enabled);
        assert!(registry.lookup(&query("bank"), 8).unwrap().is_empty());
        registry.set_enabled("fixture", "1", true).unwrap();
        assert_eq!(registry.lookup(&query("bank"), 8).unwrap().len(), 1);
        assert!(registry.entry(&reference("bank-en")).unwrap().is_some());
    }
    {
        let mut registry = SourceRegistry::open(&dir.0).unwrap();
        assert!(registry.list()[0].enabled);
        registry.set_enabled("fixture", "1", false).unwrap();
    }
    let registry = SourceRegistry::open(&dir.0).unwrap();
    assert!(registry.list()[0].installed && !registry.list()[0].enabled);
    assert!(registry.lookup(&query("bank"), 8).unwrap().is_empty());
    assert!(registry.entry(&reference("bank-en")).unwrap().is_none());
}

#[test]
fn exact_lemma_language_and_explicit_source_selection_never_silently_fall_back() {
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open(&dir.0).unwrap();
    let mut pack = fixture();
    pack.entries[1].lemma = "bank".into(); // Same spelling, distinct language.
    seal(&mut pack);
    registry.import_json(&bytes(&pack), None).unwrap();
    registry.set_enabled("fixture", "1", true).unwrap();
    assert_eq!(registry.lookup(&query("bank"), 8).unwrap().len(), 2);
    let mut french = query("bank");
    french.language = Some("fr".into());
    assert_eq!(registry.lookup(&french, 8).unwrap()[0].language, "fr");
    french.language = Some("de".into());
    assert!(registry.lookup(&french, 8).unwrap().is_empty());
    assert!(registry.lookup(&query("Bank"), 8).unwrap().is_empty());
    french.sources.clear();
    assert!(registry.lookup(&french, 8).unwrap().is_empty());
    assert_eq!(registry.lookup(&query("bank"), 1).unwrap().len(), 1);
    assert!(registry.lookup(&query("bank"), 0).unwrap().is_empty());
}

#[test]
fn hostile_identifiers_never_become_paths_and_invalid_import_does_not_mutate() {
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open(&dir.0).unwrap();
    for hostile in [
        "../escape",
        "..",
        "/absolute",
        "a/b",
        "a\\b",
        "",
        "nul\0",
        "日本語",
    ] {
        let mut pack = fixture();
        pack.manifest.id = hostile.into();
        seal(&mut pack);
        assert!(
            registry.import_json(&bytes(&pack), None).is_err(),
            "accepted {hostile:?}"
        );
        assert!(registry.list().is_empty());
        assert!(!dir.0.join("registry.json").exists());
    }
    assert_eq!(fs::read_dir(dir.0.join("packs")).unwrap().count(), 0);
}

#[test]
fn utf8_schema_unknown_fields_and_executable_mod_shapes_are_rejected() {
    assert!(ValidatedPack::from_json(&[0xff], None, &Limits::default()).is_err());
    let mut pack = fixture();
    pack.schema_version += 1;
    assert!(parse(&pack).is_err());
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["script"] = "run()".into();
    assert!(
        ValidatedPack::from_json(
            &serde_json::to_vec(&value).unwrap(),
            None,
            &Limits::default()
        )
        .is_err()
    );
    value = serde_json::to_value(fixture()).unwrap();
    value["manifest"]["upstream"] = "javascript:run()".into();
    assert!(
        ValidatedPack::from_json(
            &serde_json::to_vec(&value).unwrap(),
            None,
            &Limits::default()
        )
        .is_err()
    );
    assert!(
        ValidatedPack::from_json(
            br#"{"name":"mod","entrypoint":"code.wasm"}"#,
            None,
            &Limits::default()
        )
        .is_err()
    );
}

#[test]
fn digest_mismatch_missing_digest_and_pinned_mismatch_are_rejected() {
    let mut pack = fixture();
    pack.entries[0].lemma.push('x');
    assert!(matches!(parse(&pack), Err(ReferenceError::DigestMismatch)));
    pack = fixture();
    pack.manifest.digest = None;
    assert!(parse(&pack).is_err());
    assert!(matches!(
        ValidatedPack::from_json(
            &bytes(&fixture()),
            Some(&"0".repeat(64)),
            &Limits::default()
        ),
        Err(ReferenceError::DigestMismatch)
    ));
}

#[test]
fn duplicate_entry_sense_relation_features_and_dangling_targets_are_rejected() {
    let mut pack = fixture();
    pack.entries.push(pack.entries[0].clone());
    seal(&mut pack);
    assert!(parse(&pack).is_err());
    pack = fixture();
    let duplicate = pack.entries[0].senses[0].clone();
    pack.entries[0].senses.push(duplicate);
    seal(&mut pack);
    assert!(parse(&pack).is_err());
    pack = fixture();
    let duplicate = pack.entries[0].relations[0].clone();
    pack.entries[0].relations.push(duplicate);
    seal(&mut pack);
    assert!(parse(&pack).is_err());
    pack = fixture();
    pack.manifest.features.push(ReferenceFeature::Definitions);
    assert!(parse(&pack).is_err());
    pack = fixture();
    pack.entries[0].relations[0].target.entry = "missing".into();
    seal(&mut pack);
    assert!(parse(&pack).is_err());
    pack = fixture();
    pack.entries[0].relations[0].target.source = "other-source".into();
    seal(&mut pack);
    assert!(parse(&pack).is_err());
    pack = fixture();
    pack.manifest.features.clear();
    assert!(parse(&pack).is_err());
}

#[test]
fn every_entry_belongs_to_its_source_and_declared_language() {
    let mut pack = fixture();
    pack.entries[0].id.version = "other".into();
    seal(&mut pack);
    assert!(parse(&pack).is_err());
    pack = fixture();
    pack.manifest.language = "en".into();
    assert!(parse(&pack).is_err());
    pack = fixture();
    pack.entries[0].language = "en--US".into();
    seal(&mut pack);
    assert!(parse(&pack).is_err());
    pack = fixture();
    pack.entries[0].lemma = "a\0b".into();
    seal(&mut pack);
    assert!(parse(&pack).is_err());
}

#[test]
fn sense_relations_preserve_exact_meaning_and_reject_wrong_sense_identity() {
    let pack = fixture();
    assert_eq!(
        parse(&pack).unwrap().entries()[0].relations[0]
            .target_sense
            .as_deref(),
        Some("bord")
    );
    let mut wrong = pack.clone();
    wrong.entries[0].relations[0].target_sense = Some("edge".into());
    seal(&mut wrong);
    assert!(parse(&wrong).is_err());
    wrong = pack;
    wrong.entries[0].relations[0].target_sense = Some("../outside".into());
    seal(&mut wrong);
    assert!(parse(&wrong).is_err());
}

#[test]
fn upstream_shaped_dotted_sense_ids_are_preserved_without_becoming_paths() {
    let mut pack = fixture();
    let id = "oewn--apos-hood__1.14.01..";
    pack.entries[1].senses[0].id = id.into();
    pack.entries[0].relations[0].target_sense = Some(id.into());
    seal(&mut pack);
    let parsed = parse(&pack).unwrap();
    assert_eq!(parsed.entries()[1].senses[0].id, id);
    assert_eq!(
        parsed.entries()[0].relations[0].target_sense.as_deref(),
        Some(id)
    );
    for hostile in [
        ".",
        "..",
        "...",
        "./thing",
        "../thing",
        "thing/..",
        "thing\\..",
    ] {
        pack.entries[1].senses[0].id = hostile.into();
        seal(&mut pack);
        assert!(parse(&pack).is_err(), "accepted unsafe sense {hostile:?}");
    }
}

#[test]
fn dotted_versions_are_opaque_metadata_but_traversal_and_empty_segments_are_not() {
    let mut pack = fixture();
    pack.manifest.version = "1.0-beta".into();
    for entry in &mut pack.entries {
        entry.id.version = pack.manifest.version.clone();
        for relation in &mut entry.relations {
            relation.target.version = pack.manifest.version.clone();
        }
    }
    seal(&mut pack);
    assert!(parse(&pack).is_ok());
    for bad in ["..", "1..0", "../1", "/1", "1\\0", ""] {
        pack.manifest.version = bad.into();
        assert!(parse(&pack).is_err(), "accepted version {bad:?}");
    }
}

#[test]
fn unicode_exact_lemma_lookup_is_not_limited_to_english() {
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open(&dir.0).unwrap();
    let mut pack = fixture();
    pack.entries[1].lemma = "雪".into();
    pack.entries[1].language = "ja".into();
    seal(&mut pack);
    registry.import_json(&bytes(&pack), None).unwrap();
    registry.set_enabled("fixture", "1", true).unwrap();
    let mut japanese = query("雪");
    japanese.language = Some("ja".into());
    assert_eq!(registry.lookup(&japanese, 8).unwrap()[0].language, "ja");
    japanese.language = Some("en".into());
    assert!(registry.lookup(&japanese, 8).unwrap().is_empty());
}

#[test]
fn preexisting_invalid_content_addressed_file_is_never_overwritten() {
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open(&dir.0).unwrap();
    let data = bytes(&fixture());
    let name = format!("{}.json", blake3::hash(&data).to_hex());
    let path = dir.0.join("packs").join(name);
    fs::write(&path, b"invalid existing data").unwrap();
    assert!(matches!(
        registry.import_json(&data, None),
        Err(ReferenceError::Conflict)
    ));
    assert_eq!(fs::read(&path).unwrap(), b"invalid existing data");
    assert!(registry.list().is_empty());
    assert!(!dir.0.join("registry.json").exists());
}

#[test]
fn reopen_rechecks_total_pack_and_registry_limits_before_loading() {
    let dir = TempDirectory::new();
    let data = bytes(&fixture());
    {
        SourceRegistry::open(&dir.0)
            .unwrap()
            .import_json(&data, None)
            .unwrap();
    }
    for limits in [
        Limits {
            max_installed_bytes: data.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_pack_bytes: data.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_registry_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_sources: 0,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            SourceRegistry::open_with_limits(&dir.0, limits),
            Err(ReferenceError::Limit(_))
        ));
    }
    assert!(SourceRegistry::open(&dir.0).unwrap().list()[0].installed);
}

#[test]
fn pack_entry_sense_relation_string_and_installed_byte_limits_are_enforced() {
    let pack = fixture();
    let data = bytes(&pack);
    for limits in [
        Limits {
            max_pack_bytes: data.len() - 1,
            ..Limits::default()
        },
        Limits {
            max_entries: 1,
            ..Limits::default()
        },
        Limits {
            max_senses: 1,
            ..Limits::default()
        },
        Limits {
            max_relations: 0,
            ..Limits::default()
        },
        Limits {
            max_string_bytes: 8,
            ..Limits::default()
        },
        Limits {
            max_lemma_bytes: 3,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            ValidatedPack::from_json(&data, None, &limits),
            Err(ReferenceError::Limit(_))
        ));
    }
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open_with_limits(
        &dir.0,
        Limits {
            max_installed_bytes: data.len() - 1,
            ..Limits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        registry.import_json(&data, None),
        Err(ReferenceError::Limit(_))
    ));
    assert!(registry.list().is_empty());
    assert_eq!(fs::read_dir(dir.0.join("packs")).unwrap().count(), 0);
}

#[test]
fn lookup_and_registry_metadata_are_bounded() {
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open_with_limits(
        &dir.0,
        Limits {
            max_sources: 1,
            max_query_sources: 1,
            max_lookup_results: 2,
            ..Limits::default()
        },
    )
    .unwrap();
    registry.advertise(manifest()).unwrap();
    let mut other = manifest();
    other.id = "other".into();
    assert!(matches!(
        registry.advertise(other),
        Err(ReferenceError::Limit(_))
    ));
    assert!(matches!(
        registry.lookup(&query("bank"), 3),
        Err(ReferenceError::Limit(_))
    ));
    let mut many = query("bank");
    many.sources.push(manifest().source_key());
    assert!(matches!(
        registry.lookup(&many, 1),
        Err(ReferenceError::Limit(_))
    ));
    assert_eq!(registry.list().len(), 1);
}

#[test]
fn reimport_preserves_enabled_and_never_replaces_an_installed_version() {
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open(&dir.0).unwrap();
    let pack = fixture();
    registry.import_json(&bytes(&pack), None).unwrap();
    registry.set_enabled("fixture", "1", true).unwrap();
    assert!(registry.import_json(&bytes(&pack), None).unwrap().enabled);
    let before = fs::read(dir.0.join("registry.json")).unwrap();
    let mut altered = pack;
    altered.entries[0].lemma = "replacement".into();
    seal(&mut altered);
    assert!(matches!(
        registry.import_json(&bytes(&altered), None),
        Err(ReferenceError::Conflict)
    ));
    assert_eq!(fs::read(dir.0.join("registry.json")).unwrap(), before);
    assert!(registry.lookup(&query("bank"), 8).unwrap().len() == 1);
}

#[test]
fn invalid_existing_registry_is_reported_without_overwriting_it() {
    let dir = TempDirectory::new();
    let path = dir.0.join("registry.json");
    fs::write(&path, b"invalid").unwrap();
    assert!(SourceRegistry::open(&dir.0).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"invalid");
    // Failed open releases the OS lock; no stale writer recovery required.
    assert!(!matches!(
        SourceRegistry::open(&dir.0),
        Err(ReferenceError::Busy)
    ));
}

#[test]
fn corrupted_installed_pack_cannot_be_reopened_or_silently_repaired() {
    let dir = TempDirectory::new();
    {
        SourceRegistry::open(&dir.0)
            .unwrap()
            .import_json(&bytes(&fixture()), None)
            .unwrap();
    }
    let path = fs::read_dir(dir.0.join("packs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&path, b"corrupted").unwrap();
    let registry_bytes = fs::read(dir.0.join("registry.json")).unwrap();
    assert!(SourceRegistry::open(&dir.0).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"corrupted");
    assert_eq!(
        fs::read(dir.0.join("registry.json")).unwrap(),
        registry_bytes
    );
}

#[test]
fn a_competing_handle_cannot_write_and_drop_releases_lock_without_unlinking() {
    let dir = TempDirectory::new();
    let registry = SourceRegistry::open(&dir.0).unwrap();
    assert!(matches!(
        SourceRegistry::open(&dir.0),
        Err(ReferenceError::Busy)
    ));
    drop(registry);
    assert!(dir.0.join("writer.lock").exists());
    assert!(SourceRegistry::open(&dir.0).is_ok());
}

#[test]
fn process_exit_without_drop_releases_advisory_writer_lock() {
    let dir = TempDirectory::new();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::subprocess_writer_exit", "--ignored"])
        .env("MERE_REFERENCE_TEST_EXIT_ROOT", &dir.0)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(dir.0.join("writer.lock").exists());
    assert!(SourceRegistry::open(&dir.0).is_ok());
}

#[test]
#[ignore = "subprocess helper, run only with dedicated temporary root"]
fn subprocess_writer_exit() {
    let Some(root) = std::env::var_os("MERE_REFERENCE_TEST_EXIT_ROOT") else {
        return;
    };
    let _registry = SourceRegistry::open(root).unwrap();
    // Deliberately skip Rust Drop to prove the OS releases the lock on exit.
    std::process::exit(0);
}

#[test]
fn registry_path_traversal_and_enabled_uninstalled_records_are_rejected() {
    let dir = TempDirectory::new();
    let source =
        serde_json::json!({"manifest":manifest(),"enabled":true,"pack":"../../escape.json"});
    let path = dir.0.join("registry.json");
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"schema_version":1,"sources":[source]})).unwrap(),
    )
    .unwrap();
    assert!(SourceRegistry::open(&dir.0).is_err());
    let source = serde_json::json!({"manifest":manifest(),"enabled":true,"pack":null});
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"schema_version":1,"sources":[source]})).unwrap(),
    )
    .unwrap();
    assert!(SourceRegistry::open(&dir.0).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_registry_packs_and_writer_lock_are_rejected() {
    use std::os::unix::fs::symlink;
    for name in ["registry.json", "writer.lock", "packs"] {
        let dir = TempDirectory::new();
        let target = TempDirectory::new();
        symlink(&target.0, dir.0.join(name)).unwrap();
        assert!(
            SourceRegistry::open(&dir.0).is_err(),
            "accepted symlink {name}"
        );
    }
}

#[test]
fn failed_atomic_publication_does_not_enable_memory_state() {
    let dir = TempDirectory::new();
    let mut registry = SourceRegistry::open(&dir.0).unwrap();
    registry.import_json(&bytes(&fixture()), None).unwrap();
    let path = dir.0.join("registry.json");
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(registry.set_enabled("fixture", "1", true).is_err());
    assert!(!registry.list()[0].enabled);
    assert!(path.is_dir());
    fs::remove_dir(&path).unwrap();
    fs::write(&path, saved).unwrap();
}
