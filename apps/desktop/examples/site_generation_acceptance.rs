// SPDX-License-Identifier: MPL-2.0
//! Marked synthetic files through the real native desktop and site tile.
use knot_desktop::{DesktopLaunch, run_desktop_with_targets};
use knot_document::KnotDocumentSession;
use site_host::{Site, SiteFormat};
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("absolute fixture root required")?,
    );
    if !root.is_absolute() {
        return Err("absolute fixture root required".into());
    }
    std::fs::create_dir_all(&root)?;
    let marker = root.join("SYNTHETIC-SITE-ACCEPTANCE");
    let source = root.join("source");
    if !marker.exists() {
        if std::fs::read_dir(&root)?.next().is_some() {
            return Err("new fixture must be empty".into());
        }
        std::fs::write(&marker, "Synthetic site content only; no personal data.\n")?;
        let site = Site::create_for(&source, SiteFormat::Gemini)?;
        std::fs::write(
            site.page_path("index.gmi")?,
            "# Garden catalogue\n\nPainting and music.\n=> about.gmi About the garden\n",
        )?;
    }
    let site = Site::open(&source)?;
    run_desktop_with_targets(
        DesktopLaunch {
            first: KnotDocumentSession::open(site.page_path("index.gmi")?)?,
            first_path: Some(source),
            behind: vec![],
            sites: vec![],
            failures: vec![],
            recovery_path: Some(root.join("recovery")),
            mere_overlay: None,
            site_history: None,
        },
        None,
        1_048_576,
        None,
        Some(root.join("preferences.json")),
        vec![],
        vec![],
        None,
    )?;
    Ok(())
}
