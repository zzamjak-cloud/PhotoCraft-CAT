use super::{
    Align2, Color32, Ev, FontId, GroupView, ItemView, PhotocraftApp, Place, Pos2, Rect, Sense, Stroke, TextureHandle, Tokens, Tool, Value,
    browser, canvas_drop_rect, empty, generic, json, name_matches, paint_gradient, pattern_texture, pos2, run, shape_texture, style_texture, vec2,
};

// ------------------------------------------------------------------ Gradients

pub fn gradients_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let (fg, bg) = (app.session.tools.foreground, app.session.tools.background);
    let groups_src = app.session.presets.gradients.clone();
    // Current gradient (what the Gradient tool paints), like the panel's top "recent" row.
    let cur = app.session.presets.gradient.clone();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    paint_gradient(ui, r, &cur.resolve(fg, bg));
    ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, Tokens::get(ui.ctx()).card_border), egui::StrokeKind::Inside);
    ui.add_space(4.0);
    let groups: Vec<GroupView> = groups_src
        .iter()
        .map(|g| GroupView { name: g.name.clone(), items: g.items.iter().map(|i| ItemView { key: i.name.clone(), name: i.name.clone() }).collect() })
        .collect();
    let canvas = canvas_drop_rect(app);
    let mut st = std::mem::take(&mut app.ui.presets_ui);
    // 목록은 패널 높이를 따라 늘어난다 (browser 가 푸터 높이를 빼고 스크롤한다).
    let max_h = (ui.available_height() - 40.0).max(90.0);
    let events =
        browser(ui, &mut st, "gradients", &groups, Place { canvas, max_h, new_tip: tl!("Create new gradient"), flush: false }, &mut |ui, r, gi, ii| {
            let stops = groups_src[gi].items[ii].resolve(fg, bg);
            paint_gradient(ui, r, &stops);
        });
    app.ui.presets_ui = st;
    for e in events {
        match &e {
            Ev::Select(k) => {
                app.ui.presets_ui.selected.insert("gradients".into(), k.clone());
                run(app, "gradient.presets.select", json!({"preset": k}));
            }
            Ev::Activate(k) | Ev::Drop(k, _) => {
                app.ui.presets_ui.selected.insert("gradients".into(), k.clone());
                run(app, "gradient.presets.apply", json!({"preset": k}));
            }
            _ => {
                generic(app, "gradients", "gradient.presets", &e, json!({"name": "Custom"}));
            }
        }
    }
}

// ------------------------------------------------------------------ Patterns

pub fn patterns_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // Until the preset store has loaded the saved patterns, their folder entries would look
    // deleted: don't prune them yet.
    if app.services.preset_store.is_none() {
        photocraft_engine::presets::patterns::sync(&mut app.session);
    }
    let lib = app.session.patterns.items.clone();
    let groups: Vec<GroupView> = app
        .session
        .presets
        .pattern_groups
        .iter()
        .map(|g| GroupView {
            name: g.name.clone(),
            items: g
                .items
                .iter()
                .filter_map(|id| lib.iter().find(|p| &p.id == id))
                .map(|p| ItemView { key: p.id.clone(), name: p.display_name().to_string() })
                .collect(),
        })
        .collect();
    let pats: Vec<Vec<photocraft_doc::Pattern>> =
        app.session.presets.pattern_groups.iter().map(|g| g.items.iter().filter_map(|id| lib.iter().find(|p| &p.id == id).cloned()).collect()).collect();
    let canvas = canvas_drop_rect(app);
    let mut st = std::mem::take(&mut app.ui.presets_ui);
    let max_h = (ui.available_height() - 40.0).max(90.0);
    let ctx = ui.ctx().clone();
    let events = browser(
        ui,
        &mut st,
        "patterns",
        &groups,
        Place { canvas, max_h, new_tip: tl!("Create new pattern from the selection"), flush: false },
        &mut |ui, r, gi, ii| {
            let tex = pattern_texture(&ctx, &pats[gi][ii]);
            ui.painter().image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        },
    );
    app.ui.presets_ui = st;
    for e in events {
        match &e {
            Ev::Select(k) => {
                app.ui.presets_ui.selected.insert("patterns".into(), k.clone());
                run(app, "pattern.presets.select", json!({"pattern": k}));
            }
            Ev::Activate(k) | Ev::Drop(k, _) => {
                app.ui.presets_ui.selected.insert("patterns".into(), k.clone());
                run(app, "pattern.presets.apply", json!({"pattern": k}));
            }
            _ => {
                generic(app, "patterns", "pattern.presets", &e, json!({}));
            }
        }
    }
}

