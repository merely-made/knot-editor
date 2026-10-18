// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Native owner for metrical alignment. The surface supplies geometry; Knot
//! guards the captured reading and retains only an explicitly collected copy.
use super::{SelectionSnapshot, collect};
use crate::{
    documents::DocKey,
    workspace::{DesktopState, DesktopView},
};
use cambium::{
    BackdropSceneEvent, backdrop_scene_reading, button, custom_leaf, el, on_key, on_pointer,
};
use cambium_genet_winit_host::{AppCtx, SceneProducer};
use cambium_rootstock::{ProducerRole, ProducerSemantics};
use knot_composition::{CollectionItem, DocumentAnchor, ItemKind};
use knot_readings::{
    metre_alignment::{Edit, MetreDraft, Proposal},
    metre_scene::{MetreTemplate, metre_scene},
};
use pictograph::canvas::backdrop_surface::{BackdropSurface, SurfaceTheme};
use scenomise::backdrops::{SceneFacts, SceneInput};
use std::{cell::RefCell, rc::Rc};
pub const KEY: u64 = 0x4b4e_4d45;
#[derive(Default)]
pub struct Surface {
    pub surface: Option<BackdropSurface>,
    pub input: Option<SceneInput>,
    pub revision: u64,
    pub theme: SurfaceTheme,
    pub rendered_size: Option<(u32, u32)>,
    pub error: Option<String>,
}
struct Drag {
    baseline: SceneInput,
    proposal: Proposal,
    edit: Edit,
    x: f32,
    ticks_per_pixel: f32,
    size: (f32, f32),
    pointer: i32,
    owner: DocKey,
}
pub struct MetreState {
    pub draft: Option<MetreDraft>,
    pub surfaces: Rc<RefCell<Surface>>,
    source: Option<SelectionSnapshot>,
    reading: Option<knot_readings::sound::SoundReading>,
    anchor: Option<DocumentAnchor>,
    drag: Option<Drag>,
    saved: Option<String>,
    pub feet: u16,
    pub positions: u16,
    pub paired_rhyme: bool,
    pub notice: Option<String>,
    pub close_pending: bool,
}
impl Default for MetreState {
    fn default() -> Self {
        Self {
            draft: None,
            surfaces: Rc::new(RefCell::new(Surface::default())),
            source: None,
            reading: None,
            anchor: None,
            drag: None,
            saved: None,
            feet: 5,
            positions: 2,
            paired_rhyme: false,
            notice: None,
            close_pending: false,
        }
    }
}
impl MetreState {
    pub fn dirty(&self) -> bool {
        self.draft
            .as_ref()
            .is_some_and(|d| self.saved.as_deref() != Some(d.scene.revision.as_str()))
    }
    pub fn mark_retained(&mut self, scene: &SceneInput) {
        if self.draft.as_ref().is_some_and(|d| &d.scene == scene) {
            self.saved = Some(scene.revision.clone());
        }
    }
    pub fn preview_active(&self) -> bool {
        self.drag.is_some()
    }
    fn current(&self, state: &DesktopState, key: DocKey) -> bool {
        state.focused_key() == Some(key)
            && self.source.as_ref().is_none_or(|s| {
                s.key == key
                    && s.current(state)
                    && state.composition.cmudict_enabled
                    && self.reading.as_ref() == state.composition.sound.as_ref()
            })
    }
    fn refresh(&mut self) -> Result<(), String> {
        let input = if let Some(d) = &self.draft {
            Some(if let Some(g) = &self.drag {
                d.preview(&g.proposal)?
            } else {
                d.scene.clone()
            })
        } else {
            None
        };
        let mut surface = self.surfaces.borrow_mut();
        let selection = surface
            .surface
            .as_ref()
            .and_then(|s| s.selected())
            .map(str::to_owned);
        match &input {
            Some(input) => {
                if let Some(s) = &mut surface.surface {
                    s.replace(input).map_err(|e| e.to_string())?;
                } else {
                    let mut s = BackdropSurface::new(input).map_err(|e| e.to_string())?;
                    s.register_font(
                        include_bytes!("../../assets/fonts/ibm-plex-mono/IBMPlexMono-Regular.ttf")
                            .to_vec(),
                    );
                    s.show_span_handles = true;
                    surface.surface = Some(s);
                }
                if let Some(key) = selection {
                    let _ = surface.surface.as_mut().unwrap().select(Some(&key));
                }
            },
            None => surface.surface = None,
        }
        surface.input = input;
        surface.revision = surface.revision.wrapping_add(1);
        Ok(())
    }
    pub(crate) fn cancel(&mut self) {
        self.drag = None;
        let _ = self.refresh();
    }
    fn selected(&self) -> Option<String> {
        self.surfaces
            .borrow()
            .surface
            .as_ref()
            .and_then(|s| s.selected())
            .map(str::to_owned)
    }
    fn commit(&mut self) {
        if let Some(g) = self.drag.take() {
            if let Some(d) = &mut self.draft {
                if d.scene == g.baseline {
                    if let Err(e) = d.commit(&g.proposal) {
                        self.notice = Some(e);
                    }
                }
            }
        }
        let _ = self.refresh();
    }
    pub fn open_copy(
        &mut self,
        scene: SceneInput,
        anchor: Option<DocumentAnchor>,
    ) -> Result<(), String> {
        if self.dirty() {
            return Err("Collect or discard the current metre draft before opening another".into());
        }
        self.draft = Some(MetreDraft::new(scene.clone())?);
        self.source = None;
        self.reading = None;
        self.anchor = anchor;
        self.saved = Some(scene.revision);
        self.drag = None;
        self.notice = None;
        self.refresh()
    }
}
pub fn start(state: &mut DesktopState, key: DocKey) {
    if state.composition.metre.dirty() {
        state.composition.metre.notice =
            Some("Collect or discard this metre draft before starting another".into());
        return;
    }
    let result = (|| {
        let selection = state
            .composition
            .sound_selection
            .clone()
            .ok_or("Analyze a selected passage in Sound first")?;
        if selection.key != key || !selection.current(state) || !state.composition.cmudict_enabled {
            return Err("The source reading changed; analyze it again".into());
        }
        let reading = state
            .composition
            .sound
            .as_ref()
            .ok_or("Analyze sound first")?;
        let m = &state.composition.metre;
        let mut scene = metre_scene(
            &selection.text,
            reading,
            sceno::SourceRef {
                adapter: "knot.document".into(),
                id: selection.address.clone(),
            },
            &MetreTemplate {
                feet: m.feet,
                positions_per_foot: m.positions,
                rhyme_groups: vec![],
            },
        )?;
        if m.paired_rhyme {
            if let SceneFacts::MetreGrid(grid) = &mut scene.facts {
                for (i, line) in grid.lines.iter_mut().enumerate() {
                    line.rhyme_group = Some(format!("R{}", i / 2 + 1));
                }
            }
            scene.revision = blake3::hash(
                &serde_json::to_vec(&(&scene.revision, &scene.facts)).map_err(|e| e.to_string())?,
            )
            .to_hex()
            .to_string();
        }
        let anchor = selection.anchor()?;
        Ok::<_, String>((MetreDraft::new(scene)?, selection, anchor, reading.clone()))
    })();
    match result {
        Ok((draft, source, anchor, reading)) => {
            let m = &mut state.composition.metre;
            m.draft = Some(draft);
            m.source = Some(source);
            m.reading = Some(reading);
            m.anchor = Some(anchor);
            m.saved = None;
            m.drag = None;
            m.notice = None;
            let _ = m.refresh();
        },
        Err(e) => state.composition.metre.notice = Some(e),
    }
}
pub fn discard(state: &mut DesktopState) {
    let m = &mut state.composition.metre;
    m.draft = None;
    m.source = None;
    m.reading = None;
    m.anchor = None;
    m.drag = None;
    m.saved = None;
    m.close_pending = false;
    m.notice = None;
    let _ = m.refresh();
}
pub fn retain(state: &mut DesktopState) {
    let m = &mut state.composition.metre;
    m.cancel();
    if let Some(d) = &m.draft {
        let mut item = CollectionItem::new(
            ItemKind::MetreAlignment,
            "Metre alignment",
            d.scene.title.clone(),
        )
        .with_metre_scene(d.scene.clone());
        if let Some(anchor) = &m.anchor {
            item = item.with_document_source(anchor.clone());
        }
        collect(state, item);
    }
}
fn selection(state: &mut DesktopState, event: BackdropSceneEvent) {
    let m = &mut state.composition.metre;
    m.cancel();
    let mut p = m.surfaces.borrow_mut();
    if let Some(s) = &mut p.surface {
        match event {
            BackdropSceneEvent::Select(k) => {
                let _ = s.select(Some(&k));
            },
            BackdropSceneEvent::Clear => {
                let _ = s.select(None);
            },
            BackdropSceneEvent::DismissRetained => s.dismiss_retained(),
        };
    }
    p.revision = p.revision.wrapping_add(1);
}
fn pointer(state: &mut DesktopState, key: DocKey, event: cambium::PointerEvent) {
    use cambium::{PointerButton, PointerPhase as Phase};
    if event.button != PointerButton::Primary {
        return;
    }
    if !state.composition.metre.current(state, key) {
        state.composition.metre.cancel();
        return;
    }
    let m = &mut state.composition.metre;
    if event.phase == Phase::Cancel {
        m.cancel();
        return;
    }
    if !event.local.0.is_finite()
        || !event.local.1.is_finite()
        || !event.size.0.is_finite()
        || !event.size.1.is_finite()
        || event.size.0 <= 0.0
        || event.size.1 <= 0.0
    {
        m.cancel();
        return;
    }
    if event.phase == Phase::Down {
        m.cancel();
        let p = m.surfaces.borrow();
        let Some(s) = &p.surface else {
            return;
        };
        let (w, h) = (event.size.0.round() as u32, event.size.1.round() as u32);
        let found = s
            .pick(event.local.0, event.local.1, w, h)
            .map(str::to_owned);
        let geometry = found
            .as_ref()
            .and_then(|k| s.occurrence_rect(k, w, h))
            .and_then(|r| {
                Some((
                    r,
                    (s.scene_point(1.0, 0.0, w, h)?.x - s.scene_point(0.0, 0.0, w, h)?.x) / 47.0,
                ))
            });
        drop(p);
        {
            let mut p = m.surfaces.borrow_mut();
            if let Some(s) = &mut p.surface {
                let _ = s.select(found.as_deref());
            }
            p.revision = p.revision.wrapping_add(1);
        }
        if let (Some(k), Some((r, scale)), Some(d)) = (found, geometry, &m.draft) {
            let edge = 8.0_f32.min(r.width / 3.0);
            let edit = if event.local.0 <= r.x + edge {
                Edit::Start
            } else if event.local.0 >= r.x + r.width - edge {
                Edit::End
            } else {
                Edit::Move
            };
            if let Ok(proposal) = d.propose(&k, edit, 0.0) {
                m.drag = Some(Drag {
                    baseline: d.scene.clone(),
                    proposal,
                    edit,
                    x: event.local.0,
                    ticks_per_pixel: scale,
                    size: event.size,
                    pointer: event.pointer_id,
                    owner: key,
                });
            }
        }
    } else if let Some(mut g) = m.drag.take() {
        if g.pointer != event.pointer_id {
            m.drag = Some(g);
            return;
        }
        if event.size == g.size && g.owner == key {
            if let Some(d) = &m.draft {
                if d.scene == g.baseline {
                    let dx = event.local.0 - g.x;
                    if let Ok(p) = d.propose(
                        &g.proposal.occurrence,
                        g.edit,
                        if dx.abs() < 3.0 {
                            0.0
                        } else {
                            dx * g.ticks_per_pixel
                        },
                    ) {
                        g.proposal = p;
                        m.drag = Some(g);
                        if event.phase == Phase::Up {
                            m.commit();
                        }
                    }
                }
            }
        }
    }
    event.prop.prevent_default();
    let _ = m.refresh();
}
fn change(state: &mut DesktopState, key: DocKey, edit: Edit, delta: f32) {
    if !state.composition.metre.current(state, key) {
        state.composition.metre.cancel();
        return;
    }
    let m = &mut state.composition.metre;
    m.cancel();
    if let (Some(selected), Some(d)) = (m.selected(), &mut m.draft) {
        match d.propose(&selected, edit, delta).and_then(|p| d.commit(&p)) {
            Ok(()) => {
                let _ = m.refresh();
            },
            Err(e) => m.notice = Some(e),
        }
    }
}
fn keypress(state: &mut DesktopState, key: DocKey, event: cambium::KeyEvent) {
    use cambium::{Key, NamedKey};
    if !state.composition.metre.current(state, key) {
        state.composition.metre.cancel();
        return;
    }
    match event.key {
        Key::Named(NamedKey::Escape) if state.composition.metre.drag.is_some() => {
            state.composition.metre.cancel()
        },
        Key::Named(NamedKey::Enter) if state.composition.metre.drag.is_some() => {
            state.composition.metre.commit()
        },
        Key::Named(k @ (NamedKey::ArrowLeft | NamedKey::ArrowRight))
            if !event.mods.ctrl && !event.mods.meta =>
        {
            change(
                state,
                key,
                if event.mods.alt {
                    Edit::Start
                } else if event.mods.shift {
                    Edit::End
                } else {
                    Edit::Move
                },
                if k == NamedKey::ArrowLeft { -1.0 } else { 1.0 },
            )
        },
        _ => return,
    }
    event.prevent_default();
    event.stop_propagation();
}
fn control(
    label: impl Into<String>,
    class: &str,
    action: impl Fn(&mut DesktopState) + 'static,
) -> DesktopView {
    Box::new(
        button(label.into(), move |s: &mut DesktopState, _| action(s))
            .attr("class", class.to_owned()),
    )
}
pub fn view(state: &DesktopState, key: DocKey) -> DesktopView {
    let m = &state.composition.metre;
    let mut children: Vec<DesktopView> = vec![
        Box::new(el("h3", "Metre alignment")),
        Box::new(el(
            "p",
            "Position spans are authored alignment. Heights show candidate lexical stress; rhyme arcs retain their disclosed evidence.",
        )),
        // A stable status slot keeps pointer capture alive through message changes.
        Box::new(el("p", m.notice.clone().unwrap_or_default()).attr("role", "status")),
    ];
    if m.draft.is_none() {
        let mut options: Vec<DesktopView> = vec![Box::new(el(
            "span",
            format!("{} feet · {} positions per foot", m.feet, m.positions),
        ))];
        for (feet, delta, label) in [
            (true, -1, "Fewer feet"),
            (true, 1, "More feet"),
            (false, -1, "Fewer positions per foot"),
            (false, 1, "More positions per foot"),
        ] {
            options.push(control(label, "knot-metre-template", move |s| {
                let m = &mut s.composition.metre;
                if feet {
                    m.feet = (i32::from(m.feet) + delta).clamp(1, 32) as u16;
                } else {
                    m.positions = (i32::from(m.positions) + delta).clamp(1, 8) as u16;
                }
            }));
        }
        options.push(control(
            if m.paired_rhyme {
                "Expected rhyme: paired"
            } else {
                "Expected rhyme: none"
            },
            "knot-metre-rhyme",
            |s| s.composition.metre.paired_rhyme = !s.composition.metre.paired_rhyme,
        ));
        options.push(control(
            "Start metre from sound reading",
            "knot-metre-start",
            move |s| start(s, key),
        ));
        children.push(Box::new(
            el("div", options).attr("class", "knot-metre-actions"),
        ));
    } else {
        let current = m.current(state, key);
        children.push(Box::new(el("p",if !current{"Source reading changed. This captured reading is inspectable; collect/discard it or reopen an explicit retained copy before editing."}else if m.drag.is_some(){"Drag preview — release to update the draft"}else if m.dirty(){"Metre draft — collect explicitly to retain"}else{"Retained metre copy"}).attr("role","status")));
        children.push(Box::new(
            el(
                "div",
                vec![
                    control("Collect metre alignment", "knot-metre-collect", retain),
                    control(
                        if m.dirty() {
                            "Discard metre draft"
                        } else {
                            "Close metre editor"
                        },
                        "knot-metre-discard",
                        discard,
                    ),
                    control("Undo metre edit", "knot-metre-undo", move |s| {
                        if !s.composition.metre.current(s, key) {
                            return;
                        }
                        let m = &mut s.composition.metre;
                        m.cancel();
                        if let Some(d) = &mut m.draft {
                            d.undo();
                        }
                        let _ = m.refresh();
                    }),
                    control("Redo metre edit", "knot-metre-redo", move |s| {
                        if !s.composition.metre.current(s, key) {
                            return;
                        }
                        let m = &mut s.composition.metre;
                        m.cancel();
                        if let Some(d) = &mut m.draft {
                            d.redo();
                        }
                        let _ = m.refresh();
                    }),
                ],
            )
            .attr("class", "knot-metre-actions"),
        ));
        let surface = m.surfaces.borrow();
        if let Some(error) = &surface.error {
            children.push(Box::new(
                el("p", format!("Metre drawing refused: {error}")).attr("role", "alert"),
            ));
        }
        if let Some(s) = &surface.surface {
            let width = 600;
            let b = s.scene().scene.bounds.size;
            let height = ((width as f32 - 48.0) * b.h / b.w + 48.0)
                .ceil()
                .clamp(280.0, 1000.0) as u32;
            let stage=on_key(on_pointer(custom_leaf::<DesktopState,()>(KEY,width,height).attr("class","knot-metre-scene").attr("aria-label","Metre alignment scene. Arrows move; Shift changes end; Alt changes start; Escape cancels."),move|s,e|pointer(s,key,e)),move|s,e|keypress(s,key,e));
            children.push(Box::new(
                el("div", stage).attr("class", "knot-metre-scroll"),
            ));
            if let Some(k) = s.selected() {
                if let Some(span) = m.draft.as_ref().and_then(|d| d.span(k)) {
                    let span = m.drag.as_ref().map(|g| g.proposal.after).unwrap_or(span);
                    let mut edits: Vec<DesktopView> = vec![Box::new(
                        el(
                            "p",
                            format!(
                                "{k} · start {} / length {} positions{}",
                                span.start,
                                span.length,
                                if m.drag.is_some() { " · preview" } else { "" }
                            ),
                        )
                        .attr("class", "knot-metre-status")
                        .attr("role", "status"),
                    )];
                    for (edit, label) in [
                        (Edit::Move, "Move"),
                        (Edit::Start, "Start"),
                        (Edit::End, "End"),
                    ] {
                        for (delta, dir) in [(-1.0, "earlier"), (1.0, "later")] {
                            edits.push(control(
                                format!("{label} {dir} 1 position"),
                                "knot-metre-edit",
                                move |s| change(s, key, edit, delta),
                            ));
                        }
                    }
                    children.push(Box::new(on_key(
                        el("div", edits).attr("class", "knot-metre-actions"),
                        move |s, e| keypress(s, key, e),
                    )));
                }
            }
            children.push(Box::new(el("p","Drag a syllable span or either edge. Snap: one metrical position. Left/Right move; Shift changes end; Alt changes start; Escape cancels. Narrow views scroll sideways.")));
            children.push(Box::new(backdrop_scene_reading(
                s.scene(),
                s.selected(),
                s.retained(),
                selection,
            )));
        }
    }
    Box::new(el("section", children).attr("class", "knot-metre"))
}
pub fn close_prompt(state: &DesktopState) -> DesktopView {
    let mut children: Vec<DesktopView> = vec![];
    if state.composition.metre.close_pending {
        children.extend([Box::new(el("p","A metre draft has not been retained. Keep editing, or explicitly discard it before closing.")) as DesktopView,control("Keep editing metre","knot-metre-close-keep",|s|s.composition.metre.close_pending=false),control("Discard metre and close","knot-metre-close-discard",|s|{discard(s);s.window_commands().close();})]);
    }
    let style = if state.composition.metre.close_pending {
        let resolved = state.theme_session.resolve(&state.appearance);
        let (ground, text) = resolved
            .theme
            .palette_for_mode(&resolved.mode)
            .map(|mode| {
                (
                    tinct::color_to_hex(mode.palette.surface),
                    tinct::color_to_hex(mode.palette.text),
                )
            })
            .unwrap_or_else(|_| ("#ffffff".into(), "#161616".into()));
        format!(
            "position:fixed;left:10%;right:10%;top:15%;z-index:200;padding:20px;background:{ground};color:{text};border:2px solid currentColor"
        )
    } else {
        "display:none".into()
    };
    Box::new(
        el("div", children)
            .attr("class", "knot-metre-close")
            .attr("role", "alertdialog")
            .attr("aria-label", "Unretained metre draft")
            .attr("style", style),
    )
}
type Ctx<'a> = AppCtx<'a, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;
#[derive(Default)]
pub struct Bindings {
    producer: Option<Rc<RefCell<SceneProducer<Surface>>>>,
}
impl Bindings {
    pub fn frame(&mut self, ctx: &mut Ctx<'_>) {
        let state = ctx.runner.state();
        let mounted = !taproot::matching_classes(
            &*ctx.runner.dom().borrow(),
            &taproot::Selector::class("knot-metre-scene"),
        )
        .is_empty();
        let stale = state.composition.metre.drag.as_ref().is_some_and(|g| {
            !state.composition.metre.current(state, g.owner)
                || state.composition.section != 1
                || !mounted
                || state.theme_session.workshop_open
        });
        if stale {
            ctx.runner.update(|s| s.composition.metre.cancel());
        }
        let visible = mounted
            && !ctx.runner.state().theme_session.workshop_open
            && ctx.runner.state().composition.section == 1
            && ctx.runner.state().composition.metre.draft.is_some();
        if !visible {
            ctx.producers.remove(KEY);
            return;
        }
        let state = ctx.runner.state();
        let source = state.composition.metre.surfaces.clone();
        let resolved = state.theme_session.resolve(&state.appearance);
        if let Ok(mode) = resolved.theme.palette_for_mode(&resolved.mode) {
            let p = mode.palette;
            let rgba = |c| {
                let h = tinct::color_to_hex(c);
                [
                    u8::from_str_radix(&h[1..3], 16).unwrap() as f32 / 255.0,
                    u8::from_str_radix(&h[3..5], 16).unwrap() as f32 / 255.0,
                    u8::from_str_radix(&h[5..7], 16).unwrap() as f32 / 255.0,
                    1.0,
                ]
            };
            let mut theme = SurfaceTheme {
                ground: rgba(p.surface),
                text: rgba(p.text),
                guide: rgba(p.text_dim),
                category: rgba(p.primary),
                root: rgba(p.tertiary),
                fifth: rgba(p.secondary),
                warning: rgba(p.danger),
            };
            theme.guide[3] = 0.28;
            let mut s = source.borrow_mut();
            if format!("{:?}", s.theme) != format!("{theme:?}") {
                s.theme = theme;
                s.revision = s.revision.wrapping_add(1);
            }
        }
        let producer = self.producer.get_or_insert_with(|| {
            Rc::new(RefCell::new(SceneProducer::new(
                source.clone(),
                KEY,
                |s, w, h| {
                    let theme = s.theme.clone();
                    s.rendered_size = Some((w, h));
                    match s.surface.as_mut().unwrap().paint(w, h, &theme) {
                        Ok(scene) => {
                            s.error = None;
                            scene
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
                        name: Some("Metre alignment".into()),
                        children: vec![],
                    })
                },
            )))
        });
        producer.borrow_mut().set_source(source);
        if !ctx.producers.contains(KEY) {
            ctx.producers
                .register(KEY, producer.clone(), &[])
                .expect("Knot metre owns its surface key");
        }
    }
}
pub const CSS: &str = ".knot-composition .knot-metre{display:flex;flex-direction:column;gap:10px;min-width:0;width:100%;flex:none}.knot-composition .knot-metre-actions{display:flex;flex-direction:row;flex-wrap:wrap;gap:6px;align-items:center;width:100%;flex:none}.knot-composition .knot-metre-scroll{display:block;overflow-x:auto;width:100%;max-width:100%;flex:none}.knot-metre-scene{flex:none;display:block}.knot-metre-status{width:100%}.knot-metre-scene:focus{outline:2px solid currentColor}";

