// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn effect_authority(
    resolve: &OsStr,
    run: &OsStr,
    schemes: &OsStr,
    languages: &OsStr,
    max_depth: &OsStr,
    max_ops: &OsStr,
    max_fetch_bytes: &OsStr,
    file_root: Option<&Path>,
) -> knot_editor::KnotEffectAuthority {
    let policy = knot_editor::KnotEffectPolicy {
        resolve: parse_effect_mode(resolve),
        run: parse_effect_mode(run),
        allowed_schemes: parse_csv(schemes),
        allowed_languages: parse_csv(languages),
        max_depth: parse_u64(max_depth, "effect max depth")
            .try_into()
            .expect("effect max depth must fit in u8"),
        max_ops: parse_u64(max_ops, "effect operation limit"),
    };
    assert!(
        !policy
            .allowed_schemes
            .iter()
            .any(|scheme| scheme == "titan"),
        "Titan is an upload protocol and cannot be admitted as a Knot read effect",
    );
    let has_file = policy.allowed_schemes.iter().any(|scheme| scheme == "file");
    let has_network = policy
        .allowed_schemes
        .iter()
        .any(|scheme| is_read_network_scheme(scheme));
    let has_rhai = policy
        .allowed_languages
        .iter()
        .any(|language| language == "rhai");
    let mut authority = knot_editor::KnotEffectAuthority::new(policy);
    if has_file || has_network {
        let file = has_file.then(|| {
            RootedFileFetcher::new(
                file_root.expect("file effects require a directory-root endpoint"),
            )
            .expect("could not admit effect file root")
        });
        let network = has_network.then(|| {
            NetworkEffectFetcher::new(
                parse_u64(max_fetch_bytes, "effect fetch byte limit")
                    .try_into()
                    .expect("effect fetch byte limit must fit in usize"),
            )
            .expect("could not start Knot network effect provider")
        });
        authority = authority.with_fetcher(RoutedEffectFetcher { file, network });
    }
    if has_rhai {
        authority = authority.register_evaluator(script_rhai::RhaiEvaluator::new());
    }
    authority
}

pub(super) fn is_read_network_scheme(scheme: &str) -> bool {
    matches!(
        scheme,
        "http" | "https" | "gemini" | "gopher" | "finger" | "spartan" | "nex" | "guppy"
    )
}

pub(super) struct RoutedEffectFetcher {
    pub(super) file: Option<RootedFileFetcher>,
    pub(super) network: Option<NetworkEffectFetcher>,
}

impl knot_editor::KnotEffectFetcher for RoutedEffectFetcher {
    fn fetch(&mut self, address: &str) -> Result<inker::Fetched, String> {
        let scheme = url::Url::parse(address)
            .map_err(|error| format!("could not parse effect address {address}: {error}"))?
            .scheme()
            .to_ascii_lowercase();
        match scheme.as_str() {
            "file" => self
                .file
                .as_mut()
                .ok_or_else(|| "Knot has no file effect provider".to_string())?
                .fetch(address),
            scheme if is_read_network_scheme(scheme) => self
                .network
                .as_mut()
                .ok_or_else(|| format!("Knot has no {scheme} effect provider"))?
                .fetch(address),
            _ => Err(format!("Knot has no fetch provider for {address}")),
        }
    }

    fn cache_version(&self) -> String {
        let network = self
            .network
            .as_ref()
            .map(knot_editor::KnotEffectFetcher::cache_version)
            .unwrap_or_else(|| "none".into());
        format!(
            "knot.routed-effect-fetcher/v1;file={};network={network}",
            self.file.is_some()
        )
    }
}

pub(super) struct NetworkEffectFetcher {
    pub(super) runtime: tokio::runtime::Runtime,
    pub(super) max_bytes: usize,
}

impl NetworkEffectFetcher {
    pub(super) fn new(max_bytes: usize) -> Result<Self, String> {
        fetch::install_in_memory_smolweb_tofu();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("could not build effect fetch runtime: {error}"))?;
        Ok(Self { runtime, max_bytes })
    }
}

impl knot_editor::KnotEffectFetcher for NetworkEffectFetcher {
    fn fetch(&mut self, address: &str) -> Result<inker::Fetched, String> {
        let fetched = self
            .runtime
            .block_on(fetch::fetch_page_anonymous_capped(address, self.max_bytes))?;
        Ok(inker::Fetched {
            content_type: fetched.content_type,
            body: fetched.body,
        })
    }

    fn cache_version(&self) -> String {
        format!(
            "knot.anonymous-network-effect-fetcher/v1;max-bytes={}",
            self.max_bytes
        )
    }
}

pub(super) struct RootedFileFetcher {
    pub(super) root: PathBuf,
}

impl RootedFileFetcher {
    pub(super) fn new(root: &Path) -> Result<Self, String> {
        Ok(Self {
            root: fs::canonicalize(root)
                .map_err(|error| format!("could not canonicalize file effect root: {error}"))?,
        })
    }
}

impl knot_editor::KnotEffectFetcher for RootedFileFetcher {
    fn fetch(&mut self, address: &str) -> Result<inker::Fetched, String> {
        let url = url::Url::parse(address)
            .map_err(|error| format!("could not parse effect address {address}: {error}"))?;
        if url.scheme() != "file" {
            return Err(format!("Knot has no fetch provider for {address}"));
        }
        let path = url
            .to_file_path()
            .map_err(|_| format!("could not map {address} to a local file"))?;
        let path = fs::canonicalize(&path)
            .map_err(|error| format!("could not resolve {}: {error}", path.display()))?;
        if !path.starts_with(&self.root) {
            return Err(format!(
                "{} is outside the admitted Knot directory",
                path.display()
            ));
        }
        let body = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        Ok(inker::Fetched {
            content_type: content_type(&path),
            body,
        })
    }
}

pub(super) fn content_type(path: &Path) -> Option<String> {
    match path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("gmi" | "gemini") => Some("text/gemini".into()),
        Some("md" | "markdown") => Some("text/markdown".into()),
        Some("knot") => Some("text/x-knot".into()),
        Some("txt") => Some("text/plain".into()),
        _ => None,
    }
}