// ------------------------------------------------------------------ floating panels

/// Styles, Shapes, Tool Presets, Clone Source 는 도크 탭(`dock::Group::Presets`)이다. 예전의
/// 플로팅 플래그(저장된 UI 상태, 가져오기 후 "보여 줘")는 Window 메뉴 명령으로 바꿔 도크에서
/// 연다 (사용자가 꾸민 레이아웃이면 그 레이아웃에서).
pub fn windows(app: &mut PhotocraftApp, ctx: &egui::Context) {
    for id in ["window.panel.toolPresets", "window.panel.shapes", "window.panel.styles", "window.panel.cloneSource"] {
        let Some(flag) = super::float_flag(&mut app.ui.presets_ui, id) else { continue };
        if std::mem::take(flag) {
            let _ = crate::menus::invoke(app, ctx, id, serde_json::json!({"show": true}));
        }
    }
}

// ------------------------------------------------------------------ Styles

pub fn styles_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let src = app.session.presets.styles.clone();
    let groups: Vec<GroupView> = src
        .iter()
        .map(|g| GroupView { name: g.name.clone(), items: g.items.iter().map(|i| ItemView { key: i.name.clone(), name: i.name.clone() }).collect() })
        .collect();
    let canvas = canvas_drop_rect(app);
    let mut st = std::mem::take(&mut app.ui.presets_ui);
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let mut texes: Vec<Vec<TextureHandle>> = Vec::new();
    for g in &src {
        texes.push(g.items.iter().map(|s| style_texture(app, &ctx, s)).collect());
    }
    let events = browser(
        ui,
        &mut st,
        "styles",
        &groups,
        Place { canvas, max_h: f32::INFINITY, new_tip: tl!("Create new style from the selected layer"), flush: false },
        &mut |ui, r, gi, ii| {
            ui.painter().rect_filled(r, 2.0, t.field);
            ui.painter().image(texes[gi][ii].id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        },
    );
    app.ui.presets_ui = st;
    let shift = ui.input(|i| i.modifiers.shift);
    for e in events {
        match &e {
            Ev::Select(k) | Ev::Activate(k) => {
                app.ui.presets_ui.selected.insert("styles".into(), k.clone());
                // Clicking applies to the selected layers (⇧ adds to their effects).
                run(app, "style.presets.apply", json!({"preset": k, "add": shift}));
            }
            Ev::Drop(k, pos) => {
                // Dropping on the canvas styles the layer under the pointer's selection: the active layer.
                let _ = pos;
                run(app, "style.presets.apply", json!({"preset": k}));
            }
            _ => {
                generic(app, "styles", "style.presets", &e, json!({"name": "Style"}));
            }
        }
    }
}

/// The Styles panel menu ("…"): Import Styles… / Export Selected Styles… / Export Styles…
/// (Photoshop layer style libraries, `.asl`).
pub fn styles_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let any = app.session.presets.styles.iter().any(|g| !g.items.is_empty());
    let selected = app.ui.presets_ui.selected.get("styles").cloned();
    let mut act: Option<&str> = None;
    if ui.button(tl!("Import Styles…")).clicked() {
        act = Some("import");
    }
    if ui.add_enabled(selected.is_some(), egui::Button::new(tl!("Export Selected Styles…"))).clicked() {
        act = Some("selected");
    }
    if ui.add_enabled(any, egui::Button::new(tl!("Export Styles…"))).clicked() {
        act = Some("all");
    }
    let Some(a) = act else { return };
    ui.close();
    let r = match a {
        "import" => app.pick_file_bytes_filtered(&["asl"], |app, name, bytes| match crate::preset_files_ui::open(app, &name, &bytes) {
            Some(r) => r.map(|()| Value::Null),
            None => Err(crate::i18n::fmt(tl!("{file} is not a style library (.asl)"), &[("file", &crate::file_open::display_name(&name))])),
        }),
        "selected" => match selected {
            Some(name) => export_styles(app, json!({"styles": [name.clone()]}), &name),
            None => Ok(Value::Null),
        },
        _ => export_styles(app, json!({}), "Styles"),
    };
    if let Err(e) = r
        && e != "cancelled"
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// Builds the `.asl` through `style.presets.exportAsl`, then asks where to save it.
fn export_styles(app: &mut PhotocraftApp, params: Value, suggested: &str) -> Result<Value, String> {
    let r = app.run("style.presets.exportAsl", params)?;
    let bytes = r["data"].as_str().and_then(photocraft_engine::paint::tile::b64_decode).ok_or("the export returned no data")?;
    let n = r["count"].as_u64().unwrap_or(0);
    let stem: String = suggested.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let stem = if stem.trim().is_empty() { "Styles".to_string() } else { stem.trim().to_string() };
    app.pick_save(&format!("{stem}.asl"), move |app, path| {
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, &bytes)?;
        app.ui.status_error = false;
        app.ui.status = crate::i18n::fmt(
            &crate::i18n::trn(crate::i18n::current(), n, "Exported {n} style to {file}", "Exported {n} styles to {file}"),
            &[("file", &crate::file_open::display_name(&path))],
        );
        Ok(json!({"path": path, "count": n}))
    })
}

