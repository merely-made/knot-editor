// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// Local verification launcher: native site, explicit publication, public cert.
use knot_scroll_site::{LocalServer, Site};
fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("Usage: local SITE_FOLDER PUBLIC_CERT_PATH".into());
    }
    let root = std::path::Path::new(&args[0]);
    let site = if root.exists() {
        Site::open(root)?
    } else {
        Site::create(root)?
    };
    let server = LocalServer::start(site.publication()?, 0)?;
    // Public certificate only; the ephemeral private key never leaves memory.
    std::fs::write(&args[1], &server.certificate_pem).map_err(|e| e.to_string())?;
    println!("{}", server.url());
    println!("Press Enter to stop serving.");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;
    Ok(())
}
