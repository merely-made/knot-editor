// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use p2panda_core::SigningKey;
use stickleback::DataKeyring;

fn main() {
    knot_editor::declaration::claim_launch();
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    #[cfg(feature = "projection-provider")]
    if args
        .first()
        .is_some_and(|mode| mode == "projection-provider")
    {
        let provider = parse_provider_args(&args)
            .unwrap_or_else(|message| panic!("{message}\nusage: knot_endpoint projection-provider <directory> <route> [--endpoint <app-endpoint>] [--max-sessions <1..64>]"));
        run_projection_provider(provider);
        return;
    }
    #[cfg(not(feature = "projection-provider"))]
    if args
        .first()
        .is_some_and(|mode| mode == "projection-provider")
    {
        panic!("projection-provider mode requires --features peers,projection-provider");
    }
    let mut endpoint = match args.as_slice() {
        [] => knot_editor::KnotEndpoint::fixture(),
        [root] => knot_editor::KnotEndpoint::open(PathBuf::from(root))
            .expect("Knot could not open the requested directory"),
        [mode, root] if mode == "directory" => knot_editor::KnotEndpoint::open(PathBuf::from(root))
            .expect("Knot could not open the requested directory"),
        [mode, root, max_source_bytes] if mode == "directory-write" => {
            let max_source_bytes = max_source_bytes
                .to_string_lossy()
                .parse::<u64>()
                .expect("directory-write byte limit must be an integer");
            knot_editor::KnotEndpoint::open_writable(
                PathBuf::from(root),
                knot_editor::KnotWriteGrant::new(max_source_bytes),
            )
            .expect("Knot could not open the requested writable directory")
        },
        [
            mode,
            root,
            max_source_bytes,
            evidence_root,
            max_evidence_bytes,
        ] if mode == "directory-write-evidence" => {
            let mut endpoint = knot_editor::KnotEndpoint::open_writable(
                PathBuf::from(root),
                knot_editor::KnotWriteGrant::new(parse_u64(
                    max_source_bytes,
                    "directory-write byte limit",
                )),
            )
            .expect("Knot could not open the requested writable directory");
            endpoint.grant_clip_evidence(
                knot_editor::BlobClipEvidenceStore::open(
                    PathBuf::from(evidence_root),
                    parse_u64(max_evidence_bytes, "clip evidence byte limit"),
                )
                .expect("Knot could not open the clip evidence blob store"),
            );
            endpoint
        },
        [
            mode,
            root,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
        ] if mode == "directory-write-effects" => {
            let root = PathBuf::from(root);
            let mut endpoint = knot_editor::KnotEndpoint::open_writable(
                &root,
                knot_editor::KnotWriteGrant::new(parse_u64(
                    max_source_bytes,
                    "directory-write byte limit",
                )),
            )
            .expect("Knot could not open the requested writable directory");
            endpoint.grant_effects(effect_authority(
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                max_source_bytes,
                Some(&root),
            ));
            endpoint
        },
        [
            mode,
            root,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
            evidence_root,
            max_evidence_bytes,
        ] if mode == "directory-write-effects-evidence" => {
            let root = PathBuf::from(root);
            let mut endpoint = knot_editor::KnotEndpoint::open_writable(
                &root,
                knot_editor::KnotWriteGrant::new(parse_u64(
                    max_source_bytes,
                    "directory-write byte limit",
                )),
            )
            .expect("Knot could not open the requested writable directory");
            endpoint.grant_effects(effect_authority(
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                max_source_bytes,
                Some(&root),
            ));
            endpoint.grant_clip_evidence(
                knot_editor::BlobClipEvidenceStore::open(
                    PathBuf::from(evidence_root),
                    parse_u64(max_evidence_bytes, "clip evidence byte limit"),
                )
                .expect("Knot could not open the clip evidence blob store"),
            );
            endpoint
        },
        [mode, data_root, persona, max_source_bytes] if mode == "persona-vault" => {
            let persona = persona
                .to_string_lossy()
                .parse::<uuid::Uuid>()
                .map(personae::PersonaId::from_uuid)
                .expect("persona-vault persona must be a UUID");
            let max_source_bytes = max_source_bytes
                .to_string_lossy()
                .parse::<u64>()
                .expect("persona-vault byte limit must be an integer");
            knot_editor::StartupUnlockedPersonalVault::open(
                PathBuf::from(data_root),
                persona,
                persona_keys(persona),
                [],
            )
            .and_then(|authority| {
                authority.into_endpoint(knot_editor::KnotWriteGrant::new(max_source_bytes))
            })
            .expect("Knot could not startup-unlock the requested persona vault")
        },
        [
            mode,
            data_root,
            persona,
            max_source_bytes,
            evidence_root,
            max_evidence_bytes,
        ] if mode == "persona-vault-evidence" => {
            let persona = persona
                .to_string_lossy()
                .parse::<uuid::Uuid>()
                .map(personae::PersonaId::from_uuid)
                .expect("persona-vault persona must be a UUID");
            let mut endpoint = knot_editor::StartupUnlockedPersonalVault::open(
                PathBuf::from(data_root),
                persona,
                persona_keys(persona),
                [],
            )
            .and_then(|authority| {
                authority.into_endpoint(knot_editor::KnotWriteGrant::new(parse_u64(
                    max_source_bytes,
                    "persona-vault byte limit",
                )))
            })
            .expect("Knot could not startup-unlock the requested persona vault");
            endpoint.grant_clip_evidence(
                knot_editor::BlobClipEvidenceStore::open(
                    PathBuf::from(evidence_root),
                    parse_u64(max_evidence_bytes, "clip evidence byte limit"),
                )
                .expect("Knot could not open the clip evidence blob store"),
            );
            endpoint
        },
        [
            mode,
            data_root,
            persona,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
        ] if mode == "persona-vault-effects" => {
            let persona = persona
                .to_string_lossy()
                .parse::<uuid::Uuid>()
                .map(personae::PersonaId::from_uuid)
                .expect("persona-vault persona must be a UUID");
            let mut endpoint = knot_editor::StartupUnlockedPersonalVault::open(
                PathBuf::from(data_root),
                persona,
                persona_keys(persona),
                [],
            )
            .and_then(|authority| {
                authority.into_endpoint(knot_editor::KnotWriteGrant::new(parse_u64(
                    max_source_bytes,
                    "persona-vault byte limit",
                )))
            })
            .expect("Knot could not startup-unlock the requested persona vault");
            endpoint.grant_effects(effect_authority(
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                max_source_bytes,
                None,
            ));
            endpoint
        },
        [
            mode,
            data_root,
            persona,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
            evidence_root,
            max_evidence_bytes,
        ] if mode == "persona-vault-effects-evidence" => {
            let persona = persona
                .to_string_lossy()
                .parse::<uuid::Uuid>()
                .map(personae::PersonaId::from_uuid)
                .expect("persona-vault persona must be a UUID");
            let mut endpoint = knot_editor::StartupUnlockedPersonalVault::open(
                PathBuf::from(data_root),
                persona,
                persona_keys(persona),
                [],
            )
            .and_then(|authority| {
                authority.into_endpoint(knot_editor::KnotWriteGrant::new(parse_u64(
                    max_source_bytes,
                    "persona-vault byte limit",
                )))
            })
            .expect("Knot could not startup-unlock the requested persona vault");
            endpoint.grant_effects(effect_authority(
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                max_source_bytes,
                None,
            ));
            endpoint.grant_clip_evidence(
                knot_editor::BlobClipEvidenceStore::open(
                    PathBuf::from(evidence_root),
                    parse_u64(max_evidence_bytes, "clip evidence byte limit"),
                )
                .expect("Knot could not open the clip evidence blob store"),
            );
            endpoint
        },
        [
            mode,
            root,
            max_source_bytes,
            resolve,
            run,
            schemes,
            languages,
            max_depth,
            max_ops,
        ] if mode == "communal-fixture-effects" => communal_fixture_endpoint(
            PathBuf::from(root),
            parse_u64(max_source_bytes, "communal fixture byte limit"),
            effect_authority(
                resolve,
                run,
                schemes,
                languages,
                max_depth,
                max_ops,
                max_source_bytes,
                None,
            ),
        ),
        _ => panic!(
            "usage: knot_endpoint [directory] | directory <root> | \
             directory-write <root> <max-source-bytes> | \
             directory-write-evidence <root> <max-source-bytes> <evidence-root> <max-evidence-bytes> | \
             directory-write-effects <root> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops> | \
             directory-write-effects-evidence <root> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops> \
             <evidence-root> <max-evidence-bytes> | \
             persona-vault <data-root> <persona-id> <max-source-bytes> | \
             persona-vault-evidence <data-root> <persona-id> <max-source-bytes> \
             <evidence-root> <max-evidence-bytes> | \
             persona-vault-effects <data-root> <persona-id> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops> | \
             persona-vault-effects-evidence <data-root> <persona-id> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops> \
             <evidence-root> <max-evidence-bytes> | \
             communal-fixture-effects <root> <max-source-bytes> \
             <resolve-mode> <run-mode> <schemes> <languages> <max-depth> <max-ops>"
        ),
    };
    graphshell_endpoint::stdio::serve_resumable_notifying(
        &mut endpoint,
        std::io::stdin(),
        std::io::stdout().lock(),
        Duration::from_millis(100),
    )
    .expect("Knot Graphshell endpoint failed");
}

