use super::path_ops::active_path_name;
use super::pen::path_operations_menu;
use super::shapes::is_shape_tool;
use super::{ArrowDefaultLink, arrow_default_link_id, copy_arrow_to_tool_defaults};
use super::{GradientChoices, LayerContent, PaintChange, PhotocraftApp, Tokens, Tool, Value, json, paint_param, swatch};

// ---------------------------------------------------------------------------------------------
// Options bars

/// Options bar for vector tools; false for other tools.
pub fn options_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui, tool: Tool) -> bool {
    if !(is_shape_tool(tool)
        || matches!(
            tool,
            Tool::Pen | Tool::FreeformPen | Tool::CurvaturePen | Tool::AddAnchorPoint | Tool::DeleteAnchorPoint | Tool::PathSelection | Tool::DirectSelection
        ))
    {
        return false;
    }
    let t = Tokens::get(ui.ctx());
    let lbl = |ui: &mut egui::Ui, s: &str| {
        let label = ui.label(egui::RichText::new(s).color(t.text_dim).size(12.0));
        // The caption scrubs and names the control drawn right after it.
        crate::widgets::scrub_option_label(ui, &label);
    };
    let target = selected_shape(&app.session);
    if tool == Tool::PathSelection && !crate::direct_select::active_paths(app).is_empty() {
        path_operations_menu(app, ui);
        crate::widgets::vline(ui, 22.0);
    }
    let o = &mut app.ui.tool_options;
    if tool == Tool::PathSelection && target.is_none() {
        lbl(ui, if app.ui.vector_mask_target { "Drag to move the targeted vector mask" } else { tl!("Drag to move the selected path") });
        return true;
    }
    if tool == Tool::DirectSelection && target.is_none() {
        let (shift, alt) = (crate::shortcuts::pretty("Shift"), crate::shortcuts::pretty("Alt"));
        lbl(
            ui,
            &crate::i18n::fmt(
                tl!("Click: select anchor · Drag: move anchor, handle or segment · {shift}-click: add · {alt}-click: whole subpath"),
                &[("shift", &shift), ("alt", &alt)],
            ),
        );
        return true;
    }
    if matches!(tool, Tool::AddAnchorPoint | Tool::DeleteAnchorPoint) {
        let hint = if tool == Tool::AddAnchorPoint {
            tl!("Click a segment to add an anchor point · {alt}-click an anchor to delete it")
        } else {
            tl!("Click an anchor point to delete it · {alt}-click a segment to add one")
        };
        lbl(ui, &crate::i18n::fmt(hint, &[("alt", &crate::shortcuts::pretty("Alt"))]));
        return true;
    }
    if matches!(tool, Tool::Pen | Tool::FreeformPen | Tool::CurvaturePen) {
        let opts = [("path".to_string(), tl!("Path")), ("shape".to_string(), tl!("Shape"))];
        crate::widgets::dropdown(ui, "pen-mode", tl!("Mode"), &mut o.vector_mode, &opts, 80.0);
        crate::widgets::vline(ui, 22.0);
        if o.vector_mode == "shape" {
            crate::widgets::value_field(ui, &mut o.stroke_width, 0.0..=288.0, "px", 58.0).accessible_name(tl!("Stroke:"));
            if let Some(change) =
                crate::shape_stroke_ui::button(ui, "pen-stroke", &o.shape_stroke, &mut app.ui.stroke_editor, false, true, None, o.stroke_width > 0.0)
            {
                o.shape_stroke = change.options;
                o.stroke_width = enabled_stroke_width(o.stroke_width);
            }
        }
        crate::widgets::vline(ui, 22.0);
        path_operations_menu(app, ui);
        let o = &mut app.ui.tool_options;
        if tool == Tool::Pen {
            crate::widgets::vline(ui, 22.0);
            crate::widgets::checkbox(ui, &mut o.pen_auto_add_delete, tl!("Auto Add/Delete"));
        }
        let enter = crate::shortcuts::pretty("Enter");
        match tool {
            Tool::FreeformPen => {
                crate::widgets::vline(ui, 22.0);
                lbl(ui, tl!("Curve Fit:"));
                crate::widgets::value_field(ui, &mut o.curve_fit, 0.5..=10.0, "px", 58.0);
                let alt = crate::shortcuts::pretty("Alt");
                lbl(ui, &crate::i18n::fmt(tl!("Drag: draw freehand · {alt}-click: straight segment · Release on the start point: close"), &[("alt", &alt)]));
            }
            Tool::CurvaturePen => lbl(
                ui,
                &crate::i18n::fmt(
                    tl!("Click: smooth point · Double-click: corner · Drag a point: move · Click first point: close · {key} finish"),
                    &[("key", &enter)],
                ),
            ),
            _ => lbl(
                ui,
                &crate::i18n::fmt(
                    tl!("Click: corner · Drag: smooth · {alt}-drag: break handle · Click or drag first point: close · {key} finish · Esc cancel"),
                    &[("alt", &crate::shortcuts::pretty("Alt")), ("key", &enter)],
                ),
            ),
        }
        // Path mode: Make Selection from the path being drawn or the active path, as Photoshop's
        // options bar's "Make: Selection…" does (#5703).
        if app.ui.tool_options.vector_mode == "path" {
            crate::widgets::vline(ui, 22.0);
            let has_path = active_path_name(app).is_some() || app.ui.pen.as_ref().is_some_and(|p| p.knots.len() >= 2);
            if ui.add_enabled(has_path, egui::Button::new(tl!("Make Selection"))).clicked()
                && let Err(e) = crate::canvas_tool_menu::make_selection(app)
            {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            // Make: Shape turns the path into a shape layer with the shape fill and stroke (#10934).
            let has_shape_path = super::path_ops::shape_source_path(app).is_some() || app.ui.pen.as_ref().is_some_and(|p| p.knots.len() >= 2);
            if ui.add_enabled(has_shape_path, egui::Button::new(tl!("Make Shape"))).on_hover_text(tl!("Make a filled shape layer from the path")).clicked()
                && let Err(e) = super::make_shape(app)
            {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
        return true;
    }
    if is_shape_tool(tool) {
        // Shape draws a shape layer, Path adds the outline to the work path (#9453).
        let opts = [("shape".to_string(), tl!("Shape")), ("path".to_string(), tl!("Path"))];
        crate::widgets::dropdown(ui, "shape-mode", tl!("Mode"), &mut app.ui.tool_options.shape_mode, &opts, 80.0);
    } else {
        // Path Selection and Direct Selection edit the selected shape layer: the Shape tools'
        // Path mode isn't theirs to show or change.
        let mut mode = "shape".to_string();
        crate::widgets::dropdown(ui, "shape-mode", tl!("Mode"), &mut mode, &[("shape".to_string(), tl!("Shape"))], 80.0);
    }
    crate::widgets::vline(ui, 22.0);
    // The Shape tools' Path mode hides Fill and Stroke (an outline has no paint); the tool's
    // geometry, the Line's arrowheads included, stays.
    let paint = !(is_shape_tool(tool) && draws_path(&app.ui.tool_options));
    crate::shape_options::options_bar(app, ui, tool, paint);
    // Photoshop-style arrowhead settings at the right of Line options.
    if tool == Tool::Line {
        crate::widgets::vline(ui, 22.0);
        ui.menu_image_button(crate::icons::image("settings", 18.0, t.icon), |ui| {
            let o = &mut app.ui.tool_options;
            ui.set_min_width(170.0);
            ui.label(tl!("Arrowheads"));
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut o.arrow_start, tl!("Start"));
                ui.checkbox(&mut o.arrow_end, tl!("End"));
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Width"));
                ui.add(egui::DragValue::new(&mut o.arrow_width).range(1.0..=4096.0).suffix(" px"));
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Length"));
                ui.add(egui::DragValue::new(&mut o.arrow_length).range(1.0..=4096.0).suffix(" px"));
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Concavity"));
                ui.add(egui::DragValue::new(&mut o.arrow_concavity).range(0.0..=100.0).suffix(" %"));
            });
        })
        .response
        .on_hover_text(tl!("Arrowhead options"));
    }
    true
}

