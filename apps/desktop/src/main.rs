// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Thin standalone host for the reusable Knot document surface.
use knot_desktop::{DesktopLaunch, run_desktop_with_targets_and_relations, workspace};

use knot_document::KnotDocumentSession;
use knot_file_catalog::KnotFileCatalog;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use workspace::DEFAULT_CAPTURE_MAX_BYTES;
const SCRATCH_ADDRESS: &str = "scratch:untitled";
#[derive(Debug, PartialEq, Eq)]
struct CatalogOptions {
    root: PathBuf,
    path: PathBuf,
}
#[derive(Debug, PartialEq, Eq)]
struct LaunchOptions {
    /// Document paths in the order named; none opens a scratch document.
    documents: Vec<PathBuf>,
    catalog: Option<CatalogOptions>,
    capture_max_bytes: usize,
    mere: Option<MereOptions>,
    /// The resident mere's domain whose `mere/<domain>` route shows as the
    /// overmap layer above the catalog graph. Needs `--mere-root` and
    /// `--mere-persona`.
    mere_domain: Option<String>,
    site_history: Option<PathBuf>,
    site_history_revisions: Option<usize>,
    site_history_journal_bytes: Option<usize>,
}
#[derive(Debug, PartialEq, Eq)]
struct MereOptions {
    root: PathBuf,
    persona: uuid::Uuid,
}
fn select_document<I: IntoIterator<Item = OsString>>(args: I) -> Result<LaunchOptions, String> {
    let mut args = args.into_iter();
    let _ = args.next();
    let mut documents = Vec::new();
    let mut root = None;
    let mut catalog_path = None;
    let mut capture_max_bytes = None;
    let mut positional_only = false;
    let mut mere_root = None;
    let mut mere_persona = None;
    let mut mere_domain = None;
    let mut site_history = None;
    let mut site_history_revisions = None;
    let mut site_history_journal_bytes = None;
    while let Some(arg) = args.next() {
        if !positional_only && arg == "--" {
            positional_only = true;
        } else if !positional_only
            && (arg == "--site-history-revisions" || arg == "--site-history-journal-bytes")
        {
            let value = args
                .next()
                .ok_or("site history limits require a positive integer")?;
            let number = value
                .to_str()
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .ok_or("site history limits require a positive integer")?
                .parse::<usize>()
                .map_err(|_| "site history limit exceeds the supported size")?;
            if number == 0 {
                return Err("site history limits must be positive".into());
            }
            let slot = if arg == "--site-history-revisions" {
                &mut site_history_revisions
            } else {
                &mut site_history_journal_bytes
            };
            if slot.replace(number).is_some() {
                return Err("duplicate site history limit".into());
            }
        } else if !positional_only && arg == "--site-history" {
            let value = args.next().ok_or("--site-history requires a directory")?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err("--site-history requires an explicit directory".into());
            }
            if site_history.replace(PathBuf::from(value)).is_some() {
                return Err("duplicate --site-history".into());
            }
        } else if !positional_only && (arg == "--mere-root" || arg == "--mere-persona") {
            let value = args
                .next()
                .ok_or("--mere-root and --mere-persona require values")?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err("--mere-root and --mere-persona require explicit values".into());
            }
            if arg == "--mere-root" {
                if mere_root.replace(PathBuf::from(value)).is_some() {
                    return Err("duplicate --mere-root".into());
                }
            } else {
                let persona =
                    uuid::Uuid::parse_str(value.to_str().ok_or("persona UUID must be UTF-8")?)
                        .map_err(|_| "--mere-persona requires a UUID")?;
                if mere_persona.replace(persona).is_some() {
                    return Err("duplicate --mere-persona".into());
                }
            }
        } else if !positional_only && arg == "--mere-domain" {
            let value = args.next().ok_or("--mere-domain requires a domain")?;
            let value = value
                .to_str()
                .filter(|value| {
                    !value.is_empty()
                        && !value.starts_with("--")
                        && !value.chars().any(char::is_whitespace)
                })
                .ok_or("--mere-domain requires a domain without whitespace")?;
            if mere_domain.replace(value.to_owned()).is_some() {
                return Err("duplicate --mere-domain".into());
            }
        } else if !positional_only && arg == "--capture-max-bytes" {
            if capture_max_bytes.is_some() {
                return Err("--capture-max-bytes was supplied more than once".into());
            }
            let value = args
                .next()
                .ok_or("--capture-max-bytes requires a byte count")?;
            let value = value
                .to_str()
                .filter(|value| {
                    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                })
                .ok_or("--capture-max-bytes requires a non-negative integer byte count")?;
            capture_max_bytes = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "--capture-max-bytes exceeds the supported byte count")?,
            );
        } else if !positional_only && (arg == "--catalog-root" || arg == "--catalog") {
            let slot = if arg == "--catalog-root" {
                &mut root
            } else {
                &mut catalog_path
            };
            if slot.is_some() {
                return Err(format!(
                    "{} was supplied more than once",
                    arg.to_string_lossy()
                ));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{} requires a path", arg.to_string_lossy()))?;
            if value.is_empty()
                || value == "--catalog-root"
                || value == "--catalog"
                || value == "--capture-max-bytes"
                || value == "--site-history"
                || value == "--site-history-revisions"
                || value == "--site-history-journal-bytes"
                || value == "--"
            {
                return Err(format!("{} requires a path", arg.to_string_lossy()));
            }
            *slot = Some(PathBuf::from(value));
        } else if !positional_only && arg.to_string_lossy().starts_with('-') {
            return Err(format!(
                "unknown option {}; use -- before a document path starting with -",
                arg.to_string_lossy()
            ));
        } else {
            documents.push(PathBuf::from(arg));
        }
    }
    let catalog = match (root, catalog_path) {
        (None, None) => None,
        (Some(root), Some(path)) => Some(CatalogOptions { root, path }),
        _ => return Err("--catalog-root and --catalog must be supplied together".into()),
    };
    if capture_max_bytes.is_some() && catalog.is_none() {
        return Err("--capture-max-bytes requires --catalog-root and --catalog".into());
    }
    let mere = match (mere_root, mere_persona) {
        (None, None) => None,
        (Some(root), Some(persona)) => Some(MereOptions { root, persona }),
        _ => return Err("--mere-root and --mere-persona must be supplied together".into()),
    };
    if mere_domain.is_some() && mere.is_none() {
        return Err("--mere-domain requires --mere-root and --mere-persona".into());
    }
    if site_history.is_none()
        && (site_history_revisions.is_some() || site_history_journal_bytes.is_some())
    {
        return Err("site history limits require --site-history".into());
    }
    Ok(LaunchOptions {
        documents,
        catalog,
        capture_max_bytes: capture_max_bytes.unwrap_or(DEFAULT_CAPTURE_MAX_BYTES),
        mere,
        mere_domain,
        site_history,
        site_history_revisions,
        site_history_journal_bytes,
    })
}
/// Open the named documents in order, the first in front. Every site folder
/// opens its own site tile and index page. A path that fails is reported
/// unless none opens.
fn open_documents(paths: Vec<PathBuf>) -> Result<DesktopLaunch, String> {
    let mut opened = Vec::new();
    let mut failures = Vec::new();
    for path in paths {
        match open_path(&path) {
            Ok(session) => opened.push((session, path)),
            Err(error) => {
                // Name the path unless the error already does.
                let shown = workspace::display_path(&path);
                failures.push(if error.contains(&shown) {
                    error
                } else {
                    format!("{shown}: {error}")
                });
            },
        }
    }
    if opened.is_empty() && !failures.is_empty() {
        return Err(failures.join("; "));
    }
    let sites = opened
        .iter()
        .skip(1)
        .filter(|(_, path)| path.is_dir())
        .map(|(_, path)| path.clone())
        .collect();
    let mut opened = opened.into_iter();
    let (first, first_path) = match opened.next() {
        Some((session, path)) => (session, Some(path)),
        None => (KnotDocumentSession::scratch(SCRATCH_ADDRESS, ""), None),
    };
    Ok(DesktopLaunch {
        first,
        first_path,
        behind: opened.map(|(session, _)| session).collect(),
        sites,
        failures,
        recovery_path: None,
        mere_overlay: None,
        site_history: None,
    })
}
fn open_path(path: &Path) -> Result<KnotDocumentSession, String> {
    if path.is_dir() {
        let site = knot_site::Site::open(path)?;
        KnotDocumentSession::open(site.page_path(site.config.format.index_file())?)
    } else {
        KnotDocumentSession::open(path)
    }
}
fn main() {
    // Only this feature talks to Djinn's door.
    #[cfg(feature = "mere-retention")]
    knot_editor::declaration::claim_launch();
    let options = select_document(std::env::args_os()).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    let mere_attachment = attach_mere(options.mere).unwrap_or_else(|error| {
        eprintln!("knot: mere attachment refused: {error}");
        std::process::exit(1)
    });
    let settings_root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("Knot");
    let titan_submission_error =
        knot_site::submission::initialize_submission_trust(&settings_root.join("titan-trust.json"))
            .err();
    let catalog = options
        .catalog
        .map(|options| KnotFileCatalog::open(options.root, options.path))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("knot: catalog could not be opened: {error}");
            std::process::exit(1)
        });
    let mut launch = open_documents(options.documents).unwrap_or_else(|error| {
        eprintln!("knot: {error}");
        std::process::exit(1)
    });
    launch.recovery_path = Some(settings_root.join("recovery"));
    launch.mere_overlay = options.mere_domain;
    launch.site_history = options
        .site_history
        .map(|path| {
            let backend = muniment::DirectoryBackend::open(&path)
                .map_err(|e| format!("site history could not be opened: {e}"))?;
            let backend: Box<dyn muniment::Backend + Send + Sync> = Box::new(backend);
            Ok::<_, String>(knot_site::history::SiteHistory::new(
                backend,
                knot_site::history::HistoryLimits {
                    max_revisions: options
                        .site_history_revisions
                        .unwrap_or(knot_site::history::HistoryLimits::default().max_revisions),
                    max_journal_bytes: options
                        .site_history_journal_bytes
                        .unwrap_or(knot_site::history::HistoryLimits::default().max_journal_bytes),
                },
            ))
        })
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("knot: {error}");
            std::process::exit(1)
        });
    if let Err(error) = run_desktop_with_targets_and_relations(
        launch,
        catalog,
        options.capture_max_bytes,
        Some(settings_root.join("readings")),
        Some(settings_root.join(knot_desktop::preferences::PREFERENCES_FILE)),
        vec![],
        mere_attachment.composition_targets,
        titan_submission_error,
        mere_attachment.relation_view,
    ) {
        eprintln!("knot: host failed: {error}");
        std::process::exit(1);
    }
}