#[cfg(feature = "projection-provider")]
struct ProjectionProviderArgs {
    root: PathBuf,
    route: ostiary::app_admission::AppRouteId,
    endpoint: Option<String>,
    max_sessions: usize,
}

#[cfg(feature = "projection-provider")]
fn parse_provider_args(args: &[std::ffi::OsString]) -> Result<ProjectionProviderArgs, String> {
    let [mode, root, route, options @ ..] = args else {
        return Err("projection-provider expects a directory, route, and optional options".into());
    };
    if mode != "projection-provider" {
        return Err("expected projection-provider mode".into());
    }
    let route = route
        .to_str()
        .ok_or("projection-provider route must be valid UTF-8")?;
    let route = ostiary::app_admission::AppRouteId::new(route.to_owned())
        .map_err(|error| format!("invalid projection-provider route: {error:?}"))?;
    if route.as_str() == "knot" {
        return Err(
            "projection-provider route `knot` is reserved for Djinn's built-in Knot route".into(),
        );
    }
    let mut endpoint = None;
    let mut max_sessions = 8usize;
    let mut max_sessions_seen = false;
    let mut index = 0;
    while index < options.len() {
        match options[index].to_str() {
            Some("--endpoint") if endpoint.is_none() => {
                index += 1;
                let value = options
                    .get(index)
                    .ok_or("--endpoint requires an app endpoint")?
                    .to_str()
                    .ok_or("projection-provider app endpoint must be valid UTF-8")?;
                endpoint = Some(value.to_owned());
            },
            Some("--max-sessions") if !max_sessions_seen => {
                max_sessions_seen = true;
                index += 1;
                let value = options
                    .get(index)
                    .ok_or("--max-sessions requires an integer")?
                    .to_str()
                    .ok_or("--max-sessions must be valid UTF-8")?;
                max_sessions = value
                    .parse::<usize>()
                    .map_err(|_| "--max-sessions must be an integer".to_owned())?;
                if !(1..=64).contains(&max_sessions) {
                    return Err("--max-sessions must be between 1 and 64".into());
                }
            },
            Some(option) => return Err(format!("unknown projection-provider option: {option}")),
            None => return Err("projection-provider options must be valid UTF-8".into()),
        }
        index += 1;
    }
    Ok(ProjectionProviderArgs {
        root: PathBuf::from(root),
        route,
        endpoint,
        max_sessions,
    })
}

