// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read-only adapter from Knot's authenticated personal relation projection
//! to the shared opened-link reading.

mod facets;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use scenograph::SourceBinding;
use scenograph::link::{
    LinkAssertion, LinkAssertionBasis, LinkBucket, LinkEndpoint, LinkGraphScope, LinkKey,
    LinkPairReading, LinkReading, LinkSource, LinkSourceRevision, LinkStratum,
};

use crate::workspace::{DesktopState, DesktopView};

#[derive(Clone, Debug)]
struct CapturedPassage {
    quote: String,
    head: [u8; 32],
    bytes: Option<(u64, u64)>,
}

impl CapturedPassage {
    #[cfg(feature = "mere-retention")]
    fn from_endpoint(endpoint: &knot_editor::KnotRelationEndpointV1) -> Self {
        Self {
            quote: endpoint.quote.clone(),
            head: endpoint.document_head,
            bytes: endpoint
                .position
                .map(|position| (position.start, position.end)),
        }
    }

    fn location(&self) -> String {
        let revision = hex(&self.head);
        match self.bytes {
            Some((start, end)) => format!("Revision {revision} · bytes {start}–{end}"),
            None => format!("Revision {revision}"),
        }
    }
}

/// A transient view over one authenticated Knot projection. Held and retained
/// history are separate readings so the host never paints a tombstone as held.
#[derive(Clone, Debug)]
pub struct KnotRelationView {
    pub held: LinkPairReading,
    pub history: LinkPairReading,
    pub show_history: bool,
    selected_predicate: Option<String>,
    /// Captured subject and object quotations, keyed by the signed assertion ID.
    passages: BTreeMap<String, (CapturedPassage, CapturedPassage)>,
    pub layout: scenograph::link::OpenedLinkLayout,
}

impl KnotRelationView {
    #[cfg(feature = "mere-retention")]
    pub fn from_projection(
        projection: &knot_editor::KnotDocumentProjection,
        space_id: [u8; 32],
    ) -> Result<Self, String> {
        let admitted = projection
            .documents
            .iter()
            .map(|document| document.id.clone())
            .collect::<BTreeSet<_>>();
        let visible = projection.relation_history_visible_to(&admitted);
        let source = LinkSource::Available {
            binding: SourceBinding {
                authority: "knot".into(),
                domain: "personal".into(),
                resource: format!("urn:knot:space:{}", hex(&space_id)),
            },
            revision: LinkSourceRevision::Unavailable {
                reason: "Knot exposes no stable public revision for this projection".into(),
            },
        };
        let omitted = projection.relations.len().saturating_sub(visible.len());
        let mut limits = Vec::new();
        if omitted != 0 {
            limits.push(scenograph::link::LinkCoverageLimit {
                layer: scenograph::link::LinkCoverageLayer::Disclosure,
                reason: "valid assertions whose captured endpoint documents are outside the admitted projection are omitted".into(),
                resources: Vec::new(),
                surfaces: Vec::new(),
                count: Some(omitted),
            });
        }
        if !projection.unverified_relations.is_empty() {
            limits.push(scenograph::link::LinkCoverageLimit {
                layer: scenograph::link::LinkCoverageLayer::Projection,
                reason: "unverified source captures are not shown as assertions".into(),
                resources: Vec::new(),
                surfaces: Vec::new(),
                count: Some(projection.unverified_relations.len()),
            });
        }
        if !projection.rejected_relations.is_empty() {
            limits.push(scenograph::link::LinkCoverageLimit {
                layer: scenograph::link::LinkCoverageLayer::Projection,
                reason: "rejected relation operations are not shown as assertions".into(),
                resources: Vec::new(),
                surfaces: Vec::new(),
                count: Some(projection.rejected_relations.len()),
            });
        }
        let history = make_reading(&source, limits.clone(), &visible, projection)?;
        let held_relations = visible
            .iter()
            .filter(|relation| relation.retractions.is_empty())
            .cloned()
            .collect::<Vec<_>>();
        let held = make_reading(&source, limits, &held_relations, projection)?;
        let mut view = Self::from_readings(held, history);
        view.passages = visible
            .iter()
            .map(|assertion| {
                (
                    format!("urn:knot:assertion:{}", hex(&assertion.id)),
                    (
                        CapturedPassage::from_endpoint(&assertion.subject),
                        CapturedPassage::from_endpoint(&assertion.object),
                    ),
                )
            })
            .collect();
        Ok(view)
    }