#[cfg(test)]
mod tests {
    use super::*;
    use cambium::{PointerEvent, PointerPhase};
    use cambium_genet_winit_host::WindowCommands;
    use knot_document::KnotDocumentSession;
    fn fixture() -> (DesktopState, DocKey) {
        let text = "the night\nthe light";
        let mut state = DesktopState::new(
            KnotDocumentSession::scratch("scratch:metre", text),
            WindowCommands::new(),
        );
        let key = state.focused_key().unwrap();
        let snapshot = state.document().snapshot();
        state
            .document_mut()
            .session_mut()
            .select_source_span(&snapshot.source.address, text, 0, text.len())
            .unwrap();
        state.composition.cmudict_enabled = true;
        state.composition.layers.perfect_rhyme = true;
        state.composition.metre.paired_rhyme = true;
        super::super::analyze_sound(&mut state, key, false);
        start(&mut state, key);
        assert!(
            state.composition.metre.draft.is_some(),
            "{:?}",
            state.composition.metre.notice
        );
        (state, key)
    }
    #[test]
    fn native_metre_body_and_edges_preview_cancel_commit_and_preserve_source() {
        let k = "bytes-0-9/syllable/1";
        for edit in [Edit::Move, Edit::Start, Edit::End] {
            let (mut state, key) = fixture();
            let before = state
                .composition
                .metre
                .draft
                .as_ref()
                .unwrap()
                .scene
                .clone();
            let source = state.document().snapshot().text;
            let (point, dx) = {
                let p = state.composition.metre.surfaces.borrow();
                let s = p.surface.as_ref().unwrap();
                let r = s.occurrence_rect(k, 800, 280).unwrap();
                let x = match edit {
                    Edit::Move => r.x + r.width / 2.0,
                    Edit::Start => r.x + 1.0,
                    Edit::End => r.x + r.width - 1.0,
                };
                let dx = s
                    .viewport_point(sceno::Vec2::new(95.0, 50.0), 800, 280)
                    .unwrap()
                    .x
                    - s.viewport_point(sceno::Vec2::new(48.0, 50.0), 800, 280)
                        .unwrap()
                        .x;
                ((x, r.y + r.height / 2.0), dx)
            };
            let target = (
                point.0 + if edit == Edit::Start { -dx } else { dx },
                point.1,
            );
            let send = |s: &mut DesktopState, phase, p| {
                pointer(s, key, PointerEvent::new(phase, p, (800.0, 280.0)))
            };
            send(&mut state, PointerPhase::Down, point);
            send(&mut state, PointerPhase::Move, target);
            assert!(state.composition.metre.preview_active());
            assert_eq!(
                state.composition.metre.draft.as_ref().unwrap().scene,
                before
            );
            state.composition.metre.cancel();
            assert_eq!(
                state.composition.metre.surfaces.borrow().input.as_ref(),
                Some(&before)
            );
            send(&mut state, PointerPhase::Down, point);
            send(&mut state, PointerPhase::Move, target);
            send(&mut state, PointerPhase::Up, target);
            assert!(!state.composition.metre.preview_active());
            let m = &mut state.composition.metre;
            let after = m.draft.as_ref().unwrap().scene.clone();
            assert_ne!(after, before);
            m.mark_retained(&before);
            assert!(m.dirty());
            m.mark_retained(&after);
            assert!(!m.dirty());
            m.draft.as_mut().unwrap().undo();
            assert_eq!(m.draft.as_ref().unwrap().scene, before);
            m.draft.as_mut().unwrap().redo();
            assert_eq!(m.draft.as_ref().unwrap().scene, after);
            assert_eq!(state.document().snapshot().text, source);
            assert!(!state.document().snapshot().dirty);
        }
    }
    #[test]
    fn native_metre_stale_source_selection_and_resized_gestures_are_atomic() {
        let (mut state, key) = fixture();
        let occurrence = "bytes-0-9/syllable/1";
        let before = state
            .composition
            .metre
            .draft
            .as_ref()
            .unwrap()
            .scene
            .clone();
        let point = {
            let p = state.composition.metre.surfaces.borrow();
            let r = p
                .surface
                .as_ref()
                .unwrap()
                .occurrence_rect(occurrence, 800, 280)
                .unwrap();
            (r.x + r.width / 2.0, r.y + r.height / 2.0)
        };
        pointer(
            &mut state,
            key,
            PointerEvent::new(PointerPhase::Down, point, (800.0, 280.0)),
        );
        pointer(
            &mut state,
            key,
            PointerEvent::new(
                PointerPhase::Move,
                (point.0 + 60.0, point.1),
                (801.0, 280.0),
            ),
        );
        assert!(!state.composition.metre.preview_active());
        assert_eq!(
            state.composition.metre.draft.as_ref().unwrap().scene,
            before
        );
        pointer(
            &mut state,
            key,
            PointerEvent::new(PointerPhase::Down, point, (800.0, 280.0)),
        );
        let snapshot = state.document().snapshot();
        state
            .document_mut()
            .session_mut()
            .select_source_span(&snapshot.source.address, &snapshot.text, 0, 3)
            .unwrap();
        pointer(
            &mut state,
            key,
            PointerEvent::new(PointerPhase::Up, (point.0 + 60.0, point.1), (800.0, 280.0)),
        );
        change(&mut state, key, Edit::Move, 1.0);
        assert!(!state.composition.metre.preview_active());
        assert_eq!(
            state.composition.metre.draft.as_ref().unwrap().scene,
            before
        );
        assert!(state.composition.metre.open_copy(before, None).is_err());
    }
    #[test]
    fn changed_pronunciation_reading_cannot_receive_an_old_alignment_gesture() {
        let (mut state, key) = fixture();
        let before = state
            .composition
            .metre
            .draft
            .as_ref()
            .unwrap()
            .scene
            .clone();
        selection(
            &mut state,
            BackdropSceneEvent::Select("bytes-0-9/syllable/1".into()),
        );
        state.composition.sound.as_mut().unwrap().tokens[1].selected = None;
        change(&mut state, key, Edit::Move, 1.0);
        assert_eq!(
            state.composition.metre.draft.as_ref().unwrap().scene,
            before
        );
        assert!(!state.composition.metre.current(&state, key));
    }
    #[test]
    fn production_panel_exposes_a_painted_surface_within_the_reading_width() {
        use cambium_genet_winit_host::{Harness, Init};
        fn panel(s: &DesktopState) -> DesktopView {
            super::super::view(s, s.focused_key().unwrap())
        }
        let (mut state, _) = fixture();
        state.composition.section = 1;
        let observed = Rc::new(std::cell::Cell::new(false));
        let observe = observed.clone();
        let mut hooks = crate::host_hooks();
        let mut frame = hooks.frame;
        hooks.frame = Box::new(move |ctx| {
            let result = frame(ctx);
            observe.set(ctx.producers.contains(KEY));
            result
        });
        let mut host = Harness::with_hooks(
            Init {
                state,
                logic: panel as fn(&DesktopState) -> DesktopView,
                sheet: crate::desktop_sheet(),
                fonts: crate::fonts::bundled_fonts(),
                images: vec![],
            },
            hooks,
        );
        host.layout_at(400.0, 1000.0);
        host.prepare_frame();
        host.relayout();
        let node = host.with_dom(|dom| {
            taproot::matching_classes(dom, &taproot::Selector::class("knot-metre-scene"))[0]
        });
        let rect = host.painted_rect(node).unwrap();
        assert!(rect.0 >= -1.0 && rect.0 < 100.0, "metre geometry {rect:?}");
        assert!(
            (rect.2 - 600.0).abs() < 1.0 && rect.3 > 280.0,
            "metre geometry {rect:?}"
        );
        assert!(observed.get(), "native metre producer is registered");
        let (point, delta) = {
            let p = host.state().composition.metre.surfaces.borrow();
            let s = p.surface.as_ref().unwrap();
            let r = s
                .occurrence_rect(
                    "bytes-0-9/syllable/1",
                    rect.2.round() as u32,
                    rect.3.round() as u32,
                )
                .unwrap();
            let a = s
                .viewport_point(
                    sceno::Vec2::new(48.0, 50.0),
                    rect.2.round() as u32,
                    rect.3.round() as u32,
                )
                .unwrap();
            let b = s
                .viewport_point(
                    sceno::Vec2::new(95.0, 50.0),
                    rect.2.round() as u32,
                    rect.3.round() as u32,
                )
                .unwrap();
            (
                (rect.0 + r.x + r.width / 2.0, rect.1 + r.y + r.height / 2.0),
                b.x - a.x,
            )
        };
        assert!(
            point.0 >= 0.0 && point.0 < 400.0 && point.1 >= 0.0 && point.1 < 1000.0,
            "actual pointer target {point:?}"
        );
        host.press_at(point.0, point.1);
        host.after_dispatch();
        host.prepare_frame();
        host.relayout();
        host.move_to(point.0 + delta, point.1);
        host.after_dispatch();
        host.prepare_frame();
        host.relayout();
        assert!(
            host.state().composition.metre.preview_active(),
            "host-routed preview"
        );
        host.release_at(point.0 + delta, point.1);
        host.after_dispatch();
        host.prepare_frame();
        host.relayout();
        assert_eq!(
            host.state()
                .composition
                .metre
                .draft
                .as_ref()
                .unwrap()
                .span("bytes-0-9/syllable/1")
                .unwrap()
                .start,
            2
        );
        host.request_close(cambium_genet_winit_host::CloseRequest::Native);
        assert!(!host.close_requested());
        assert!(host.state().composition.metre.close_pending);
        host.update(|state| {
            let scene = state
                .composition
                .metre
                .draft
                .as_ref()
                .unwrap()
                .scene
                .clone();
            state.composition.metre.mark_retained(&scene);
        });
        host.request_close(cambium_genet_winit_host::CloseRequest::Native);
        assert!(
            host.close_requested(),
            "a confirmed retained draft permits normal close"
        );
    }
    #[test]
    fn retained_copy_is_explicit_and_cannot_replace_an_unsaved_draft() {
        let (mut state, key) = fixture();
        let before = state
            .composition
            .metre
            .draft
            .as_ref()
            .unwrap()
            .scene
            .clone();
        assert!(
            state
                .composition
                .metre
                .open_copy(before.clone(), None)
                .is_err()
        );
        discard(&mut state);
        state
            .composition
            .metre
            .open_copy(before.clone(), None)
            .unwrap();
        selection(
            &mut state,
            BackdropSceneEvent::Select("bytes-0-9/syllable/1".into()),
        );
        change(&mut state, key, Edit::Move, 1.0);
        assert!(state.composition.metre.dirty());
        let mut wrong = before.clone();
        wrong.revision.push('x');
        state.composition.metre.mark_retained(&wrong);
        assert!(state.composition.metre.dirty());
    }
}

pub(crate) fn scenario_fields(state: &DesktopState) -> std::collections::BTreeMap<String, String> {
    let m = &state.composition.metre;
    let scene = m.draft.as_ref().map(|d| &d.scene);
    let selected = m.selected();
    let surface = m.surfaces.borrow();
    let span = selected
        .as_ref()
        .and_then(|key| scene?.span_target(key))
        .map(|t| t.span);
    [
        (
            "metre_rendered_size",
            format!("{:?}", surface.rendered_size),
        ),
        (
            "metre_render_error",
            surface.error.clone().unwrap_or_default(),
        ),
        ("metre_dirty", m.dirty().to_string()),
        ("metre_preview", m.preview_active().to_string()),
        (
            "metre_digest",
            scene
                .map(|s| {
                    blake3::hash(&serde_json::to_vec(s).unwrap())
                        .to_hex()
                        .to_string()
                })
                .unwrap_or_default(),
        ),
        ("metre_selected", selected.unwrap_or_default()),
        (
            "metre_start",
            span.map(|s| s.start.to_string()).unwrap_or_default(),
        ),
        (
            "metre_length",
            span.map(|s| s.length.to_string()).unwrap_or_default(),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect()
}
