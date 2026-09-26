// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Navigator: the catalog's document history as a tile (slice 1 step 6).
//!
//! Each record says whether its file is available. An available record opens
//! its document, or activates the tab already showing it. An unavailable
//! record stays in the history, labelled, and does nothing: rebinding it is a
//! later, explicit action, never inferred from a name. Listing reads the
//! catalog and each file's metadata, never a document's bytes.

use cambium::{Keyed, button, el, span};
use knot_file_catalog::KnotFileCatalogAvailability;
use workbench::TileId;

use crate::workspace::{DesktopState, DesktopView};

/// The Navigator tile's tab title.
pub(crate) const TITLE: &str = "Navigator";

pub const CSS: &str = "\
.knot-navigator { display:flex; flex-direction:column; gap:8px; min-width:0; }\
.knot-navigator-records { display:flex; flex-direction:column; gap:4px; min-width:0; }\
.knot-navigator-row { display:flex; align-items:baseline; gap:8px; min-width:0; }\
.knot-navigator-row > :first-child { flex:1 1 auto; min-width:0; text-align:left; overflow-wrap:anywhere; }\
.knot-navigator-row[data-availability=unavailable] { opacity:0.7; }\
.knot-navigator-availability { flex:none; font-size:13px; }\
";

/// The id of a Navigator tile's region, which the command row's toggle names.
pub(crate) fn region_id(tile: TileId) -> String {
    format!("knot-navigator-{}", tile.0)
}

