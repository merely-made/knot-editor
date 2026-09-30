// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Pinned, unmodified OFL faces embedded in the standalone desktop. Register by
//! their native metadata so upright/italic and static/variable weights survive.
use cambium_genet_winit_host::HostFont;

pub const IBM_PLEX_MONO_LICENSE: &str = include_str!("../assets/fonts/ibm-plex-mono/OFL.txt");
pub const SOURCE_SERIF_4_LICENSE: &str = include_str!("../assets/fonts/source-serif-4/OFL.txt");

pub fn bundled_fonts() -> Vec<HostFont> {
    const FACES: &[&[u8]] = &[
        include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-Regular.ttf"),
        include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-Bold.ttf"),
        include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-Italic.ttf"),
        include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-BoldItalic.ttf"),
        include_bytes!("../assets/fonts/source-serif-4/SourceSerif4.ttf"),
        include_bytes!("../assets/fonts/source-serif-4/SourceSerif4-Italic.ttf"),
    ];
    FACES
        .iter()
        .map(|bytes| HostFont {
            family: None,
            bytes: bytes.to_vec(),
        })
        .collect()
}
