// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Geometry receipts for Knot's bundled typefaces through the desktop host.

use cambium::{AnyView, GenetCtx, GenetElement, el, text};
use cambium_genet_winit_host::{Harness, HostFont, Init, WindowCommands, inert_hooks};
use knot_desktop::{
    appearance::{Appearance, Measure},
    desktop_sheet,
    fonts::bundled_fonts,
    host_hooks,
    workspace::{DesktopState, DesktopView, desktop_view},
};
use knot_document::KnotDocumentSession;
use taproot::Selector;

const SERIF_STYLE_SAMPLE: &str = "AVATAR fff iiiiii WWWW jjjj";

#[derive(Default)]
struct App;

type Child = Box<dyn AnyView<App, (), GenetCtx, GenetElement>>;
type FontHarness = Harness<App, fn(&App) -> Child, Child>;

fn root(_state: &App) -> Child {
    Box::new(el(
        "div",
        (
            el("span", text("00000000")).attr("class", "plex-zeroes"),
            el("span", text("iiiiiiii")).attr("class", "serif-narrow"),
            el("span", text("WWWWWWWW")).attr("class", "serif-wide"),
            el("span", text("iiiiiiii")).attr("class", "serif-italic"),
            el("span", text(SERIF_STYLE_SAMPLE.repeat(3))).attr("class", "serif-upright-long"),
            el("span", text(SERIF_STYLE_SAMPLE.repeat(3))).attr("class", "serif-italic-long"),
            el("span", text("00000000")).attr("class", "plex-italic"),
            el("span", text("00000000")).attr("class", "plex-bold"),
            el("span", text("00000000")).attr("class", "synthetic-run"),
            el("div", text(" ")).attr("class", "plex-measure"),
        ),
    ))
}

fn style(size: u8) -> String {
    format!(
        ".plex-zeroes,.plex-measure,.plex-italic,.plex-bold {{ position:absolute;display:block;left:0px;top:0px;font-family:'IBM Plex Mono';font-size:{size}px; }}\
         .serif-narrow {{ position:absolute;display:block;left:0px;top:40px;font-family:'Source Serif 4';font-size:{size}px; }}\
         .serif-wide {{ position:absolute;display:block;left:0px;top:80px;font-family:'Source Serif 4';font-size:{size}px; }}\
         .serif-italic {{ position:absolute;display:block;left:0px;top:100px;font-family:'Source Serif 4';font-size:{size}px;font-style:italic; }}\
         .serif-upright-long {{ position:absolute;display:block;left:0px;top:230px;font-family:'Source Serif 4';font-size:{size}px; }}\
         .serif-italic-long {{ position:absolute;display:block;left:0px;top:270px;font-family:'Source Serif 4';font-size:{size}px;font-style:italic; }}\
         .plex-measure {{ width:72ch;height:1px;top:120px; }}\
         .plex-italic {{ font-style:italic;top:150px; }}\
         .plex-bold {{ font-weight:700;top:180px; }}\
         .synthetic-run {{ position:absolute;display:block;left:0px;top:210px;font-family:'KnotTypographySyntheticFamily';font-size:{size}px; }}"
    )
}

fn harness_with_fonts(size: u8, fonts: Vec<HostFont>) -> FontHarness {
    let mut h = Harness::with_hooks(
        Init {
            state: App,
            logic: root as fn(&App) -> Child,
            sheet: style(size),
            fonts,
            images: Vec::new(),
        },
        inert_hooks(),
    );
    h.layout_at(900.0, 400.0);
    h
}

fn harness(size: u8) -> FontHarness {
    harness_with_fonts(size, bundled_fonts())
}

fn width(h: &FontHarness, class: &str) -> f32 {
    let node = h
        .with_dom(|dom| {
            taproot::matching(dom, &Selector::class(class))
                .first()
                .copied()
        })
        .unwrap_or_else(|| panic!(".{class} exists in the font probe"));
    h.painted_rect(node)
        .unwrap_or_else(|| panic!(".{class} has laid-out geometry"))
        .2
}