/// Whether the Shape tools are in Path mode: a drag adds its outline to the work path.
pub(crate) fn draws_path(o: &crate::state::ToolOptions) -> bool {
    o.shape_mode == "path"
}

/// Whether the active layer supplies the selected-shape options.
pub(super) fn selected_shape(session: &photocraft_engine::Session) -> Option<photocraft_doc::LayerId> {
    let st = session.active()?;
    let layer = st.active_layer?;
    matches!(st.doc.layer(layer)?.content, LayerContent::Shape(_)).then_some(layer)
}

// ---------------------------------------------------------------------------------------------
// Properties (shape layers)

pub(super) fn solid_paint(c: [f32; 4]) -> photocraft_doc::Fill {
    photocraft_doc::Fill::Solid(photocraft_doc::Color::rgba(c[0], c[1], c[2], c[3]))
}

pub(crate) fn enabled_stroke_width(width: f32) -> f32 {
    if width.is_finite() && width > 0.0 { width } else { 3.0 }
}

/// One corner of the live rectangle, preserving the other three radii.
/// The engine takes the full [top-left, top-right, bottom-right, bottom-left] array.
pub(super) fn corner_radii_patch(radii: [f64; 4], corner: usize, radius: f32) -> Value {
    let mut next = radii;
    if let Some(r) = next.get_mut(corner) {
        *r = f64::from(radius.max(0.0));
    }
    json!({"radii": next})
}