// ------------------------------------------------------------------ Shapes

pub fn shapes_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let src = photocraft_engine::presets::shapes::all_groups(&app.session);
    // 검색창: 셰이프 이름이나 폴더 이름이 맞는 셰이프만 (한글 초성 검색 지원).
    let query = shapes_search_field(app, ui);
    // (원본 그룹, 원본 셰이프 인덱스들): 걸러진 목록에서 썸네일을 찾을 때 쓴다.
    let shown: Vec<(usize, Vec<usize>)> = src
        .iter()
        .enumerate()
        .filter_map(|(gi, g)| {
            let group_hit = name_matches(&g.name, &query);
            let items: Vec<usize> = (0..g.items.len()).filter(|&ii| group_hit || g.items.get(ii).is_some_and(|i| name_matches(&i.name, &query))).collect();
            (query.trim().is_empty() || !items.is_empty()).then_some((gi, items))
        })
        .collect();
    if !query.trim().is_empty() && shown.is_empty() {
        let t = Tokens::get(ui.ctx());
        ui.label(egui::RichText::new(tl!("No matching shapes")).color(t.text_dim).size(11.0));
    }
    let groups: Vec<GroupView> = shown
        .iter()
        .filter_map(|(gi, items)| {
            let g = src.get(*gi)?;
            let items = items.iter().filter_map(|&ii| g.items.get(ii)).map(|i| ItemView { key: i.name.clone(), name: i.name.clone() }).collect();
            Some(GroupView { name: g.name.clone(), items })
        })
        .collect();
    let canvas = canvas_drop_rect(app);
    let mut st = std::mem::take(&mut app.ui.presets_ui);
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let events = browser(
        ui,
        &mut st,
        "shapes",
        &groups,
        Place { canvas, max_h: f32::INFINITY, new_tip: tl!("Create new shape from the current path"), flush: false },
        &mut |ui, r, gi, ii| {
            let item = shown.get(gi).and_then(|(sg, items)| src.get(*sg)?.items.get(*items.get(ii)?));
            if let Some(item) = item {
                let tex = shape_texture(&ctx, item);
                ui.painter().image(tex.id(), r.shrink(2.0), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), t.text);
            }
        },
    );
    app.ui.presets_ui = st;
    let fill = shape_fill(app);
    for e in events {
        match &e {
            Ev::Select(k) => {
                // Selecting a shape picks it for the Custom Shape tool (and switches to it).
                app.ui.presets_ui.selected.insert("shapes".into(), k.clone());
                app.ui.presets_ui.custom_shape = k.clone();
                app.ui.tool = Tool::CustomShape;
            }
            Ev::Activate(k) => {
                app.ui.presets_ui.custom_shape = k.clone();
                run(app, "shape.presets.place", json!({"preset": k, "fill": fill}));
            }
            Ev::Drop(k, pos) => {
                app.ui.presets_ui.custom_shape = k.clone();
                let mut p = json!({"preset": k, "fill": fill});
                if let (Some(c), Some(d)) = (crate::canvas::doc_point_at(app, *pos), app.session.active()) {
                    let side = (d.doc.size.width.min(d.doc.size.height) as f64 * 0.3).max(8.0);
                    p["rect"] = json!([c[0] - side / 2.0, c[1] - side / 2.0, side, side]);
                }
                run(app, "shape.presets.place", p);
            }
            _ => {
                generic(app, "shapes", "shape.presets", &e, json!({"name": "Shape"}));
            }
        }
    }
}