/// A Navigator tile: the catalog's records by path, each with whether its
/// file is available, or why there is nothing to list.
pub(crate) fn view(state: &DesktopState, tile: TileId) -> DesktopView {
    let body: DesktopView = match state.catalog.as_ref() {
        None => Box::new(
            el(
                "div",
                (
                    span("No catalog is configured."),
                    span(
                        "Launch Knot with --catalog-root <folder> and --catalog <file> \
                         to keep document history.",
                    ),
                ),
            )
            .attr("class", "knot-navigator-empty"),
        ),
        Some(catalog) => {
            let mut records = catalog.records();
            records.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
            if records.is_empty() {
                Box::new(
                    span("The catalog holds no documents yet.")
                        .attr("class", "knot-navigator-empty"),
                )
            } else {
                let root = catalog.root().to_path_buf();
                let rows: Vec<(String, DesktopView)> = records
                    .into_iter()
                    .map(|record| {
                        let label = record.relative_path.display().to_string();
                        let row: DesktopView = match record.availability {
                            KnotFileCatalogAvailability::Available => {
                                let path = root.join(&record.relative_path);
                                Box::new(
                                    el(
                                        "div",
                                        (
                                            button(
                                                label.clone(),
                                                move |state: &mut DesktopState, _| {
                                                    state.open_path(path.clone());
                                                },
                                            )
                                            .attr("data-catalog-record", label),
                                            span("Available")
                                                .attr("class", "knot-navigator-availability"),
                                        ),
                                    )
                                    .attr("class", "knot-navigator-row")
                                    .attr("data-availability", "available"),
                                )
                            },
                            KnotFileCatalogAvailability::Unavailable => Box::new(
                                el(
                                    "div",
                                    (
                                        span(label),
                                        span("Unavailable")
                                            .attr("class", "knot-navigator-availability"),
                                    ),
                                )
                                .attr("class", "knot-navigator-row")
                                .attr("data-availability", "unavailable"),
                            ),
                        };
                        (record.id, row)
                    })
                    .collect();
                Box::new(el("div", Keyed::new(rows)).attr("class", "knot-navigator-records"))
            }
        },
    };
    Box::new(
        el("section", body)
            .attr("class", "knot-navigator")
            .attr("id", region_id(tile))
            .attr("aria-label", TITLE),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::desktop_view;
    use cambium_genet_winit_host::{Harness, Init, WindowCommands};
    use knot_document::KnotDocumentSession;
    use knot_file_catalog::KnotFileCatalog;
    use layout_dom_api::LayoutDom;
    use taproot::Selector;

    type DesktopHarness = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

    fn text(dom: &genet_scripted_dom::ScriptedDom, node: genet_scripted_dom::NodeId) -> String {
        let own = dom.text(node).unwrap_or_default();
        let children = dom
            .dom_children(node)
            .map(|child| text(dom, child))
            .collect::<String>();
        format!("{own}{children}")
    }

    fn host(catalog: Option<KnotFileCatalog>) -> DesktopHarness {
        let mut host = Harness::with_hooks(
            Init {
                state: DesktopState::with_catalog(
                    KnotDocumentSession::scratch("scratch:navigator", ""),
                    WindowCommands::new(),
                    None,
                    catalog,
                ),
                logic: desktop_view as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            crate::host_hooks(),
        );
        host.layout_at(1100.0, 730.0);
        host
    }

    /// The Navigator's rows as (path, availability, whether it is a button).
    fn rows(host: &DesktopHarness) -> Vec<(String, String, bool)> {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        dom.all_with_class(dom.document(), "knot-navigator-row")
            .into_iter()
            .map(|row| {
                let mut children = dom.dom_children(row);
                let path = children.next().expect("the path");
                let availability = children.next().expect("the availability");
                let is_button = dom
                    .element_name(path)
                    .is_some_and(|name| name.local.as_ref() == "button");
                (text(&dom, path), text(&dom, availability), is_button)
            })
            .collect()
    }

    fn navigator_text(host: &DesktopHarness) -> String {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        let section = dom
            .all_with_class(dom.document(), "knot-navigator")
            .into_iter()
            .next()
            .expect("the Navigator is open");
        text(&dom, section)
    }

    /// A catalog of two bound documents, `a.djot` and `b.djot`, with `b.djot`
    /// since deleted.
    fn catalog_with_history(temp: &std::path::Path) -> (std::path::PathBuf, KnotFileCatalog) {
        let root = temp.join("documents");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("a.djot"), "# A\n").unwrap();
        std::fs::write(root.join("b.djot"), "# B\n").unwrap();
        let mut catalog = KnotFileCatalog::open(&root, temp.join("catalog.redb")).unwrap();
        catalog.bind(root.join("a.djot")).unwrap();
        catalog.bind(root.join("b.djot")).unwrap();
        std::fs::remove_file(root.join("b.djot")).unwrap();
        (root, catalog)
    }

    #[test]
    fn the_navigator_labels_available_and_unavailable_history_without_touching_sources() {
        let temp = tempfile::tempdir().unwrap();
        let (root, catalog) = catalog_with_history(temp.path());
        let mut host = host(Some(catalog));
        assert!(host.click_on(&Selector::role("button").containing("Navigator")));
        assert_eq!(
            rows(&host),
            [
                ("a.djot".to_owned(), "Available".to_owned(), true),
                ("b.djot".to_owned(), "Unavailable".to_owned(), false),
            ]
        );
        assert_eq!(std::fs::read(root.join("a.djot")).unwrap(), b"# A\n");
        assert!(!root.join("b.djot").exists(), "nothing recreated it");
    }

    #[test]
    fn selecting_an_available_record_opens_its_document_then_activates_it() {
        let temp = tempfile::tempdir().unwrap();
        let (_root, catalog) = catalog_with_history(temp.path());
        let mut host = host(Some(catalog));
        assert!(host.click_on(&Selector::role("button").containing("Navigator")));
        let record = Selector::role("button").with_attr("data-catalog-record", "a.djot");

        assert!(host.click_on(&record));
        assert_eq!(
            host.state().docs.len(),
            2,
            "a.djot opened beside the scratch"
        );
        assert_eq!(host.state().document().snapshot().display_label, "a.djot");
        // The catalog's root is canonical; the message shows the path as a
        // person reads it, without Windows' verbatim prefix.
        let message = host.state().message.clone().unwrap_or_default();
        assert!(
            message.starts_with("Opened ") && !message.contains(r"\\?\"),
            "{message}"
        );

        assert!(host.click_on(&Selector::role("tab").containing("scratch:navigator")));
        assert!(host.click_on(&record));
        assert_eq!(host.state().docs.len(), 2, "no second tab");
        assert_eq!(host.state().document().snapshot().display_label, "a.djot");
        assert_eq!(
            host.state().message.as_deref(),
            Some("a.djot is already open.")
        );
    }

    #[test]
    fn a_launch_without_a_catalog_says_so_in_the_navigator() {
        let mut host = host(None);
        assert!(host.click_on(&Selector::role("button").containing("Navigator")));
        assert!(navigator_text(&host).contains("No catalog is configured."));
        assert!(rows(&host).is_empty());
    }
}
