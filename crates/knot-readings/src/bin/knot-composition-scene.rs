// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Export one authored metre block through the same adapter the editor uses.
use knot_document::KnotDocumentSession;
use knot_readings::metre_block;
use nematic::knot::djot::divs::Div;
use scenomise::document::SceneDocument;
use std::io::Write;
fn find<'a>(divs: &'a [Div], id: &str, out: &mut Vec<&'a Div>) {
    for div in divs {
        if div.attributes.get("id").is_some_and(|s| s == id) {
            out.push(div)
        }
        find(&div.children, id, out);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: knot-composition-scene INPUT.djot BLOCK-ID OUTPUT.json|OUTPUT.djot".into(),
        );
    }
    let session = KnotDocumentSession::open_read_only(&args[0])?;
    let snapshot = session.div_snapshot()?;
    let mut found = Vec::new();
    find(&snapshot.divs, &args[1], &mut found);
    if found.len() != 1 {
        return Err("Choose one block by its unique id".into());
    }
    let div = found[0];
    if !div
        .attributes
        .get("class")
        .is_some_and(|s| s.split_whitespace().any(|s| s == "metre"))
    {
        return Err("Block does not declare a metre lens".into());
    }
    let (input, notes) = metre_block::scene(
        &snapshot.address,
        &args[1],
        &snapshot.source_text[div.body.clone()],
        &div.attributes,
    )?;
    let doc = SceneDocument::new(input, Vec::new());
    let bytes = if args[2].ends_with(".djot") {
        doc.to_djot("metre")?.into_bytes()
    } else {
        serde_json::to_vec_pretty(&doc)?
    };
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?
        .write_all(&bytes)?;
    println!(
        "Exported independent authored metre; {} review note(s)",
        notes.len()
    );
    Ok(())
}