/// Shapes 패널 위쪽 검색창 (Brushes 패널과 같은 모양). 현재 검색어를 돌려준다.
fn shapes_search_field(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> String {
    let t = Tokens::get(ui.ctx());
    let mut q = app.ui.presets_ui.search.get("shapes").cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        crate::icons::paint(ui, Rect::from_min_size(ui.cursor().min + vec2(0.0, 3.0), vec2(16.0, 16.0)), "search", 14.0, t.text_faint);
        ui.add_space(20.0);
        let w = (ui.available_width() - 4.0).max(60.0);
        crate::widgets::text_field(ui, ui.scope_id().with("shape-search"), &mut q, tl!("Search Shapes"), w, false);
    });
    ui.add_space(2.0);
    if q.is_empty() {
        app.ui.presets_ui.search.remove("shapes");
    } else {
        app.ui.presets_ui.search.insert("shapes".into(), q.clone());
    }
    q
}

fn shape_fill(app: &PhotocraftApp) -> Value {
    let f = app.session.tools.foreground;
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if app.ui.tool_options.shape_fill { json!(format!("#{:02x}{:02x}{:02x}", q(f[0]), q(f[1]), q(f[2]))) } else { Value::Null }
}

/// Options-bar shape picker for the Custom Shape tool.
pub fn shape_picker(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(tl!("Shape:")).color(t.text_dim).size(12.0));
    let groups = photocraft_engine::presets::shapes::all_groups(&app.session);
    let cur = app.ui.presets_ui.shape().to_string();
    let ctx = ui.ctx().clone();
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 22.0), Sense::click());
    ui.painter().rect_filled(r, 3.0, t.field);
    ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    if let Some(sh) = groups.iter().flat_map(|g| g.items.iter()).find(|s| s.name == cur) {
        let tex = shape_texture(&ctx, sh);
        ui.painter().image(
            tex.id(),
            Rect::from_center_size(r.center() - vec2(5.0, 0.0), vec2(18.0, 18.0)),
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            t.text,
        );
    }
    crate::icons::paint(ui, Rect::from_center_size(pos2(r.right() - 8.0, r.center().y), vec2(10.0, 10.0)), "chevron-down", 9.0, t.text_dim);
    let resp = resp.on_hover_text(&cur);
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(232.0);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for g in &groups {
                ui.label(egui::RichText::new(&g.name).color(t.text_dim).size(11.0));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                    for sh in &g.items {
                        let (cr, cresp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                        let on = sh.name == cur;
                        ui.painter().rect_filled(
                            cr,
                            2.0,
                            if on {
                                t.accent_soft
                            } else if cresp.hovered() {
                                t.hover
                            } else {
                                t.field
                            },
                        );
                        let tex = shape_texture(&ctx, sh);
                        ui.painter().image(tex.id(), cr.shrink(4.0), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), t.text);
                        if cresp.on_hover_text(&sh.name).clicked() {
                            app.ui.presets_ui.custom_shape = sh.name.clone();
                            ui.close();
                        }
                    }
                });
            }
        });
    });
}

/// Custom Shape tool drag: places the selected shape in the dragged rect (⇧ keeps proportions).
pub fn finish_custom_shape(app: &mut PhotocraftApp, rect: [f64; 4], keep: bool, fill: Value, stroke: Value) {
    let name = app.ui.presets_ui.shape().to_string();
    let r = run(app, "shape.presets.place", json!({"preset": name, "rect": rect, "keepAspect": keep, "fill": fill, "stroke": stroke}));
    if r.is_none() && !photocraft_engine::presets::shapes::all_groups(&app.session).iter().any(|g| g.items.iter().any(|s| s.name == name)) {
        let template = "No custom shape \"{name}\": pick one in the options bar or Window › Shapes";
        app.ui.status = crate::i18n::fmt(tl!(template), &[("name", &name)]);
    }
}

// ------------------------------------------------------------------ Tool Presets

pub(super) fn tool_id(t: Tool) -> String {
    let s = format!("{t:?}");
    let mut c = s.chars();
    c.next().map(|f| f.to_ascii_lowercase().to_string() + c.as_str()).unwrap_or_default()
}

