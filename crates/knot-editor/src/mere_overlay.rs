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

use std::collections::BTreeMap;

use chirograph::{CarrierRequestBody, CarrierResponseBody, IntentResult, ProjectionSnapshot};
use graphshell_endpoint::door::projection::AppBrokerClient;
use mere_overmap::materialization::{self, GraphRead};
use mere_overmap::route;
use mere_overmap::wire::PendingMergeV1;
use mere_view::{
    ArchiveModel, MereViewModel, MereViewRequest, MergeModel, MergeOption, MergeRequest,
    SessionEntry,
};
use ostiary::app_admission::{AppId, AppRouteId, configured_app_endpoint};

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
        let mut client = AppBrokerClient::open_declared_route_at(
            endpoint,
            AppId::new(MERE_OVERLAY_APP),
            route,
            crate::declaration::app_declaration(),
        )
        .await
        .map_err(|e| e.to_string())?;
        let descriptor = client
            .open_session()
            .await
            .map_err(|e| e.to_string())?
            .descriptor;
        route::requests(&descriptor)
            .ok_or("The resident's route offers no mere sessions and graph.")?;
        Ok(Self {
            client,
            sessions: None,
            archive: None,
            merge: None,
            merge_choices: BTreeMap::new(),
        })
    }

    /// Read the route's sessions and archive.
    pub async fn read(&mut self) -> Result<MereOverlayReading, String> {
        let reading = match materialization::read(
            &mut self.client,
            MERE_OVERLAY_APP,
            GraphRead::ProjectionOnly,
        )
        .await
        {
            Ok(reading) => reading,
            Err(reason) => {
                // No cached owner action remains valid after a failed refresh.
                self.sessions = None;
                self.archive = None;
                self.merge = None;
                self.merge_choices.clear();
                return Err(reason);
            },
        };
        reconcile_merge_choices(
            &mut self.merge_choices,
            self.merge.as_ref(),
            reading.pending_merge.as_ref(),
        );
        let mut model = MereViewModel::default();
        reading.fill_model(&mut model);
        route::fill_merge(
            &mut model,
            reading.pending_merge.as_ref(),
            &self.merge_choices,
        );
        self.sessions = Some(reading.sessions);
        self.archive = reading.archive;
        self.merge = reading.pending_merge;
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
}

/// Adopt owner-carried choices once per materialized plan. Ordinary refreshes
/// must retain choices the user made after that plan was first observed.
fn reconcile_merge_choices(
    choices: &mut BTreeMap<String, MergeOption>,
    previous: Option<&PendingMergeV1>,
    incoming: Option<&PendingMergeV1>,
) {
    if incoming.map(|merge| merge.source) != previous.map(|merge| merge.source) {
        choices.clear();
    }
    if let Some(merge) = incoming {
        if previous != incoming {
            route::adopt_merge(choices, merge);
        }
        choices.retain(|key, _| {
            merge
                .conflicts
                .iter()
                .any(|conflict| route::conflict_key(&conflict.part) == *key)
        });
    } else {
        choices.clear();
    }
}

#[cfg(test)]
mod tests;
