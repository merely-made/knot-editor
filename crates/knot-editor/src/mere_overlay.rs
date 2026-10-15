//! A resident mere's sessions and archive as an overmap layer above Knot's
//! own graph.
//!
//! Knot's graph is its file catalog, and it stays the graph. When Djinn holds
//! a mere, Knot reads that mere's route through the first-party door as
//! `knot-editor` and shows the route's sessions and archive above the
//! catalog: sessions as containers, archives on them, and a pending merge as
//! its card. Session, archive and merge requests go to the route; graph and
//! node requests stay Knot's. Mark's ruling, 2026-10-15: "catalog, route as
//! overmap".
//!
//! The mapping is `mere-overmap`'s, not a copy: [`route::fill`] and
//! [`route::fill_archive`] and [`route::fill_merge`] read the projections,
//! [`route::intent`], [`route::archive_intent`] and [`route::merge_intent`]
//! write against exactly what the owner advertised. A merge's choices stay
//! here until Apply sends them all, as they do in `MereRouteSession`.
//! The route's own graph projection is read only so `fill` can mark the
//! attached session; it is never shown, so the catalog wins for display.
//! Without a resident mere, [`overlay`] leaves no sessions and no archive,
//! and [`declined_without_resident`] is the view's notice for a request that
//! needs one.

use std::collections::{BTreeMap, HashMap};

use chirograph::{
    CarrierRequestBody, CarrierResponseBody, ContentHash, IntentResult, PortableCardV1,
    ProjectionRequest, ProjectionSnapshot,
};
use graphshell_endpoint::door::app_admission::{AppId, AppRouteId, configured_app_endpoint};
use graphshell_endpoint::door::app_client::AppBrokerClient;
use mere_overmap::route;
use mere_overmap::wire::{PENDING_MERGE_SCHEMA, PendingMergeV1};
use mere_view::{
    ArchiveModel, MereViewModel, MereViewRequest, MergeModel, MergeOption, MergeRequest,
    SessionEntry,
};

/// The application id Knot is admitted under on the mere route.
pub const MERE_OVERLAY_APP: &str = "knot-editor";

/// What the overmap layer shows: the route's sessions and archive, without
/// its graph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MereOverlayReading {
    /// The mere's name, from its card.
    pub title: String,
    pub sessions: Vec<SessionEntry>,
    pub can_mint: bool,
    /// `None` when the route advertises no archive projection.
    pub archive: Option<ArchiveModel>,
    /// The pending merge, with the choices made so far; `None` when none
    /// waits.
    pub merge: Option<MergeModel>,
}

/// Whether a view request belongs to the overmap layer rather than to Knot's
/// graph: a session step, minting, an archive operation, or a step on the
/// pending merge.
pub fn is_overlay_request(request: &MereViewRequest) -> bool {
    matches!(
        request,
        MereViewRequest::Mint
            | MereViewRequest::Session(..)
            | MereViewRequest::Archive(_)
            | MereViewRequest::Merge(_)
    )
}

/// The view's notice for an overmap request with no resident mere attached.
pub fn declined_without_resident(request: &MereViewRequest) -> String {
    match request {
        MereViewRequest::Archive(_) => {
            "Archives come from a resident mere; open Knot with Djinn's mere to use them."
        },
        MereViewRequest::Merge(_) => {
            "Merges come from a resident mere; open Knot with Djinn's mere to use them."
        },
        _ => "Sessions come from a resident mere; open Knot with Djinn's mere to use them.",
    }
    .to_owned()
}

/// Lay the overmap layer over a model whose graph is Knot's own. `None` is
/// Knot without a resident mere: no sessions, no mint, no archive. The graph,
/// its layout, actions and notice are left as the host set them.
pub fn overlay(model: &mut MereViewModel, reading: Option<&MereOverlayReading>) {
    model.hide_graph = false;
    match reading {
        Some(reading) => {
            model.sessions = reading.sessions.clone();
            model.can_mint = reading.can_mint;
            model.archive = reading.archive.clone();
            model.merge = reading.merge.clone();
        },
        None => {
            model.sessions.clear();
            model.can_mint = false;
            model.archive = None;
            model.merge = None;
        },
    }
}

/// One admitted connection to a resident mere's route.
pub struct MereOverlay {
    client: AppBrokerClient,
    sessions_request: ProjectionRequest,
    graph_request: ProjectionRequest,
    archive_request: Option<ProjectionRequest>,
    sessions: Option<ProjectionSnapshot>,
    archive: Option<ProjectionSnapshot>,
    merge: Option<PendingMergeV1>,
    merge_choices: BTreeMap<String, MergeOption>,
}

