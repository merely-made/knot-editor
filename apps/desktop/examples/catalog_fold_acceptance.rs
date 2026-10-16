// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Finite fixture using the real desktop catalog, controls and preferences.
use knot_desktop::{DesktopLaunch, run_desktop_with_targets};
use knot_document::KnotDocumentSession;
use knot_file_catalog::KnotFileCatalog;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(
            "expected --initialize or --reopen and an absolute isolated fixture root".into(),
        );
    }
    let root = PathBuf::from(&args[1]);
    if !root.is_absolute() {
        return Err("fixture root must be absolute".into());
    }
    let source = root.join("source");
    let marker = source.join("PUBLIC-TEST-KEYS-NOT-A-PERSONAL-WALLET");
    let files = [
        ("Root.djot", "# Root\n[A](LeafA.djot) [B](LeafB.djot)\n"),
        ("LeafA.djot", "# Leaf A\n[Other](Other.djot)\n"),
        ("LeafB.djot", "# Leaf B\n[Other](Other.djot)\n"),
        ("Other.djot", "# Other\n[A](LeafA.djot) [C](LeafC.djot)\n"),
        ("LeafC.djot", "# Leaf C\n"),
    ];
    if args[0] == "--initialize" {
        if root.exists() && std::fs::read_dir(&root)?.next().is_some() {
            return Err("initialization requires an empty directory".into());
        }
        std::fs::create_dir_all(&source)?;
        std::fs::write(
            &marker,
            "Synthetic acceptance authority only; all keys are public test constants.\n",
        )?;
        for (name, text) in files {
            std::fs::write(source.join(name), text)?;
        }
    } else if args[0] != "--reopen" || !marker.exists() || !root.join("preferences.json").exists() {
        return Err("reopen requires the marked persisted fixture".into());
    }
    let mut catalog = KnotFileCatalog::open(&source, root.join("catalog.redb"))?;
    for (name, _) in files {
        catalog.bind(name)?;
    }
    let path = source.join("Root.djot");
    run_desktop_with_targets(
        DesktopLaunch {
            first: KnotDocumentSession::open(&path)?,
            first_path: Some(path),
            behind: vec![],
            sites: vec![],
            failures: vec![],
            recovery_path: None,
            mere_overlay: None,
        },
        Some(catalog),
        1_048_576,
        None,
        Some(root.join("preferences.json")),
        vec![],
        vec![],
        None,
    )?;
    Ok(())
}
