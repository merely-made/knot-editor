// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Appearance preferences for the desktop host. They persist across launches
//! through [`crate::preferences`] and never enter document storage.
use serde::{Deserialize, Serialize};
use tinct::{Seeds, Srgb, derive_palette};

/// Fields missing from a stored file take their defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Appearance {
    pub dark: bool,
    pub highlight: bool,
    pub font_size: u8,
    pub source_face: SourceFace,
    pub measure: Measure,
    pub follow_hard_wrap: bool,
    pub relaxed: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            dark: false,
            highlight: true,
            font_size: 16,
            source_face: SourceFace::default(),
            measure: Measure::default(),
            follow_hard_wrap: true,
            relaxed: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceFace {
    #[default]
    IbmPlexMono,
    SystemMonospace,
}

impl SourceFace {
    pub fn css_family(self) -> &'static str {
        match self {
            Self::IbmPlexMono => "'IBM Plex Mono',monospace",
            Self::SystemMonospace => "monospace",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Measure {
    Narrow,
    #[default]
    Medium,
    Wide,
    Full,
}

impl Measure {
    pub fn columns(self) -> Option<usize> {
        match self {
            Self::Narrow => Some(60),
            Self::Medium => Some(72),
            Self::Wide => Some(90),
            Self::Full => None,
        }
    }
}

impl<'de> Deserialize<'de> for Appearance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The former wide toggle meant unrestricted width, not the new 90ch.
        // An explicit measure wins when a file carries both generations.
        #[derive(Deserialize)]
        #[serde(default, rename_all = "kebab-case")]
        struct Stored {
            dark: bool,
            highlight: bool,
            font_size: u8,
            source_face: SourceFace,
            measure: Option<Measure>,
            follow_hard_wrap: bool,
            wide: bool,
            relaxed: bool,
        }
        impl Default for Stored {
            fn default() -> Self {
                let defaults = Appearance::default();
                Self {
                    dark: defaults.dark,
                    highlight: defaults.highlight,
                    font_size: defaults.font_size,
                    source_face: defaults.source_face,
                    measure: None,
                    follow_hard_wrap: defaults.follow_hard_wrap,
                    wide: false,
                    relaxed: defaults.relaxed,
                }
            }
        }
        let stored = Stored::deserialize(deserializer)?;
        Ok(Self {
            dark: stored.dark,
            highlight: stored.highlight,
            font_size: stored.font_size,
            source_face: stored.source_face,
            measure: stored.measure.unwrap_or(if stored.wide {
                Measure::Full
            } else {
                Measure::Medium
            }),
            follow_hard_wrap: stored.follow_hard_wrap,
            relaxed: stored.relaxed,
        })
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

    pub fn source_columns(&self, text: &str) -> Option<usize> {
        if self.follow_hard_wrap
            && let Some(column) = knot_document::hard_wrap_column(text)
        {
            return Some(column.saturating_add(1));
        }
        self.measure.columns()
    }

    pub fn writing_style(&self, text: &str) -> String {
        self.body_style(self.source_face.css_family(), self.source_columns(text))
    }

    pub fn source_projection_style(&self, text: &str) -> String {
        format!(
            "{}width:auto;box-sizing:content-box;padding-left:1.75em;",
            self.writing_style(text)
        )
    }

    pub fn preview_style(&self) -> String {
        self.body_style("'Source Serif 4',serif", self.measure.columns())
    }

    fn body_style(&self, family: &str, columns: Option<usize>) -> String {
        // Direct ch lengths use the host's real font advances. Keep insets
        // outside this box so the measure describes text, not frame padding.
        let max_width = columns.map_or_else(|| "none".to_owned(), |column| format!("{column}ch"));
        format!(
            "font-family:{family};font-size:{}px;line-height:{};width:100%;max-width:{max_width};margin-left:auto;margin-right:auto;",
            self.font_size
                .clamp(Self::MIN_FONT_SIZE, Self::MAX_FONT_SIZE),
            if self.relaxed { "1.7" } else { "1.35" },
        )
    }

    pub(crate) fn graph_color(&self, state: &mere_view::NodeState) -> sprigging::ColorF {
        let palette = derive_palette(&seeds(self.dark));
        let color = match state {
            mere_view::NodeState::Available => palette.primary,
            mere_view::NodeState::Unavailable => palette.text_dim,
            mere_view::NodeState::Open => palette.success,
            mere_view::NodeState::Dirty => palette.danger,
        };
        sprigging::ColorF::new(
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            1.0,
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

/// The one focus selector for every control in a theme `scope` (navigation
/// plan decision 19). Both root classes outrank the pressed-toggle outline, so
/// a focused toggle still shows focus. Genet's selectors have `:focus` but not
/// `:focus-visible`, so a click that focuses a control shows it too.
pub fn focus_selector(scope: &str) -> String {
    format!(".knot-workspace{scope} :focus")
}

/// Both palettes are installed once; a root class switches the active theme.
pub fn appearance_css() -> String {
    let mut css = String::from(
        ".knot-workspace { min-height:100vh;box-sizing:border-box;font-family:system-ui,sans-serif;font-size:13px; } \
         .knot-workspace button,.knot-workspace input { font:inherit;padding:6px 10px;border:1px solid;border-radius:4px; } \
         .knot-workspace .knot-document-body textarea { font-family:inherit;font-size:inherit;line-height:inherit;padding:0;border:none;box-sizing:border-box;width:100%;min-width:0; } \
         .knot-document-body { line-height:inherit; } \
         .knot-workspace .knot-document-read-only { padding:0;border:none;box-sizing:border-box; } \
         .knot-workspace .knot-folded-source .fold-gutter { margin-left:-1.75em; } \
         .knot-workspace .knot-folding-actions { font-family:system-ui,sans-serif;font-size:13px;line-height:normal; } \
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
             {scope} .knot-writing-area,{scope} .knot-document-body textarea,{scope} .knot-document-read-only,{scope} .knot-path-popover,{scope} .knot-command-palette,{scope} .knot-folded-source,{scope} .knot-status-detail,{scope} .status-overflow-content {{ background:{};color:{};border-color:{}; }} \
             {scope} .status-bar {{ border-color:{}; }} \
             {scope} .knot-catalog-error,{scope} .knot-review-error,{scope} .knot-retention-error,{scope} .knot-outline-error,{scope} .knot-preferences-error,{scope} .status-chip[data-severity=warning] {{ color:{}; }} \
             {scope} .status-chip[data-severity=refused] {{ color:{};border-color:{}; }} \
             {scope} .detail-key {{ color:{}; }} \
             {scope} .frisket-tabbar {{ background:{};border-color:{}; }} \
             {scope} .frisket-divider {{ background:{}; }} \
             {scope} .frisket-tab {{ background:transparent;color:{}; }} \
             {scope} .frisket-tab.active {{ background:{};color:{};border-color:{}; }} \
             {scope} .frisket-content,{scope} .frisket-open-panel {{ background:{};color:{}; }} \
             {scope} .frisket-close {{ color:{}; }} \
             {scope} .tab-mark {{ color:{}; }}",
            rgb(p.bg), rgb(p.text), rgb(p.surface_2), rgb(p.text), rgb(p.text_dim),
            rgb(p.surface_hover), rgb(p.surface), rgb(p.text), rgb(p.text_dim),
            rgb(p.text_dim), rgb(p.danger),
            rgb(p.danger), rgb(p.danger), rgb(p.text_dim),
            rgb(p.bg), rgb(p.surface_hover), rgb(p.surface_hover), rgb(p.text_dim),
            rgb(p.surface), rgb(p.text), rgb(p.surface_hover), rgb(p.surface), rgb(p.text),
            rgb(p.text_dim), rgb(p.primary),
        ));
        css.push_str(&format!(
            "{scope} .knot-command-palette .command-item.selected {{ background:{};color:{}; }}",
            rgb(p.primary),
            rgb(p.on_primary),
        ));
        for rule in cambium::syntax_css(&seeds) {
            css.push_str(&format!("{scope} {rule}"));
        }
        css.push_str(&format!(
            "{} {{ outline:2px solid {};outline-offset:2px; }}",
            focus_selector(scope),
            rgb(p.primary),
        ));
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typography_hard_wrap_measure_changes_only_the_source() {
        let source = "This paragraph has physical lines of a similar length for reading.\nAnother paragraph continuation has the same physical line lengths.\nA final short line.\n";
        let column = knot_document::hard_wrap_column(source).unwrap();
        let mut appearance = Appearance::default();
        assert_eq!(appearance.source_columns(source), Some(column + 1));
        assert!(appearance.preview_style().contains("max-width:72ch"));
        appearance.measure = Measure::Narrow;
        assert_eq!(appearance.source_columns(source), Some(column + 1));
        assert!(appearance.preview_style().contains("max-width:60ch"));
        appearance.follow_hard_wrap = false;
        assert_eq!(appearance.source_columns(source), Some(60));
        appearance.measure = Measure::Full;
        assert_eq!(appearance.source_columns(source), None);
        assert!(appearance.writing_style(source).contains("max-width:none"));
    }
    #[test]
    fn palette_selection_has_a_contrasting_fill_in_both_themes() {
        let css = appearance_css();
        for dark in [false, true] {
            let p = derive_palette(&seeds(dark));
            let scope = if dark {
                ".knot-theme-dark"
            } else {
                ".knot-theme-light"
            };
            let rule = format!(
                "{scope} .knot-command-palette .command-item.selected {{ background:{};color:{}; }}",
                rgb(p.primary),
                rgb(p.on_primary)
            );
            assert!(css.contains(&rule));
            assert!(tinct::contrast(p.on_primary, p.primary) >= 4.5);
        }
    }

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

    #[test]
    fn each_theme_has_one_focus_rule_in_its_primary_token() {
        let css = appearance_css();
        for dark in [false, true] {
            let p = derive_palette(&seeds(dark));
            let scope = if dark {
                ".knot-theme-dark"
            } else {
                ".knot-theme-light"
            };
            let rule = format!(
                "{} {{ outline:2px solid {};outline-offset:2px; }}",
                focus_selector(scope),
                rgb(p.primary)
            );
            assert_eq!(css.matches(rule.as_str()).count(), 1, "{rule}");
            assert_eq!(css.matches(":focus").count(), 2, "one rule per theme");
            // Non-text contrast (WCAG 1.4.11). The 2px offset puts the ring on
            // the parent's ground, not on the control's own fill.
            for ground in [p.bg, p.surface] {
                let ratio = tinct::contrast(p.primary, ground);
                assert!(ratio >= 3.0, "dark={dark}: {ratio:.2}");
            }
        }
    }
}
