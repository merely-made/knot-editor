// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Appearance preferences for the desktop host. They persist across launches
//! through [`crate::preferences`] and never enter document storage.
use serde::{Deserialize, Serialize};
use tinct::{Seeds, Srgb, derive_palette};

/// Fields missing from a stored file take their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Appearance {
    pub dark: bool,
    pub highlight: bool,
    pub font_size: u8,
    pub wide: bool,
    pub relaxed: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: false,
            highlight: true,
            font_size: 16,
            wide: false,
            relaxed: true,
        }
    }
}

impl Appearance {
    pub const MIN_FONT_SIZE: u8 = 12;
    pub const MAX_FONT_SIZE: u8 = 24;

    pub fn root_class(&self) -> &'static str {
        if self.dark {
            "knot-workspace knot-theme-dark"
        } else {
            "knot-workspace knot-theme-light"
        }
    }

    pub fn writing_style(&self) -> String {
        format!(
            "font-size:{}px;line-height:{};width:100%;max-width:{};margin-left:auto;margin-right:auto;",
            self.font_size
                .clamp(Self::MIN_FONT_SIZE, Self::MAX_FONT_SIZE),
            if self.relaxed { "1.7" } else { "1.35" },
            if self.wide { "none" } else { "900px" },
        )
    }
}

fn seeds(dark: bool) -> Seeds {
    Seeds {
        primary: Srgb::rgb(58, 102, 166),
        secondary: Srgb::rgb(44, 132, 125),
        tertiary: Srgb::rgb(166, 113, 47),
        neutral: Srgb::rgb(30, 34, 42),
        text_header: None,
        text_body: None,
        success: Srgb::rgb(55, 132, 80),
        danger: Srgb::rgb(200, 70, 76),
        dark,
    }
}

fn rgb(c: Srgb) -> String {
    format!("rgb({}, {}, {})", c.r, c.g, c.b)
}

/// Both palettes are installed once; a root class switches the active theme.
pub fn appearance_css() -> String {
    let mut css = String::from(
        ".knot-workspace { min-height:100vh;box-sizing:border-box;font-family:system-ui,sans-serif; } \
         .knot-workspace button,.knot-workspace input { font:inherit;padding:6px 10px;border:1px solid;border-radius:4px; } \
         .knot-document-body textarea { font-family:monospace;font-size:inherit;line-height:inherit;padding:16px;border:1px solid; } \
         .knot-document-body { line-height:inherit; } \
         .knot-document-status { font-size:13px;line-height:1.4; } \
         .knot-workspace button[aria-pressed=true] { outline:2px solid currentColor;outline-offset:1px; }",
    );
    for dark in [false, true] {
        let seeds = seeds(dark);
        let p = derive_palette(&seeds);
        let scope = if dark {
            ".knot-theme-dark"
        } else {
            ".knot-theme-light"
        };
        css.push_str(&format!(
            "{scope} {{ background:{};color:{}; }} \
             {scope} button,{scope} input {{ background:{};color:{};border-color:{}; }} \
             {scope} button:hover {{ background:{}; }} \
             {scope} .knot-document-body textarea,{scope} .knot-document-read-only {{ background:{};color:{};border-color:{}; }} \
             {scope} .knot-document-status {{ color:{}; }} \
             {scope} .knot-catalog-error,{scope} .knot-review-error,{scope} .knot-retention-error,{scope} .knot-outline-error,{scope} .knot-preferences-error {{ color:{}; }}",
            rgb(p.bg), rgb(p.text), rgb(p.surface_2), rgb(p.text), rgb(p.text_dim),
            rgb(p.surface_hover), rgb(p.surface), rgb(p.text), rgb(p.text_dim),
            rgb(p.text_dim), rgb(p.danger),
        ));
        for rule in cambium::syntax_css(&seeds) {
            css.push_str(&format!("{scope} {rule}"));
        }
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_themes_keep_body_and_syntax_legible_on_writing_surface() {
        for dark in [false, true] {
            let s = seeds(dark);
            let p = derive_palette(&s);
            assert!(tinct::contrast(p.text, p.surface) >= 4.5);
            let syntax = tinct::derive_syntax_palette(&s);
            for role in tinct::SyntaxRole::ALL {
                assert!(
                    tinct::contrast(syntax.role(role), p.surface) >= 4.5,
                    "{role:?}"
                );
            }
        }
    }
}
