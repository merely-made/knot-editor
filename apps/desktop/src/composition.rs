// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Explicit composition readings. Reference data, derived results and authored
//! collections have separate owners; merely opening this panel never analyzes
//! the document or enables a reference source.

use crate::{
    documents::DocKey,
    workspace::{DesktopState, DesktopView},
};
use cambium::{Keyed, TextInput, button, el, lens, span, text_field_typed, textarea_typed};
use cambium_genet_winit_host::HostWake;
use knot_composition::retention::{
    CompositionRetainPort, OrganizeComposition, RetainedCompositionItem,
};
use knot_composition::{CollectionItem, CollectionStore, DocumentAnchor, ItemKind, PackSource};
use knot_readings::sound::{self, SoundLayers, SoundReading};
use reference_data::{LexicalEntry, LookupQuery, ReferenceId, SourceKey, SourceRegistry};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, mpsc},
};

mod recipe;

pub(crate) use recipe::measure_cards as measure_recipe_cards;

#[derive(Clone)]
pub(crate) struct SelectionSnapshot {
    key: DocKey,
    address: String,
    text: String,
    start: usize,
    end: usize,
}

impl SelectionSnapshot {
    fn capture(state: &DesktopState, key: DocKey) -> Result<Self, String> {
        let entry = state.docs.doc(key).ok_or("Document is closed.")?;
        let snapshot = entry.document.snapshot();
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
        if start == end || snapshot.text.get(start..end).is_none() {
            return Err("Select a word or passage in the source first.".into());
        }
        if end - start > 16 * 1024 {
            return Err("Select at most 16 KiB for a composition reading.".into());
        }
        Ok(Self {
            key,
            address: snapshot.source.address,
            text: snapshot.text,
            start,
            end,
        })
    }

    fn current(&self, state: &DesktopState) -> bool {
        state.docs.doc(self.key).is_some_and(|entry| {
            let snapshot = entry.document.snapshot();
            snapshot.source.address == self.address
                && snapshot.text == self.text
                && snapshot
                    .selection
                    .anchor
                    .byte
                    .min(snapshot.selection.focus.byte)
                    == self.start
                && snapshot
                    .selection
                    .anchor
                    .byte
                    .max(snapshot.selection.focus.byte)
                    == self.end
        })
    }

    fn anchor(&self) -> Result<DocumentAnchor, String> {
        DocumentAnchor::capture(self.address.clone(), &self.text, self.start..self.end)
            .map_err(|error| error.to_string())
    }
}

#[derive(Default)]
pub struct CompositionState {
    recipe: recipe::RecipeState,
    pub(crate) query: TextInput,
    pub(crate) import_path: TextInput,
    pub(crate) import_digest: TextInput,
    pub(crate) notice: Option<String>,
    retained: Vec<RetainedCompositionItem>,
    collection_search: TextInput,
    show_archived: bool,
    organization_edit: Option<(knot_capture::KnotRetainTargetV1, OrganizeComposition)>,
    organization_label: TextInput,
    organization_collection: TextInput,
    organization_notes: TextInput,
    organization_tags: TextInput,
    organization_order: TextInput,
    targets: Vec<Arc<dyn CompositionRetainPort>>,
    selected_target: Option<usize>,
    wake: Option<HostWake>,
    generation: u64,
    receiver: Option<mpsc::Receiver<CollectionUpdate>>,
    retry_items: Vec<CollectionItem>,
    retry_target: Option<knot_capture::KnotRetainTargetV1>,
    legacy_path: Option<PathBuf>,
    pub(crate) selection: Option<SelectionSnapshot>,
    pub(crate) cmudict_enabled: bool,
    pub(crate) choices: BTreeMap<usize, usize>,
    registry: Option<SourceRegistry>,
    entries: Vec<LexicalEntry>,
    history: Vec<(Vec<LexicalEntry>, Option<String>)>,
    focused_sense: Option<String>,
    sound_selection: Option<SelectionSnapshot>,
    sound: Option<SoundReading>,
    layers: SoundLayers,
    section: usize,
    pending_root: Option<PathBuf>,
}

struct CollectionUpdate {
    generation: u64,
    result: Result<Vec<RetainedCompositionItem>, String>,
}

impl CompositionState {
    /// Merely configuring preferences must not create any files.
    pub fn configured(root: Option<PathBuf>) -> Self {
        Self {
            legacy_path: root.as_ref().map(|path| path.join("collection.json")),
            pending_root: root,
            ..Self::default()
        }
    }

    fn ensure_open(&mut self) {
        if let Some(root) = self.pending_root.take() {
            let opened = Self::open(Some(root));
            self.registry = opened.registry;
            if opened.notice.is_some() {
                self.notice = opened.notice;
            }
        }
    }

    pub fn open(root: Option<PathBuf>) -> Self {
        let mut state = Self::default();
        if let Some(root) = root {
            state.legacy_path = Some(root.join("collection.json"));
            match SourceRegistry::open(root.join("references")) {
                Ok(registry) => state.registry = Some(registry),
                Err(error) => {
                    state.notice = Some(format!("Reference registry unavailable: {error}"))
                },
            }
        }
        state
    }

    pub(crate) fn set_targets(
        &mut self,
        targets: Vec<Arc<dyn CompositionRetainPort>>,
        wake: HostWake,
    ) {
        self.generation = self.generation.wrapping_add(1);
        self.targets = targets;
        self.selected_target = None;
        self.retained.clear();
        self.organization_edit = None;
        self.wake = Some(wake);
        self.notice = Some(if self.receiver.is_some() {
            "Destinations changed during retention. The previous destination may contain the item; choose it and refresh before retrying.".into()
        } else {
            "Choose a persona and space in Collection before retaining material.".into()
        });
    }

    pub(crate) fn busy(&self) -> bool {
        self.receiver.is_some()
    }

    fn request(&mut self, items: Vec<CollectionItem>, legacy: bool) {
        self.request_operation(items, legacy, None);
    }

    fn request_organization(&mut self, change: OrganizeComposition) {
        self.request_operation(Vec::new(), false, Some(change));
    }

    fn request_operation(
        &mut self,
        items: Vec<CollectionItem>,
        legacy: bool,
        change: Option<OrganizeComposition>,
    ) {
        if self.busy() {
            self.notice = Some("A collection request is already in progress.".into());
            return;
        }
        // The projection is only visible after this authority check succeeds;
        // a locked or revoked destination must not leave old rows confirmed.
        self.retained.clear();
        let Some(port) = self
            .selected_target
            .and_then(|index| self.targets.get(index))
            .cloned()
        else {
            self.notice = Some("No collection destination selected. Choose a writable Knot mere persona and space in Collection; no plaintext fallback is written.".into());
            return;
        };
        let target = port.target().clone();
        if (!items.is_empty() || legacy)
            && !self.retry_items.is_empty()
            && (self.retry_target.as_ref() != Some(&target)
                || legacy
                || items.iter().map(|item| &item.id).collect::<Vec<_>>()
                    != self
                        .retry_items
                        .iter()
                        .map(|item| &item.id)
                        .collect::<Vec<_>>())
        {
            self.notice = Some("A previous collection request is still unconfirmed. Select its original destination and refresh or retry that same request before collecting new material.".into());
            return;
        }
        let generation = self.generation;
        let wake = self.wake.clone();
        let path = self.legacy_path.clone();
        if !items.is_empty() {
            self.retry_items = items.clone();
            self.retry_target = Some(target.clone());
        }
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.notice = Some("Checking the selected Knot mere…".into());
        let spawned = std::thread::Builder::new().name("knot-composition-retain".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let items = if legacy {
                    let path = path.ok_or("No legacy collection path is configured.")?;
                    if !path.is_file() { return Err("No legacy collection file exists.".to_owned()); }
                    CollectionStore::open(path).map_err(|error| error.to_string())?.items().to_vec()
                } else { items };
                for item in items {
                    let id = item.id.clone();
                    let receipt = port.retain(&target, item).map_err(|error| error.to_string())?;
                    if receipt.target != target || receipt.item_id != id {
                        return Err("Retention receipt did not match the requested item and destination.".into());
                    }
                }
                if let Some(change) = change {
                    let id = change.item_id.clone();
                    let receipt = port.organize(&target, change).map_err(|error| error.to_string())?;
                    if receipt.target != target || receipt.item_id != id {
                        return Err("Organization receipt did not match the requested item and destination.".into());
                    }
                }
                let retained = port.list(&target).map_err(|error| error.to_string())?;
                if retained.iter().any(|entry| entry.receipt.target != target || entry.receipt.item_id != entry.item.id) {
                    return Err("Collection returned mismatched authority receipts.".into());
                }
                Ok(retained)
            })).unwrap_or_else(|_| Err("Collection worker stopped; retention outcome is uncertain.".into()));
            drop(port);
            let _ = sender.send(CollectionUpdate { generation, result });
            if let Some(wake) = wake { wake.wake(); }
        });
        if spawned.is_err() {
            self.receiver = None;
            self.notice = Some("Collection worker could not start; nothing was submitted.".into());
        }
    }

    pub(crate) fn drain(&mut self) {
        let Some(receiver) = self.receiver.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(update) => {
                self.receiver = None;
                if update.generation != self.generation {
                    return;
                }
                match update.result {
                    Ok(retained) => {
                        self.retained = retained;
                        self.retained.sort_by(|a, b| {
                            (
                                &a.organization.collection,
                                a.organization.order,
                                &a.organization.label,
                                a.author,
                                &a.item.id,
                            )
                                .cmp(&(
                                    &b.organization.collection,
                                    b.organization.order,
                                    &b.organization.label,
                                    b.author,
                                    &b.item.id,
                                ))
                        });
                        self.organization_edit = None;
                        if self.retry_target.is_some()
                            && self
                                .selected_target
                                .and_then(|index| self.targets.get(index))
                                .map(|port| port.target())
                                == self.retry_target.as_ref()
                        {
                            let writer = self
                                .retry_target
                                .as_ref()
                                .expect("matching retry target")
                                .writer;
                            self.retry_items.retain(|item| {
                                !self
                                    .retained
                                    .iter()
                                    .any(|entry| entry.author == writer && entry.item == *item)
                            });
                            if self.retry_items.is_empty() {
                                self.retry_target = None;
                            }
                        }
                        self.notice = Some(format!(
                            "Confirmed {} retained items in the selected Knot mere. Legacy source files are never changed.",
                            self.retained.len()
                        ));
                    },
                    Err(error) => {
                        self.retained.clear();
                        self.organization_edit = None;
                        self.notice = Some(format!(
                            "Collection could not be confirmed: {error}. Refresh or retry the same request; no plaintext fallback was written."
                        ))
                    },
                }
            },
            Err(mpsc::TryRecvError::Empty) => {},
            Err(mpsc::TryRecvError::Disconnected) => {
                self.receiver = None;
                self.retained.clear();
                self.organization_edit = None;
                self.notice = Some("Collection worker disconnected; outcome is uncertain. Refresh the destination before retrying.".into());
            },
        }
    }
}

