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
type Ctx<'a> = AppCtx<'a, DesktopState, fn(&DesktopState) -> DesktopView, DesktopView>;
#[derive(Default)]
pub struct Bindings {
    producer: Option<Rc<RefCell<SceneProducer<Surface>>>>,
}
pub const CSS: &str = ".knot-composition .knot-metre{display:flex;flex-direction:column;gap:10px;min-width:0;width:100%;flex:none}.knot-composition .knot-metre-actions{display:flex;flex-direction:row;flex-wrap:wrap;gap:6px;align-items:center;width:100%;flex:none}.knot-composition .knot-metre-scroll{display:block;overflow-x:auto;width:100%;max-width:100%;flex:none}.knot-metre-scene{flex:none;display:block}.knot-metre-status{width:100%}.knot-metre-scene:focus{outline:2px solid currentColor}";

#[cfg(test)]
mod tests;

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

mod view;

pub use view::{close_prompt, view};

mod bindings;
