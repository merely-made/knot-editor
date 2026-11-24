// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Compare admitted assertions by the selected term they directly name, match,
//! or specialize. Group on the IRI so equal display labels stay distinct.

use std::collections::BTreeMap;

use cambium::{Keyed, button, el, span};
use knot_readings::ontology::{Profile, selected_predicate};
use scenograph::link::{LinkAssertion, LinkBucket, LinkPairReading};

use super::KnotRelationView;
use crate::workspace::{DesktopState, DesktopView};

struct Evidence<'a> {
    bucket: &'a LinkBucket,
    assertion: &'a LinkAssertion,
    correspondence: &'static str,
}

struct Facet<'a> {
    label: &'a str,
    rows: Vec<Evidence<'a>>,
}

fn facets<'a>(profile: &'a Profile, reading: &'a LinkPairReading) -> BTreeMap<&'a str, Facet<'a>> {
    let mut facets = BTreeMap::new();
    for bucket in reading.links.iter().flat_map(|link| &link.buckets) {
        for assertion in &bucket.assertions {
            if let Some(matched) = selected_predicate(profile, &assertion.predicate) {
                facets
                    .entry(matched.term.iri.as_str())
                    .or_insert_with(|| Facet {
                        label: matched.term.display_label("en"),
                        rows: Vec::new(),
                    })
                    .rows
                    .push(Evidence {
                        bucket,
                        assertion,
                        correspondence: matched.correspondence,
                    });
            }
        }
    }
    facets
}

/// A vocabulary choice narrows the personal relation reading to attributable
/// passages. The original opened links remain the initial view.
pub(super) fn view(
    profile: &Profile,
    view: &KnotRelationView,
) -> (DesktopView, Option<DesktopView>) {
    let facets = facets(profile, view.reading());
    if facets.is_empty() {
        return (Box::new(el("div", ())), None);
    }
    let selected = view.selected_predicate.as_deref();
    let selected_facet = selected.and_then(|iri| facets.get_key_value(iri));
    let mut label_counts = BTreeMap::new();
    for facet in facets.values() {
        *label_counts.entry(facet.label).or_insert(0usize) += 1;
    }
    let buttons = facets
        .iter()
        .map(|(&iri, facet)| {
            let key = iri.to_owned();
            let choose = key.clone();
            let label = if label_counts[facet.label] > 1 {
                format!("{} ({}) · {iri}", facet.label, facet.rows.len())
            } else {
                format!("{} ({})", facet.label, facet.rows.len())
            };
            (
                key,
                button(label, move |state: &mut DesktopState, _| {
                    if let Some(view) = &mut state.knot_relations {
                        view.selected_predicate = Some(choose.clone());
                    }
                })
                .attr("aria-pressed", (selected == Some(iri)).to_string())
                .attr("title", iri.to_owned()),
            )
        })
        .collect::<Vec<_>>();
    let clear = button("All relationships", |state: &mut DesktopState, _| {
        if let Some(view) = &mut state.knot_relations {
            view.selected_predicate = None;
        }
    })
    .attr("aria-pressed", selected_facet.is_none().to_string());
    let controls = el("div", (clear, Keyed::new(buttons)))
        .attr("class", "knot-link-history-controls")
        .attr("role", "group")
        .attr("aria-label", "Filter by vocabulary predicate");

    let comparison = selected_facet.map(|(&iri, facet)| {
        let entries = facet
            .rows
            .iter()
            .map(|row| {
                let assertion = row.assertion;
                let passages = view.passages.get(&assertion.id);
                let direction = format!(
                    "{} → {}",
                    row.bucket.source_from.id, row.bucket.source_to.id
                );
                let predicate = format!("Authored predicate · {}", assertion.predicate);
                let quotes = passages.map(|(subject, object)| {
                    el(
                        "div",
                        (
                            span(format!(
                                "From {} · {}",
                                row.bucket.source_from.id, subject.quote
                            )),
                            span(subject.location()),
                            span(format!("To {} · {}", row.bucket.source_to.id, object.quote)),
                            span(object.location()),
                        ),
                    )
                    .attr("class", "knot-captured-passages")
                });
                (
                    assertion.id.clone(),
                    el(
                        "article",
                        (
                            el("h4", direction),
                            span(predicate),
                            span(format!("{} · {iri}", row.correspondence)),
                            quotes,
                            assertion.basis.note.as_ref().map(|note| span(note.clone())),
                            (!assertion.basis.evidence.is_empty())
                                .then(|| span(assertion.basis.evidence.join(" · "))),
                            span(if assertion.withdrawn.is_some() {
                                "Withdrawn"
                            } else {
                                "Held"
                            }),
                            assertion
                                .asserter
                                .as_ref()
                                .map(|author| span(format!("Signed by · {author}"))),
                            span(format!("Assertion · {}", assertion.id)),
                        ),
                    )
                    .attr("class", "knot-predicate-evidence"),
                )
            })
            .collect::<Vec<_>>();
        Box::new(el(
            "section",
            (
                el("h3", format!("Evidence for {}", facet.label)),
                span(iri.to_owned()),
                Keyed::new(entries),
            ),
        )) as DesktopView
    });
    (Box::new(controls), comparison)
}