fn lookup(state: &mut DesktopState) {
    state.composition.ensure_open();
    let result = (|| {
        let registry = state
            .composition
            .registry
            .as_ref()
            .ok_or("Reference registry unavailable.")?;
        let sources = registry
            .list()
            .into_iter()
            .filter(|source| source.enabled)
            .map(|source| SourceKey {
                source: source.manifest.id,
                version: source.manifest.version,
            })
            .collect();
        registry
            .lookup(
                &LookupQuery {
                    lemma: state.composition.query.text().trim().to_owned(),
                    language: None,
                    sources,
                },
                32,
            )
            .map_err(|error| error.to_string())
    })();
    match result {
        Ok(entries) => {
            state.composition.history.clear();
            state.composition.focused_sense = None;
            state.composition.entries = entries;
            state.composition.notice =
                Some("Lexical results from explicitly enabled sources (up to 32 entries).".into());
        },
        Err(error) => {
            state.composition.entries.clear();
            state.composition.notice = Some(error);
        },
    }
}

fn lookup_selection(state: &mut DesktopState, key: DocKey) {
    match SelectionSnapshot::capture(state, key) {
        Ok(selection) => {
            state.composition.query =
                TextInput::new(&selection.text[selection.start..selection.end]);
            state.composition.selection = Some(selection);
            state.composition.choices.clear();
            lookup(state);
        },
        Err(error) => state.composition.notice = Some(error),
    }
}

fn follow(state: &mut DesktopState, reference: &ReferenceId, sense: Option<String>) {
    let result = state
        .composition
        .registry
        .as_ref()
        .ok_or("Reference registry unavailable.".to_owned())
        .and_then(|registry| registry.entry(reference).map_err(|error| error.to_string()));
    match result {
        Ok(Some(entry)) => {
            if state.composition.history.len() >= 64 {
                state.composition.history.remove(0);
            }
            let prior = std::mem::replace(&mut state.composition.entries, vec![entry]);
            let prior_sense = std::mem::replace(&mut state.composition.focused_sense, sense);
            state.composition.history.push((prior, prior_sense));
        },
        Ok(None) => {
            state.composition.notice =
                Some("Related entry is not available in an enabled source.".into())
        },
        Err(error) => state.composition.notice = Some(error),
    }
}

fn import_pack(state: &mut DesktopState) {
    state.composition.ensure_open();
    use std::io::Read;
    let result: Result<(), String> = (|| {
        let path = state.composition.import_path.text().trim();
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        if !file
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("Reference import requires a regular file.".into());
        }
        let mut bytes = Vec::new();
        file.take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("Reference pack exceeds 32 MiB.".into());
        }
        let digest = state.composition.import_digest.text().trim().to_owned();
        if digest.is_empty() {
            return Err("Supply the expected entries BLAKE3 digest from your pack receipt.".into());
        }
        let registry = state
            .composition
            .registry
            .as_mut()
            .ok_or("Reference registry unavailable.")?;
        registry
            .import_json(&bytes, Some(&digest))
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    state.composition.notice = Some(match result {
        Ok(()) => "Reference pack installed, disabled. Enable it explicitly to use it.".into(),
        Err(error) => format!("Reference import refused: {error}"),
    });
}