#[cfg(feature = "projection-provider")]
const PROVIDER_LABEL: &str = "Knot directory";

#[cfg(feature = "projection-provider")]
fn run_projection_provider(args: ProjectionProviderArgs) {
    use graphshell_endpoint::door::{
        projection_offer::PROJECTION_PROVIDER_ROUTE, projection_provider,
    };
    use ostiary::{
        app_admission::{AppId, AppRouteId},
        app_client::AppConnection,
    };

    let provider_route =
        AppRouteId::new(PROJECTION_PROVIDER_ROUTE).expect("the provider protocol route is valid");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Knot could not start its app-door runtime");
    runtime.block_on(async move {
        let app = AppId::new(knot_editor::KNOT_APP_ID);
        let declaration = knot_editor::declaration::offering(args.route.clone(), PROVIDER_LABEL);
        let connection = match args.endpoint.as_deref() {
            Some(endpoint) => {
                AppConnection::open_declared_route_at(endpoint, app, provider_route, declaration)
                    .await
            },
            None => AppConnection::open_declared_route(app, provider_route, declaration).await,
        }
        .expect("Knot could not connect to Djinn's projection-provider route");
        let root = args.root;
        projection_provider::serve(
            connection,
            args.route,
            PROVIDER_LABEL.into(),
            args.max_sessions,
            move |_viewer| open_directory_projection(&root),
        )
        .await
        .expect("Knot projection-provider session failed");
    });
}

