// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Knot-owned native files and publication. The server can only see an explicit
//! immutable snapshot, never the draft filesystem or editor buffer.
mod local;
mod nomadnet;
mod server;
pub mod submission;
pub use local::LocalServer;
pub use nomadnet::NomadNetServerConfig;

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_PAGE_BYTES: usize = 1_048_576;
pub const MAX_SITE_BYTES: usize = 16 * MAX_PAGE_BYTES;
pub const CONFIG: &str = "site.json";

/// Native site profile. Gemini and Spartan both author Gemtext; their effects differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SiteFormat {
    #[default]
    Scroll,
    Gemini,
    Spartan,
    Micron,
}

impl SiteFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Scroll => "Scroll",
            Self::Gemini => "Gemini",
            Self::Spartan => "Spartan",
            Self::Micron => "Micron",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Scroll => "scroll",
            Self::Gemini | Self::Spartan => "gmi",
            Self::Micron => "mu",
        }
    }
    pub fn index_file(self) -> &'static str {
        match self {
            Self::Scroll => "index.scroll",
            Self::Gemini | Self::Spartan => "index.gmi",
            Self::Micron => "index.mu",
        }
    }
    pub fn default_port(self) -> Option<u16> {
        match self {
            Self::Scroll => Some(5699),
            Self::Gemini => Some(1965),
            Self::Spartan => Some(300),
            Self::Micron => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub path: String,
    pub author: String,
    pub language: String,
    pub classification: u8,
    pub published: String,
    pub modified: String,
    pub abstract_source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    pub version: u8,
    #[serde(default)]
    pub format: SiteFormat,
    pub pages: Vec<Page>,
}

fn plain_line(value: &str) -> bool {
    value.len() <= 1024 && !value.chars().any(char::is_control)
}

fn page_name(value: &str, format: SiteFormat) -> bool {
    value.ends_with(&format!(".{}", format.extension()))
        && value.len() <= 128
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

impl SiteConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.pages.is_empty() || self.pages.len() > 64 {
            return Err("Expected site version 1 with 1–64 pages".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for page in &self.pages {
            if !page_name(&page.path, self.format) || !names.insert(page.path.to_ascii_lowercase())
            {
                return Err(
                    "Page names must be unique plain filenames for the selected site format".into(),
                );
            }
            if !plain_line(&page.author) || page.classification > 9 {
                return Err("Author must be one line; classification must be 0–9".into());
            }
            if !page.language.is_empty()
                && (page.language.len() > 128
                    || page.language.parse::<language_tags::LanguageTag>().is_err())
            {
                return Err("Language must be a BCP47 tag, such as en-US, or empty".into());
            }
            for date in [&page.published, &page.modified] {
                if !date.is_empty()
                    && (!date.ends_with('Z')
                        || time::OffsetDateTime::parse(
                            date,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .is_err())
                {
                    return Err(
                        "Dates must be UTC timestamps, such as 2026-09-11T12:00:00Z, or empty"
                            .into(),
                    );
                }
            }
            if page.abstract_source.len() > MAX_PAGE_BYTES
                || (self.format == SiteFormat::Scroll
                    && !page
                        .abstract_source
                        .lines()
                        .any(|line| line.starts_with("# ")))
            {
                return Err("Each abstract needs a level-1 title (# Title), within 1 MiB".into());
            }
        }
        if !self
            .pages
            .iter()
            .any(|page| page.path == self.format.index_file())
        {
            return Err(format!("A site needs {}", self.format.index_file()));
        }
        Ok(())
    }
}

pub struct Site {
    root: PathBuf,
    baseline: Vec<u8>,
    pub config: SiteConfig,
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("{} exceeds {} bytes", path.display(), limit));
    }
    Ok(bytes)
}

impl Site {
    /// Creation requires a new directory. Existing folders are never populated
    /// or overwritten implicitly, including after an incomplete earlier create.
    pub fn create(root: &Path) -> Result<Self, String> {
        Self::create_for(root, SiteFormat::Scroll)
    }

    pub fn create_for(root: &Path, format: SiteFormat) -> Result<Self, String> {
        fs::create_dir(root).map_err(|e| e.to_string())?;
        fs::create_dir(root.join("assets")).map_err(|e| e.to_string())?;
        let pages = [
            (format!("index.{}", format.extension()), "My site"),
            (format!("about.{}", format.extension()), "About"),
            (format!("notes.{}", format.extension()), "Notes"),
        ];
        let config = SiteConfig {
            version: 1,
            format,
            pages: pages
                .iter()
                .map(|(path, title)| Page {
                    path: path.clone(),
                    author: String::new(),
                    language: "en".into(),
                    classification: 4,
                    published: String::new(),
                    modified: String::new(),
                    abstract_source: format!("# {title}\n"),
                })
                .collect(),
        };
        for (path, title) in pages {
            let links = config
                .pages
                .iter()
                .filter(|p| p.path != path)
                .map(|p| {
                    format!(
                        "=> /{} {}\n",
                        p.path,
                        p.path.trim_end_matches(&format!(".{}", format.extension()))
                    )
                })
                .collect::<String>();
            let body = if format == SiteFormat::Micron {
                // Guide-qualified Micron headings and ordinary same-node page
                // links. The source remains literal bytes owned by the site.
                let micron_links = config
                    .pages
                    .iter()
                    .filter(|p| p.path != path)
                    .map(|p| {
                        let label = p.path.trim_end_matches(&format!(".{}", format.extension()));
                        format!("`[{label}`:/page/{}]\n", p.path)
                    })
                    .collect::<String>();
                format!(">{title}\n\nWrite here.\n\n{micron_links}")
            } else {
                format!("# {title}\n\nWrite here.\n\n{links}")
            };
            fs::write(root.join(path), body).map_err(|e| e.to_string())?;
        }
        fs::write(
            root.join(CONFIG),
            serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Self::open(root)
    }

    pub fn open(root: &Path) -> Result<Self, String> {
        let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
        let config_path = fs::canonicalize(root.join(CONFIG)).map_err(|e| e.to_string())?;
        if config_path.parent() != Some(root.as_path()) {
            return Err("Site config escapes its folder".into());
        }
        let baseline = read_bounded(&config_path, MAX_SITE_BYTES)?;
        let config: SiteConfig = serde_json::from_slice(&baseline).map_err(|e| e.to_string())?;
        config.validate()?;
        let site = Self {
            root,
            baseline,
            config,
        };
        for page in &site.config.pages {
            site.page_path(&page.path)?;
        }
        Ok(site)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn page_path(&self, name: &str) -> Result<PathBuf, String> {
        if !self.config.pages.iter().any(|p| p.path == name) || !page_name(name, self.config.format)
        {
            return Err("Page is not in this site's manifest".into());
        }
        let path = fs::canonicalize(self.root.join(name)).map_err(|e| e.to_string())?;
        if path.parent() != Some(self.root.as_path()) || !path.is_file() {
            return Err("Page must be a file inside the site folder".into());
        }
        Ok(path)
    }

    /// Caller supplies the document writer's checked atomic replacement seam.
    /// A failed replacement never advances the accepted metadata baseline.
    pub fn save_config_with(
        &mut self,
        replace: impl FnOnce(&Path, &[u8], &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        self.config.validate()?;
        let path = self.root.join(CONFIG);
        if fs::canonicalize(&path).map_err(|e| e.to_string())? != path {
            return Err("Site config path changed".into());
        }
        if read_bounded(&path, MAX_SITE_BYTES)? != self.baseline {
            return Err("Site metadata changed on disk; reopen the site before saving".into());
        }
        let bytes = serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?;
        replace(&path, &self.baseline, &bytes)?;
        self.baseline = bytes;
        Ok(())
    }

    /// Only saved metadata and saved source enter the snapshot. Unsaved changes
    /// in this object are excluded just like an editor's unsaved text.
    pub fn publication(&self) -> Result<Publication, String> {
        let saved = Self::open(&self.root)?;
        let mut pages = BTreeMap::new();
        let mut total = 0;
        for page in saved.config.pages {
            let path = fs::canonicalize(saved.root.join(&page.path)).map_err(|e| e.to_string())?;
            if path.parent() != Some(saved.root.as_path()) {
                return Err("Page escapes site folder".into());
            }
            let source = read_bounded(&path, MAX_PAGE_BYTES)?;
            std::str::from_utf8(&source).map_err(|e| e.to_string())?;
            total += source.len() + page.abstract_source.len();
            if total > MAX_SITE_BYTES {
                return Err("Publication exceeds 16 MiB".into());
            }
            pages.insert(
                format!("/{}", page.path),
                PublishedPage {
                    metadata: page,
                    source,
                },
            );
        }
        Ok(Publication {
            pages,
            format: saved.config.format,
        })
    }
}

#[derive(Clone)]
struct PublishedPage {
    metadata: Page,
    source: Vec<u8>,
}

#[derive(Clone)]
pub struct Publication {
    pub format: SiteFormat,
    pages: BTreeMap<String, PublishedPage>,
}

impl Publication {
    fn native_page(&self, path: &str) -> Option<&PublishedPage> {
        if path.is_empty() || path == "/" {
            self.pages.get(&format!("/{}", self.format.index_file()))
        } else {
            self.pages.get(path)
        }
    }

    pub fn gemini_reply(&self, url: &url::Url, port: u16) -> gemini_protocol::server::Reply {
        use gemini_protocol::server::Reply;
        if self.format != SiteFormat::Gemini
            || url.scheme() != "gemini"
            || !matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
            || url.port().unwrap_or(1965) != port
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path().contains(';')
        {
            return Reply::header(59, "Invalid static-site request");
        }
        match self.native_page(url.path()) {
            Some(page) => {
                let language = &page.metadata.language;
                let mime = if language.is_empty() {
                    "text/gemini;charset=utf-8".into()
                } else {
                    format!("text/gemini;charset=utf-8;lang={language}")
                };
                Reply::success(mime, page.source.clone())
            },
            None => Reply::not_found("Not published"),
        }
    }

    pub fn spartan_reply(
        &self,
        request: &spartan_protocol::Request,
    ) -> spartan_protocol::SpartanResponse {
        use spartan_protocol::SpartanResponse;
        if self.format != SiteFormat::Spartan
            || !matches!(request.host.as_str(), "localhost" | "127.0.0.1")
            || !request.data.is_empty()
        {
            return SpartanResponse::ClientError {
                message: "This static site accepts read requests only".into(),
            };
        }
        match self.native_page(&request.path) {
            Some(page) => SpartanResponse::Success {
                mime: "text/gemini;charset=utf-8".into(),
                body: page.source.clone(),
            },
            None => SpartanResponse::ClientError {
                message: "Not published".into(),
            },
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Strict request envelope around the pinned protocol parser. URL paths are
    /// looked up in a snapshot map, never appended to a filesystem path.
    pub fn response(&self, line: &str, port: u16) -> Vec<u8> {
        let bad = || b"59 Invalid request\r\n".to_vec();
        if self.format != SiteFormat::Scroll
            || line.len() > 4096
            || !line.ends_with("\r\n")
            || !line.contains(' ')
            || line[..line.len().saturating_sub(2)]
                .chars()
                .any(char::is_control)
        {
            return bad();
        }
        let Some(request) = scroll_protocol::wire::parse_request(line) else {
            return bad();
        };
        let Ok(uri) = url::Url::parse(&request.uri) else {
            return bad();
        };
        if uri.scheme() != "scroll"
            || !matches!(uri.host_str(), Some("localhost" | "127.0.0.1"))
            || uri.port().unwrap_or(5699) != port
            || !uri.username().is_empty()
            || uri.password().is_some()
            || uri.query().is_some()
            || uri.fragment().is_some()
            || uri.path().contains(';')
        {
            return bad();
        }
        let path = if uri.path() == "/" {
            "/index.scroll"
        } else {
            uri.path()
        };
        let Some(page) = self.pages.get(path) else {
            return b"51 Not found\r\n".to_vec();
        };
        let metadata = &page.metadata;
        let language = if metadata.language.is_empty() {
            String::new()
        } else {
            format!(";lang={}", metadata.language)
        };
        let mut response = format!(
            "2{} text/scroll;charset=utf-8{}\r\n{}\r\n{}\r\n{}\r\n",
            metadata.classification,
            language,
            metadata.author,
            metadata.published,
            metadata.modified
        )
        .into_bytes();
        response.extend_from_slice(if request.metadata {
            metadata.abstract_source.as_bytes()
        } else {
            &page.source
        });
        response
    }
}