/// Apply a tool preset: the engine sets the brush/colour, the shell switches tool and options.
pub fn select_tool_preset(app: &mut PhotocraftApp, name: &str) {
    let before_ratio = crate::crop_ui::preset_ratio(app);
    let Some(r) = run(app, "tool.presets.select", json!({"preset": name})) else { return };
    if let Some(tool) = r["tool"].as_str().and_then(Tool::from_name) {
        crate::tool_memory::select(app, tool);
        // Each tool keeps its own brush (#218), so switch it in before the preset's brush lands
        // on it, and re-apply the brush the command just set for the old tool.
        crate::paint_mouse::sync_tool_brush(app);
        if let Some(b) = r["options"].get("brush").filter(|b| b.is_object()) {
            let _ = app.run("tools.setBrush", json!({"brush": b}));
        }
    }
    if let Some(o) = r["options"].get("toolOptions").and_then(Value::as_object) {
        let mut cur = serde_json::to_value(&app.ui.tool_options).unwrap_or_default();
        if let Some(c) = cur.as_object_mut() {
            for (k, v) in o {
                // An object option (`sample_all`, one per tool) takes only the entries the
                // preset stored, so the other tools' keep theirs (#5524).
                match (c.get_mut(k), v) {
                    (Some(Value::Object(have)), Value::Object(set)) => have.extend(set.clone()),
                    _ => {
                        c.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        if let Ok(opts) = serde_json::from_value(cur) {
            app.ui.tool_options = opts;
        }
    }
    // As the options bar does: a crop preset's ratio (or W x H) fits the frame at once.
    if crate::crop_ui::preset_ratio(app) != before_ratio {
        crate::crop_ui::apply_ratio(app);
    }
}

/// The `tool.presets.new` params for the + button: `tool`'s own options only, so choosing the
/// preset later leaves the other tools' settings alone.
pub(super) fn new_tool_preset_params(app: &PhotocraftApp, tool: Tool) -> Value {
    let all = serde_json::to_value(&app.ui.tool_options).unwrap_or_default();
    let mut own: serde_json::Map<String, Value> =
        crate::tool_reset::option_keys(tool).iter().filter_map(|k| Some(((*k).to_string(), all.get(*k)?.clone()))).collect();
    // Sample All Layers is each tool's own (#5524): store `tool`'s alone.
    if let Some(k) = crate::tool_reset::sample_all_key(tool)
        && let Some(v) = all.get("sample_all").and_then(|m| m.get(&k))
    {
        own.insert("sample_all".into(), json!({ k: v }));
    }
    json!({"name": tool.label(), "tool": tool_id(tool), "options": {"toolOptions": own}})
}

/// Tool Presets 목록에서 끄는 프리셋 이름.
#[derive(Clone, Debug)]
struct ToolPresetDrag(String);

/// `dragged` 를 `target` 의 앞(`after` false) 또는 뒤에 놓을 때 `tool.presets.edit` move 의
/// `index` (끌어 온 항목을 뺀 뒤의 위치). 제자리면 `None`.
pub(super) fn reorder_index(names: &[String], dragged: &str, target: &str, after: bool) -> Option<usize> {
    let from = names.iter().position(|n| n == dragged)?;
    let to = names.iter().position(|n| n == target)? + usize::from(after);
    let at = if from < to { to - 1 } else { to };
    (at != from).then_some(at)
}

/// Tool Presets 푸터(체크박스·버튼 한 줄)에 남겨 두는 높이.
const TOOL_PRESETS_FOOTER_H: f32 = 34.0;

pub fn tool_presets_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let only = app.ui.presets_ui.current_tool_only;
    let cur = app.ui.tool;
    let items: Vec<(String, String)> =
        app.session.presets.tool_presets.iter().filter(|p| !only || Tool::from_name(&p.tool) == Some(cur)).map(|p| (p.name.clone(), p.tool.clone())).collect();
    let selected = app.ui.presets_ui.selected.get("toolPresets").cloned();
    let mut action: Option<(String, Value)> = None;
    let mut pick: Option<String> = None;
    // 이름 입력: 레이어 이름 변경처럼 Enter·Tab·다른 곳 클릭은 확정, Esc 만 취소한다.
    // 포커스는 처음 한 번만 요청한다(매 프레임 요청하면 포커스를 잃는 클릭이 입력을 버렸다).
    if let Some((_, key, mut text)) = app.ui.presets_ui.renaming.clone().filter(|r| r.0 == "toolPresets") {
        let id = egui::Id::unique("tool-preset-rename-field");
        let focused_key = id.with("focused");
        ui.horizontal(|ui| {
            let name_label = ui.label(egui::RichText::new(tl!("Name:")).color(t.text_dim).size(11.5));
            let r = ui.add(egui::TextEdit::singleline(&mut text).id(id).desired_width(ui.available_width() - 4.0)).labelled_by(name_label.id);
            let ctx = ui.ctx().clone();
            let started = ctx.data(|d| d.get_temp::<String>(focused_key)).as_deref() == Some(key.as_str());
            if !started {
                r.request_focus();
                // Photoshop 처럼 이름 전체를 선택해 바로 덮어 쓸 수 있게 한다.
                if let Some(mut state) = egui::TextEdit::load_state(&ctx, id) {
                    let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()));
                    state.cursor.set_char_range(Some(all));
                    state.store(&ctx, id);
                }
                ctx.data_mut(|d| d.insert_temp(focused_key, key.clone()));
                app.ui.presets_ui.renaming = Some(("toolPresets".into(), key.clone(), text.clone()));
                return;
            }
            let (enter, tab, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Tab), i.key_pressed(egui::Key::Escape)));
            if esc || enter || tab || r.lost_focus() || !r.has_focus() {
                app.ui.presets_ui.renaming = None;
                ctx.data_mut(|d| d.remove::<String>(focused_key));
                let name = text.trim();
                if !esc && !name.is_empty() && name != key {
                    action = Some(("tool.presets.edit".into(), json!({"action": "rename", "preset": key, "name": name})));
                }
            } else {
                app.ui.presets_ui.renaming = Some(("toolPresets".into(), key.clone(), text.clone()));
            }
        });
    }
    // 목록은 패널 높이를 따라 늘어나고, 아래 푸터(Current Tool Only · + · 삭제)는 항상 보인다.
    let list_h = (ui.available_height() - TOOL_PRESETS_FOOTER_H).max(72.0);
    let all_names: Vec<String> = app.session.presets.tool_presets.iter().map(|p| p.name.clone()).collect();
    let sort_item = |ui: &mut egui::Ui, action: &mut Option<(String, Value)>| {
        if ui.button(tl!("Sort by Name")).clicked() {
            *action = Some(("tool.presets.edit".into(), json!({"action": "sort"})));
            ui.close();
        }
    };
    egui::ScrollArea::vertical().id_salt("tool-presets").max_height(list_h).min_scrolled_height(list_h.min(72.0)).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if items.is_empty() {
            empty(ui, if only { tl!("No presets for the current tool.") } else { tl!("No tool presets.") });
        }
        for (name, tool) in &items {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected.as_deref() == Some(name), name));
            if selected.as_deref() == Some(name) {
                ui.painter().rect_filled(r, 0.0, t.row_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, 0.0, t.hover.gamma_multiply(0.5));
            }
            let icon = Tool::from_name(tool).map(crate::icons::tool_icon).unwrap_or("settings");
            crate::icons::paint(ui, Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(16.0, 16.0)), icon, 13.0, t.icon);
            ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, name, FontId::proportional(11.5), t.text);
            if resp.clicked() {
                pick = Some(name.clone());
            }
            // 끌어서 순서 바꾸기: 놓은 행의 위/아래 절반에 따라 그 앞/뒤로 옮긴다.
            resp.dnd_set_drag_payload(ToolPresetDrag(name.clone()));
            let after = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.y > r.center().y);
            if let Some(dragged) = resp.dnd_hover_payload::<ToolPresetDrag>()
                && dragged.0 != *name
            {
                let y = if after { r.bottom() } else { r.top() };
                ui.painter().hline(r.x_range(), y, egui::Stroke::new(2.0, t.accent));
            }
            if let Some(dragged) = resp.dnd_release_payload::<ToolPresetDrag>()
                && let Some(index) = reorder_index(&all_names, &dragged.0, name, after)
            {
                action = Some(("tool.presets.edit".into(), json!({"action": "move", "preset": dragged.0, "index": index})));
            }
            resp.context_menu(|ui| {
                if ui.button(tl!("Rename Tool Preset…")).clicked() {
                    app.ui.presets_ui.renaming = Some(("toolPresets".into(), name.clone(), name.clone()));
                    ui.close();
                }
                if ui.button(tl!("Delete Tool Preset")).clicked() {
                    action = Some(("tool.presets.edit".into(), json!({"action": "delete", "preset": name})));
                    ui.close();
                }
                ui.separator();
                sort_item(ui, &mut action);
            });
        }
        // 목록 아래 빈 곳의 메뉴에서도 이름순 정렬.
        let rest = ui.available_size_before_wrap();
        if rest.y > 0.0 {
            let (_, bg) = ui.allocate_exact_size(vec2(ui.available_width(), rest.y), Sense::click());
            bg.context_menu(|ui| sort_item(ui, &mut action));
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        crate::widgets::checkbox(ui, &mut app.ui.presets_ui.current_tool_only, tl!("Current Tool Only"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::icons::button(ui, "trash", 24.0, false, tl!("Delete tool preset")).clicked()
                && let Some(n) = selected.clone()
            {
                action = Some(("tool.presets.edit".into(), json!({"action": "delete", "preset": n})));
            }
            if crate::icons::button(ui, "plus", 24.0, false, tl!("Create new tool preset")).clicked() {
                action = Some(("tool.presets.new".into(), new_tool_preset_params(app, cur)));
            }
        });
    });
    if let Some(n) = pick {
        app.ui.presets_ui.selected.insert("toolPresets".into(), n.clone());
        select_tool_preset(app, &n);
    }
    if let Some((cmd, p)) = action {
        let created = cmd == "tool.presets.new";
        if let Some(r) = run(app, &cmd, p)
            && created
        {
            let n = r["name"].as_str().unwrap_or_default().to_string();
            app.ui.presets_ui.selected.insert("toolPresets".into(), n.clone());
            app.ui.presets_ui.renaming = Some(("toolPresets".into(), n.clone(), n));
        }
    }
}

