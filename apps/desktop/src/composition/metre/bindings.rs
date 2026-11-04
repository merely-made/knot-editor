// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