#[test]
fn bundled_native_families_shape_and_ch_measure_tracks_the_selected_face() {
    let mut small = harness(12);

    // IBM Plex Mono's OpenType advance is 600/1000 em: eight zero glyphs at
    // 12px are 57.6px wide. This checks the bytes under the native family
    // name, rather than only checking that the app's CSS mentions that name.
    let zeroes_12 = width(&small, "plex-zeroes");
    assert!((zeroes_12 - 57.6).abs() < 0.6, "got {zeroes_12}px");

    // The engine's shaping-derived `ch` advance is 0.6em for Plex Mono.
    let measure_12 = width(&small, "plex-measure");
    assert!(
        (measure_12 - 72.0 * 12.0 * 0.6).abs() < 0.5,
        "72ch={measure_12}px, expected 518.4px from 600/1000 em"
    );
    assert!((zeroes_12 - 8.0 * 12.0 * 0.6).abs() < 1.0);

    let serif_narrow = width(&small, "serif-narrow");
    let serif_wide = width(&small, "serif-wide");
    assert!(
        serif_wide > serif_narrow * 1.8,
        "Source Serif 4 i/W advances: {serif_narrow}px / {serif_wide}px"
    );
    let serif_italic = width(&small, "serif-italic");
    assert_ne!(
        width(&small, "serif-italic-long"),
        width(&small, "serif-upright-long"),
        "long Source Serif sample distinguishes upright and italic advances; short i runs can round equal (short widths {serif_narrow}px/{serif_italic}px)"
    );

    // Weight and italic requests run through actual geometry as well. IBM Plex
    // Mono's static faces retain their shared 600-unit advance across styles.
    assert!((width(&small, "plex-italic") - zeroes_12).abs() < 0.2);
    assert!((width(&small, "plex-bold") - zeroes_12).abs() < 0.2);

    // Use the real Plex regular bytes under a synthetic, fixed CSS family to
    // prove the host registration path, then an empty ledger as its negative
    // control. The unique name cannot be supplied by an installed font.
    let regular_bytes = include_bytes!("../assets/fonts/ibm-plex-mono/IBMPlexMono-Regular.ttf");
    let synthetic = HostFont {
        family: Some("KnotTypographySyntheticFamily".to_owned()),
        bytes: regular_bytes.to_vec(),
    };
    let synthetic_loaded = harness_with_fonts(12, vec![synthetic]);
    let synthetic_width = width(&synthetic_loaded, "synthetic-run");
    assert!((synthetic_width - 57.6).abs() < 0.6);
    let synthetic_missing = harness_with_fonts(12, Vec::new());
    assert_ne!(
        width(&synthetic_missing, "synthetic-run"),
        synthetic_width,
        "unique synthetic family has no fallback face"
    );

    // Resize creates a new retained text system; host fonts must be registered
    // again there as well.
    small.layout_at(1000.0, 260.0);
    assert!((width(&small, "plex-zeroes") - zeroes_12).abs() < 0.15);

    let large = harness(24);
    let zeroes_24 = width(&large, "plex-zeroes");
    let measure_24 = width(&large, "plex-measure");
    assert!(
        (zeroes_24 - 8.0 * 24.0 * 0.6).abs() < 1.0,
        "8 Plex Mono zeroes at 24px: {zeroes_24}px, expected 115.2px"
    );
    assert!(
        (measure_24 - 72.0 * 24.0 * 0.6).abs() < 0.5,
        "72ch at 24px: {measure_24}px, expected 1036.8px"
    );
}

#[derive(Default)]
struct MeasureApp {
    source_style: String,
    preview_style: String,
    font_size: u8,
    hard_wrap_source: String,
    hard_wrap_style: String,
}

type MeasureChild = Box<dyn AnyView<MeasureApp, (), GenetCtx, GenetElement>>;
type MeasureHarness = Harness<MeasureApp, fn(&MeasureApp) -> MeasureChild, MeasureChild>;

fn measure_root(state: &MeasureApp) -> MeasureChild {
    Box::new(el(
        "div",
        (
            el("div", text("source probe")).attr("class", "source-box").attr(
                "style",
                format!(
                    "position:absolute;left:0px;top:0px;{}",
                    state.source_style
                ),
            ),
            el("div", text("preview probe")).attr("class", "preview-box").attr(
                "style",
                format!(
                    "position:absolute;left:0px;top:80px;{}",
                    state.preview_style
                ),
            ),
            el("div", text(state.hard_wrap_source.clone())).attr("class", "hard-wrap-box").attr(
                "style",
                format!(
                    "position:absolute;left:0px;top:240px;{}",
                    state.hard_wrap_style
                ),
            ),
            el("span", text("00000000")).attr("class", "source-zero").attr(
                "style",
                format!(
                    "position:absolute;left:0px;top:160px;font-family:'IBM Plex Mono';font-size:{}px;",
                    state.font_size
                ),
            ),
            el("span", text("00000000")).attr("class", "preview-zero").attr(
                "style",
                format!(
                    "position:absolute;left:0px;top:180px;font-family:'Source Serif 4';font-size:{}px;",
                    state.font_size
                ),
            ),
        ),
    ))
}

