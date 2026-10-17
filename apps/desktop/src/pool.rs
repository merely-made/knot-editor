// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! One Pool per workspace, independent of document tabs and collection snapshots.
use crate::{
    documents::DocKey,
    workspace::{DesktopState, DesktopView},
};
use cambium::{button, el, span};
use inker::pool::Pool;
use knot_document::DocumentFormat;
use std::{io::Write, path::PathBuf};
use uuid::Uuid;

#[derive(Default)]
pub struct State {
    pub open: bool,
    data: Pool,
    path: Option<PathBuf>,
    original: Option<Vec<u8>>,
    blocked: Option<String>,
    message: String,
}
impl State {
    pub fn configured(path: Option<PathBuf>) -> Self {
        let mut state = Self {
            path,
            ..Self::default()
        };
        if let Some(path) = &state.path {
            let loaded = (|| match std::fs::read(path) {
                Ok(bytes) => {
                    let pool: Pool = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    pool.validate()?;
                    Ok(Some((bytes, pool)))
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            })();
            match loaded {
                Ok(Some((bytes, pool))) => {
                    state.original = Some(bytes);
                    state.data = pool;
                },
                Ok(None) => {},
                Err(error) => {
                    state.blocked = Some(format!(
                        "Pool could not be opened: {error}. Stored contents are unchanged."
                    ))
                },
            }
        }
        state
    }
    fn commit(&mut self, candidate: Pool) -> Result<(), String> {
        if let Some(error) = &self.blocked {
            return Err(error.clone());
        }
        candidate.validate()?;
        let path = self
            .path
            .as_ref()
            .ok_or("Pool storage is unavailable in this workspace")?;
        let current = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if current != self.original {
            return Err("The stored Pool changed in another workspace; reopen it before changing its contents".into());
        }
        let bytes = serde_json::to_vec_pretty(&candidate).map_err(|e| e.to_string())?;
        let parent = path.parent().ok_or("Pool has no storage directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = parent.join(format!(".pool-{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp).map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            std::fs::rename(&temp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result?;
        self.original = Some(bytes);
        self.data = candidate;
        Ok(())
    }
}
fn report(state: &mut DesktopState, result: Result<(), String>, success: &str) {
    state.pool.message = match result {
        Ok(()) => success.into(),
        Err(e) => e,
    };
}
fn add(
    state: &mut DesktopState,
    text: String,
    media_type: &str,
    source: String,
) -> Result<(), String> {
    if text.is_empty() {
        return Err("Select some content to add to the Pool".into());
    }
    let title = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Material")
        .chars()
        .take(60)
        .collect();
    let mut candidate = state.pool.data.clone();
    candidate.add(title, media_type.into(), text, Some(source))?;
    state.pool.commit(candidate)?;
    state.pool.open = true;
    Ok(())
}
fn add_selection(state: &mut DesktopState) {
    let result = (|| {
        let snapshot = state.document().snapshot();
        let a = snapshot
            .selection
            .anchor
            .byte
            .min(snapshot.selection.focus.byte);
        let b = snapshot
            .selection
            .anchor
            .byte
            .max(snapshot.selection.focus.byte);
        let text = snapshot
            .text
            .get(a..b)
            .ok_or("Selection changed")?
            .to_string();
        let media = if matches!(snapshot.format, DocumentFormat::Djot | DocumentFormat::Knot) {
            "text/djot"
        } else {
            "text/plain"
        };
        add(state, text, media, snapshot.source.address)
    })();
    report(state, result, "Added to Pool · saved in this workspace");
}
pub(crate) fn add_block(state: &mut DesktopState, key: DocKey, id: &str) {
    let result = (|| {
        let snapshot = state.surface_for(key).session().div_snapshot()?;
        fn find<'a>(
            divs: &'a [nematic::knot::djot::divs::Div],
            id: &str,
            found: &mut Vec<&'a nematic::knot::djot::divs::Div>,
        ) {
            for div in divs {
                if div.attributes.get("id").is_some_and(|s| s == id) {
                    found.push(div);
                }
                find(&div.children, id, found);
            }
        }
        let mut found = Vec::new();
        find(&snapshot.divs, id, &mut found);
        if found.len() != 1 {
            return Err("Choose a block with a unique id".into());
        }
        let text = snapshot.source_text[found[0].range.clone()].to_string();
        let title = found[0]
            .attributes
            .get("form")
            .cloned()
            .unwrap_or_else(|| "Metre block".into());
        let mut candidate = state.pool.data.clone();
        candidate.add(
            title,
            "text/djot".into(),
            text,
            Some(format!("{}#{id}", snapshot.address)),
        )?;
        state.pool.commit(candidate)?;
        state.pool.open = true;
        Ok(())
    })();
    report(
        state,
        result,
        "Block added to Pool · saved in this workspace",
    );
}
/// Reidentify editable metre block copies. Source addresses remain provenance;
/// this deliberately does not invent a universal identity rewrite protocol.
fn independent_text(item: &inker::pool::Material) -> Result<String, String> {
    if item.media_type != "text/djot" {
        return Ok(item.text.clone());
    }
    let mut copy = knot_document::KnotDocumentSession::scratch("memory:pool-copy.djot", &item.text);
    let snapshot = copy.div_snapshot()?;
    fn collect(divs: &[nematic::knot::djot::divs::Div], out: &mut Vec<String>) {
        for div in divs {
            if div
                .attributes
                .get("class")
                .is_some_and(|s| s.split_whitespace().any(|c| c == "metre"))
            {
                if let Some(id) = div.attributes.get("id") {
                    out.push(id.clone());
                }
            }
            collect(&div.children, out);
        }
    }
    let mut ids = Vec::new();
    collect(&snapshot.divs, &mut ids);
    for id in ids {
        let snapshot = copy.div_snapshot()?;
        fn target<'a>(
            divs: &'a [nematic::knot::djot::divs::Div],
            id: &str,
        ) -> Option<&'a nematic::knot::djot::divs::Div> {
            divs.iter().find_map(|d| {
                if d.attributes.get("id").is_some_and(|s| s == id) {
                    Some(d)
                } else {
                    target(&d.children, id)
                }
            })
        }
        if let Some(div) = target(&snapshot.divs, &id) {
            let mut attrs = div.attributes.clone();
            attrs.insert("id".into(), format!("verse-{}", Uuid::new_v4()));
            copy.set_div_attributes(&snapshot, div, attrs)?;
        }
    }
    Ok(copy.snapshot().text)
}
fn insert(state: &mut DesktopState, id: Uuid) {
    let result = (|| {
        let item = state.pool.data.get(id)?.clone();
        let text = independent_text(&item)?;
        let snapshot = state.document().snapshot();
        let start = snapshot
            .selection
            .anchor
            .byte
            .min(snapshot.selection.focus.byte);
        let end = snapshot
            .selection
            .anchor
            .byte
            .max(snapshot.selection.focus.byte);
        let is_block =
            item.media_type == "text/djot" && !nematic::knot::djot::divs::divs(&text).is_empty();
        // A complete block needs separation on both sides, even when the
        // captured range ends at the closing fence rather than its newline.
        let before = &snapshot.text[..start];
        let after = &snapshot.text[end..];
        let prefix = if is_block && !before.is_empty() && !before.ends_with("\n\n") {
            if before.ends_with('\n') { "\n" } else { "\n\n" }
        } else {
            ""
        };
        let suffix = if is_block && !after.is_empty() && !text.ends_with("\n\n") {
            if text.ends_with('\n') { "\n" } else { "\n\n" }
        } else {
            ""
        };
        let replacement = format!("{prefix}{text}{suffix}");
        state.document_mut().session_mut().replace_source_span(
            &snapshot.source.address,
            &snapshot.text,
            start,
            end,
            replacement,
        )
    })();
    report(
        state,
        result,
        "Inserted independent content · the original stays in Pool",
    );
}
fn remove(state: &mut DesktopState, id: Uuid) {
    let result = (|| {
        let mut candidate = state.pool.data.clone();
        candidate.remove(id)?;
        state.pool.commit(candidate)
    })();
    report(state, result, "Removed from Pool");
}
fn preview_text(item: &inker::pool::Material) -> String {
    if item.media_type == "text/djot" {
        let divs = nematic::knot::djot::divs::divs(&item.text);
        if let Some(div) = divs.first() {
            return item.text[div.body.clone()].chars().take(180).collect();
        }
    }
    item.text.chars().take(180).collect()
}
pub fn view(state: &DesktopState) -> DesktopView {
    let mut rows: Vec<DesktopView> = vec![Box::new(
        el(
            "div",
            (
                button("Pool", |s: &mut DesktopState, _| s.pool.open = !s.pool.open)
                    .attr("aria-expanded", state.pool.open.to_string()),
                span(format!(
                    "{} {}",
                    state.pool.data.items.len(),
                    if state.pool.data.items.len() == 1 {
                        "item"
                    } else {
                        "items"
                    }
                )),
            ),
        )
        .attr(
            "style",
            "display:flex;flex-direction:row;align-items:center;gap:8px",
        ),
    )];
    if state.pool.open {
        rows.push(Box::new(button(
            "Add selection to Pool",
            |s: &mut DesktopState, _| add_selection(s),
        )));
        rows.push(Box::new(span(
            "Workspace material · insertion makes an independent copy",
        )));
        if state.pool.data.items.is_empty() && state.pool.blocked.is_none() {
            rows.push(Box::new(span(
                "Pool is empty. Add a selection or an authored block.",
            )));
        }
        for item in &state.pool.data.items {
            let id = item.id;
            let mut card: Vec<DesktopView> = vec![
                Box::new(el("strong", item.title.clone())),
                Box::new(span(preview_text(item)).attr("style", "white-space:pre-wrap")),
                Box::new(
                    el(
                        "div",
                        (
                            button("Insert from Pool", move |s: &mut DesktopState, _| {
                                insert(s, id)
                            })
                            .attr("data-pool-id", id.to_string()),
                            button("Remove from Pool", move |s: &mut DesktopState, _| {
                                remove(s, id)
                            })
                            .attr("data-pool-id", id.to_string()),
                        ),
                    )
                    .attr(
                        "style",
                        "display:flex;flex-direction:row;flex-wrap:wrap;gap:6px",
                    ),
                ),
            ];
            if let Some(source) = &item.source {
                card.push(Box::new(span(format!("Source: {source}"))));
                if source.starts_with("file:///") {
                    let source = source
                        .split('#')
                        .next()
                        .unwrap()
                        .trim_start_matches("file://")
                        .to_string();
                    card.push(Box::new(button(
                        "Open source",
                        move |s: &mut DesktopState, _| {
                            let path =
                                percent_encoding::percent_decode_str(&source).decode_utf8_lossy();
                            s.open_path(PathBuf::from(path.as_ref()));
                        },
                    )));
                }
            }
            rows.push(Box::new(el("article",card).attr("style","display:flex;flex-direction:column;gap:6px;padding:10px;border:1px solid;border-radius:6px;overflow-wrap:anywhere")));
        }
        if let Some(error) = &state.pool.blocked {
            rows.push(Box::new(span(error.clone()).attr("role", "status")));
        }
        if !state.pool.message.is_empty() {
            rows.push(Box::new(
                span(state.pool.message.clone()).attr("role", "status"),
            ));
        }
    }
    Box::new(el("section",rows).attr("aria-label","Pool").attr("class","knot-pool").attr("style","display:flex;flex-direction:column;gap:8px;flex:none;max-height:35vh;overflow:auto;padding:4px;min-width:0"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::TextCommand;
    use cambium_genet_winit_host::WindowCommands;
    use knot_document::{KnotDocumentIntentV1 as Intent, KnotDocumentSession};
    fn state(path: PathBuf, source: &str) -> DesktopState {
        let mut s = DesktopState::new(
            KnotDocumentSession::scratch("memory:pool.djot", source),
            WindowCommands::new(),
        );
        s.pool = State::configured(Some(path));
        s
    }
    #[test]
    fn block_copy_is_shared_across_documents_survives_reopen_and_inserts_in_one_history_step() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("pool.json");
        let source = "{#verse}\n::: metre\nquux light\n:::\n";
        let mut s = state(path.clone(), source);
        let original = s.focused_key().unwrap();
        add_block(&mut s, original, "verse");
        assert_eq!(s.pool.data.items.len(), 1, "{}", s.pool.message);
        assert_eq!(s.document().snapshot().text, source);
        assert!(!s.document().snapshot().dirty);
        let id = s.pool.data.items[0].id;
        s.open_behind(
            vec![KnotDocumentSession::scratch(
                "memory:destination.djot",
                "destination\n",
            )],
            &[],
        );
        let other = s
            .docs
            .docs()
            .map(|(key, _)| key)
            .find(|key| *key != original)
            .unwrap();
        s.focus_document(other);
        let before = s.document().snapshot().text;
        insert(&mut s, id);
        assert!(s.pool.message.starts_with("Inserted"), "{}", s.pool.message);
        let inserted = s.document().snapshot().text;
        let snapshot = s.document().session().div_snapshot().unwrap();
        assert_eq!(snapshot.divs.len(), 1, "{inserted:?}");
        assert_ne!(snapshot.divs[0].attributes["id"], "verse");
        assert!(inserted.contains("quux light"));
        s.document_mut()
            .apply(Intent::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(s.document().snapshot().text, before);
        s.document_mut()
            .apply(Intent::Edit(TextCommand::Redo))
            .unwrap();
        assert_eq!(s.document().snapshot().text, inserted);
        s.focus_document(original);
        assert_eq!(s.document().snapshot().text, source);
        let reopened = State::configured(Some(path.clone()));
        assert_eq!(reopened.data, s.pool.data);
        remove(&mut s, id);
        assert!(s.pool.data.items.is_empty());
        assert!(State::configured(Some(path)).data.items.is_empty());
    }
    #[test]
    fn plain_selection_is_copied_and_read_only_insertion_preserves_the_pool() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.gmi");
        std::fs::write(&source, "chosen words").unwrap();
        let mut s = DesktopState::new(
            KnotDocumentSession::open_read_only(&source).unwrap(),
            WindowCommands::new(),
        );
        s.pool = State::configured(Some(root.path().join("pool.json")));
        let snap = s.document().snapshot();
        s.document_mut()
            .session_mut()
            .select_source_span(&snap.source.address, &snap.text, 0, 6)
            .unwrap();
        add_selection(&mut s);
        assert_eq!(s.pool.data.items[0].text, "chosen");
        let before = s.pool.data.clone();
        let id = before.items[0].id;
        insert(&mut s, id);
        assert_eq!(s.pool.data, before);
        assert_eq!(std::fs::read_to_string(source).unwrap(), "chosen words");
        assert!(!s.pool.message.starts_with("Inserted"));
    }
    #[test]
    fn failed_or_stale_storage_never_replaces_material_with_defaults() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("pool.json");
        std::fs::write(&path, "damaged original").unwrap();
        let mut s = state(path.clone(), "words");
        assert!(add(&mut s, "words".into(), "text/plain", "memory:source".into()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "damaged original");
        std::fs::remove_file(&path).unwrap();
        s.pool = State::configured(Some(path.clone()));
        add(&mut s, "words".into(), "text/plain", "memory:source".into()).unwrap();
        let mut stale = State::configured(Some(path.clone()));
        let before = stale.data.clone();
        add(&mut s, "other".into(), "text/plain", "memory:source".into()).unwrap();
        assert!(stale.commit(before.clone()).is_err());
        assert_eq!(State::configured(Some(path)).data.items.len(), 2);
        let bad = root.path().join("not-directory");
        std::fs::write(&bad, "file").unwrap();
        let mut failing = State::configured(Some(bad.join("pool.json")));
        let pool = failing.data.clone();
        assert!(failing.commit(pool.clone()).is_err());
        assert_eq!(failing.data, pool);
    }
}