/// 코너 필드 앞의 모서리 글리프: 해당 모서리만 둥근 선. `corner` 는 radii 순서(TL, TR, BR, BL).
fn corner_glyph(ui: &mut egui::Ui, corner: usize, size: f32, tooltip: &str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    response.on_hover_text(tooltip);
    let rect = rect.shrink(1.5);
    let (h, v) = match corner {
        0 => (rect.right_top(), rect.left_bottom()),
        1 => (rect.left_top(), rect.right_bottom()),
        2 => (rect.left_bottom(), rect.right_top()),
        _ => (rect.right_bottom(), rect.left_top()),
    };
    // 두 끝점을 모서리 쪽으로 잇는 2차 곡선을 선분으로 근사한다.
    let apex = egui::pos2(v.x, h.y);
    let points = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            let u = 1.0 - t;
            egui::pos2(u * u * h.x + 2.0 * u * t * apex.x + t * t * v.x, u * u * h.y + 2.0 * u * t * apex.y + t * t * v.y)
        })
        .collect();
    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.2, Tokens::get(ui.ctx()).text_dim)));
}

/// 링크된 모서리 반경 입력 하나를 전체 값으로 푼 `shape.edit` 패치, 또는 해당 모서리만 바꾼 패치.
pub(super) fn linked_radii_patch(radii: [f64; 4], corner: usize, radius: f32, linked: bool) -> Value {
    if linked { json!({"radii": f64::from(radius.max(0.0))}) } else { corner_radii_patch(radii, corner, radius) }
}

/// 직사각형 Properties 의 모서리 반경: Photoshop 처럼 2×2 그리드와 가운데 링크 토글.
/// 링크 상태는 레이어별 임시 값이고, 값이 서로 다르면(캔버스 코너 핸들 편집 등) 항상 해제로 표시한다.
pub(super) fn corner_radii_grid(ui: &mut egui::Ui, radii: [f64; 4], layer: u64, key: &dyn Fn(&str) -> String) -> Option<Value> {
    const GLYPH: f32 = 14.0;
    const LINK_W: f32 = 20.0;
    const GAP: f32 = 4.0;
    let link_key = egui::Id::unique(("corner-radii-link", layer));
    let uniform = radii.iter().all(|r| *r == radii[0]);
    let linked = uniform && ui.data(|d| d.get_temp::<bool>(link_key)).unwrap_or(true);
    let field_w = ((ui.available_width() - 2.0 * GLYPH - LINK_W - 4.0 * GAP) / 2.0).max(44.0);
    let mut edit = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        let column = |ui: &mut egui::Ui, corners: [(usize, &str); 2], edit: &mut Option<Value>| {
            ui.vertical(|ui| {
                for (index, name) in corners {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        corner_glyph(ui, index, GLYPH, name);
                        let mut radius = radii[index] as f32;
                        if crate::widgets::value_field_named(ui, name, &mut radius, 0.0..=100000.0, "px", field_w).changed() {
                            let mut patch = linked_radii_patch(radii, index, radius, linked);
                            let field = if linked { "radius-all".to_string() } else { format!("radius-{index}") };
                            patch["coalesce"] = json!(key(&field));
                            *edit = Some(patch);
                        }
                    });
                }
            });
        };
        column(ui, [(0, tl!("Top Left")), (3, tl!("Bottom Left"))], &mut edit);
        if crate::icons::button(ui, if linked { "link" } else { "unlink" }, LINK_W, linked, tl!("Link corner radius values")).clicked() {
            ui.data_mut(|d| d.insert_temp(link_key, !linked));
            // 값이 다른 채로 링크를 켜면 왼쪽 위 값으로 맞춘다.
            if !linked && !uniform {
                edit = Some(json!({"radii": radii[0], "coalesce": key("radius-link")}));
            }
        }
        column(ui, [(1, tl!("Top Right")), (2, tl!("Bottom Right"))], &mut edit);
    });
    edit
}