impl MereOverlay {
    /// Open `mere/<domain>` at the configured first-party endpoint.
    pub async fn open(domain: &str) -> Result<Self, String> {
        Self::open_at(&configured_app_endpoint(), domain).await
    }

    /// Open `mere/<domain>` at an explicit endpoint. Tests and receipts use
    /// this.
    pub async fn open_at(endpoint: &str, domain: &str) -> Result<Self, String> {
        let route = AppRouteId::new(format!("mere/{domain}")).map_err(|e| e.to_string())?;
        let mut client =
            AppBrokerClient::open_route_at(endpoint, AppId::new(MERE_OVERLAY_APP), route)
                .await
                .map_err(|e| e.to_string())?;
        let descriptor = client
            .open_session()
            .await
            .map_err(|e| e.to_string())?
            .descriptor;
        let (sessions_request, graph_request) = route::requests(&descriptor)
            .ok_or("The resident's route offers no mere sessions and graph.")?;
        Ok(Self {
            client,
            sessions_request,
            graph_request,
            archive_request: route::archive_request(&descriptor),
            sessions: None,
            archive: None,
            merge: None,
            merge_choices: BTreeMap::new(),
        })
    }

    /// Read the route's sessions and archive.
    pub async fn read(&mut self) -> Result<MereOverlayReading, String> {
        let sessions = self.snapshot(self.sessions_request.clone()).await?;
        let graph = self.snapshot(self.graph_request.clone()).await?;
        let archive = match self.archive_request.clone() {
            Some(request) => Some(self.snapshot(request).await?),
            None => None,
        };
        let mut bytes: HashMap<ContentHash, Vec<u8>> = HashMap::new();
        for snapshot in [&sessions].into_iter().chain(archive.iter()) {
            for request in route::card_requests(snapshot) {
                let content = self
                    .client
                    .resource(request.session, request.resource)
                    .await
                    .map_err(|e| e.to_string())?;
                bytes.insert(request.resource, content);
            }
        }
        let merge = self.pending_merge(&sessions, &bytes).await?;
        if merge.as_ref().map(|merge| merge.source) != self.merge.as_ref().map(|merge| merge.source)
        {
            // Another merge (or none): the choices made for the last one are
            // not choices for this one.
            self.merge_choices.clear();
        }
        let mut model = MereViewModel::default();
        route::fill(&mut model, &sessions, &graph, &bytes);
        if let Some(archive) = &archive {
            route::fill_archive(&mut model, &sessions, archive, &bytes);
        }
        route::fill_merge(&mut model, merge.as_ref(), &self.merge_choices);
        self.sessions = Some(sessions);
        self.archive = archive;
        self.merge = merge;
        Ok(MereOverlayReading {
            title: model.title,
            sessions: model.sessions,
            can_mint: model.can_mint,
            archive: model.archive,
            merge: model.merge,
        })
    }

    /// Send one overmap request against the last reading. `Ok(None)` is
    /// accepted; `Ok(Some(reason))` is the route's refusal for the view's
    /// notice. Read again afterwards.
    pub async fn take(&mut self, request: &MereViewRequest) -> Result<Option<String>, String> {
        let Some(sessions) = &self.sessions else {
            return Ok(Some("Read the resident's mere before acting on it.".into()));
        };
        // A choice stays here until Apply sends them all.
        if let MereViewRequest::Merge(MergeRequest::Choose(key, option)) = request {
            let known = self.merge.as_ref().is_some_and(|merge| {
                merge
                    .conflicts
                    .iter()
                    .any(|conflict| &route::conflict_key(&conflict.part) == key)
            });
            if !known {
                return Ok(Some("That conflict is no longer pending.".into()));
            }
            self.merge_choices.insert(key.clone(), option.clone());
            return Ok(None);
        }
        let invocation = route::intent(request, sessions)
            .or_else(|| {
                self.archive
                    .as_ref()
                    .and_then(|archive| route::archive_intent(request, sessions, archive))
            })
            .or_else(|| {
                self.merge.as_ref().and_then(|merge| {
                    route::merge_intent(request, sessions, merge, &self.merge_choices)
                })
            });
        if invocation.is_none()
            && self.merge.is_some()
            && matches!(request, MereViewRequest::Merge(MergeRequest::Apply))
        {
            return Ok(Some(
                "Choose a side for every conflict before applying.".into(),
            ));
        }
        let Some(invocation) = invocation else {
            return Ok(Some(
                "The resident no longer offers that action; read the mere again.".into(),
            ));
        };
        match self
            .client
            .request_body(CarrierRequestBody::Intent(invocation))
            .await
            .map_err(|e| e.to_string())?
        {
            CarrierResponseBody::Intent(result) => {
                if matches!(
                    request,
                    MereViewRequest::Merge(MergeRequest::Apply | MergeRequest::Cancel)
                ) && result == IntentResult::Accepted
                {
                    self.merge_choices.clear();
                    self.merge = None;
                }
                Ok(route::declined(&result))
            },
            _ => Err("The resident did not answer the action.".into()),
        }
    }