type CompositionTargets =
    Vec<std::sync::Arc<dyn knot_composition::retention::CompositionRetainPort>>;

struct MereAttachment {
    composition_targets: CompositionTargets,
    relation_view: Option<knot_desktop::knot_relations::KnotRelationView>,
}

#[cfg(feature = "mere-retention")]
fn attach_mere(options: Option<MereOptions>) -> Result<MereAttachment, String> {
    let Some(options) = options else {
        return Ok(MereAttachment {
            composition_targets: vec![],
            relation_view: None,
        });
    };
    let persona = personae::PersonaId::from_uuid(options.persona);
    // Keys come from djinn; with djinn absent or Locked the mere is pending
    // and nothing attaches (dramatis D12). No device identity is minted here.
    let authority =
        knot_editor::StartupUnlockedPersonalVault::open_existing(&options.root, persona, || {
            knot_editor::PersonalVaultKeys::from_djinn(persona, None)
        })?;
    let relation_projection = authority.relation_projection()?;
    let relation_view = knot_desktop::knot_relations::KnotRelationView::from_projection(
        &relation_projection,
        authority.store().space_id(),
    )?;
    let resident = authority.into_resident_source()?;
    let port = resident
        .composition_retention(knot_editor::CompositionGrant::new(
            2 * 1024 * 1024,
            knot_composition::MAX_ITEMS,
        ))
        .map_err(|error| error.to_string())?;
    let bound = knot_editor::KnotResidentCompositionPort::new(
        knot_capture::KnotPersonaDisplayV1 {
            stable_id: format!("persona:{}", options.persona),
            label: format!("Knot mere · {}", options.persona),
        },
        port,
    )
    .map_err(|error| error.to_string())?;
    Ok(MereAttachment {
        composition_targets: vec![std::sync::Arc::new(bound)],
        relation_view: Some(relation_view),
    })
}

#[cfg(not(feature = "mere-retention"))]
fn attach_mere(options: Option<MereOptions>) -> Result<MereAttachment, String> {
    if options.is_some() {
        Err("this build lacks mere-retention; rebuild with --features mere-retention".into())
    } else {
        Ok(MereAttachment {
            composition_targets: vec![],
            relation_view: None,
        })
    }
}
#[cfg(test)]
mod tests;