#[test]
fn appearance_source_and_preview_measures_are_glyph_geometry_at_12_and_24px() {
    for size in [12, 24] {
        // Precomposed Latin-1 glyphs count as one source column even though
        // their UTF-8 encoding is two bytes per character.
        let hard_wrap_source =
            format!("{}\n{}\n{}", "é".repeat(76), "é".repeat(76), "é".repeat(76));
        let appearance = Appearance {
            font_size: size,
            measure: Measure::Medium,
            ..Appearance::default()
        };
        let source_style = appearance.writing_style("# Heading\n\nBody\n");
        let preview_style = appearance.preview_style();
        let hard_wrap_style = appearance.writing_style(&hard_wrap_source);
        assert_eq!(appearance.source_columns(&hard_wrap_source), Some(77));
        let mut h = Harness::with_hooks(
            Init {
                state: MeasureApp {
                    source_style,
                    preview_style,
                    font_size: size,
                    hard_wrap_source,
                    hard_wrap_style,
                },
                logic: measure_root as fn(&MeasureApp) -> MeasureChild,
                sheet: String::new(),
                fonts: bundled_fonts(),
                images: Vec::new(),
            },
            inert_hooks(),
        );
        h.layout_at(1800.0, 220.0);
        let source = width_measure(&h, "source-box");
        let preview = width_measure(&h, "preview-box");
        let source_zero = width_measure(&h, "source-zero");
        let preview_zero = width_measure(&h, "preview-zero");
        let hard_wrap_rect = rect_measure(&h, "hard-wrap-box");
        assert!((source - 72.0 * f32::from(size) * 0.6).abs() < 1.0);
        assert!((source - 9.0 * source_zero).abs() < 5.0);
        // Nine eight-glyph advances equal 72ch; a few pixels allow for the
        // host's integer painted glyph bounds in the independent reference.
        assert!((preview - 9.0 * preview_zero).abs() < 5.0);
        assert!(
            (hard_wrap_rect.2 - 77.0 * f32::from(size) * 0.6).abs() < 1.0,
            "hard-wrapped source box {}px vs 77 Plex Mono columns at {size}px",
            hard_wrap_rect.2,
        );
        let line_height = f32::from(size) * 1.7;
        assert!(
            hard_wrap_rect.3 > line_height * 2.5 && hard_wrap_rect.3 < line_height * 3.5,
            "three 76-column lines stay three visual lines: {hard_wrap_rect:?} at {size}px"
        );
    }
}

fn rect_measure(h: &MeasureHarness, class: &str) -> (f32, f32, f32, f32) {
    let node = h
        .with_dom(|dom| {
            taproot::matching(dom, &Selector::class(class))
                .first()
                .copied()
        })
        .unwrap_or_else(|| panic!(".{class} exists"));
    h.painted_rect(node)
        .unwrap_or_else(|| panic!(".{class} has laid-out geometry"))
}

fn width_measure(h: &MeasureHarness, class: &str) -> f32 {
    rect_measure(h, class).2
}

type DesktopHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

fn desktop_harness(size: u8) -> DesktopHarness {
    let mut state = DesktopState::with_path(
        KnotDocumentSession::scratch("memory:typography-chrome", "# Typography\n\nBody.\n"),
        WindowCommands::new(),
        None,
    );
    state.appearance.font_size = size;
    state.message = Some("Typography chrome stays fixed".to_owned());
    let mut h = Harness::with_hooks(
        Init {
            state,
            logic: desktop_view as fn(&DesktopState) -> DesktopView,
            sheet: desktop_sheet(),
            fonts: bundled_fonts(),
            images: Vec::new(),
        },
        host_hooks(),
    );
    h.layout_at(1200.0, 800.0);
    h
}

fn desktop_rect(h: &DesktopHarness, class: &str) -> (f32, f32, f32, f32) {
    let node = h
        .with_dom(|dom| {
            taproot::matching(dom, &Selector::class(class))
                .first()
                .copied()
        })
        .unwrap_or_else(|| panic!(".{class} exists in the desktop"));
    h.painted_rect(node)
        .unwrap_or_else(|| panic!(".{class} has laid-out geometry"))
}

#[test]
fn desktop_chrome_geometry_is_stable_across_source_sizes_12_and_24() {
    let small = desktop_harness(12);
    let large = desktop_harness(24);
    // On macOS the menu bar is native and is not in this DOM. These rendered
    // chrome elements cover the app-owned status text and document tabs.
    for class in ["status-bar", "status-message", "frisket-tab"] {
        let a = desktop_rect(&small, class);
        let b = desktop_rect(&large, class);
        assert!((a.2 - b.2).abs() < 0.1, ".{class} width: {a:?} vs {b:?}");
        assert!((a.3 - b.3).abs() < 0.1, ".{class} height: {a:?} vs {b:?}");
    }
}
