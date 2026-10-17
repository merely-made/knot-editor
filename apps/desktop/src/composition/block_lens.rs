// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Composition lenses over authored document blocks. All committed edits enter
//! the ordinary document source/history/save path; UI drafts are session-local.
mod gestures;
use super::metre::Surface;
use crate::{
    documents::DocKey,
    workspace::{DesktopState, DesktopView},
};
use cambium::{TextInput, button, custom_leaf, el, lens, span, text_field_typed};
use cambium_genet_winit_host::{AppCtx, SceneProducer};
use cambium_rootstock::{ProducerRole, ProducerSemantics};
use knot_readings::metre_block;
use nematic::knot::djot::divs::Div;
use pictograph::canvas::backdrop_surface::BackdropSurface;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub const KEY: u64 = 0x4b4e_424c;
#[derive(Default)]
pub struct State {
    edit: Option<(DocKey, String, String)>,
    edit_attributes: BTreeMap<String, String>,
    units: TextInput,
    scan: TextInput,
    form: TextInput,
    feet: TextInput,
    positions: TextInput,
    unit: TextInput,
    expected: TextInput,
    rhyme: TextInput,
    overview: Option<(DocKey, String)>,
    drag: Option<gestures::Drag>,
    surface: Rc<RefCell<Surface>>,
}
fn find<'a>(divs: &'a [Div], id: &str) -> Option<&'a Div> {
    fn collect<'a>(divs: &'a [Div], id: &str, found: &mut Vec<&'a Div>) {
        for div in divs {
            if div.attributes.get("id").is_some_and(|value| value == id) {
                found.push(div);
            }
            collect(&div.children, id, found);
        }
    }
    let mut found = Vec::new();
    collect(divs, id, &mut found);
    (found.len() == 1).then(|| found[0])
}
fn metre_divs<'a>(divs: &'a [Div], out: &mut Vec<&'a Div>) {
    for div in divs {
        if div
            .attributes
            .get("class")
            .is_some_and(|s| s.split_whitespace().any(|s| s == "metre"))
        {
            out.push(div);
        }
        metre_divs(&div.children, out);
    }
}
fn report(state: &mut DesktopState, result: Result<(), String>) {
    state.message = result.err();
}

