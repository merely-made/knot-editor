// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Explicit file export over the same bounded lexical reading used by Knot.
use knot_readings::{
    metre_scene::{MetreTemplate, metre_scene},
    sound::{MAX_SELECTION_BYTES, SoundLayers, analyze},
};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 6 || args[5] != "--cmudict" {
        return Err("usage: knot-metre-scene INPUT OUTPUT FEET POSITIONS_PER_FOOT RHYME_PATTERN_OR_DASH --cmudict".into());
    }
    let input = Path::new(&args[0]);
    let mut text = String::new();
    std::fs::File::open(input)?
        .take(MAX_SELECTION_BYTES as u64 + 1)
        .read_to_string(&mut text)?;
    let reading = analyze(
        &text,
        0..text.len(),
        &BTreeMap::new(),
        &SoundLayers::default(),
    )?;
    let template = MetreTemplate {
        feet: args[2].parse()?,
        positions_per_foot: args[3].parse()?,
        rhyme_groups: if args[4] == "-" {
            vec![]
        } else {
            args[4].chars().map(|c| Some(c.to_string())).collect()
        },
    };
    let recipe = metre_scene(
        &text,
        &reading,
        sceno::SourceRef {
            adapter: "knot.file".into(),
            id: input.canonicalize()?.display().to_string(),
        },
        &template,
    )?;
    let bytes = serde_json::to_vec_pretty(&recipe)?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?
        .write_all(&bytes)?;
    println!(
        "Exported lexical candidate reading at revision {}",
        recipe.revision
    );
    Ok(())
}