// ------------------------------------------------------------------ Clone Source

pub fn clone_source_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let cs = app.session.presets.clone.clone();
    let slot = cs.active().clone();
    let mut action: Option<Value> = None;
    // The five source buttons.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for i in 0..photocraft_engine::presets::clone_source::SLOTS {
            let set = cs.slots[i].source.is_some();
            let tip = match cs.slots[i].source {
                Some(s) => format!("Clone source {}: {:.0}, {:.0}", i + 1, s[0], s[1]),
                None => crate::i18n::fmt(
                    tl!("Clone source {n} ({key}-click with Clone Stamp to set)"),
                    &[("n", &(i + 1).to_string()), ("key", &crate::shortcuts::pretty("Alt"))],
                ),
            };
            let r = crate::icons::button(ui, "stamp", 30.0, cs.active == i, &tip);
            if set {
                let d = r.rect.right_bottom() - vec2(6.0, 6.0);
                ui.painter().circle_filled(d, 2.5, t.accent);
            }
            if r.clicked() {
                run(app, "cloneSource.select", json!({"index": i}));
            }
        }
    });
    ui.add_space(4.0);
    let src_line =
        match (slot.source, slot.layer.and_then(|l| app.session.active().and_then(|d| d.doc.layer(photocraft_doc::LayerId(l)).map(|x| x.name.clone())))) {
            (Some(_), Some(l)) => format!("Source: {} : {l}", app.session.active().map(|d| d.doc.name.clone()).unwrap_or_default()),
            (Some(_), None) => "Source: set".to_string(),
            _ => crate::i18n::fmt(tl!("Source: not set ({key}-click with Clone Stamp)"), &[("key", &crate::shortcuts::pretty("Alt"))]),
        };
    ui.label(egui::RichText::new(src_line).color(t.text_dim).size(11.0));
    ui.add_space(4.0);
    let lbl = |ui: &mut egui::Ui, s: &str| {
        let label = ui.add_sized(vec2(18.0, 18.0), egui::Label::new(egui::RichText::new(s).color(t.text_dim).size(11.5)));
        crate::widgets::scrub_option_label(ui, &label);
    };
    let off = slot.offset().unwrap_or([0.0, 0.0]);
    let (mut ox, mut oy) = (off[0] as f32, off[1] as f32);
    let (mut w, mut h) = (slot.scale[0] as f32, slot.scale[1] as f32);
    let mut rot = slot.rotation as f32;
    egui::Grid::new("clone-src-grid").num_columns(5).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
        lbl(ui, "X:");
        if crate::widgets::value_field(ui, &mut ox, -30000.0..=30000.0, "px", 72.0).changed() && slot.source.is_some() {
            action = Some(json!({"offset": [ox, oy]}));
        }
        lbl(ui, "W:");
        if crate::widgets::value_field(ui, &mut w, -1000.0..=1000.0, "%", 64.0).changed() {
            action = Some(json!({"width": w}));
        }
        // The W/H chain: with it on, a new W or H scales the other by the same factor.
        let icon = if slot.linked { "link" } else { "unlink" };
        if crate::icons::button(ui, icon, 22.0, slot.linked, tl!("Maintain aspect ratio")).clicked() {
            action = Some(json!({"linked": !slot.linked}));
        }
        ui.end_row();
        lbl(ui, "Y:");
        if crate::widgets::value_field(ui, &mut oy, -30000.0..=30000.0, "px", 72.0).changed() && slot.source.is_some() {
            action = Some(json!({"offset": [ox, oy]}));
        }
        lbl(ui, "H:");
        if crate::widgets::value_field(ui, &mut h, -1000.0..=1000.0, "%", 64.0).changed() {
            action = Some(json!({"height": h}));
        }
        ui.end_row();
        lbl(ui, "A:");
        if crate::widgets::value_field(ui, &mut rot, -180.0..=180.0, "°", 72.0).changed() {
            action = Some(json!({"rotation": rot}));
        }
        ui.horizontal(|ui| {
            if crate::icons::button(ui, "arrow-left-right", 22.0, slot.flip_h, tl!("Flip Horizontal")).clicked() {
                action = Some(json!({"flipH": !slot.flip_h}));
            }
            if crate::icons::button(ui, "rotate-cw", 22.0, false, tl!("Reset Transform")).clicked() {
                run(app, "cloneSource.resetTransform", json!({}));
            }
        });
        ui.horizontal(|ui| {
            if crate::icons::button(ui, "scaling", 22.0, slot.flip_v, tl!("Flip Vertical")).clicked() {
                action = Some(json!({"flipV": !slot.flip_v}));
            }
        });
        ui.end_row();
    });
    if let Some(p) = action {
        run(app, "cloneSource.set", p);
    }
    ui.add_space(6.0);
    crate::widgets::hairline(ui);
    ui.add_space(4.0);
    // Overlay options.
    let mut o = cs.overlay.clone();
    let before = o.clone();
    crate::widgets::checkbox(ui, &mut o.show, tl!("Show Overlay"));
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        ui.label(egui::RichText::new(tl!("Opacity:")).color(t.text_dim).size(11.5));
        crate::widgets::value_field(ui, &mut o.opacity, 0.0..=100.0, "%", 56.0).accessible_name(tl!("Opacity:"));
        let opts = [
            ("normal".to_string(), tl!("Normal")),
            ("darken".to_string(), tl!("Darken")),
            ("lighten".to_string(), tl!("Lighten")),
            ("difference".to_string(), tl!("Difference")),
        ];
        crate::widgets::dropdown(ui, "clone-overlay-mode", tl!("Blend Mode"), &mut o.blend, &opts, 90.0);
    });
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        crate::widgets::checkbox(ui, &mut o.clipped, tl!("Clipped"));
        crate::widgets::checkbox(ui, &mut o.auto_hide, tl!("Auto Hide"));
        crate::widgets::checkbox(ui, &mut o.invert, tl!("Invert"));
    });
    if o != before {
        run(
            app,
            "cloneSource.overlay",
            json!({"show": o.show, "opacity": o.opacity, "clipped": o.clipped, "autoHide": o.auto_hide, "invert": o.invert, "blend": o.blend}),
        );
    }
}

/// Where the active clone source samples for document point `at` (for the canvas marker).
/// The first stroke anchors at its press, so the marker follows before the stroke commits.
pub fn clone_sample_point(app: &PhotocraftApp, at: Option<[f64; 2]>) -> Option<[f64; 2]> {
    use photocraft_engine::presets::clone_source::{Mapping, transform_matrix};
    let s = app.session.presets.clone.active();
    let src = s.source?;
    let stroking = app.drag.as_ref().filter(|d| matches!(d.tool, Tool::CloneStamp | Tool::Healing)).map(|d| d.start);
    let anchor = if app.ui.tool_options.clone_aligned { s.anchor.or(stroking) } else { stroking };
    match (anchor, at) {
        (Some(a), Some(h)) => {
            let m = Mapping { source: (src[0], src[1]), anchor: (a[0], a[1]), m: transform_matrix(s.scale, s.rotation, s.flip_h, s.flip_v) };
            let (x, y) = m.map(h[0], h[1]);
            Some([x, y])
        }
        _ => Some(src),
    }
}