    /// Attach host-supplied admitted readings without captured passage text.
    pub fn from_readings(held: LinkPairReading, history: LinkPairReading) -> Self {
        let layout = scenograph::link::OpenedLinkLayout::default_for(&held);
        Self {
            held,
            history,
            layout,
            show_history: false,
            selected_predicate: None,
            passages: BTreeMap::new(),
        }
    }

    pub fn reading(&self) -> &LinkPairReading {
        if self.show_history {
            &self.history
        } else {
            &self.held
        }
    }
}

/// The Readings tile's shared opened-link presentation. Member actions are
/// deliberately empty: the view is a read-only projection, with only transient
/// history and arrangement controls owned by this window.
pub(crate) fn view(state: &DesktopState) -> DesktopView {
    use cambium::{button, el, span};
    let Some(view) = &state.knot_relations else {
        return Box::new(span(
            "No personal relation projection is attached to this window.",
        ));
    };
    let reading = view.reading();
    let held_selected = !view.show_history;
    let held = button("Held", |state: &mut DesktopState, _| {
        if let Some(view) = &mut state.knot_relations {
            if view.show_history {
                view.selected_predicate = None;
            }
            view.show_history = false;
        }
    })
    .attr("aria-pressed", held_selected.to_string());
    let history = button("Retained history", |state: &mut DesktopState, _| {
        if let Some(view) = &mut state.knot_relations {
            if !view.show_history {
                view.selected_predicate = None;
            }
            view.show_history = true;
        }
    })
    .attr("aria-pressed", view.show_history.to_string());
    let controls = el("div", (held, history)).attr("class", "knot-link-history-controls");
    let snapshot = el(
        "p",
        "Relation history below is a startup snapshot from the authenticated Knot store; it does not live-refresh.",
    );
    let (facet_controls, comparison) = facets::view(&state.ontology, view);
    let coverage: DesktopView = if reading.coverage.limits.is_empty() {
        Box::new(span(
            "This reading includes assertions from the disclosed documents.",
        ))
    } else {
        let limits = reading
            .coverage
            .limits
            .iter()
            .map(|limit| {
                format!(
                    "{}{}",
                    limit.reason,
                    limit
                        .count
                        .map_or(String::new(), |count| format!(" ({count})"))
                )
            })
            .collect::<Vec<_>>();
        Box::new(el("p", format!("Coverage limits: {}", limits.join("; "))))
    };
    if let Some(comparison) = comparison {
        return Box::new(
            el(
                "section",
                (controls, snapshot, coverage, facet_controls, comparison),
            )
            .attr("class", "knot-personal-opened-links")
            .attr("aria-label", "Personal relation assertions"),
        );
    }
    let mut links: Vec<DesktopView> = Vec::new();
    for (index, link) in reading.links.iter().enumerate() {
        let scope = scenograph::swatch::Scope::Link(link.key.clone());
        let swatch = scenograph::swatch::Swatch {
            id: format!("knot-opened-link-{index}"),
            scope,
            mode: scenograph::swatch::SwatchMode::Projection {
                definition_id: "knot.opened-link.read-only".into(),
                variant_id: None,
            },
        };
        let retained_history = view.show_history;
        let layout = view.layout.clone();
        let item = cambium::opened_link_swatch(
            &swatch,
            reading,
            &layout,
            scenograph::link::OpenedLinkOptions::default(),
            retained_history,
            |_, _, _, _| Vec::new(),
            |state: &mut DesktopState, event| {
                if let cambium::OpenedLinkEvent::Layout(layout) = event {
                    state.set_knot_relation_layout(layout);
                }
            },
        );
        let terms = link
            .buckets
            .iter()
            .flat_map(|bucket| &bucket.assertions)
            .filter_map(|assertion| {
                let vocabulary =
                    knot_readings::ontology::predicate_note(&state.ontology, &assertion.predicate)?;
                let label = assertion.label.as_deref().unwrap_or(&assertion.predicate);
                Some(format!("{label} · {vocabulary}"))
            })
            .collect::<BTreeSet<_>>();
        let vocabulary = (!terms.is_empty()).then(|| {
            let rows = terms
                .into_iter()
                .enumerate()
                .map(|(index, text)| (index, span(text)))
                .collect::<Vec<_>>();
            el("div", cambium::Keyed::new(rows))
        });
        links.push(Box::new(el("div", (item, vocabulary))));
    }
    if links.is_empty() {
        links.push(Box::new(span(if view.show_history {
            "No valid retained relation history connects the currently admitted documents."
        } else {
            "No held assertions connect the currently admitted documents."
        })));
    }
    Box::new(
        el(
            "section",
            (
                controls,
                snapshot,
                coverage,
                facet_controls,
                el(
                    "div",
                    cambium::Keyed::new(links.into_iter().enumerate().collect::<Vec<_>>()),
                ),
            ),
        )
        .attr("class", "knot-personal-opened-links")
        .attr("aria-label", "Personal relation assertions"),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(feature = "mere-retention")]
fn make_reading(
    source: &LinkSource,
    limits: Vec<scenograph::link::LinkCoverageLimit>,
    relations: &[knot_editor::KnotRelationAssertionV1],
    projection: &knot_editor::KnotDocumentProjection,
) -> Result<LinkPairReading, String> {
    let mut grouped = BTreeMap::<LinkKey, BTreeMap<(String, String), LinkBucket>>::new();
    for relation in relations {
        let predicate = projection.predicate_iri(relation).ok_or_else(|| {
            format!(
                "valid Knot relation {} has no resolved predicate IRI",
                hex(&relation.id)
            )
        })?;
        let key = LinkKey::new(
            LinkStratum::Surface,
            relation.subject.document_id.clone(),
            relation.object.document_id.clone(),
        );
        let direction = (
            relation.subject.document_id.clone(),
            relation.object.document_id.clone(),
        );
        let bucket = grouped
            .entry(key)
            .or_default()
            .entry(direction.clone())
            .or_insert_with(|| LinkBucket {
                displayed_from: direction.0.clone(),
                displayed_to: direction.1.clone(),
                source_from: LinkEndpoint {
                    id: direction.0.clone(),
                    address: None,
                },
                source_to: LinkEndpoint {
                    id: direction.1.clone(),
                    address: None,
                },
                assertions: Vec::new(),
                legacy_members: Vec::new(),
                legacy_summaries: Vec::new(),
            });
        let withdrawn =
            relation
                .retractions
                .first()
                .map(|retraction| scenograph::link::LinkWithdrawal {
                    at_ms: retraction.withdrawn_at_ms,
                    by: Some(format!("urn:knot:author:{}", hex(&retraction.author))),
                });
        bucket.assertions.push(LinkAssertion {
            id: format!("urn:knot:assertion:{}", hex(&relation.id)),
            predicate: predicate.to_owned(),
            recognized_kind: projection
                .predicates
                .lowers_to(&relation.predicate)
                .map(str::to_owned),
            label: projection.predicate_label(relation).map(str::to_owned),
            scope: LinkGraphScope::Custom(format!("urn:knot:space:{}", hex(&relation.scope))),
            asserter: Some(format!("urn:knot:author:{}", hex(&relation.author))),
            first_asserted_ms: relation.asserted_at_ms,
            asserted_at_ms: relation.asserted_at_ms,
            withdrawn,
            basis: LinkAssertionBasis {
                note: relation.qualification.clone(),
                method: None,
                evidence: relation.evidence.iter().cloned().collect(),
            },
        });
    }
    let links = grouped
        .into_iter()
        .map(|(key, buckets)| LinkReading {
            key,
            buckets: buckets.into_values().collect(),
        })
        .collect();
    Ok(LinkPairReading {
        source: source.clone(),
        links,
        coverage: scenograph::link::LinkCoverage { limits },
    })
}
