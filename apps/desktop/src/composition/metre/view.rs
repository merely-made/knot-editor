// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
