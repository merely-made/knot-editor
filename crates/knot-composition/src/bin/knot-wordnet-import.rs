// SPDX-License-Identifier: MPL-2.0
//! An explicit offline conversion tool, never a downloader or pack installer.

use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use knot_composition::wordnet_import::{ImportOptions, MAX_XML_BYTES, import_wordnet};

const USAGE: &str = "knot-wordnet-import --input LOCAL.xml --output NEW.json --source oewn --version 2025 --lemma tide [--lemma estuary ...]\nReads only local uncompressed UTF-8 WN-LMF; selects exact lemmas and one-hop neighbors.\nNo fetching, installation or overwriting. XML source/version must match explicit arguments.\nPrints a JSON conversion report on stdout; review exclusions before importing the pack.";

fn main() {
    if let Err(error) = run() {
        eprintln!("knot-wordnet-import: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = None;
    let mut output = None;
    let mut source = None;
    let mut version = None;
    let mut lemmas = Vec::new();
    let mut arguments = env::args_os().skip(1);
    while let Some(flag) = arguments.next() {
        if flag == "--help" || flag == "-h" {
            println!("{USAGE}");
            return Ok(());
        }
        let flag = flag.to_str().ok_or("argument flag must be UTF-8")?;
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {flag}\n{USAGE}"))?;
        match flag {
            "--input" if input.is_none() => input = Some(PathBuf::from(value)),
            "--output" if output.is_none() => output = Some(PathBuf::from(value)),
            "--source" if source.is_none() => {
                source = Some(value.into_string().map_err(|_| "source must be UTF-8")?)
            },
            "--version" if version.is_none() => {
                version = Some(value.into_string().map_err(|_| "version must be UTF-8")?)
            },
            "--lemma" => lemmas.push(value.into_string().map_err(|_| "lemma must be UTF-8")?),
            _ => return Err(format!("unknown or duplicate option {flag}\n{USAGE}").into()),
        }
    }
    let input = input.ok_or_else(|| format!("--input is required\n{USAGE}"))?;
    let output = output.ok_or_else(|| format!("--output is required\n{USAGE}"))?;
    let options = ImportOptions {
        source: source.ok_or("--source is required")?,
        version: version.ok_or("--version is required")?,
        lemmas,
    };
    if output.symlink_metadata().is_ok() {
        return Err("output already exists; choose a new path".into());
    }
    let file = File::open(&input)?;
    if !file.metadata()?.is_file() {
        return Err("input must be a regular local file".into());
    }
    if file.metadata()?.len() > MAX_XML_BYTES as u64 {
        return Err("input exceeds 256 MiB".into());
    }
    let mut xml = Vec::new();
    file.take(MAX_XML_BYTES as u64 + 1).read_to_end(&mut xml)?;
    let imported = import_wordnet(&xml, &options)?;
    let bytes = serde_json::to_vec_pretty(&imported.pack)?;
    if bytes.len() > reference_data::Limits::default().max_pack_bytes {
        return Err("pretty-printed output exceeds the reference pack size limit".into());
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    #[cfg(unix)]
    let directory = File::open(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".knot-wordnet-import-")
        .tempfile_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&output)?;
    #[cfg(unix)]
    directory.sync_all().map_err(|error| {
        format!("output was saved, but directory sync failed: {error}; inspect it before retrying")
    })?;
    println!("{}", serde_json::to_string_pretty(&imported.report)?);
    Ok(())
}
