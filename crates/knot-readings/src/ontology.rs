// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Vocabulary annotations for the outgoing-links and sound readings.

use std::path::Path;

use crate::sound::{SoundKind, SoundReading};
use mere::profile::intern::TermEntry;
use mere::profile::vocab::{Alignment, alignment};
pub use mere_ontology::Profile;
use mere_ontology::TermKind;

const LENS: &str = include_str!("../ontology/lens.toml");
const PROSODY: &str = include_str!("../ontology/prosody.toml");
const RHYME_MATCH: &str =
    "http://postdata.linhd.uned.es/ontology/postdata-poeticAnalysis#RhymeMatch";
const FOOT: &str = "http://postdata.linhd.uned.es/ontology/postdata-poeticAnalysis#Foot";

/// Resolve the bundled lens or an explicitly selected local directory.
///
/// # Errors
///
/// Returns the admission or file error for an unreadable or invalid selection.
pub fn load(directory: Option<&Path>) -> Result<Profile, String> {
    mere_ontology::load(LENS, &[mere_ontology::CITATION, PROSODY], directory)
}

fn subject_term<'a>(profile: &'a Profile, iri: &str, kind: TermKind) -> Option<&'a TermEntry> {
    profile
        .terms
        .entry(iri)
        .filter(|term| term.kind == kind && term.pack != mere::profile::core::CORE_PACK_ID)
}

/// The selected term under which a reader can compare this authored predicate.
pub struct PredicateReading<'a> {
    pub term: &'a TermEntry,
    pub correspondence: &'static str,
}

pub fn selected_predicate<'a>(profile: &'a Profile, iri: &str) -> Option<PredicateReading<'a>> {
    if let Some(term) = subject_term(profile, iri, TermKind::Predicate) {
        return Some(PredicateReading {
            term,
            correspondence: "Declared predicate",
        });
    }
    let predicate = mere::profile::core::terms().id(iri)?;
    let (correspondence, standard) = match alignment(predicate)? {
        Alignment::Exact(iri) => ("Equivalent predicate", iri),
        Alignment::Approximate(iri) => ("Broader predicate", iri),
        Alignment::MereOnly => return None,
    };
    let term = subject_term(profile, standard, TermKind::Predicate)?;
    Some(PredicateReading {
        term,
        correspondence,
    })
}

/// Describe the source predicate using the selected subject vocabularies.
pub fn predicate_note(profile: &Profile, iri: &str) -> Option<String> {
    let reading = selected_predicate(profile, iri)?;
    Some(format!(
        "{} · {} · {}",
        reading.correspondence,
        reading.term.display_label("en"),
        reading.term.iri
    ))
}

/// Only describe concepts for which the existing sound reading found a candidate.
pub fn prosody_note(profile: &Profile, reading: &SoundReading) -> Option<String> {
    let rhyme = reading.relations.iter().any(|relation| {
        matches!(
            relation.kind,
            SoundKind::PerfectRhyme | SoundKind::SlantRhyme
        )
    });
    let mut terms = Vec::new();
    if rhyme && let Some(term) = subject_term(profile, RHYME_MATCH, TermKind::Class) {
        terms.push(format!("{} · {}", term.display_label("en"), term.iri));
    }
    if reading.meter.is_some()
        && let Some(term) = subject_term(profile, FOOT, TermKind::Class)
    {
        terms.push(format!("{} · {}", term.display_label("en"), term.iri));
    }
    (!terms.is_empty()).then(|| format!("Vocabulary for this reading · {}", terms.join(" · ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::{self, SoundLayers};
    use std::collections::BTreeMap;

    #[test]
    fn evidence_reading_distinguishes_an_equivalent_predicate_from_a_broader_one() {
        let profile = load(None).unwrap();
        assert_eq!(
            predicate_note(&profile, "https://mere.computer/ns/rel#supports").as_deref(),
            Some("Equivalent predicate · supports · http://purl.org/spar/cito/supports")
        );
        assert_eq!(
            predicate_note(&profile, "https://mere.computer/ns/rel#summarizes").as_deref(),
            Some("Broader predicate · cites · http://purl.org/spar/cito/cites")
        );
        assert!(predicate_note(&profile, "https://mere.computer/ns/rel#example-of").is_none());
        assert_eq!(
            predicate_note(&profile, "http://purl.org/spar/cito/supports").as_deref(),
            Some("Declared predicate · supports · http://purl.org/spar/cito/supports")
        );
        assert!(predicate_note(&profile, FOOT).is_none());
    }

    #[test]
    fn comparable_predicates_keep_their_identities_under_equal_local_labels() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lens.toml"), LENS).unwrap();
        let local = mere_ontology::CITATION
            .replace(r#"en = "cites""#, r#"en = "local relation""#)
            .replace(r#"en = "supports""#, r#"en = "local relation""#);
        std::fs::write(dir.path().join("citation.toml"), local).unwrap();
        let profile = load(Some(dir.path())).unwrap();
        let direct = selected_predicate(&profile, "http://purl.org/spar/cito/cites").unwrap();
        let cites = selected_predicate(&profile, "https://mere.computer/ns/rel#cites").unwrap();
        let questions =
            selected_predicate(&profile, "https://mere.computer/ns/rel#questions").unwrap();
        let supports =
            selected_predicate(&profile, "https://mere.computer/ns/rel#supports").unwrap();
        assert_eq!(direct.term.iri, cites.term.iri);
        assert_eq!(cites.term.iri, questions.term.iri);
        assert_ne!(cites.correspondence, questions.correspondence);
        assert_ne!(cites.term.iri, supports.term.iri);
        for matched in [direct, cites, questions, supports] {
            assert_eq!(matched.term.display_label("en"), "local relation");
        }
    }

    #[test]
    fn selecting_prosody_alone_annotates_observed_results_and_omits_citation() {
        let folder = tempfile::tempdir().unwrap();
        let lens = LENS.replace("\r\n", "\n").replace(
            "    { pack = \"pack:citation\", version = \"0.1.0\" },\n",
            "",
        );
        std::fs::write(folder.path().join("lens.toml"), lens).unwrap();
        let profile = load(Some(folder.path())).unwrap();
        assert!(predicate_note(&profile, "https://mere.computer/ns/rel#supports").is_none());

        let text = "cat bat";
        let plain = sound::analyze(
            text,
            0..text.len(),
            &BTreeMap::new(),
            &SoundLayers::default(),
        )
        .unwrap();
        assert!(prosody_note(&profile, &plain).is_none());
        let layers = SoundLayers {
            meter: true,
            perfect_rhyme: true,
            ..Default::default()
        };
        let reading = sound::analyze(text, 0..text.len(), &BTreeMap::new(), &layers).unwrap();
        let note = prosody_note(&profile, &reading).unwrap();
        assert!(note.contains(RHYME_MATCH) && note.contains(FOOT));
    }
}