/// Follow the existing brush/mask continuous-edit convention: one history state per press or
/// keyboard edit, with every value tick of a held-pointer gesture sharing its key.
pub(crate) fn paint_gesture(ui: &egui::Ui, key: egui::Id) -> u64 {
    let pass = ui.ctx().cumulative_pass_nr();
    let pressed = ui.input(|i| i.pointer.any_pressed());
    ui.data_mut(|d| {
        let gesture = d.get_temp_mut_or_default::<(u64, u64)>(key);
        if pressed && gesture.1 != pass {
            gesture.0 = gesture.0.wrapping_add(1);
            gesture.1 = pass;
        }
        gesture.0
    })
}

/// Properties panel for a shape layer: Appearance (fill, stroke) and live shape geometry.
pub fn shape_properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, id: photocraft_doc::LayerId) {
    if app.session.active().is_some_and(|st| {
        let locks = st.doc.effective_locks(id);
        locks.all || locks.pixels
    }) {
        ui.disable();
    }
    let Some(sh) = app.session.active().and_then(|s| s.doc.layer(id)).and_then(|l| match &l.content {
        LayerContent::Shape(sh) => Some(sh.clone()),
        _ => None,
    }) else {
        return;
    };
    let t = Tokens::get(ui.ctx());
    let mut edit: Option<Value> = None;
    // A swatch picked in a Fill/Stroke popover joins the Recently Used Colors once the edit took it.
    let mut recent: Option<[f32; 3]> = None;
    let doc_id = app.session.active().map(|st| st.doc.id);
    let key = |k: &str| format!("shape-{doc_id:?}-{}-{k}", id.0);
    let gesture_key = ui.scope_id().with(key("paint-gesture"));
    let gesture = paint_gesture(ui, gesture_key);
    let coalesce = |field: &str| format!("{}:{}:{gesture}", key(field), gesture_key.value());
    // The shared collapsible section headers (#155).
    // Prefs "Appearance" is theme chrome (Giao diện); shape fill/stroke is Diện mạo.
    if crate::props_layout::section(ui, "appearance", crate::i18n::tr_ctx(crate::i18n::current(), "shape", "Appearance")) {
        let (fg, bg) = (app.session.tools.foreground, app.session.tools.background);
        let lib = crate::paint_picker::library(&app.session, ui.ctx());
        let mut picks: Vec<(bool, PaintChange)> = Vec::new();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("Fill")).color(t.text_dim).size(12.0));
            let mut gradients = GradientChoices {
                presets: &app.session.presets.gradients,
                current: &app.session.presets.gradient,
                recent: &mut app.ui.recent_gradients,
                fg,
                bg,
            };
            if let Some(change) = swatch(ui, &lib, sh.fill.as_ref(), fg, tl!("Set shape fill type"), &mut gradients) {
                let fill = match &change {
                    PaintChange::None => Some(Value::Null),
                    PaintChange::Color(color) => Some(json!(color)),
                    PaintChange::Gradient(g) => Some(paint_param(g)),
                    PaintChange::OpenPicker => None,
                };
                if let Some(fill) = fill {
                    edit = Some(json!({"fill": fill, "coalesce": coalesce("fill")}));
                }
                picks.push((false, change));
            }
            ui.add_space(12.0);
            ui.label(egui::RichText::new(tl!("Stroke")).color(t.text_dim).size(12.0));
            if let Some(change) = swatch(ui, &lib, sh.stroke.as_ref().map(|s| &s.paint), fg, tl!("Set shape stroke type"), &mut gradients) {
                let width = enabled_stroke_width(sh.stroke.as_ref().map_or(0.0, |s| s.width));
                let stroke = match &change {
                    PaintChange::None => Some(Value::Null),
                    PaintChange::Color(color) => Some(json!({"color": color, "width": width})),
                    PaintChange::Gradient(g) => Some(json!({"color": paint_param(g), "width": width})),
                    PaintChange::OpenPicker => None,
                };
                if let Some(stroke) = stroke {
                    edit = Some(json!({"stroke": stroke, "coalesce": coalesce("stroke")}));
                }
                picks.push((true, change));
            }
            let mut w = sh.stroke.as_ref().map_or(0.0, |s| s.width);
            if crate::widgets::value_field(ui, &mut w, 0.0..=288.0, "px", 60.0).accessible_name(tl!("Stroke:")).changed() && (sh.stroke.is_some() || w > 0.0) {
                edit = Some(json!({"stroke": {"width": w}, "coalesce": coalesce("stroke-w")}));
            }
        });
        let style = sh.stroke.as_ref().map(crate::shape_stroke_ui::StrokeOptions::from).unwrap_or_default();
        let stroke_enabled = sh.stroke.as_ref().is_some_and(|stroke| stroke.width > 0.0);
        if let Some(change) = crate::shape_stroke_ui::button(
            ui,
            &key("style"),
            &style,
            &mut app.ui.stroke_editor,
            sh.path.subpaths.iter().all(|s| s.closed),
            !matches!(sh.live, Some(photocraft_doc::LiveShape::Ellipse { .. })),
            app.session.active().map(|st| (st.doc.id, id)),
            stroke_enabled,
        ) {
            let mut stroke = change.options.params();
            if !stroke_enabled {
                stroke["width"] = json!(3.0);
            }
            edit = Some(json!({"stroke": stroke, "coalesce": change.coalesce}));
        }
        for (is_stroke, change) in picks {
            match change {
                PaintChange::OpenPicker => {
                    let which = if is_stroke { crate::shape_fill_ui::Paint::Stroke } else { crate::shape_fill_ui::Paint::Fill };
                    crate::shape_fill_ui::open_paint(app, which, Some(id), false, fg);
                }
                PaintChange::Color(c) => recent = Some([c[0], c[1], c[2]]),
                PaintChange::None | PaintChange::Gradient(_) => {}
            }
        }
    }
    if let Some(live) = &sh.live
        && crate::props_layout::section(ui, "liveShape", tl!("Shape"))
    {
        let num = |ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, unit: &str| -> bool {
            ui.label(egui::RichText::new(tl!(&label)).color(t.text_dim).size(12.0));
            crate::widgets::value_field(ui, v, range, unit, 64.0).accessible_name(tl!(&label)).changed()
        };
        match live {
            photocraft_doc::vector::LiveShape::Rect { rect, .. }
            | photocraft_doc::vector::LiveShape::Ellipse { rect }
            | photocraft_doc::vector::LiveShape::Polygon { rect, .. } => {
                ui.horizontal(|ui| {
                    let (mut w, mut h) = (rect[2] as f32, rect[3] as f32);
                    let cw = num(ui, "W", &mut w, 1.0..=300000.0, "px");
                    let ch = num(ui, "H", &mut h, 1.0..=300000.0, "px");
                    if cw || ch {
                        edit = Some(json!({"rect": [rect[0], rect[1], w.round(), h.round()], "coalesce": key("wh")}));
                    }
                });
            }
            photocraft_doc::vector::LiveShape::Line { .. } | photocraft_doc::vector::LiveShape::ArrowLine { .. } => {}
        }
        ui.vertical(|ui| match live {
            photocraft_doc::vector::LiveShape::Rect { radii, .. } => {
                if let Some(patch) = corner_radii_grid(ui, *radii, id.0, &key) {
                    edit = Some(patch);
                }
            }
            photocraft_doc::vector::LiveShape::Polygon { sides, star_ratio, corner_radius, .. } => {
                // One radius rounds every vertex of a triangle, polygon or star, as in Photoshop.
                ui.horizontal(|ui| {
                    let mut r = *corner_radius as f32;
                    if num(ui, tl!("Corner radius"), &mut r, 0.0..=100000.0, "px") {
                        edit = Some(json!({"cornerRadius": r, "coalesce": key("corner-radius")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut n = *sides as f32;
                    if num(ui, tl!("Sides"), &mut n, 3.0..=100.0, "") {
                        edit = Some(json!({"sides": n.round() as u32, "coalesce": key("sides")}));
                    }
                    let mut sr = (*star_ratio * 100.0) as f32;
                    if num(ui, tl!("Star ratio"), &mut sr, 1.0..=100.0, "%") {
                        edit = Some(json!({"starRatio": sr as f64 / 100.0, "coalesce": key("star")}));
                    }
                })
                .inner
            }
            photocraft_doc::vector::LiveShape::ArrowLine { weight, start, end, width, length, concavity, .. } => {
                // This is a selected, saved Arrow; unlike the tool's defaults, edits update its geometry.
                ui.horizontal(|ui| {
                    let mut value = *start;
                    if ui.checkbox(&mut value, tl!("Start")).changed() {
                        edit = Some(json!({"arrowStart": value, "coalesce": key("arrow-start")}));
                    }
                    let mut value = *end;
                    if ui.checkbox(&mut value, tl!("End")).changed() {
                        edit = Some(json!({"arrowEnd": value, "coalesce": key("arrow-end")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = *width as f32;
                    if num(ui, "Width", &mut v, 1.0..=4096.0, "px") {
                        edit = Some(json!({"arrowWidth": v, "coalesce": key("arrow-width")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = *length as f32;
                    if num(ui, "Length", &mut v, 1.0..=4096.0, "px") {
                        edit = Some(json!({"arrowLength": v, "coalesce": key("arrow-length")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = *weight as f32;
                    if num(ui, "Weight", &mut v, 1.0..=10000.0, "px") {
                        edit = Some(json!({"weight": v, "coalesce": key("arrow-weight")}));
                    }
                });
                ui.horizontal(|ui| {
                    let mut v = *concavity as f32;
                    if num(ui, "Concavity", &mut v, 0.0..=100.0, "%") {
                        edit = Some(json!({"arrowConcavity": v, "coalesce": key("arrow-concavity")}));
                    }
                });

                // Explicitly adopt this arrow's values as defaults for new arrows.
                let target = app.session.active().map(|st| (st.doc.id.0, id.0));
                let mut linked = ui.ctx().data(|data| {
                    let state = data.get_temp::<ArrowDefaultLink>(arrow_default_link_id()).unwrap_or_default();
                    state.target == target && state.enabled
                });

                if ui.checkbox(&mut linked, tl!("Set as default")).on_hover_text(tl!("Use these settings for new arrows")).changed() {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(arrow_default_link_id(), ArrowDefaultLink { target, enabled: linked });
                    });
                    if linked {
                        copy_arrow_to_tool_defaults(app, id);
                    }
                }
            }
            photocraft_doc::vector::LiveShape::Line { weight, .. } => {
                ui.horizontal(|ui| {
                    let mut w = *weight as f32;
                    if num(ui, tl!("Weight"), &mut w, 1.0..=10000.0, "px") {
                        edit = Some(json!({"weight": w, "coalesce": key("weight")}));
                    }
                })
                .inner
            }
            _ => {}
        });
    }
    if let Some(mut p) = edit {
        p["layer"] = json!(id.0);

        match app.run("shape.edit", p) {
            Ok(_) => {
                // Remember recently used colors after a successful edit.
                if let Some(rgb) = recent {
                    crate::paint_picker::remember(app, rgb);
                }

                // Only a successful Properties edit updates new-arrow defaults,
                // and only while the user explicitly enables the checkbox.
                let target = app.session.active().map(|st| (st.doc.id.0, id.0));
                let linked = ui.ctx().data(|data| {
                    let state = data.get_temp::<ArrowDefaultLink>(arrow_default_link_id()).unwrap_or_default();
                    state.enabled && state.target == target
                });

                if linked {
                    copy_arrow_to_tool_defaults(app, id);
                }
            }
            Err(e) => {
                app.ui.status = e;
                app.ui.status_error = true;
            }
        }
        if !ui.input(|i| i.pointer.any_down()) {
            ui.data_mut(|d| {
                let gesture = d.get_temp_mut_or_default::<(u64, u64)>(gesture_key);
                gesture.0 = gesture.0.wrapping_add(1);
            });
        }
    }
}
