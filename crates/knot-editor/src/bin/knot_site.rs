// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Hand a completed Knot site to the resident, which keeps serving it after
//! Knot exits.
//!
//! It reads a completed site once, turns it into a process-neutral snapshot,
//! and uploads bounded chunks on the resident's published-site route, as
//! Knot and under Knot's declaration. The listener is the resident's.

use std::path::Path;

use chirograph::{
    CarrierRequestBody, CarrierResponseBody, IntentInvocation, IntentResult, ProjectionSession,
};
use djinn::door::app_admission::{AppId, AppRouteId};
use graphshell_endpoint::door::projection::AppBrokerClient;
use knot_site::Site;
use knot_site::handoff::{
    GeminiListenerPolicyV1, PUBLISHED_SITE_CHUNK_INTENT, PUBLISHED_SITE_COMMIT_INTENT,
    PUBLISHED_SITE_PREPARE_INTENT, PUBLISHED_SITE_REMOVE_INTENT, PUBLISHED_SITE_ROUTE,
    PUBLISHED_SITE_SESSION, PUBLISHED_SITE_STOP_INTENT, PublishedSitePrepareV1,
    PublishedSiteStatusV1, upload_chunks,
};
use sceno::InstanceId;
use scenotime::{Revision, SceneEpoch};

const USAGE: &str = "usage: knot_site publish SITE_DIR [PORT] [--resume] | status | stop | remove";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Before the runtime's threads start.
    knot_editor::declaration::claim_launch();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or(USAGE)?;
    let mut client = AppBrokerClient::open_declared_route(
        AppId::new(knot_editor::KNOT_APP_ID),
        AppRouteId::new(PUBLISHED_SITE_ROUTE)?,
        knot_editor::declaration::app_declaration(),
    )
    .await?;
    client.open_session().await?;
    match command.as_str() {
        "publish" => {
            let site = args.next().ok_or("knot_site publish needs SITE_DIR")?;
            let mut port = 0;
            let mut resume_on_restart = false;
            for argument in args {
                if argument == "--resume" {
                    resume_on_restart = true;
                } else if port == 0 {
                    port = argument.parse::<u16>()?;
                } else {
                    return Err(USAGE.into());
                }
            }
            publish(&mut client, Path::new(&site), port, resume_on_restart).await?;
            print_status(&mut client).await?;
        },
        "status" => print_status(&mut client).await?,
        "stop" => {
            invoke(&mut client, PUBLISHED_SITE_STOP_INTENT, Vec::new()).await?;
            print_status(&mut client).await?;
        },
        "remove" => {
            invoke(&mut client, PUBLISHED_SITE_REMOVE_INTENT, Vec::new()).await?;
            print_status(&mut client).await?;
        },
        _ => return Err(USAGE.into()),
    }
    client.close().await?;
    Ok(())
}

async fn publish(
    client: &mut AppBrokerClient,
    site_dir: &Path,
    port: u16,
    resume_on_restart: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = Site::open(site_dir)?.publication()?.to_snapshot_v1()?;
    let bytes = snapshot.encode()?;
    let prepare = PublishedSitePrepareV1::for_snapshot(
        &bytes,
        GeminiListenerPolicyV1 {
            port,
            resume_on_restart,
            ..Default::default()
        },
    );
    invoke(
        client,
        PUBLISHED_SITE_PREPARE_INTENT,
        serde_json::to_vec(&prepare)?,
    )
    .await?;
    for chunk in upload_chunks(&bytes) {
        invoke(client, PUBLISHED_SITE_CHUNK_INTENT, serde_json::to_vec(&chunk)?).await?;
    }
    invoke(client, PUBLISHED_SITE_COMMIT_INTENT, Vec::new()).await
}

async fn print_status(client: &mut AppBrokerClient) -> Result<(), Box<dyn std::error::Error>> {
    let opened = client.open_session().await?;
    let request = opened
        .descriptor
        .projections
        .first()
        .ok_or("published-site route did not offer a status projection")?
        .request
        .clone();
    let snapshot = client.snapshot(request).await?;
    let offer = snapshot
        .presentation
        .offers
        .values()
        .flatten()
        .next()
        .ok_or("published-site status snapshot has no resource")?;
    let card_bytes = client
        .resource(snapshot.session.clone(), offer.resource)
        .await?;
    let card: chirograph::PortableCardV1 = serde_json::from_slice(&card_bytes)?;
    let status_resource = card
        .media
        .first()
        .ok_or("published-site status card has no typed status resource")?;
    let bytes = client.resource(snapshot.session, *status_resource).await?;
    let status: PublishedSiteStatusV1 = serde_json::from_slice(&bytes)?;
    println!("{}", serde_json::to_string_pretty(&status)?);
    Ok(())
}

async fn invoke(
    client: &mut AppBrokerClient,
    action: &str,
    payload: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = client
        .request_body(CarrierRequestBody::Intent(IntentInvocation {
            session: ProjectionSession(PUBLISHED_SITE_SESSION.into()),
            target: InstanceId(0),
            observed_epoch: SceneEpoch(1),
            observed_revision: Revision(1),
            intent: action.into(),
            payload,
        }))
        .await?;
    match response {
        CarrierResponseBody::Intent(IntentResult::Accepted) => Ok(()),
        CarrierResponseBody::Intent(IntentResult::Rejected { reason }) => Err(reason.into()),
        CarrierResponseBody::Intent(IntentResult::Stale { .. }) => {
            Err("published-site route is stale".into())
        },
        other => {
            Err(format!("published-site route returned unexpected response: {other:?}").into())
        },
    }
}