pub fn apply_selection(state: &mut DesktopState, key: DocKey) {
    let result = (|| {
        let session = state.surface_for(key).session();
        let snapshot = session.div_snapshot()?;
        let selected = session.snapshot().selection;
        let a = selected.anchor.byte.min(selected.focus.byte);
        let b = selected.anchor.byte.max(selected.focus.byte);
        let start = snapshot.source_text[..a].rfind('\n').map_or(0, |i| i + 1);
        let end = if b > start && snapshot.source_text.as_bytes().get(b - 1) == Some(&b'\n') {
            b
        } else {
            snapshot.source_text[b..]
                .find('\n')
                .map_or(snapshot.source_text.len(), |i| b + i + 1)
        };
        if snapshot.source_text[start..end].trim().is_empty() {
            return Err("Select composition lines first".into());
        }
        let attrs = BTreeMap::from([
            ("class".into(), "metre".into()),
            ("id".into(), format!("verse-{}", uuid::Uuid::new_v4())),
        ]);
        state
            .surface_mut_for(key)
            .session_mut()
            .wrap_source_div(&snapshot, start..end, attrs)
    })();
    report(state, result);
}
fn begin(state: &mut DesktopState, key: DocKey, id: &str, text: &str) {
    let result = (|| {
        let snapshot = state.surface_for(key).session().div_snapshot()?;
        let div = find(&snapshot.divs, id).ok_or("Block is no longer available")?;
        let authored = metre_block::authorship(&div.attributes)?;
        let current: Vec<_> = snapshot.source_text[div.body.clone()]
            .lines()
            .filter(|s| !s.trim().is_empty())
            .collect();
        let line = metre_block::matched(&authored, &current, text);
        let units = line
            .map(|l| l.units.join(" | "))
            .unwrap_or_else(|| text.to_string());
        let scan = line
            .map(|l| {
                l.stress
                    .iter()
                    .map(|s| match s {
                        Some(true) => "/",
                        Some(false) => "u",
                        None => "?",
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| "?".into());
        if current.iter().filter(|line| **line == text).count() == 1 {
            if let Some(offset) = snapshot.source_text[div.body.clone()].find(text) {
                let start = div.body.start + offset;
                state
                    .surface_mut_for(key)
                    .session_mut()
                    .select_source_span(
                        &snapshot.address,
                        &snapshot.source_text,
                        start,
                        start + text.len(),
                    )?;
            }
        }
        let ui = &mut state.composition.block_lens;
        ui.edit = Some((key, id.into(), text.into()));
        ui.edit_attributes = div.attributes.clone();
        ui.units = TextInput::new(units);
        ui.scan = TextInput::new(scan);
        ui.form = TextInput::new(div.attributes.get("form").cloned().unwrap_or_default());
        ui.unit = TextInput::new(div.attributes.get("unit").cloned().unwrap_or_default());
        ui.expected = TextInput::new(
            div.attributes
                .get("expected-stress")
                .cloned()
                .unwrap_or_default(),
        );
        ui.rhyme = TextInput::new(div.attributes.get("rhyme").cloned().unwrap_or_default());
        ui.feet = TextInput::new(div.attributes.get("feet").cloned().unwrap_or_default());
        ui.positions = TextInput::new(
            div.attributes
                .get("positions-per-foot")
                .cloned()
                .unwrap_or_default(),
        );
        Ok(())
    })();
    report(state, result);
}
fn commit(state: &mut DesktopState) {
    let result = (|| {
        let ui = &state.composition.block_lens;
        let (key, id, text) = ui.edit.clone().ok_or("Choose a composition line")?;
        let units = ui.units.text().to_string();
        let scan = ui.scan.text().to_string();
        let form = ui.form.text().to_string();
        let feet = ui.feet.text().to_string();
        let positions = ui.positions.text().to_string();
        let unit = ui.unit.text().to_string();
        let expected = ui.expected.text().to_string();
        let rhyme = ui.rhyme.text().to_string();
        let expected_attributes = ui.edit_attributes.clone();
        if expected
            .chars()
            .any(|c| !c.is_whitespace() && !"uU/?".contains(c))
        {
            return Err("Expected stress uses u, / or ?".into());
        }
        let snapshot = state.surface_for(key).session().div_snapshot()?;
        let div = find(&snapshot.divs, &id).ok_or("Block is no longer available")?;
        if div.attributes != expected_attributes {
            return Err(
                "This block's declarations changed; review them before applying scansion".into(),
            );
        }
        let body = &snapshot.source_text[div.body.clone()];
        if body.lines().filter(|line| *line == text).count() != 1 {
            return Err(
                "This line changed or became ambiguous; review it before applying scansion".into(),
            );
        }
        let mut attrs = div.attributes.clone();
        metre_block::set_line(&mut attrs, &text, &units, &scan)?;
        for (name, value) in [
            ("form", form),
            ("feet", feet),
            ("positions-per-foot", positions),
            ("unit", unit),
            ("expected-stress", expected),
            ("rhyme", rhyme),
        ] {
            if value.trim().is_empty() {
                attrs.remove(name);
            } else {
                attrs.insert(name.into(), value);
            }
        }
        metre_block::scene(&snapshot.address, &id, body, &attrs)?;
        state
            .surface_mut_for(key)
            .session_mut()
            .set_div_attributes(&snapshot, div, attrs)?;
        state.composition.block_lens.edit = None;
        Ok(())
    })();
    report(state, result);
}
fn form_template(state: &mut DesktopState, key: DocKey, id: &str, feet: u16) {
    let result = (|| {
        let snapshot = state.surface_for(key).session().div_snapshot()?;
        let div = find(&snapshot.divs, id).ok_or("Block is no longer available")?;
        let mut attrs = div.attributes.clone();
        attrs.insert(
            "form".into(),
            if feet == 5 {
                "Iambic pentameter"
            } else {
                "Iambic tetrameter"
            }
            .into(),
        );
        attrs.insert("unit".into(), "iamb".into());
        attrs.insert("feet".into(), feet.to_string());
        attrs.insert("positions-per-foot".into(), "2".into());
        attrs.insert("expected-stress".into(), "u/".into());
        state
            .surface_mut_for(key)
            .session_mut()
            .set_div_attributes(&snapshot, div, attrs)
    })();
    report(state, result);
}

fn field(label: &str, name: &'static str) -> DesktopView {
    let accessible = label.to_string();
    Box::new(
        el(
            "label",
            (
                span(label.to_string()),
                lens(
                    move |input: &mut TextInput| {
                        text_field_typed(input)
                            .attr("id", format!("knot-block-lens-{name}"))
                            .attr("aria-label", accessible.clone())
                    },
                    move |state: &mut DesktopState| {
                        let ui = &mut state.composition.block_lens;
                        match name {
                            "units" => &mut ui.units,
                            "scan" => &mut ui.scan,
                            "form" => &mut ui.form,
                            "feet" => &mut ui.feet,
                            "unit" => &mut ui.unit,
                            "expected" => &mut ui.expected,
                            "rhyme" => &mut ui.rhyme,
                            _ => &mut ui.positions,
                        }
                    },
                ),
            ),
        )
        .attr(
            "style",
            "display:flex;flex-direction:column;gap:4px;min-width:0",
        ),
    )
}

pub fn view(state: &DesktopState, key: DocKey) -> DesktopView {
    let Ok(snapshot) = state.surface_for(key).session().div_snapshot() else {
        return Box::new(el("div", ()));
    };
    let mut rows: Vec<DesktopView> = vec![Box::new(button(
        "Apply metre lens to selected lines",
        move |s: &mut DesktopState, _| apply_selection(s, key),
    ))];
    let mut divs = Vec::new();
    metre_divs(&snapshot.divs, &mut divs);
    for div in divs {
        let id = div.attributes.get("id").cloned().unwrap_or_default();
        if id.is_empty() {
            rows.push(Box::new(span(
                "This metre block needs an id before its declarations can be edited.",
            )));
            continue;
        }
        let pool_id = id.clone();
        rows.push(Box::new(button("Add block to Pool", move |s: &mut DesktopState, _| crate::pool::add_block(s,key,&pool_id))));
        let body = &snapshot.source_text[div.body.clone()];
        let title = div
            .attributes
            .get("form")
            .cloned()
            .unwrap_or_else(|| "Free composition".into());
        rows.push(Box::new(el("h3", title)));
        let mut templates: Vec<DesktopView> = Vec::new();
        for (feet, label) in [
            (5, "Apply iambic pentameter template"),
            (4, "Apply iambic tetrameter template"),
        ] {
            let template_id = id.clone();
            templates.push(Box::new(button(label, move |s: &mut DesktopState, _| {
                form_template(s, key, &template_id, feet)
            })));
        }
        rows.push(Box::new(el("div", templates).attr(
            "style",
            "display:flex;flex-direction:row;flex-wrap:wrap;gap:6px",
        )));

        let authored = metre_block::authorship(&div.attributes).unwrap_or_default();
        let current: Vec<_> = body.lines().filter(|s| !s.trim().is_empty()).collect();
        for text in &current {
            let mut line: Vec<DesktopView> = Vec::new();
            if let Some(authored) = metre_block::matched(&authored, &current, text) {
                for (unit, stress) in authored.units.iter().zip(&authored.stress) {
                    let label = match stress {
                        Some(true) => "/",
                        Some(false) => "u",
                        None => "?",
                    };
                    let height = match stress {
                        Some(true) => 24,
                        Some(false) => 7,
                        None => 12,
                    };
                    let mark = if stress.is_some() {
                        el("span", "").attr("style", format!("display:block;width:2px;height:{height}px;background:currentColor;flex:none"))
                    } else {
                        el("span", label).attr(
                            "style",
                            "display:block;height:12px;font-size:12px;line-height:12px;flex:none",
                        )
                    };
                    line.push(Box::new(el("span", (mark, span(unit.clone())))
                        .attr("style", "display:flex;flex-direction:column;align-items:center;gap:4px;flex:none")
                        .attr("aria-label", format!("{unit}: {}", match stress {Some(true)=>"strong",Some(false)=>"weak",None=>"stress unknown"}))));
                }
            } else {
                line.push(Box::new(span((*text).to_string())));
                line.push(Box::new(span(" · scansion unresolved")));
            }
            let click_id = id.clone();
            let click_text = (*text).to_string();
            let accessible = format!("Edit scansion: {text}");
            line.push(Box::new(
                button("Edit scansion", move |s: &mut DesktopState, _| {
                    begin(s, key, &click_id, &click_text)
                })
                .attr("aria-label", accessible),
            ));
            rows.push(Box::new(
                el("div", line).attr("class", "knot-composition-line").attr(
                    "style",
                    "display:flex;flex-direction:row;flex-wrap:wrap;align-items:flex-end;gap:6px;white-space:pre-wrap;overflow-wrap:anywhere;margin-block:8px",
                ),
            ));
        }
        match metre_block::scene(&snapshot.address, &id, body, &div.attributes) {
            Ok((input, notes)) => {
                for note in notes {
                    rows.push(Box::new(span(note).attr("role", "status")));
                }
                let show = state
                    .composition
                    .block_lens
                    .overview
                    .as_ref()
                    .is_some_and(|(owner, block)| *owner == key && block == &id);
                let toggle_id = id.clone();
                rows.push(Box::new(button(
                    if show {
                        "Hide wider metre overview"
                    } else {
                        "Show wider metre overview"
                    },
                    move |s: &mut DesktopState, _| {
                        let ui = &mut s.composition.block_lens;
                        ui.drag = None;
                        ui.overview = if show {
                            None
                        } else {
                            Some((key, toggle_id.clone()))
                        };
                    },
                )));
                if show {
                    let input = gestures::preview(state, key, &id, &input);
                    let mut surface = state.composition.block_lens.surface.borrow_mut();
                    if surface.input.as_ref() != Some(&input) {
                        if let Some(s) = surface.surface.as_mut() {
                            if let Err(error) = s.replace(&input) {
                                surface.error = Some(error.to_string());
                            }
                        } else if let Ok(mut s) = BackdropSurface::new(&input) {
                            s.register_font(
                                include_bytes!(
                                    "../../assets/fonts/ibm-plex-mono/IBMPlexMono-Regular.ttf"
                                )
                                .to_vec(),
                            );
                            s.show_span_handles = true;
                            surface.surface = Some(s);
                        }
                        surface.input = Some(input);
                        surface.revision = surface.revision.wrapping_add(1);
                    }
                    if let Some(error) = &surface.error {
                        rows.push(Box::new(span(error.clone()).attr("role", "status")));
                    }
                    rows.push(Box::new(
                        el(
                            "div",
                            {
                                let pointer_id = id.clone(); let keyboard_id = id.clone();
                                cambium::on_key(cambium::on_pointer(custom_leaf::<DesktopState, ()>(KEY, 600, 320)
                                    .attr("class", "knot-block-metre-overview")
                                    .attr("style", "display:block;flex:none;width:600px;height:320px;max-width:none")
                                    .attr("aria-label", "Wider metre overview. Drag units or edges; arrows move, Shift changes end, Alt changes start, Escape cancels."),
                                    move |s, e| gestures::pointer(s, key, &pointer_id, e)),
                                    move |s, e| gestures::keypress(s, key, &keyboard_id, e))
                            },
                        )
                        .attr("class", "knot-block-overview-scroll")
                        .attr("style", "display:block;overflow-x:auto;width:100%"),
                    ));
                }
            },
            Err(error) => rows.push(Box::new(span(error).attr("role", "status"))),
        }
    }
    if state
        .composition
        .block_lens
        .edit
        .as_ref()
        .is_some_and(|(owner, _, _)| *owner == key)
    {
        rows.extend([
            field("Authored syllables / units, separated by |", "units"),
            field("Scansion: / strong, u weak, ? unknown", "scan"),
            field("Poetic form (optional)", "form"),
            field("Metrical unit (optional)", "unit"),
            field("Expected stress pattern per unit (optional)", "expected"),
            field("Expected rhyme groups by line (optional)", "rhyme"),
            field("Feet (empty for free composition)", "feet"),
            field("Positions per foot (optional)", "positions"),
        ]);
        rows.push(Box::new(button(
            "Apply scansion and form",
            |s: &mut DesktopState, _| commit(s),
        )));
        rows.push(Box::new(button(
            "Cancel lens changes",
            |s: &mut DesktopState, _| s.composition.block_lens.edit = None,
        )));
    }
    Box::new(
        el("section", rows)
            .attr("class", "knot-composition-lens")
            .attr("aria-label", "Composition lenses")
            .attr(
                "style",
                "display:flex;flex-direction:column;gap:8px;width:100%;min-width:0;flex:none",
            ),
    )
}

type Ctx<'a> = AppCtx<'a, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;
#[derive(Default)]
pub struct Bindings {
    producer: Option<Rc<RefCell<SceneProducer<Surface>>>>,
}
impl Bindings {
    pub fn frame(&mut self, ctx: &mut Ctx<'_>) {
        let mounted = !taproot::matching(
            &*ctx.runner.dom().borrow(),
            &taproot::Selector::class("knot-block-metre-overview"),
        )
        .is_empty();
        if !mounted {
            if ctx.runner.state().composition.block_lens.drag.is_some() {
                ctx.runner.update(|s| s.composition.block_lens.drag = None);
            }
            ctx.producers.remove(KEY);
            return;
        }
        let source = ctx.runner.state().composition.block_lens.surface.clone();
        super::metre::apply_theme(ctx.runner.state(), &source);
        if source.borrow().surface.is_none() {
            return;
        }
        let producer = self.producer.get_or_insert_with(|| {
            Rc::new(RefCell::new(SceneProducer::new(
                source.clone(),
                KEY,
                |s, w, h| {
                    let theme = s.theme.clone();
                    s.rendered_size = Some((w, h));
                    match s.surface.as_mut().unwrap().paint(w, h, &theme) {
                        Ok(paint) => {
                            s.error = None;
                            paint
                        },
                        Err(error) => {
                            s.error = Some(error);
                            Default::default()
                        },
                    }
                },
                |s| s.revision,
                |_| {
                    Some(ProducerSemantics {
                        role: Some(ProducerRole::Image),
                        name: Some("Metre overview".into()),
                        children: vec![],
                    })
                },
            )))
        });
        producer.borrow_mut().set_source(source);
        if !ctx.producers.contains(KEY) {
            ctx.producers
                .register(KEY, producer.clone(), &[])
                .expect("block lens owns its leaf");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::TextCommand;
    use cambium_genet_winit_host::WindowCommands;
    use knot_document::{KnotDocumentIntentV1 as Intent, KnotDocumentSession};
    #[test]
    fn manual_lens_uses_document_history_and_save_without_dictionary_or_collection() {
        let source = "quux light\nthe night\n";
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("memory:manual", source),
            WindowCommands::new(),
        );
        let key = state.focused_key().unwrap();
        state
            .surface_mut_for(key)
            .session_mut()
            .select_source_span("memory:manual", source, 0, source.len())
            .unwrap();
        apply_selection(&mut state, key);
        assert!(state.message.is_none(), "{:?}", state.message);
        assert!(!state.composition.cmudict_enabled);
        assert!(state.composition.metre.draft.is_none());
        let wrapped = state.surface_for(key).session().div_snapshot().unwrap();
        let id = wrapped.divs[0].attributes["id"].clone();
        assert!(!wrapped.divs[0].attributes.contains_key("feet"));
        begin(&mut state, key, &id, "quux light");
        state.composition.block_lens.units = TextInput::new("quux | light");
        state.composition.block_lens.scan = TextInput::new("? /");
        commit(&mut state);
        assert!(state.message.is_none(), "{:?}", state.message);
        let authored = state.surface_for(key).snapshot().text;
        state
            .surface_mut_for(key)
            .apply(Intent::Edit(TextCommand::Undo))
            .unwrap();
        assert_eq!(state.surface_for(key).snapshot().text, wrapped.source_text);
        state
            .surface_mut_for(key)
            .apply(Intent::Edit(TextCommand::Redo))
            .unwrap();
        assert_eq!(state.surface_for(key).snapshot().text, authored);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manual.djot");
        state
            .surface_mut_for(key)
            .apply(Intent::SaveAs(path.clone()))
            .unwrap();
        let reopened = KnotDocumentSession::open(path).unwrap();
        assert_eq!(reopened.snapshot().text, authored);
        assert!(
            reopened.div_snapshot().unwrap().divs[0]
                .attributes
                .contains_key(metre_block::AUTHORSHIP)
        );
    }
    #[test]
    fn scansion_draft_cannot_overwrite_newer_block_declarations() {
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("memory:draft", "{#verse}\n::: metre\nquux\n:::\n"),
            WindowCommands::new(),
        );
        let key = state.focused_key().unwrap();
        begin(&mut state, key, "verse", "quux");
        let before = state.surface_for(key).session().div_snapshot().unwrap();
        let mut attrs = before.divs[0].attributes.clone();
        attrs.insert("form".into(), "newer manual form".into());
        state
            .surface_mut_for(key)
            .session_mut()
            .set_div_attributes(&before, &before.divs[0], attrs)
            .unwrap();
        let newer = state.surface_for(key).snapshot().text;
        commit(&mut state);
        assert!(
            state
                .message
                .as_deref()
                .is_some_and(|s| s.contains("declarations changed"))
        );
        assert_eq!(state.surface_for(key).snapshot().text, newer);
    }
    #[test]
    fn form_changes_keep_authorship_and_disclose_conflicts() {
        let mut attrs = BTreeMap::new();
        metre_block::set_line(&mut attrs, "the night", "the | night", "/ u").unwrap();
        let before = attrs[metre_block::AUTHORSHIP].clone();
        attrs.insert("feet".into(), "5".into());
        attrs.insert("expected-stress".into(), "u/".into());
        let (_, notes) = metre_block::scene("memory:test", "verse", "the night\n", &attrs).unwrap();
        assert_eq!(attrs[metre_block::AUTHORSHIP], before);
        assert!(notes.iter().any(|note| note.contains("differs")));
    }
}