    /// The pending merge the sessions projection discloses, read through its
    /// card's first media: the conflicts and the two sessions' names.
    async fn pending_merge(
        &mut self,
        sessions: &ProjectionSnapshot,
        cards: &HashMap<ContentHash, Vec<u8>>,
    ) -> Result<Option<PendingMergeV1>, String> {
        let Some(instance) = route::merge_cards(sessions).first().copied() else {
            return Ok(None);
        };
        let card: PortableCardV1 = sessions
            .presentation
            .offers_for(instance)
            .and_then(|offers| offers.first())
            .and_then(|offer| cards.get(&offer.resource))
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
            .ok_or("The pending merge's card is not disclosed.")?;
        let material = *card
            .media
            .first()
            .ok_or("The pending merge's card discloses no material.")?;
        let bytes = self
            .client
            .resource(sessions.session.clone(), material)
            .await
            .map_err(|e| e.to_string())?;
        let merge: PendingMergeV1 = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid pending merge material: {e}"))?;
        if merge.schema != PENDING_MERGE_SCHEMA {
            return Err("The pending merge material has another schema.".into());
        }
        Ok(Some(merge))
    }

    async fn snapshot(&mut self, request: ProjectionRequest) -> Result<ProjectionSnapshot, String> {
        self.client
            .snapshot(request)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use mere_view::{ArchiveRequest, GraphModel, NodeEntry, NodeState};

    use super::*;

    fn catalog_model() -> MereViewModel {
        MereViewModel {
            title: "Knot graph".into(),
            hide_graph: true,
            graph: GraphModel {
                nodes: vec![NodeEntry {
                    key: "doc".into(),
                    label: "Doc".into(),
                    state: NodeState::Available,
                }],
                relations: Vec::new(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn without_a_resident_the_layer_is_empty_and_the_catalog_graph_shows() {
        let mut model = catalog_model();
        model.can_mint = true;
        model.archive = Some(ArchiveModel::default());
        overlay(&mut model, None);
        assert!(model.sessions.is_empty());
        assert!(!model.can_mint);
        assert_eq!(model.archive, None);
        assert!(!model.hide_graph);
        assert_eq!(model.graph, catalog_model().graph);
        assert_eq!(model.title, "Knot graph");
    }

    #[test]
    fn a_reading_adds_sessions_and_archive_and_keeps_the_catalog_graph() {
        let mut model = catalog_model();
        let reading = MereOverlayReading {
            title: "Divination".into(),
            sessions: Vec::new(),
            can_mint: true,
            archive: Some(ArchiveModel {
                can_compose: true,
                ..Default::default()
            }),
            merge: Some(MergeModel {
                source: pandect::SessionId::from_uuid(uuid::Uuid::from_u128(7)),
                source_name: "Branch".into(),
                target_name: "Main".into(),
                conflicts: Vec::new(),
            }),
        };
        overlay(&mut model, Some(&reading));
        assert!(model.can_mint);
        assert_eq!(model.archive, reading.archive);
        assert_eq!(
            model.merge, reading.merge,
            "the pending merge shows as its card"
        );
        overlay(&mut model, None);
        assert_eq!(model.merge, None, "no resident, no merge");
        assert_eq!(model.graph, catalog_model().graph);
        assert_eq!(
            model.title, "Knot graph",
            "the catalog keeps the region's name"
        );
    }

    #[test]
    fn only_session_archive_and_merge_requests_belong_to_the_layer() {
        assert!(is_overlay_request(&MereViewRequest::Mint));
        for step in [
            MergeRequest::Choose("part".into(), MergeOption::Source),
            MergeRequest::Apply,
            MergeRequest::Cancel,
        ] {
            assert!(
                is_overlay_request(&MereViewRequest::Merge(step)),
                "a merge step goes to the route"
            );
        }
        assert!(
            declined_without_resident(&MereViewRequest::Merge(MergeRequest::Apply))
                .contains("Merges")
        );
        assert!(is_overlay_request(&MereViewRequest::Archive(
            ArchiveRequest::Compose(Vec::new())
        )));
        assert!(!is_overlay_request(&MereViewRequest::Activate(
            "doc".into()
        )));
        assert!(!is_overlay_request(&MereViewRequest::Layout(
            "grid.default".into()
        )));
        assert!(!is_overlay_request(&MereViewRequest::Host("open".into())));
        assert!(
            declined_without_resident(&MereViewRequest::Archive(ArchiveRequest::Open("x".into())))
                .contains("Archives")
        );
    }
}