#[cfg(feature = "projection-provider")]
fn open_directory_projection(root: &Path) -> Result<knot_editor::KnotEndpoint, String> {
    knot_editor::KnotEndpoint::open(root)
        .map_err(|error| format!("Knot could not open the offered directory: {error}"))
}

#[cfg(test)]
#[cfg(feature = "projection-provider")]
#[path = "knot_endpoint/provider_cli_tests.rs"]
mod provider_cli_tests;

/// Process-only received-content fixture. Its keys are minted inside the
/// endpoint process and the root is caller-owned scratch storage, so this mode
/// does not turn group keys into CLI or environment authority.
fn communal_fixture_endpoint(
    root: PathBuf,
    max_source_bytes: u64,
    effects: knot_editor::KnotEffectAuthority,
) -> knot_editor::KnotEndpoint {
    const SPACE: [u8; 32] = [0xC1; 32];
    const RECEIVED_SEED: [u8; 32] = [0xC2; 32];
    const LOCAL_SEED: [u8; 32] = [0xC3; 32];
    const VAULT_KEY: [u8; 32] = [0xC4; 32];

    fs::create_dir_all(&root).expect("could not create communal fixture root");
    let received_writer = *SigningKey::from_bytes(&RECEIVED_SEED)
        .verifying_key()
        .as_bytes();
    let local_writer = *SigningKey::from_bytes(&LOCAL_SEED)
        .verifying_key()
        .as_bytes();
    let mut keys = DataKeyring::new();
    keys.rotate_random()
        .expect("could not mint communal fixture data epoch");
    let vault = knot_editor::KnotVault::open(root.join("vault"), VAULT_KEY)
        .expect("could not open fixture vault");
    let (store, _checkpoint_repair) =
        pollster::block_on(knot_editor::KnotSyncFileStore::open_with_checkpoint_repair(
            root.join("commons.redb"),
            SPACE,
            [received_writer, local_writer],
            knot_editor::KnotSyncCipher::CommonsData(&keys),
            &vault,
        ))
        .expect("could not open protected communal fixture sync store");
    let source = "\
# Received calculation

```rhai eval
40 + 2
```
";
    pollster::block_on(store.author_communal(
        &RECEIVED_SEED,
        &keys,
        &knot_editor::KnotSyncEvent::Put(knot_editor::VaultDocument {
            id: "received".into(),
            title: "Received calculation".into(),
            body: source.as_bytes().to_vec(),
            media_type: "text/vnd.knot".into(),
        }),
    ))
    .expect("could not author received communal fixture document");
    let mut endpoint = knot_editor::KnotEndpoint::from_communal_vault(
        vault,
        store,
        &LOCAL_SEED,
        keys,
        knot_editor::KnotWriteGrant::new(max_source_bytes),
    )
    .expect("could not open communal fixture endpoint");
    endpoint.grant_effects(effects);
    endpoint
}

/// The persona's keys, released by djinn. A pending persona (djinn absent or
/// Locked) ends the endpoint before anything opens (D12).
fn persona_keys(persona: personae::PersonaId) -> knot_editor::PersonalVaultKeys {
    knot_editor::PersonalVaultKeys::from_djinn(persona, Some("knot"))
        .expect("Knot could not obtain the persona's keys from djinn")
}

fn parse_u64(value: &OsStr, label: &str) -> u64 {
    value
        .to_string_lossy()
        .parse()
        .unwrap_or_else(|_| panic!("{label} must be an integer"))
}

fn parse_effect_mode(value: &OsStr) -> knot_editor::KnotEffectMode {
    match value.to_string_lossy().as_ref() {
        "auto" => knot_editor::KnotEffectMode::Auto,
        "ask" => knot_editor::KnotEffectMode::Ask,
        "never" => knot_editor::KnotEffectMode::Never,
        other => panic!("effect mode must be auto, ask, or never; got {other}"),
    }
}

fn parse_csv(value: &OsStr) -> Vec<String> {
    value
        .to_string_lossy()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
#[path = "knot_endpoint/tests.rs"]
mod tests;

#[path = "knot_endpoint/effects.rs"]
mod effects;

use effects::effect_authority;
#[cfg(test)]
use effects::{NetworkEffectFetcher, RoutedEffectFetcher};
