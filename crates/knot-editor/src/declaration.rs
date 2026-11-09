// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! What Knot declares to Djinn: the resident routes it opens, the projections
//! it offers and the custody it needs. The declaration rides every hello Knot
//! sends; the owner's grant decides what Knot actually receives, and anything
//! Knot leaves out here is refused however the owner grants it.

use ostiary::app_admission::AppRouteId;
use castellan_client::custody::CUSTODY_ROUTE;
use ostiary::declaration::{
    AppDeclaration, CustodyOperation, CustodyUse, EpochUse, NamePattern, OfferedProjection,
};

use crate::PersonalVaultKeys;

/// The resident's mere routes Knot's overmap layer opens (`mere/<domain>`).
pub const MERE_ROUTE_PREFIX: &str = "mere/";
/// The route Knot hands a completed site to the resident on
/// (`site_host::handoff::PUBLISHED_SITE_ROUTE`).
pub const PUBLISHED_SITE_ROUTE: &str = "published-site-v1";
/// The route Knot offers projections through
/// (`graphshell_door::projection_offer::PROJECTION_PROVIDER_ROUTE`).
pub const PROJECTION_PROVIDER_ROUTE: &str = "mere.projection-provider";
/// The resident's blob custody route, which a supervised Knot host reaches its
/// evidence through (cycle breaks P2 step 3).
pub const BLOB_CUSTODY_ROUTE: &str = "blob-custody-v1";

/// Take the handle Djinn launched this process with, before anything is
/// spawned (`ostiary::launch::claim`). Every Knot binary calls this first
/// in `main`; a process Djinn did not launch has none, and nothing happens.
pub fn claim_launch() {
    let _ = ostiary::launch::claim();
}

/// Knot's declaration, offering no projection.
pub fn app_declaration() -> AppDeclaration {
    AppDeclaration {
        routes: vec![
            NamePattern::exact(CUSTODY_ROUTE),
            NamePattern::prefix(MERE_ROUTE_PREFIX),
            NamePattern::exact(PUBLISHED_SITE_ROUTE),
            NamePattern::exact(PROJECTION_PROVIDER_ROUTE),
            NamePattern::exact(BLOB_CUSTODY_ROUTE),
        ],
        offers: Vec::new(),
        custody: CustodyUse {
            operations: [
                CustodyOperation::Status,
                CustodyOperation::Roster,
                CustodyOperation::WatchLock,
                CustodyOperation::ReleaseEpochKeys,
            ]
            .into(),
            epoch: PersonalVaultKeys::requests()
                .into_iter()
                .map(|request| EpochUse {
                    context: request.context,
                    device_bound: request.device_bound,
                    secret: !request.public_only,
                })
                .collect(),
            ..CustodyUse::default()
        },
    }
}

/// Knot's declaration, offering one projection under `route`.
pub fn offering(route: AppRouteId, label: impl Into<String>) -> AppDeclaration {
    let mut declaration = app_declaration();
    declaration.offers.push(OfferedProjection {
        route,
        label: label.into(),
    });
    declaration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declaration_names_exactly_what_knot_asks_custody_for() {
        let declaration = app_declaration();
        assert!(declaration.validate().is_ok());
        for request in PersonalVaultKeys::requests() {
            assert!(
                declaration
                    .custody
                    .epoch
                    .iter()
                    .any(|used| used.covers(&request.context, request.device_bound, request.public_only)),
                "{} is requested but undeclared",
                request.context
            );
        }
        assert_eq!(declaration.custody.epoch.len(), PersonalVaultKeys::requests().len());
        assert!(
            !declaration.custody.epoch.iter().any(|used| used.context == crate::SIGNING_KEY_CONTEXT
                && !used.device_bound
                && used.secret),
            "only the legacy writer's public key is declared"
        );
        assert!(declaration.declares_route(&AppRouteId::new("mere/notes").unwrap()));
        assert!(!declaration.declares_route(&AppRouteId::new("reservoir").unwrap()));
    }

    #[test]
    fn an_offer_is_declared_by_route() {
        let route = AppRouteId::new("knot-documents").unwrap();
        let declaration = offering(route.clone(), "Knot directory");
        assert_eq!(declaration.offer(&route).unwrap().label, "Knot directory");
        assert!(app_declaration().offer(&route).is_none());
    }

    #[cfg(feature = "projection-provider")]
    #[test]
    fn the_provider_route_matches_graphshell() {
        assert_eq!(
            PROJECTION_PROVIDER_ROUTE,
            graphshell_door::projection_offer::PROJECTION_PROVIDER_ROUTE
        );
    }

    #[cfg(feature = "door-custody")]
    #[test]
    fn the_blob_route_matches_the_door() {
        assert_eq!(BLOB_CUSTODY_ROUTE, graphshell_door::blob_door::BLOB_CUSTODY_ROUTE);
    }

    #[cfg(feature = "site-handoff")]
    #[test]
    fn the_site_route_matches_the_handoff() {
        assert_eq!(PUBLISHED_SITE_ROUTE, site_host::handoff::PUBLISHED_SITE_ROUTE);
    }
}
