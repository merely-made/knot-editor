// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Family-only application preferences. The shared editor owns draft fields;
//! Knot owns their save boundary and its explicit writer metrics.
use std::collections::BTreeMap;
use tabard::typography::{FontFamily, FontPreferences, FontRole};

pub fn defaults() -> BTreeMap<FontRole, FontFamily> {
    FontRole::ALL
        .into_iter()
        .map(|role| {
            let family = match role {
                FontRole::Reading | FontRole::Heading => "Source Serif 4",
                FontRole::Code => "IBM Plex Mono",
                FontRole::Interface | FontRole::Label => "system-ui",
            };
            (
                role,
                FontFamily::new(family).expect("Knot's family defaults are valid"),
            )
        })
        .collect()
}

/// Family declarations precede exact authored CSS. Ordinary selector
/// specificity applies; size, leading, measure and writer face stay host-owned.
pub const ROLE_CSS: &str = concat!(
    ".knot-workspace { font-family:var(--tabard-font-interface,system-ui,sans-serif); }",
    ".knot-document-preview { font-family:var(--tabard-font-reading,'Source Serif 4',serif); }",
    ".knot-document-preview h1,.knot-document-preview h2,.knot-document-preview h3,.knot-document-preview h4,.knot-document-preview h5,.knot-document-preview h6 { font-family:var(--tabard-font-heading,'Source Serif 4',serif); }",
    ".knot-document-preview code,.knot-document-preview pre { font-family:var(--tabard-font-code,'IBM Plex Mono',monospace); }",
    ".knot-workspace .knot-recipe-occurrence-button { font-family:var(--tabard-font-label,system-ui,sans-serif); }",
);

pub fn diagnostics(preferences: &FontPreferences, available: &[String]) -> Vec<String> {
    preferences
        .resolve(available, &defaults())
        .into_iter()
        .filter_map(|resolved| {
            let missing: Vec<_> = resolved
                .requested
                .iter()
                .filter(|family| {
                    !family.is_generic()
                        && !available
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(family.name()))
                })
                .map(FontFamily::name)
                .collect();
            (!missing.is_empty()).then(|| format!(
            "{}: {} unavailable here; using {} with glyph fallback. Saved requests are retained.",
            resolved.role.label(), missing.join(", "), resolved.family.name()
        ))
        })
        .collect()
}