fn registry_view(state: &DesktopState) -> DesktopView {
    let sources: Vec<(String, DesktopView)> = state
        .composition
        .registry
        .as_ref()
        .map(|registry| {
            registry
                .list()
                .into_iter()
                .map(|source| {
                    let id = source.manifest.id.clone();
                    let version = source.manifest.version.clone();
                    let enabled = source.enabled;
                    let installed = source.installed;
                    let label = format!(
                        "{} {} · {} · {} · {}",
                        source.manifest.label,
                        version,
                        source.manifest.language,
                        source.manifest.license.name,
                        if enabled {
                            "enabled"
                        } else if installed {
                            "installed, disabled"
                        } else {
                            "available, not installed"
                        }
                    );
                    (
                        format!("{id}:{version}"),
                        Box::new(el(
                            "div",
                            (
                                span(label),
                                span(source.manifest.attribution),
                                button(
                                    if enabled {
                                        "Disable source"
                                    } else {
                                        "Enable source"
                                    },
                                    move |state: &mut DesktopState, _| {
                                        let result = state
                                            .composition
                                            .registry
                                            .as_mut()
                                            .ok_or("Reference registry unavailable.".to_owned())
                                            .and_then(|registry| {
                                                registry
                                                    .set_enabled(&id, &version, !enabled)
                                                    .map_err(|error| error.to_string())
                                            });
                                        state.composition.entries.clear();
                                        state.composition.history.clear();
                                        state.composition.notice = Some(match result {
                                            Ok(()) => "Reference choice saved.".into(),
                                            Err(error) => error,
                                        });
                                    },
                                )
                                .attr("aria-disabled", (!installed).to_string())
                                .attr("aria-pressed", enabled.to_string()),
                            ),
                        )) as DesktopView,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Box::new(el(
        "section",
        (
            el("h4", "Reference sources — local, opt-in"),
            span(
                "Optional reference: Open English WordNet (https://en-word.net/). Import a chosen edition or subset; no download or source is enabled automatically.",
            ),
            Keyed::new(sources),
            el(
                "label",
                (
                    span("Normalized reference pack path"),
                    lens(
                        |input: &mut TextInput| text_field_typed(input),
                        |state: &mut DesktopState| &mut state.composition.import_path,
                    ),
                ),
            ),
            el(
                "label",
                (
                    span("Expected entries BLAKE3"),
                    lens(
                        |input: &mut TextInput| text_field_typed(input),
                        |state: &mut DesktopState| &mut state.composition.import_digest,
                    ),
                ),
            ),
            button("Import reference pack", |state: &mut DesktopState, _| {
                import_pack(state)
            }),
        ),
    ))
}

fn lexical_view(state: &DesktopState, key: DocKey) -> DesktopView {
    let entries: Vec<(String, DesktopView)> = state
        .composition
        .entries
        .iter()
        .map(|entry| {
            let reference = entry.id.clone();
            let senses: Vec<(String, DesktopView)> = entry
                .senses
                .iter()
                .filter(|sense| {
                    state
                        .composition
                        .focused_sense
                        .as_ref()
                        .is_none_or(|id| id == &sense.id)
                })
                .map(|sense| {
                    let definition = sense.definition.clone();
                    let label = format!("{} · {}", entry.lemma, sense.id);
                    let source = reference.clone();
                    let sense_id = sense.id.clone();
                    let relations: Vec<(usize, DesktopView)> = sense
                        .relations
                        .iter()
                        .chain(entry.relations.iter())
                        .enumerate()
                        .map(|(index, relation)| {
                            let target = relation.target.clone();
                            let target_sense = relation.target_sense.clone();
                            (
                                index,
                                Box::new(button(
                                    format!(
                                        "{:?}: {}{}",
                                        relation.kind,
                                        target.entry,
                                        target_sense
                                            .as_ref()
                                            .map(|id| format!(" / {id}"))
                                            .unwrap_or_default()
                                    ),
                                    move |state: &mut DesktopState, _| {
                                        follow(state, &target, target_sense.clone())
                                    },
                                )) as DesktopView,
                            )
                        })
                        .collect();
                    (
                        sense.id.clone(),
                        Box::new(el(
                            "div",
                            (
                                span(sense.definition.clone()),
                                span(sense.examples.join(" · ")),
                                button("Collect sense", move |state: &mut DesktopState, _| {
                                    let item = CollectionItem::new(
                                        ItemKind::Sense,
                                        label.clone(),
                                        definition.clone(),
                                    )
                                    .with_pack_source(PackSource {
                                        pack_id: source.source.clone(),
                                        pack_version: source.version.clone(),
                                        entry_ref: format!("{}#{}", source.entry, sense_id),
                                    });
                                    collect(state, item);
                                }),
                                Keyed::new(relations),
                            ),
                        )) as DesktopView,
                    )
                })
                .collect();
            (
                format!(
                    "{}:{}:{}",
                    reference.source, reference.version, reference.entry
                ),
                Box::new(el(
                    "article",
                    (
                        el("h4", entry.lemma.clone()),
                        span(format!(
                            "{} · {} {} · {:?}",
                            entry.language,
                            reference.source,
                            reference.version,
                            entry.part_of_speech
                        )),
                        span(
                            entry
                                .pronunciations
                                .iter()
                                .map(|pronunciation| {
                                    format!(
                                        "{}: {}{}",
                                        pronunciation.notation,
                                        pronunciation.value,
                                        pronunciation
                                            .variant
                                            .as_ref()
                                            .map(|variant| format!(" ({variant})"))
                                            .unwrap_or_default()
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" · "),
                        ),
                        Keyed::new(senses),
                    ),
                )) as DesktopView,
            )
        })
        .collect();
    Box::new(el(
        "section",
        (
            el("h4", "Connected lexical reading"),
            el(
                "label",
                (
                    span("Word or phrase"),
                    lens(
                        |input: &mut TextInput| text_field_typed(input),
                        |state: &mut DesktopState| &mut state.composition.query,
                    ),
                ),
            ),
            button("Look up", |state: &mut DesktopState, _| {
                state.composition.selection = None;
                lookup(state);
            }),
            button(
                "Read selected word or phrase",
                move |state: &mut DesktopState, _| lookup_selection(state, key),
            ),
            button("Back through connections", |state: &mut DesktopState, _| {
                if let Some((entries, sense)) = state.composition.history.pop() {
                    state.composition.entries = entries;
                    state.composition.focused_sense = sense;
                }
            })
            .attr(
                "aria-disabled",
                state.composition.history.is_empty().to_string(),
            ),
            button("Show all senses", |state: &mut DesktopState, _| {
                state.composition.focused_sense = None
            }),
            state.composition.selection.as_ref().map(|selection| {
                span(format!(
                    "Query from {} · bytes {}–{}",
                    selection.address, selection.start, selection.end
                ))
            }),
            state
                .composition
                .selection
                .as_ref()
                .filter(|selection| selection.key != key || !selection.current(state))
                .map(|_| {
                    span("Selection changed: these lexical results retain the earlier query.")
                }),
            Keyed::new(entries),
        ),
    ))
}

fn collect(state: &mut DesktopState, item: CollectionItem) {
    state.composition.request(vec![item], false);
}

fn collect_selection(state: &mut DesktopState, key: DocKey) {
    let result = SelectionSnapshot::capture(state, key).and_then(|selection| {
        let text = selection.text[selection.start..selection.end].to_owned();
        let anchor = selection.anchor()?;
        Ok(CollectionItem::new(
            ItemKind::Passage,
            text.chars().take(64).collect::<String>(),
            text,
        )
        .with_document_source(anchor))
    });
    match result {
        Ok(item) => collect(state, item),
        Err(error) => state.composition.notice = Some(error),
    }
}

fn analyze_sound(state: &mut DesktopState, key: DocKey, retain_choices: bool) {
    if !state.composition.cmudict_enabled {
        state.composition.notice =
            Some("Enable the bundled CMUdict source explicitly first.".into());
        return;
    }
    let result = SelectionSnapshot::capture(state, key).and_then(|selection| {
        if !retain_choices
            || state
                .composition
                .sound_selection
                .as_ref()
                .is_none_or(|old| {
                    old.key != selection.key
                        || old.text != selection.text
                        || old.start != selection.start
                        || old.end != selection.end
                })
        {
            state.composition.choices.clear();
        }
        let result = sound::analyze(
            &selection.text,
            selection.start..selection.end,
            &state.composition.choices,
            &state.composition.layers,
        )?;
        state.composition.sound_selection = Some(selection);
        Ok(result)
    });
    match result {
        Ok(reading) => {
            state.composition.notice = Some(format!(
                "Sound reading: {} tokens, {} unresolved. English lexical stress is a candidate reading, not intended performance.",
                reading.tokens.len(),
                reading.unresolved.len()
            ));
            state.composition.sound = Some(reading);
        },
        Err(error) => {
            state.composition.sound = None;
            state.composition.notice = Some(error);
        },
    }
}

fn sound_view(state: &DesktopState, key: DocKey) -> DesktopView {
    let active = state.composition.cmudict_enabled;
    let stale = state
        .composition
        .sound_selection
        .as_ref()
        .is_some_and(|snapshot| snapshot.key != key || !snapshot.current(state));
    let toggles: Vec<(usize, DesktopView)> = [
        ("Perfect rhyme", state.composition.layers.perfect_rhyme),
        ("Slant rhyme", state.composition.layers.slant_rhyme),
        ("Assonance", state.composition.layers.assonance),
        ("Alliteration", state.composition.layers.alliteration),
        ("Candidate meter", state.composition.layers.meter),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (label, enabled))| {
        (
            index,
            Box::new(
                button(
                    format!("{label}: {}", if enabled { "on" } else { "off" }),
                    move |state: &mut DesktopState, _| {
                        let layers = &mut state.composition.layers;
                        match index {
                            0 => layers.perfect_rhyme = !enabled,
                            1 => layers.slant_rhyme = !enabled,
                            2 => layers.assonance = !enabled,
                            3 => layers.alliteration = !enabled,
                            _ => layers.meter = !enabled,
                        }
                        // A changed configuration invalidates the old result; it never silently reruns.
                        state.composition.sound = None;
                    },
                )
                .attr("aria-pressed", enabled.to_string()),
            ) as DesktopView,
        )
    })
    .collect();
    let tokens: Vec<(usize, DesktopView)> = state.composition.sound.as_ref().map(|reading| {
        reading.tokens.iter().enumerate().map(|(index, token)| {
            let variants: Vec<(usize, DesktopView)> = token.variants.iter().enumerate().map(|(variant, label)| {
                (variant, Box::new(button(label.clone(), move |state: &mut DesktopState, _| {
                    if state.composition.sound_selection.as_ref().is_none_or(|selection| !selection.current(state) || selection.key != key) {
                        state.composition.notice = Some("Selection changed; rerun before choosing a pronunciation.".into()); return;
                    }
                    state.composition.choices.insert(index, variant);
                    analyze_sound(state, key, true);
                }).attr("aria-pressed", (token.selected == Some(variant)).to_string())
                  .attr("aria-disabled", stale.to_string())) as DesktopView)
            }).collect();
            (index, Box::new(el("div", (
                span(format!("{} · bytes {}–{}{}", token.word, token.start, token.end,
                    if token.variants.is_empty() { " · unresolved" } else if token.defaulted { " · first pronunciation (default)" } else { " · chosen pronunciation" })),
                Keyed::new(variants),
            ))) as DesktopView)
        }).collect()
    }).unwrap_or_default();
    let relations: Vec<(usize, DesktopView)> = state
        .composition
        .sound
        .as_ref()
        .map(|reading| {
            reading
                .relations
                .iter()
                .enumerate()
                .take(256)
                .map(|(index, relation)| {
                    (
                        index,
                        Box::new(span(format!(
                            "{}: {} ↔ {}",
                            relation.kind.label(),
                            reading.tokens[relation.left].word,
                            reading.tokens[relation.right].word
                        ))) as DesktopView,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let meter = state
        .composition
        .sound
        .as_ref()
        .and_then(|reading| reading.meter.as_ref())
        .map(|meter| {
            span(format!(
                "Candidate meter: {} · fit {:.0}%",
                meter.label,
                meter.fit * 100.0
            ))
        });
    Box::new(el(
        "section",
        (
            el("h4", "Sound-pattern reading"),
            button(
                "Use sound relationship recipe",
                move |state: &mut DesktopState, _| recipe::from_sound(state, key, false),
            ),
            span(
                "CMUdict · bundled English pronunciation data · Carnegie Mellon University · opt-in each launch",
            ),
            button(
                if active {
                    "Disable bundled CMUdict"
                } else {
                    "Enable bundled CMUdict"
                },
                |state: &mut DesktopState, _| {
                    state.composition.cmudict_enabled = !state.composition.cmudict_enabled;
                    state.composition.sound = None;
                },
            )
            .attr("aria-pressed", active.to_string()),
            el("div", Keyed::new(toggles)).attr("class", "knot-composition-tabs"),
            button(
                "Read selected sounds",
                move |state: &mut DesktopState, _| analyze_sound(state, key, false),
            )
            .attr("aria-disabled", (!active).to_string()),
            stale.then(|| span("Sound reading is stale: document or selection changed.")),
            state.composition.sound_selection.as_ref().map(|selection| {
                span(format!(
                    "Analyzed {} · bytes {}–{}",
                    selection.address, selection.start, selection.end
                ))
            }),
            Keyed::new(tokens),
            el(
                "div",
                (
                    state.composition.sound.as_ref().map(|reading| {
                        span(format!(
                            "{} sound connections (showing up to 256)",
                            reading.relations.len()
                        ))
                    }),
                    Keyed::new(relations),
                ),
            ),
            meter,
            button(
                "Collect sound reading",
                move |state: &mut DesktopState, _| {
                    let Some(selection) = state.composition.sound_selection.clone() else {
                        return;
                    };
                    if !selection.current(state) || selection.key != key {
                        state.composition.notice = Some(
                            "Selection changed; rerun before collecting this analysis.".into(),
                        );
                        return;
                    }
                    let Some(reading) = state.composition.sound.as_ref() else {
                        return;
                    };
                    let analysis = sound_note(reading, &state.composition.layers);
                    match selection.anchor() {
                        Ok(anchor) => collect(
                            state,
                            CollectionItem::new(ItemKind::Note, "Sound reading", analysis)
                                .with_document_source(anchor),
                        ),
                        Err(error) => state.composition.notice = Some(error),
                    }
                },
            ),
        ),
    ))
}

fn sound_note(reading: &SoundReading, layers: &SoundLayers) -> String {
    let enabled = [
        ("perfect rhyme", layers.perfect_rhyme),
        ("slant rhyme", layers.slant_rhyme),
        ("assonance", layers.assonance),
        ("alliteration", layers.alliteration),
        ("candidate meter", layers.meter),
    ]
    .into_iter()
    .filter_map(|(name, on)| on.then_some(name))
    .collect::<Vec<_>>()
    .join(", ");
    let mut lines = vec![format!(
        "Layers: {}",
        if enabled.is_empty() { "none" } else { &enabled }
    )];
    for token in &reading.tokens {
        let selected = token
            .selected
            .and_then(|index| token.variants.get(index))
            .map(String::as_str)
            .unwrap_or("unresolved");
        lines.push(format!(
            "{} · bytes {}–{} · {}{}",
            token.word,
            token.start,
            token.end,
            selected,
            if token.defaulted {
                " (first pronunciation)"
            } else {
                ""
            }
        ));
        if token.variants.len() > 1 {
            lines.push(format!("Alternatives: {}", token.variants.join("; ")));
        }
    }
    for relation in &reading.relations {
        lines.push(format!(
            "{}: {} ↔ {}",
            relation.kind.label(),
            reading.tokens[relation.left].word,
            reading.tokens[relation.right].word
        ));
    }
    if let Some(meter) = &reading.meter {
        lines.push(format!(
            "Candidate meter: {} · fit {:.0}%",
            meter.label,
            meter.fit * 100.0
        ));
    }
    lines.push(format!(
        "{} unresolved words. Lexical stress is not intended performance.",
        reading.unresolved.len()
    ));
    lines.push("Mora 0.1.0; CMUdict bundled SHA-256: 59d6398f55297e59afb2ca3276380827524c0940fcbbfcd19022bb76fd55f719".into());
    lines.join("\n")
}

fn start_organization_edit(state: &mut DesktopState, change: OrganizeComposition) {
    if state.composition.busy() {
        return;
    }
    let Some(target) = state
        .composition
        .selected_target
        .and_then(|i| state.composition.targets.get(i))
        .map(|p| p.target().clone())
    else {
        return;
    };
    let organization = &change.organization;
    state.composition.organization_label = TextInput::new(&organization.label);
    state.composition.organization_collection = TextInput::new(&organization.collection);
    state.composition.organization_notes = TextInput::new(&organization.author_notes);
    state.composition.organization_tags = TextInput::new(organization.tags.join(", "));
    state.composition.organization_order = TextInput::new(organization.order.to_string());
    state.composition.organization_edit = Some((target, change));
}

fn save_organization_edit(state: &mut DesktopState) {
    if state.composition.busy() {
        return;
    }
    let Some((target, mut change)) = state.composition.organization_edit.clone() else {
        return;
    };
    if state
        .composition
        .selected_target
        .and_then(|i| state.composition.targets.get(i))
        .map(|p| p.target())
        != Some(&target)
    {
        state.composition.notice =
            Some("Collection destination changed; reopen the item before editing.".into());
        return;
    }
    let Ok(order) = state
        .composition
        .organization_order
        .text()
        .trim()
        .parse::<i64>()
    else {
        state.composition.notice = Some("Order must be a signed whole number.".into());
        return;
    };
    change.organization.label = state.composition.organization_label.text().to_owned();
    change.organization.collection = state.composition.organization_collection.text().to_owned();
    change.organization.author_notes = state.composition.organization_notes.text().to_owned();
    // Preserve legacy tags containing commas when this field was not edited.
    if state.composition.organization_tags.text() != change.organization.tags.join(", ") {
        change.organization.tags = state
            .composition
            .organization_tags
            .text()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
    }
    change.organization.order = order;
    if let Err(error) = change.organization.validate() {
        state.composition.notice = Some(error.to_string());
        return;
    }
    state.composition.request_organization(change);
}

fn collection_matches(entry: &RetainedCompositionItem, search: &str, show_archived: bool) -> bool {
    if entry.organization.archived && !show_archived {
        return false;
    }
    let search = search.trim().to_lowercase();
    search.is_empty()
        || [
            &entry.organization.label,
            &entry.organization.collection,
            &entry.organization.author_notes,
            &entry.item.text,
        ]
        .into_iter()
        .chain(entry.organization.tags.iter())
        .any(|text| text.to_lowercase().contains(&search))
}

fn collection_view(state: &DesktopState) -> DesktopView {
    let items: Vec<(String, DesktopView)> = state
        .composition
        .retained
        .iter()
        .filter(|entry| {
            collection_matches(
                entry,
                state.composition.collection_search.text(),
                state.composition.show_archived,
            )
        })
        .map(|retained| {
            let item = &retained.item;
            let anchor = item.document_source.clone();
            let saved_recipe = item.projection_recipe.clone();
            let change = OrganizeComposition {
                author: retained.author,
                item_id: item.id.clone(),
                expected_revision: retained.organization_revision,
                organization: retained.organization.clone(),
            };
            let edit = change.clone();
            (
                format!("{}:{}", crate::workspace::hex32(&retained.author), item.id),
                Box::new(el(
                    "div",
                    (
                        span(retained.organization.label.clone()),
                        item.document_source
                            .as_ref()
                            .map(|source| span(format!("Quotation: {}", source.exact_quote))),
                        el("pre", item.text.clone()).attr("class", "knot-readings-note"),
                        span(format!(
                            "{} · {:?} · {}",
                            retained.organization.collection,
                            item.kind,
                            retained.organization.author_notes
                        )),
                        span(format!(
                            "Tags: {} · order {}{}",
                            retained.organization.tags.join(", "),
                            retained.organization.order,
                            if retained.organization.archived {
                                " · archived"
                            } else {
                                ""
                            }
                        )),
                        span(format!(
                            "Organization revision {}",
                            crate::workspace::hex32(&retained.organization_revision)
                        )),
                        button(
                            "Edit collection details",
                            move |state: &mut DesktopState, _| {
                                start_organization_edit(state, edit.clone())
                            },
                        )
                        .attr("aria-disabled", state.composition.busy().to_string()),
                        button(
                            if retained.organization.archived {
                                "Restore collection item"
                            } else {
                                "Archive collection item"
                            },
                            move |state: &mut DesktopState, _| {
                                if state.composition.busy() {
                                    return;
                                }
                                let mut change = change.clone();
                                change.organization.archived = !change.organization.archived;
                                state.composition.request_organization(change);
                            },
                        )
                        .attr("aria-disabled", state.composition.busy().to_string()),
                        span(
                            item.pack_sources
                                .iter()
                                .map(|source| {
                                    format!(
                                        "{} {} / {}",
                                        source.pack_id, source.pack_version, source.entry_ref
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("; "),
                        ),
                        span(format!(
                            "Retained operation {} · author {}",
                            crate::workspace::hex32(&retained.receipt.operation),
                            crate::workspace::hex32(&retained.author)
                        )),
                        saved_recipe.map(|saved| {
                            button(
                                "Open retained relationship recipe",
                                move |state: &mut DesktopState, _| {
                                    recipe::reopen(state, saved.clone())
                                },
                            )
                        }),
                        button("Return to source", move |state: &mut DesktopState, _| {
                            let Some(anchor) = anchor.as_ref() else {
                                return;
                            };
                            return_to_source(state, anchor);
                        })
                        .attr("aria-disabled", item.document_source.is_none().to_string()),
                    ),
                )) as DesktopView,
            )
        })
        .collect();
    let targets: Vec<(usize, DesktopView)> = state
        .composition
        .targets
        .iter()
        .enumerate()
        .map(|(index, port)| {
            let target = port.target();
            let label = format!(
                "Retain in {} · persona {} · space {} · writer {}",
                target.persona.label,
                target.persona.stable_id,
                crate::workspace::hex32(&target.space_id),
                crate::workspace::hex32(&target.writer)
            );
            (
                index,
                Box::new(
                    button(label, move |state: &mut DesktopState, _| {
                        if state.composition.busy() {
                            return;
                        }
                        state.composition.generation = state.composition.generation.wrapping_add(1);
                        state.composition.selected_target = Some(index);
                        state.composition.organization_edit = None;
                        state.composition.retained.clear();
                        state.composition.request(Vec::new(), false);
                    })
                    .attr(
                        "aria-pressed",
                        (state.composition.selected_target == Some(index)).to_string(),
                    )
                    .attr("aria-disabled", state.composition.busy().to_string()),
                ) as DesktopView,
            )
        })
        .collect();
    Box::new(el(
        "section",
        (
            span("Composition collection — retained in the explicitly selected Knot mere"),
            state.composition.targets.is_empty().then(|| span("Unavailable: the host has supplied no writable Knot mere. No persona, space, or plaintext collection is created automatically.")),
            Keyed::new(targets),
            button("Refresh retained collection", |state: &mut DesktopState, _| state.composition.request(Vec::new(), false)),
            button("Retry same collection request", |state: &mut DesktopState, _| {
                if !state.composition.retry_items.is_empty() { state.composition.request(state.composition.retry_items.clone(), false); }
            }).attr("aria-disabled", state.composition.retry_items.is_empty().to_string()),
            button("Import legacy collection into selected mere", |state: &mut DesktopState, _| state.composition.request(Vec::new(), true)),
            span("Legacy import is explicit, preserves item IDs for safe retries, and never modifies or deletes collection.json."),
            el("label", (span("Search collection"), lens(|input: &mut TextInput| text_field_typed(input), |state: &mut DesktopState| &mut state.composition.collection_search))),
            button("Include archived items", |state: &mut DesktopState, _| state.composition.show_archived = !state.composition.show_archived)
                .attr("aria-pressed", state.composition.show_archived.to_string()),
            span("Archive hides an item from the ordinary view; it does not erase its retained history. Commas separate tags."),
            state.composition.organization_edit.as_ref().filter(|_| !state.composition.busy()).map(|_| el("section", (
                el("h4", "Edit collection details"),
                el("label", (span("Label"), lens(|input: &mut TextInput| text_field_typed(input), |state: &mut DesktopState| &mut state.composition.organization_label))),
                el("label", (span("Collection"), lens(|input: &mut TextInput| text_field_typed(input), |state: &mut DesktopState| &mut state.composition.organization_collection))),
                el("label", (span("Author notes"), lens(|input: &mut TextInput| textarea_typed(input), |state: &mut DesktopState| &mut state.composition.organization_notes))),
                el("label", (span("Tags, separated by commas"), lens(|input: &mut TextInput| text_field_typed(input), |state: &mut DesktopState| &mut state.composition.organization_tags))),
                el("label", (span("Order"), lens(|input: &mut TextInput| text_field_typed(input), |state: &mut DesktopState| &mut state.composition.organization_order))),
                button("Save collection details", |state: &mut DesktopState, _| save_organization_edit(state)).attr("aria-disabled", state.composition.busy().to_string()),
                button("Cancel collection edit", |state: &mut DesktopState, _| state.composition.organization_edit = None),
                span("Saving changes organization only; original material and provenance remain retained.").attr("class", "knot-collection-editor-end"),
            ))),
            Keyed::new(items),
        ),
    ))
}

fn return_to_source(state: &mut DesktopState, anchor: &DocumentAnchor) {
    // Scratch addresses can name multiple occurrences or documents and are
    // not an owner-issued durable source identity.
    if anchor.document_address.starts_with("scratch:") {
        state.composition.notice = Some("Scratch quotations have no durable source identity. The retained quotation remains available; save the source before collecting a navigable anchor.".into());
        return;
    }
    let targets: Vec<_> = state
        .docs
        .docs()
        .filter_map(|(key, entry)| {
            let snapshot = entry.document.snapshot();
            (snapshot.source.address == anchor.document_address).then_some((key, snapshot.text))
        })
        .collect();
    let [(key, text)] = targets.as_slice() else {
        state.composition.notice =
            Some("Open exactly one original document to return to this quotation.".into());
        return;
    };
    if let Err(error) = anchor.validate_source(text) {
        state.composition.notice = Some(format!(
            "Original source changed; retained quotation is safe: {error}"
        ));
        return;
    }
    state.focus_document(*key);
    let result = state.document_mut().session_mut().select_source_span(
        &anchor.document_address,
        text,
        anchor.byte_span.start as usize,
        anchor.byte_span.end as usize,
    );
    if result.is_ok() {
        state.entry_mut().focus_source_requested = true;
    }
    state.composition.notice = result.err();
}

pub(crate) fn view(state: &DesktopState, key: DocKey) -> DesktopView {
    let tabs: Vec<(usize, DesktopView)> = ["Lexical", "Sound", "Collection", "Sources", "Recipes"]
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            (
                index,
                Box::new(
                    button(label, move |state: &mut DesktopState, _| {
                        if index == 3 {
                            state.composition.ensure_open();
                        }
                        state.composition.section = index
                    })
                    .attr(
                        "aria-pressed",
                        (state.composition.section == index).to_string(),
                    ),
                ) as DesktopView,
            )
        })
        .collect();
    let content = match state.composition.section {
        0 => lexical_view(state, key),
        1 => sound_view(state, key),
        2 => collection_view(state),
        3 => registry_view(state),
        _ => recipe::view(state, key),
    };
    Box::new(el("section", (
        el("h3", "Composition"),
        span("Readings observe; only Collect retains material. No reference source is enabled automatically."),
        button("Collect selected passage", move |state: &mut DesktopState, _| collect_selection(state, key))
            .attr("aria-label", "Collect selected passage"),
        el("div", Keyed::new(tabs)).attr("class", "knot-composition-tabs"),
        content,
        state.composition.notice.as_ref().map(|notice| span(notice.clone()).attr("class", "knot-composition-notice")),
    )).attr("class", "knot-composition"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium_genet_winit_host::{Harness, WindowCommands};
    use genet_scripted_dom::{NodeId, ScriptedDom};
    use knot_document::KnotDocumentSession;
    use layout_dom_api::LayoutDom;
    use reference_data::{
        LexicalRelation, LexicalSense, NormalizedPack, PartOfSpeech, RelationKind, digest_entries,
    };
    use reference_data::{LicenseInfo, ReferenceFeature, SourceManifest};
    use taproot::Selector;

    struct FakePort {
        items: std::sync::Mutex<Vec<RetainedCompositionItem>>,
        revoked: std::sync::atomic::AtomicBool,
        target: knot_capture::KnotRetainTargetV1,
    }

    impl CompositionRetainPort for FakePort {
        fn target(&self) -> &knot_capture::KnotRetainTargetV1 {
            &self.target
        }
        fn retain(
            &self,
            expected: &knot_capture::KnotRetainTargetV1,
            item: CollectionItem,
        ) -> Result<knot_composition::retention::CompositionReceipt, knot_capture::KnotRetainError>
        {
            assert_eq!(expected, &self.target);
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter().find(|entry| entry.item.id == item.id) {
                return Ok(existing.receipt.clone());
            }
            let receipt = knot_composition::retention::CompositionReceipt {
                target: self.target.clone(),
                item_id: item.id.clone(),
                operation: [4; 32],
                already_retained: false,
            };
            items.push(RetainedCompositionItem::from_retention(
                item,
                self.target.writer,
                receipt.clone(),
            ));
            Ok(receipt)
        }
        fn list(
            &self,
            expected: &knot_capture::KnotRetainTargetV1,
        ) -> Result<Vec<RetainedCompositionItem>, knot_capture::KnotRetainError> {
            assert_eq!(expected, &self.target);
            if self.revoked.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(knot_capture::KnotRetainError("authority revoked".into()));
            }
            Ok(self.items.lock().unwrap().clone())
        }
        fn organize(
            &self,
            expected: &knot_capture::KnotRetainTargetV1,
            change: OrganizeComposition,
        ) -> Result<knot_composition::retention::CompositionReceipt, knot_capture::KnotRetainError>
        {
            assert_eq!(expected, &self.target);
            if self.revoked.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(knot_capture::KnotRetainError("authority revoked".into()));
            }
            let mut items = self.items.lock().unwrap();
            let entry = items
                .iter_mut()
                .find(|entry| entry.author == change.author && entry.item.id == change.item_id)
                .ok_or_else(|| knot_capture::KnotRetainError("missing item".into()))?;
            if entry.organization_revision != change.expected_revision {
                return Err(knot_capture::KnotRetainError("stale organization".into()));
            }
            entry.organization = change.organization;
            entry.organization_revision[0] = entry.organization_revision[0].wrapping_add(1);
            Ok(knot_composition::retention::CompositionReceipt {
                target: self.target.clone(),
                item_id: change.item_id,
                operation: entry.organization_revision,
                already_retained: false,
            })
        }
    }

    fn attach_fake(state: &mut DesktopState) -> Arc<FakePort> {
        let port = Arc::new(FakePort {
            items: Default::default(),
            revoked: Default::default(),
            target: knot_capture::KnotRetainTargetV1 {
                persona: knot_capture::KnotPersonaDisplayV1 {
                    stable_id: "test-persona".into(),
                    label: "Test persona".into(),
                },
                space_id: [1; 32],
                writer: [2; 32],
                encryption: knot_capture::KnotRetainEncryptionV1::PersonalVaultV1,
            },
        });
        state.composition.targets = vec![port.clone()];
        state.composition.selected_target = Some(0); // Explicit test fixture admission, not production default.
        port
    }

    fn settle_collection(state: &mut DesktopState) {
        for _ in 0..1000 {
            state.composition.drain();
            if !state.composition.busy() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("collection worker did not finish");
    }

    #[test]
    fn organization_worker_preserves_original_and_filters_archived_and_search() {
        let mut state = state("cat");
        let port = attach_fake(&mut state);
        let item = CollectionItem::new(ItemKind::Note, "Cat", "original source");
        let receipt = port.retain(port.target(), item.clone()).unwrap();
        state.composition.request(Vec::new(), false);
        settle_collection(&mut state);
        let entry = state.composition.retained[0].clone();
        start_organization_edit(
            &mut state,
            OrganizeComposition {
                author: entry.author,
                item_id: item.id.clone(),
                expected_revision: entry.organization_revision,
                organization: entry.organization,
            },
        );
        state.composition.organization_label = TextInput::new("Night study");
        state.composition.organization_collection = TextInput::new("Poetry");
        state.composition.organization_tags = TextInput::new("prosody, rhyme");
        state.composition.organization_order = TextInput::new("-2");
        save_organization_edit(&mut state);
        settle_collection(&mut state);
        let entry = state.composition.retained[0].clone();
        assert_eq!(entry.item, item);
        assert_eq!(entry.receipt, receipt);
        assert_eq!(entry.organization.label, "Night study");
        assert_eq!(entry.organization.collection, "Poetry");
        assert_eq!(entry.organization.order, -2);
        assert!(collection_matches(&entry, "PROSODY", false));
        assert!(collection_matches(&entry, "original source", false));
        assert!(!collection_matches(&entry, "unrelated", false));
        let mut change = OrganizeComposition {
            author: entry.author,
            item_id: item.id,
            expected_revision: entry.organization_revision,
            organization: entry.organization,
        };
        change.organization.archived = true;
        state.composition.request_organization(change);
        settle_collection(&mut state);
        assert!(!collection_matches(
            &state.composition.retained[0],
            "",
            false
        ));
        assert!(collection_matches(&state.composition.retained[0], "", true));
    }

    #[test]
    fn organization_editor_refuses_changed_destination_and_invalid_order_before_submission() {
        let mut state = state("cat");
        let port = attach_fake(&mut state);
        let item = CollectionItem::new(ItemKind::Note, "Cat", "source");
        port.retain(port.target(), item.clone()).unwrap();
        let entry = port.list(port.target()).unwrap().remove(0);
        start_organization_edit(
            &mut state,
            OrganizeComposition {
                author: entry.author,
                item_id: item.id,
                expected_revision: entry.organization_revision,
                organization: entry.organization,
            },
        );
        state.composition.organization_order = TextInput::new("not a number");
        save_organization_edit(&mut state);
        assert!(!state.composition.busy());
        assert!(
            state
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("whole number")
        );
        state.composition.selected_target = None;
        save_organization_edit(&mut state);
        assert!(!state.composition.busy());
        assert!(
            state
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("destination changed")
        );
        assert_eq!(
            port.list(port.target()).unwrap()[0].organization_revision,
            entry.organization_revision
        );
    }

    fn state(text: &str) -> DesktopState {
        DesktopState::new(
            KnotDocumentSession::scratch("scratch:composition", text),
            WindowCommands::new(),
        )
    }

    fn select(state: &mut DesktopState, start: usize, end: usize) {
        let snapshot = state.document().snapshot();
        state
            .document_mut()
            .session_mut()
            .select_source_span(&snapshot.source.address, &snapshot.text, start, end)
            .unwrap();
    }

    fn composition_for_focused(state: &DesktopState) -> DesktopView {
        view(state, state.focused_key().unwrap())
    }

    fn harness(
        state: DesktopState,
    ) -> Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView> {
        let mut host = Harness::new(
            String::new(),
            state,
            composition_for_focused as fn(&DesktopState) -> DesktopView,
        );
        host.layout_at(1100.0, 800.0);
        host
    }

    type RecipeHost = Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;

    fn workspace_composition(state: &DesktopState) -> DesktopView {
        Box::new(
            el("div", view(state, state.focused_key().unwrap()))
                .attr("class", state.appearance.root_class()),
        )
    }

    /// The panel under the real desktop sheet and fonts, with the frame hook
    /// that measures recipe cards.
    fn recipe_harness(state: DesktopState) -> RecipeHost {
        let mut hooks = cambium_genet_winit_host::inert_hooks();
        hooks.frame = Box::new(super::measure_recipe_cards);
        let mut host = Harness::with_hooks(
            cambium_genet_winit_host::Init {
                state,
                logic: workspace_composition as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: crate::fonts::bundled_fonts(),
                images: Vec::new(),
            },
            hooks,
        );
        host.layout_at(1100.0, 800.0);
        host
    }

    /// One native frame: the frame hook, then the layout it feeds.
    fn frame(host: &mut RecipeHost) {
        host.prepare_frame();
        host.relayout();
    }

    /// The occurrence cards and the measurement probes now in the DOM.
    fn cards_and_probes(host: &RecipeHost) -> (Vec<NodeId>, Vec<NodeId>) {
        host.with_dom(|dom| {
            fn walk(
                dom: &ScriptedDom,
                node: NodeId,
                cards: &mut Vec<NodeId>,
                probes: &mut Vec<NodeId>,
            ) {
                let is_button = dom
                    .element_name(node)
                    .is_some_and(|name| name.local.as_ref() == "button");
                if is_button && text_content(dom, node).contains(" · token-") {
                    cards.push(node);
                }
                if dom
                    .attribute(
                        node,
                        &layout_dom_api::Namespace::from(""),
                        &layout_dom_api::LocalName::from("data-knot-recipe-measure"),
                    )
                    .is_some()
                {
                    probes.push(node);
                }
                for child in dom.dom_children(node) {
                    walk(dom, child, cards, probes);
                }
            }
            let (mut cards, mut probes) = (Vec::new(), Vec::new());
            walk(dom, dom.document(), &mut cards, &mut probes);
            (cards, probes)
        })
    }

    fn recipe_state(text: &str, start: usize, end: usize) -> DesktopState {
        let mut state = state(text);
        attach_fake(&mut state);
        select(&mut state, start, end);
        let key = state.focused_key().unwrap();
        state.composition.cmudict_enabled = true;
        state.composition.layers.perfect_rhyme = true;
        analyze_sound(&mut state, key, false);
        recipe::from_sound(&mut state, key, false);
        assert!(
            state.composition.recipe.material.is_some(),
            "{:?}",
            state.composition.notice
        );
        state
    }

    #[test]
    fn recipe_cards_are_measured_once_per_label_set_and_hidden_for_one_frame() {
        let mut host = recipe_harness(recipe_state("night night light", 0, 17));
        let measurements = |host: &RecipeHost| host.state().composition.recipe.measurements;

        // The frame the labels first appear: probes only, no cards.
        let (cards, probes) = cards_and_probes(&host);
        assert!(cards.is_empty(), "the scene is hidden until measured");
        assert_eq!(probes.len(), 3, "one probe per drawn label");
        host.with_dom(|dom| {
            for probe in &probes {
                assert_eq!(
                    dom.dom_children(*probe).count(),
                    0,
                    "a probe has no DOM text"
                );
                assert!(
                    dom.element_name(*probe)
                        .is_some_and(|name| name.local.as_ref() == "span")
                );
            }
            for label in [
                "night · token-0-5",
                "night · token-6-11",
                "light · token-12-17",
            ] {
                let selector = Selector::role("button").containing(label);
                assert!(
                    taproot::matching(dom, &selector).is_empty(),
                    "{label} resolves to nothing while hidden"
                );
            }
        });
        assert!(
            !rendered_text(&host).contains(" · token-"),
            "no probe text reaches the DOM"
        );
        assert_eq!(measurements(&host), 0);

        // The next frame measures once and draws.
        frame(&mut host);
        assert_eq!(measurements(&host), 1);
        let (cards, probes) = cards_and_probes(&host);
        assert_eq!(
            (cards.len(), probes.len()),
            (3, 0),
            "the probes go once measured"
        );
        let card = host
            .state()
            .composition
            .recipe
            .measured
            .as_ref()
            .unwrap()
            .card;
        assert!(card.h < 68.0, "one line, not the old 68: {card:?}");
        for node in &cards {
            let (_, _, width, height) = host.painted_rect(*node).unwrap();
            assert_eq!(
                (width, height),
                (card.w, card.h),
                "the card footprint is the drawn button"
            );
        }
        // The scene is exactly as tall as its laid-out cards: no empty band.
        let scene = host.with_dom(|dom| dom.parent(cards[0]).unwrap());
        let (_, scene_top, _, scene_height) = host.painted_rect(scene).unwrap();
        let card_bottom = cards
            .iter()
            .map(|node| {
                let (_, top, _, height) = host.painted_rect(*node).unwrap();
                top + height
            })
            .fold(0.0f32, f32::max);
        assert_eq!(
            scene_height,
            card_bottom - scene_top,
            "the scene fits its cards"
        );

        // Selection, spacing, zoom and theme keep the cards and the measurement.
        let unchanged = |host: &mut RecipeHost, what: &str| {
            for _ in 0..3 {
                let (cards, probes) = cards_and_probes(host);
                assert_eq!(
                    (cards.len(), probes.len()),
                    (3, 0),
                    "{what} keeps the scene drawn"
                );
                frame(host);
            }
            assert_eq!(
                host.state().composition.recipe.measurements,
                1,
                "{what} does not re-measure"
            );
        };
        unchanged(&mut host, "an idle frame");
        assert!(host.click_on(&Selector::role("button").containing("night · token-6-11")));
        host.after_dispatch();
        host.relayout();
        unchanged(&mut host, "a selection");
        assert!(host.click_on(&Selector::role("button").containing("Increase recipe spacing")));
        host.after_dispatch();
        host.relayout();
        unchanged(&mut host, "a spacing edit");
        host.set_ui_zoom(2.0);
        host.relayout();
        unchanged(&mut host, "zoom");
        host.set_ui_zoom(1.0);
        host.relayout();
        host.update(|state| state.appearance.dark = !state.appearance.dark);
        unchanged(&mut host, "a theme change");

        // A new label set hides the scene for exactly one frame, then measures once.
        host.update(|state| {
            let key = state.focused_key().unwrap();
            select(state, 6, 17);
            analyze_sound(state, key, false);
            recipe::from_sound(state, key, false);
        });
        let (cards, probes) = cards_and_probes(&host);
        assert_eq!(
            (cards.len(), probes.len()),
            (0, 2),
            "the new label set is hidden"
        );
        frame(&mut host);
        assert_eq!(measurements(&host), 2);
        unchanged_after(&mut host, 2);
    }

    #[test]
    fn a_recipe_below_its_minimum_shows_its_refusal_and_no_scene() {
        // A relationship recipe needs two occurrences (Scenograph refuses a
        // lower minimum), so the compiler refuses anything smaller and no
        // empty scene can be drawn.
        let mut state = recipe_state("night night light", 0, 17);
        if let Some(material) = state.composition.recipe.material.as_mut() {
            material.dataset.dataset.occurrences.truncate(1);
        }
        let mut host = recipe_harness(state);
        frame(&mut host);
        let (cards, probes) = cards_and_probes(&host);
        assert_eq!((cards.len(), probes.len()), (0, 0));
        assert!(rendered_text(&host).contains("Recipe cannot be realized"));
    }

    #[test]
    fn a_probe_that_measures_nothing_keeps_the_scene_hidden() {
        // Without the desktop sheet a probe has no generated label, so its rect
        // is empty: nothing is stored and no card is compiled. After a few
        // frames the hook stops asking for more (Mere burn plan 13.46); a new
        // label set or window size measures again.
        let mut hooks = cambium_genet_winit_host::inert_hooks();
        hooks.frame = Box::new(super::measure_recipe_cards);
        let mut host = Harness::with_hooks(
            cambium_genet_winit_host::Init {
                state: recipe_state("night night light", 0, 17),
                logic: workspace_composition as fn(&DesktopState) -> DesktopView,
                sheet: String::new(),
                fonts: Vec::new(),
                images: Vec::new(),
            },
            hooks,
        );
        host.layout_at(1100.0, 800.0);
        let hidden = |host: &RecipeHost, probes: usize, what: &str| {
            let (cards, found) = cards_and_probes(host);
            assert_eq!(
                (cards.len(), found.len()),
                (0, probes),
                "{what}: the scene stays hidden"
            );
            assert_eq!(
                host.state().composition.recipe.measured,
                None,
                "{what}: nothing is stored"
            );
            assert_eq!(
                host.state().composition.recipe.measurements,
                0,
                "{what}: no card is compiled"
            );
        };
        // Frames come for the attempts, then stop at the cap and stay stopped.
        let gives_up = |host: &mut RecipeHost, probes: usize, what: &str| {
            for attempt in 1..=recipe::MEASURE_ATTEMPTS {
                let more = host.prepare_frame();
                host.relayout();
                assert_eq!(
                    more,
                    attempt < recipe::MEASURE_ATTEMPTS,
                    "{what}: attempt {attempt} of {} asks for another frame until the cap",
                    recipe::MEASURE_ATTEMPTS
                );
                hidden(host, probes, what);
            }
            for _ in 0..5 {
                assert!(!host.prepare_frame(), "{what}: no frames after the cap");
                host.relayout();
                hidden(host, probes, what);
            }
            let failed = host.state().composition.recipe.failed.clone().unwrap();
            assert_eq!(
                failed.frames,
                recipe::MEASURE_ATTEMPTS,
                "{what}: the cap holds"
            );
        };
        gives_up(&mut host, 3, "the first label set");

        // A resize measures again, then gives up again.
        host.layout_at(900.0, 700.0);
        gives_up(&mut host, 3, "after a resize");

        // A zoom alone is not a resize: it stays given up.
        host.set_ui_zoom(2.0);
        host.relayout();
        assert!(!host.prepare_frame(), "a zoom does not restart measuring");
        host.set_ui_zoom(1.0);
        host.relayout();

        // A new label set measures again, then gives up again.
        host.update(|state| {
            let key = state.focused_key().unwrap();
            select(state, 6, 17);
            analyze_sound(state, key, false);
            recipe::from_sound(state, key, false);
        });
        gives_up(&mut host, 2, "a new label set");
    }

    fn unchanged_after(host: &mut RecipeHost, count: u32) {
        for _ in 0..3 {
            let (cards, probes) = cards_and_probes(host);
            assert!(
                !cards.is_empty() && probes.is_empty(),
                "drawn after one hidden frame"
            );
            frame(host);
        }
        assert_eq!(host.state().composition.recipe.measurements, count);
    }

    fn text_content(dom: &ScriptedDom, node: NodeId) -> String {
        let own = dom.text(node).unwrap_or_default();
        let children = dom
            .dom_children(node)
            .map(|child| text_content(dom, child))
            .collect::<String>();
        format!("{own}{children}")
    }

    fn rendered_text(
        host: &Harness<DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>,
    ) -> String {
        let dom = host.runner().dom();
        let dom = dom.borrow();
        text_content(&dom, dom.document())
    }

    fn pack() -> (Vec<u8>, String) {
        let entries = vec![LexicalEntry {
            id: ReferenceId {
                source: "fixture".into(),
                version: "1".into(),
                entry: "bank".into(),
            },
            lemma: "bank".into(),
            language: "en".into(),
            part_of_speech: Some(PartOfSpeech::Noun),
            senses: vec![LexicalSense {
                id: "shore".into(),
                concept_id: Some("river-edge".into()),
                definition: "Edge of a river.".into(),
                examples: vec![],
                relations: vec![],
            }],
            pronunciations: vec![],
            relations: vec![],
        }];
        let digest = digest_entries(&entries).unwrap();
        let pack = NormalizedPack {
            schema_version: 1,
            entries,
            manifest: SourceManifest {
                id: "fixture".into(),
                version: "1".into(),
                label: "Synthetic test lexicon".into(),
                language: "en".into(),
                features: vec![ReferenceFeature::Definitions],
                license: LicenseInfo {
                    name: "CC0-1.0".into(),
                    text: None,
                    url: Some("https://creativecommons.org/publicdomain/zero/1.0/".into()),
                },
                attribution: "Knot test authors".into(),
                upstream: None,
                digest: Some(digest.clone()),
            },
        };
        (serde_json::to_vec(&pack).unwrap(), digest)
    }

    fn related_pack() -> (Vec<u8>, String) {
        let target = ReferenceId {
            source: "fixture".into(),
            version: "1".into(),
            entry: "edge".into(),
        };
        let entries = vec![
            LexicalEntry {
                id: ReferenceId {
                    source: "fixture".into(),
                    version: "1".into(),
                    entry: "bank".into(),
                },
                lemma: "bank".into(),
                language: "en".into(),
                part_of_speech: Some(PartOfSpeech::Noun),
                senses: vec![LexicalSense {
                    id: "shore".into(),
                    concept_id: Some("river-edge".into()),
                    definition: "Land beside water.".into(),
                    examples: vec![],
                    relations: vec![LexicalRelation {
                        kind: RelationKind::Related,
                        target: target.clone(),
                        target_sense: Some("river".into()),
                    }],
                }],
                pronunciations: vec![],
                relations: vec![],
            },
            LexicalEntry {
                id: target,
                lemma: "edge".into(),
                language: "en".into(),
                part_of_speech: Some(PartOfSpeech::Noun),
                senses: vec![
                    LexicalSense {
                        id: "boundary".into(),
                        concept_id: None,
                        definition: "Outer limit.".into(),
                        examples: vec![],
                        relations: vec![],
                    },
                    LexicalSense {
                        id: "river".into(),
                        concept_id: Some("river-edge".into()),
                        definition: "Shore of a stream.".into(),
                        examples: vec![],
                        relations: vec![],
                    },
                ],
                pronunciations: vec![],
                relations: vec![],
            },
        ];
        let digest = digest_entries(&entries).unwrap();
        let pack = NormalizedPack {
            schema_version: 1,
            entries,
            manifest: SourceManifest {
                id: "fixture".into(),
                version: "1".into(),
                label: "Synthetic test lexicon".into(),
                language: "en".into(),
                features: vec![ReferenceFeature::Definitions, ReferenceFeature::Relations],
                license: LicenseInfo {
                    name: "CC0-1.0".into(),
                    text: None,
                    url: Some("https://creativecommons.org/publicdomain/zero/1.0/".into()),
                },
                attribution: "Knot test authors".into(),
                upstream: None,
                digest: Some(digest.clone()),
            },
        };
        (serde_json::to_vec(&pack).unwrap(), digest)
    }

    #[test]
    fn relation_targets_exact_sense_and_collection_keeps_pack_provenance() {
        let root = tempfile::tempdir().unwrap();
        let mut state = state("bank");
        state.composition = CompositionState::open(Some(root.path().into()));
        let (bytes, digest) = related_pack();
        let registry = state.composition.registry.as_mut().unwrap();
        registry.import_json(&bytes, Some(&digest)).unwrap();
        registry.set_enabled("fixture", "1", true).unwrap();
        state.composition.query = TextInput::new("bank");
        lookup(&mut state);
        let relation = state.composition.entries[0].senses[0].relations[0].clone();
        follow(&mut state, &relation.target, relation.target_sense);
        assert_eq!(state.composition.entries[0].id.entry, "edge");
        assert_eq!(state.composition.focused_sense.as_deref(), Some("river"));

        let mut host = harness(state);
        host.update(|state| {
            attach_fake(state);
        });
        assert!(host.click_on(&Selector::role("button").containing("Collect sense")));
        host.after_dispatch();
        host.update(settle_collection);
        let item = &host.state().composition.retained[0].item;
        assert_eq!(item.text, "Shore of a stream.");
        assert_eq!(item.pack_sources[0].pack_id, "fixture");
        assert_eq!(item.pack_sources[0].pack_version, "1");
        assert_eq!(item.pack_sources[0].entry_ref, "edge#river");
    }

    #[test]
    fn stale_selection_refuses_sound_collection() {
        let root = tempfile::tempdir().unwrap();
        let mut state = state("cat bat");
        state.composition = CompositionState::open(Some(root.path().into()));
        select(&mut state, 0, 7);
        let key = state.focused_key().unwrap();
        state.composition.cmudict_enabled = true;
        analyze_sound(&mut state, key, false);
        assert!(state.composition.sound.is_some());
        select(&mut state, 0, 3);
        state.composition.section = 1;
        let mut host = harness(state);
        assert!(rendered_text(&host).contains("Sound reading is stale"));
        assert!(host.click_on(&Selector::role("button").containing("Collect sound reading")));
        host.after_dispatch();
        assert!(host.state().composition.retained.is_empty());
        assert!(
            host.state()
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("Selection changed")
        );
    }

    #[test]
    fn sound_collection_cannot_cross_to_new_focused_document() {
        let root = tempfile::tempdir().unwrap();
        let mut state = state("cat bat");
        state.composition = CompositionState::open(Some(root.path().join("composition")));
        select(&mut state, 0, 7);
        let first = state.focused_key().unwrap();
        state.composition.cmudict_enabled = true;
        analyze_sound(&mut state, first, false);
        assert!(state.composition.sound.is_some());

        let other = root.path().join("other.djot");
        std::fs::write(&other, "dog fog").unwrap();
        assert!(state.open_path(other));
        let second = state.focused_key().unwrap();
        assert_ne!(first, second);
        assert_eq!(
            state.composition.sound_selection.as_ref().unwrap().key,
            first
        );
        state.composition.section = 1;
        let mut host = harness(state);
        assert!(host.click_on(&Selector::role("button").containing("Collect sound reading")));
        host.after_dispatch();
        assert!(host.state().composition.retained.is_empty());
        assert!(
            host.state()
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("Selection changed")
        );
    }

    #[test]
    fn lexical_selection_from_another_document_is_reported_stale() {
        let root = tempfile::tempdir().unwrap();
        let mut state = state("bank");
        select(&mut state, 0, 4);
        let first = state.focused_key().unwrap();
        lookup_selection(&mut state, first);
        assert!(
            state
                .composition
                .selection
                .as_ref()
                .unwrap()
                .current(&state)
        );

        let other = root.path().join("other.djot");
        std::fs::write(&other, "bank").unwrap();
        assert!(state.open_path(other));
        let second = state.focused_key().unwrap();
        assert_ne!(first, second);
        // The original document is unchanged: cross-document staleness is a
        // distinct condition from source-text or selection staleness.
        assert!(
            state
                .composition
                .selection
                .as_ref()
                .unwrap()
                .current(&state)
        );
        assert_ne!(state.composition.selection.as_ref().unwrap().key, second);
        let mut host = harness(state);
        assert!(rendered_text(&host).contains("Selection changed"));
        host.update(|state| select(state, 0, 4));
        assert!(
            host.click_on(&Selector::role("button").containing("Read selected word or phrase"))
        );
        host.after_dispatch();
        assert_eq!(
            host.state().composition.selection.as_ref().unwrap().key,
            second
        );
    }

    #[test]
    fn scratch_anchor_return_is_refused_without_selecting_another_document() {
        let root = tempfile::tempdir().unwrap();
        let mut state = state("cat bat");
        state.composition = CompositionState::open(Some(root.path().into()));
        attach_fake(&mut state);
        select(&mut state, 0, 3);
        let key = state.focused_key().unwrap();
        collect_selection(&mut state, key);
        settle_collection(&mut state);
        let before = state.document().snapshot();
        state.composition.section = 2;
        let mut host = harness(state);
        assert!(host.click_on(&Selector::role("button").containing("Return to source")));
        host.after_dispatch();
        assert_eq!(
            host.state().document().snapshot().selection,
            before.selection
        );
        assert!(
            host.state()
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("no durable source identity")
        );
    }

    #[test]
    fn registry_install_is_disabled_then_explicit_query_works_and_reopens() {
        let root = tempfile::tempdir().unwrap();
        let mut state = state("bank");
        state.composition = CompositionState::open(Some(root.path().into()));
        let (bytes, digest) = pack();
        state
            .composition
            .registry
            .as_mut()
            .unwrap()
            .import_json(&bytes, Some(&digest))
            .unwrap();
        state.composition.query = TextInput::new("bank");
        lookup(&mut state);
        assert!(state.composition.entries.is_empty());
        state
            .composition
            .registry
            .as_mut()
            .unwrap()
            .set_enabled("fixture", "1", true)
            .unwrap();
        lookup(&mut state);
        assert_eq!(state.composition.entries[0].senses[0].id, "shore");
        drop(state);
        let reopened = CompositionState::open(Some(root.path().into()));
        assert!(
            reopened
                .registry
                .unwrap()
                .list()
                .iter()
                .any(|source| source.manifest.id == "fixture" && source.enabled)
        );
    }

    #[test]
    fn selected_material_reloads_from_port_without_mutating_source() {
        let root = tempfile::tempdir().unwrap();
        let mut state = state("a café by the river");
        state.composition = CompositionState::open(Some(root.path().into()));
        let port = attach_fake(&mut state);
        select(&mut state, 2, 7);
        let before = state.document().snapshot();
        let key = state.focused_key().unwrap();
        collect_selection(&mut state, key);
        settle_collection(&mut state);
        assert_eq!(state.document().snapshot().text, before.text);
        assert_eq!(state.document().snapshot().selection, before.selection);
        state.composition.retained.clear();
        state.composition.request(Vec::new(), false);
        settle_collection(&mut state);
        let retained = port.items.lock().unwrap();
        assert_eq!(retained[0].item.text, "café");
        assert!(
            retained[0]
                .item
                .document_source
                .as_ref()
                .unwrap()
                .validate_source(&before.text)
                .is_ok()
        );
    }

    #[test]
    fn sound_requires_source_opt_in_and_layers_are_independent() {
        let mut state = state("cat bat");
        select(&mut state, 0, 7);
        let key = state.focused_key().unwrap();
        analyze_sound(&mut state, key, false);
        assert!(state.composition.sound.is_none());
        state.composition.cmudict_enabled = true;
        analyze_sound(&mut state, key, false);
        assert!(
            state
                .composition
                .sound
                .as_ref()
                .unwrap()
                .relations
                .is_empty()
        );
        state.composition.layers.perfect_rhyme = true;
        analyze_sound(&mut state, key, false);
        assert!(
            !state
                .composition
                .sound
                .as_ref()
                .unwrap()
                .relations
                .is_empty()
        );
        assert!(state.composition.sound.as_ref().unwrap().meter.is_none());
        select(&mut state, 0, 3);
        assert!(
            !state
                .composition
                .sound_selection
                .as_ref()
                .unwrap()
                .current(&state)
        );
    }

    #[test]
    fn malformed_collection_is_not_reset_or_overwritten() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("collection.json"), b"broken").unwrap();
        let mut state = state("cat");
        state.composition = CompositionState::open(Some(root.path().into()));
        assert!(state.composition.retained.is_empty());
        select(&mut state, 0, 3);
        let key = state.focused_key().unwrap();
        collect_selection(&mut state, key);
        assert_eq!(
            std::fs::read(root.path().join("collection.json")).unwrap(),
            b"broken"
        );
    }

    #[test]
    fn configured_storage_is_lazy_and_first_collection_preserves_reading_state() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("composition");
        let mut state = state("night light");
        state.composition = CompositionState::configured(Some(path.clone()));
        state.composition.cmudict_enabled = true;
        state.composition.layers.perfect_rhyme = true;
        select(&mut state, 0, 11);
        let key = state.focused_key().unwrap();
        analyze_sound(&mut state, key, false);
        assert!(!path.exists());
        let reading = state.composition.sound.as_ref().unwrap();
        let note = sound_note(reading, &state.composition.layers);
        assert!(note.contains("Perfect rhyme: night ↔ light"));
        assert!(!note.contains("SoundReading {"));
        collect_selection(&mut state, key);
        assert!(!path.join("collection.json").exists());
        assert!(state.composition.cmudict_enabled);
        assert!(state.composition.sound.is_some());
        assert!(state.composition.retained.is_empty());
        assert!(
            state
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("No collection destination selected")
        );
    }

    #[test]
    fn supplied_authority_still_requires_explicit_destination_selection() {
        let mut state = state("cat");
        let port = attach_fake(&mut state);
        state.composition.selected_target = None;
        select(&mut state, 0, 3);
        let key = state.focused_key().unwrap();
        collect_selection(&mut state, key);
        assert!(!state.composition.busy());
        assert!(port.items.lock().unwrap().is_empty());
    }

    #[test]
    fn legacy_import_is_explicit_read_only_and_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("collection.json");
        let mut legacy = CollectionStore::open(&path).unwrap();
        legacy
            .collect(CollectionItem::new(
                ItemKind::Note,
                "Legacy",
                "kept quotation",
            ))
            .unwrap();
        drop(legacy);
        let before = std::fs::read(&path).unwrap();
        let mut state = state("cat");
        state.composition = CompositionState::configured(Some(root.path().into()));
        let port = attach_fake(&mut state);
        assert!(port.items.lock().unwrap().is_empty());
        state.composition.request(Vec::new(), true);
        settle_collection(&mut state);
        assert_eq!(state.composition.retained.len(), 1);
        state.composition.request(Vec::new(), true);
        settle_collection(&mut state);
        assert_eq!(state.composition.retained.len(), 1);
        assert_eq!(port.items.lock().unwrap().len(), 1);
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn malformed_legacy_import_does_not_write_to_authority_or_reset_source() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("collection.json");
        std::fs::write(&path, b"broken").unwrap();
        let mut state = state("cat");
        state.composition = CompositionState::configured(Some(root.path().into()));
        let port = attach_fake(&mut state);
        state.composition.request(Vec::new(), true);
        settle_collection(&mut state);
        assert!(port.items.lock().unwrap().is_empty());
        assert!(
            state
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("could not be confirmed")
        );
        assert_eq!(std::fs::read(path).unwrap(), b"broken");
    }

    #[test]
    fn stale_worker_generation_cannot_replace_current_collection_or_notice() {
        let mut state = state("cat");
        let port = attach_fake(&mut state);
        let item = CollectionItem::new(ItemKind::Note, "Old destination", "old");
        let receipt = port.retain(port.target(), item.clone()).unwrap();
        let (sender, receiver) = mpsc::channel();
        state.composition.receiver = Some(receiver);
        state.composition.generation = 2;
        state.composition.notice = Some("New destination".into());
        sender
            .send(CollectionUpdate {
                generation: 1,
                result: Ok(vec![RetainedCompositionItem::from_retention(
                    item,
                    port.target.writer,
                    receipt,
                )]),
            })
            .unwrap();
        state.composition.drain();
        assert!(state.composition.retained.is_empty());
        assert_eq!(state.composition.notice.as_deref(), Some("New destination"));
    }

    #[test]
    fn refresh_preserves_unconfirmed_request_identity_until_same_id_is_confirmed() {
        let mut state = state("cat");
        let port = attach_fake(&mut state);
        let item = CollectionItem::new(ItemKind::Note, "Pending", "quotation");
        state.composition.retry_items = vec![item.clone()];
        state.composition.retry_target = Some(port.target.clone());
        state.composition.request(Vec::new(), false);
        settle_collection(&mut state);
        assert_eq!(state.composition.retry_items[0].id, item.id);
        state.composition.request(
            vec![CollectionItem::new(ItemKind::Note, "New", "new")],
            false,
        );
        assert!(!state.composition.busy());
        assert!(
            state
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("still unconfirmed")
        );
        state.composition.request(vec![item.clone()], false);
        settle_collection(&mut state);
        assert!(state.composition.retry_items.is_empty());
        assert_eq!(state.composition.retained[0].item.id, item.id);
        assert_eq!(port.items.lock().unwrap().len(), 1);
    }

    #[test]
    fn same_id_from_another_writer_or_payload_does_not_confirm_pending_request() {
        let mut state = state("cat");
        let port = attach_fake(&mut state);
        let item = CollectionItem::new(ItemKind::Note, "Pending", "original");
        let receipt = port.retain(port.target(), item.clone()).unwrap();
        state.composition.retry_items = vec![item.clone()];
        state.composition.retry_target = Some(port.target.clone());
        for (author, text) in [([9; 32], "original"), (port.target.writer, "different")] {
            let mut returned = item.clone();
            returned.text = text.into();
            let (sender, receiver) = mpsc::channel();
            state.composition.receiver = Some(receiver);
            sender
                .send(CollectionUpdate {
                    generation: state.composition.generation,
                    result: Ok(vec![RetainedCompositionItem::from_retention(
                        returned,
                        author,
                        receipt.clone(),
                    )]),
                })
                .unwrap();
            state.composition.drain();
            assert_eq!(state.composition.retry_items, vec![item.clone()]);
        }
    }

    #[test]
    fn revoked_refresh_hides_cached_items_and_preserves_unconfirmed_retry() {
        let mut state = state("cat");
        let port = attach_fake(&mut state);
        let item = CollectionItem::new(ItemKind::Note, "Retained", "private quotation");
        state.composition.request(vec![item], false);
        settle_collection(&mut state);
        assert_eq!(state.composition.retained.len(), 1);
        let pending = CollectionItem::new(ItemKind::Note, "Unconfirmed", "pending");
        state.composition.retry_items = vec![pending.clone()];
        state.composition.retry_target = Some(port.target.clone());
        port.revoked
            .store(true, std::sync::atomic::Ordering::SeqCst);
        state.composition.request(Vec::new(), false);
        assert!(state.composition.retained.is_empty());
        settle_collection(&mut state);
        assert!(state.composition.retained.is_empty());
        assert_eq!(state.composition.retry_items, vec![pending]);
        assert_eq!(state.composition.retry_target, Some(port.target.clone()));
        assert!(
            state
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("authority revoked")
        );
    }

    #[test]
    fn recipe_editor_uses_shared_edits_retains_typed_material_and_reopens_selection() {
        let mut state = state("night night light");
        attach_fake(&mut state);
        select(&mut state, 0, 17);
        let key = state.focused_key().unwrap();
        state.composition.cmudict_enabled = true;
        state.composition.layers.perfect_rhyme = true;
        analyze_sound(&mut state, key, false);
        recipe::from_sound(&mut state, key, false);
        assert!(
            state.composition.recipe.material.is_some(),
            "{:?}",
            state.composition.notice
        );
        let mut host = recipe_harness(state);
        frame(&mut host);
        assert!(host.click_on(&Selector::role("button").containing("Increase recipe spacing")));
        host.after_dispatch();
        assert!(rendered_text(&host).contains("spacing 24"));
        assert!(host.click_on(&Selector::role("button").containing("night · token-6-11")));
        host.after_dispatch();
        assert!(rendered_text(&host).contains("bytes 6–11"));
        for label in [
            "Show relationship scene",
            "Hide scene background",
            "Zoom scene in",
            "Show scene camera controls",
            "Pan scene right",
        ] {
            assert!(
                host.click_on(&Selector::role("button").containing(label)),
                "{label}"
            );
            host.after_dispatch();
        }
        assert!(host.click_on(
            &Selector::role("button").containing("Relationship category: sound.perfect_rhyme")
        ));
        host.after_dispatch();
        assert!(host.click_on(&Selector::role("button").containing("Retain relationship recipe")));
        host.after_dispatch();
        host.update(settle_collection);
        let saved = host.state().composition.retained[0]
            .item
            .projection_recipe
            .clone()
            .unwrap();
        assert_eq!(
            saved.snapshot.selected_occurrence.as_deref(),
            Some("token-6-11")
        );
        assert_eq!(saved.snapshot.recipe.definition.arrangement.spacing, 24);
        let presentation = saved.presentation.as_ref().unwrap();
        assert!(presentation.overview_visible);
        assert!(!presentation.background_visible);
        assert_eq!(
            presentation.foreground_occurrences,
            ["token-6-11".to_owned()].into_iter().collect()
        );
        assert!(presentation.zoom > 1.0);
        assert!(presentation.pan_x > 0.0);
        assert_eq!(
            saved.snapshot.recipe.relationship_kind.as_deref(),
            Some("sound.perfect_rhyme")
        );
        host.update(|state| {
            state.composition.recipe.material = None;
            state.composition.section = 2;
        });
        assert!(
            host.click_on(
                &Selector::role("button").containing("Open retained relationship recipe")
            )
        );
        host.after_dispatch();
        assert_eq!(
            host.state().composition.recipe.material.as_ref().unwrap(),
            &saved
        );
        assert!(rendered_text(&host).contains("bytes 6–11"));
    }

    #[test]
    fn background_category_lenses_preserve_disclosure_and_foreground() {
        let mut state = recipe_state("night night light", 0, 17);
        let key = state.focused_key().unwrap();
        state.composition.layers.alliteration = true;
        analyze_sound(&mut state, key, false);
        recipe::from_sound(&mut state, key, false);
        let mut host = recipe_harness(state);
        frame(&mut host);
        let click = |host: &mut RecipeHost, label: &str| {
            assert!(
                host.click_on(&Selector::role("button").containing(label)),
                "{label}"
            );
            host.after_dispatch();
        };
        let nodes = |host: &RecipeHost| {
            host.with_dom(|dom| {
                dom.all_with_class(dom.document(), "graph-canvas-swatch-node")
                    .len()
            })
        };
        click(&mut host, "Show relationship scene");
        click(&mut host, "Show scene category controls");
        let original = host.state().composition.recipe.material.clone().unwrap();
        let source = host.state().document().snapshot();
        assert_eq!(nodes(&host), 3);
        click(
            &mut host,
            "Hide scene background category: sound.perfect_rhyme",
        );
        assert_eq!(
            nodes(&host),
            2,
            "the alliteration background stays disclosed"
        );
        let filtered = host.state().composition.recipe.material.as_ref().unwrap();
        assert_eq!(filtered.snapshot, original.snapshot);
        assert_eq!(filtered.dataset, original.dataset);
        assert_eq!(filtered.anchors, original.anchors);
        assert_eq!(filtered.presentation.as_ref().unwrap().version, 2);
        click(
            &mut host,
            "Hide scene background category: sound.alliteration",
        );
        assert_eq!(
            nodes(&host),
            1,
            "foreground stays visible with all categories hidden"
        );
        click(&mut host, "Show all scene background categories");
        assert_eq!(nodes(&host), 3);
        assert_eq!(
            host.state()
                .composition
                .recipe
                .material
                .as_ref()
                .unwrap()
                .presentation
                .as_ref()
                .unwrap()
                .version,
            1
        );
        assert_eq!(host.state().document().snapshot(), source);
        click(
            &mut host,
            "Hide scene background category: sound.alliteration",
        );
        let retained_filters = host.state().composition.recipe.material.clone();
        host.update(|state| {
            state.composition.layers.alliteration = false;
            analyze_sound(state, key, false);
            recipe::from_sound(state, key, true);
        });
        assert_eq!(host.state().composition.recipe.material, retained_filters);
        assert!(
            host.state()
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("Reset the scene presentation")
        );
        click(&mut host, "Reset scene presentation");
        host.update(|state| recipe::from_sound(state, key, true));
        assert!(
            host.state()
                .composition
                .recipe
                .material
                .as_ref()
                .unwrap()
                .presentation
                .is_none()
        );
    }

    #[test]
    fn scene_camera_disclosure_is_session_only_and_removes_pan_targets() {
        let mut host = recipe_harness(recipe_state("night night light", 0, 17));
        frame(&mut host);
        assert!(host.click_on(&Selector::role("button").containing("Show relationship scene")));
        host.after_dispatch();
        assert!(host.click_on(&Selector::role("button").containing("night · token-0-5")));
        host.after_dispatch();
        let material = host.state().composition.recipe.material.clone();
        let notice = host.state().composition.notice.clone();
        let has_pan = |host: &RecipeHost| {
            host.with_dom(|dom| {
                !taproot::matching(dom, &Selector::role("button").containing("Pan scene right"))
                    .is_empty()
            })
        };
        assert!(!host.state().composition.recipe.camera_controls_expanded);
        assert!(!has_pan(&host));
        for (label, expanded) in [
            ("Show scene camera controls", true),
            ("Hide scene camera controls", false),
        ] {
            assert!(host.click_on(&Selector::role("button").containing(label)));
            host.after_dispatch();
            assert_eq!(
                host.state().composition.recipe.camera_controls_expanded,
                expanded
            );
            assert_eq!(has_pan(&host), expanded);
            assert_eq!(host.state().composition.recipe.material, material);
            assert_eq!(host.state().composition.notice, notice);
            for primary in ["Zoom scene in", "Zoom scene out", "Fit relationship scene"] {
                assert!(host.with_dom(|dom| {
                    !taproot::matching(dom, &Selector::role("button").containing(primary))
                        .is_empty()
                }));
            }
        }
        assert!(rendered_text(&host).contains("Selected occurrence role: foreground"));
        assert!(rendered_text(&host).contains("Background · contextual occurrence"));
        for (label, expanded) in [
            ("Show scene category controls", true),
            ("Hide scene category controls", false),
        ] {
            assert!(host.click_on(&Selector::role("button").containing(label)));
            host.after_dispatch();
            assert_eq!(
                host.state().composition.recipe.background_controls_expanded,
                expanded
            );
            assert_eq!(host.state().composition.recipe.material, material);
            assert_eq!(host.state().composition.notice, notice);
            assert_eq!(
                host.with_dom(|dom| !taproot::matching(
                    dom,
                    &Selector::role("button")
                        .containing("Hide scene background category: sound.perfect_rhyme")
                )
                .is_empty()),
                expanded
            );
        }
    }

    #[test]
    fn scene_rebind_preserves_roles_or_refuses_until_explicit_reset() {
        let mut state = state("night light cat bat");
        select(&mut state, 0, 11);
        let key = state.focused_key().unwrap();
        state.composition.cmudict_enabled = true;
        state.composition.layers.perfect_rhyme = true;
        analyze_sound(&mut state, key, false);
        recipe::from_sound(&mut state, key, false);
        let mut host = recipe_harness(state);
        frame(&mut host);
        for label in ["Show relationship scene", "Zoom scene in"] {
            assert!(host.click_on(&Selector::role("button").containing(label)));
            host.after_dispatch();
        }
        let presentation = host
            .state()
            .composition
            .recipe
            .material
            .as_ref()
            .unwrap()
            .presentation
            .clone();
        host.update(|state| recipe::from_sound(state, key, true));
        assert_eq!(
            host.state()
                .composition
                .recipe
                .material
                .as_ref()
                .unwrap()
                .presentation,
            presentation
        );
        let original = host.state().composition.recipe.material.clone();
        host.update(|state| {
            select(state, 12, 19);
            analyze_sound(state, key, false);
            recipe::from_sound(state, key, true);
        });
        assert_eq!(host.state().composition.recipe.material, original);
        assert!(
            host.state()
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("Reset the scene presentation")
        );
        assert!(host.click_on(&Selector::role("button").containing("Reset scene presentation")));
        host.after_dispatch();
        host.update(|state| recipe::from_sound(state, key, true));
        let rebound = host.state().composition.recipe.material.as_ref().unwrap();
        assert!(rebound.presentation.is_none());
        assert_eq!(rebound.anchors[0].source.exact_quote, "cat");
    }

    #[test]
    fn explicit_recipe_rebind_preserves_arrangement_and_refuses_stale_reading() {
        let mut state = state("night light cat bat");
        select(&mut state, 0, 11);
        let key = state.focused_key().unwrap();
        state.composition.cmudict_enabled = true;
        state.composition.layers.perfect_rhyme = true;
        analyze_sound(&mut state, key, false);
        recipe::from_sound(&mut state, key, false);
        let original = state.composition.recipe.material.clone().unwrap();
        select(&mut state, 12, 19);
        recipe::from_sound(&mut state, key, true);
        assert_eq!(
            state.composition.recipe.material.as_ref().unwrap(),
            &original
        );
        assert!(
            state
                .composition
                .notice
                .as_ref()
                .unwrap()
                .contains("selection changed")
        );
        analyze_sound(&mut state, key, false);
        recipe::from_sound(&mut state, key, true);
        let rebound = state.composition.recipe.material.as_ref().unwrap();
        assert_ne!(
            rebound.dataset.dataset.source.resource,
            original.dataset.dataset.source.resource
        );
        assert_eq!(
            rebound.snapshot.recipe.definition.arrangement,
            original.snapshot.recipe.definition.arrangement
        );
        assert_eq!(rebound.anchors[0].source.exact_quote, "cat");
    }
}
