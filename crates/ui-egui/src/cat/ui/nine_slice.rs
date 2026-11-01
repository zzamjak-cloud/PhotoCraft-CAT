//! 9-Slice Images 탭 (CAT nineSlicePanel.js / JP_Plugin NineSlicePanel.tsx 대응).
//! 프로젝트 드롭다운 + "+"(이미지 추가) / "Del"(이미지 삭제) + 썸네일 그리드.
//! 카드 클릭 = 선택 영역 크기로 삽입, 카드의 "Slice" = 슬라이스 편집기(룰러 가이드 드래그), 우클릭 = 크기 지정 삽입.

use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, pos2, vec2};

use super::{DropdownEvent, DropdownItem, DropdownSpec, dropdown, text_button};
use crate::PhotocraftApp;
use crate::cat::features::nine_slice_panel::{self, ALLOWED_EXTENSIONS, DEFAULT_PROJECT, NineSliceImage};
use crate::cat::lib::core::{alert, pick_files, prompt, prompt_size, run_safe};
use crate::cat::lib::pixels;

const CARD_MAX_WIDTH: f32 = 76.0;
const CARD_GAP: f32 = 6.0;
const MIN_PER_ROW: f32 = 3.0;
const GUIDE_COLOR: Color32 = Color32::from_rgb(0, 200, 255);
const HANDLE_COLOR: Color32 = Color32::from_rgb(0, 160, 220);
const RULER: f32 = 16.0;
const HANDLE: f32 = 14.0;
const HIT: f32 = 10.0;

/// 선택된 카드 ID 를 보관하는 egui 메모리 키
fn selected_id() -> egui::Id {
    egui::Id::unique("cat-nine-slice-selected")
}

/// 열린 슬라이스 편집기 상태 키
fn editor_id() -> egui::Id {
    egui::Id::unique("cat-nine-slice-editor")
}

/// 썸네일 텍스처 캐시 키 (이미지 ID 별; 실패하면 None 을 기억해 매 프레임 재시도하지 않는다)
fn texture_id(image_id: &str) -> egui::Id {
    egui::Id::unique(("cat-nine-slice-tex", image_id))
}

/// 슬라이스 편집기 상태
#[derive(Clone, Debug, PartialEq)]
struct EditorState {
    id: String,
    name: String,
    w: u32,
    h: u32,
    l: u32,
    r: u32,
    t: u32,
    b: u32,
    /// 드래그 중인 가이드 ('l' | 'r' | 't' | 'b')
    dragging: Option<char>,
}

/// 보관 이미지 텍스처 + 원본 크기 (읽지 못하면 None)
fn texture(app: &PhotocraftApp, ctx: &egui::Context, m: &NineSliceImage) -> Option<(TextureHandle, [u32; 2])> {
    let key = texture_id(&m.id);
    if let Some(cached) = ctx.data(|d| d.get_temp::<Option<(TextureHandle, [u32; 2])>>(key)) {
        return cached;
    }
    let loaded = nine_slice_panel::image_path(app, m).and_then(|p| pixels::load_image_file(&p).ok()).map(|img| {
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.data);
        let tex = ctx.load_texture(format!("cat-ns-{}", m.id), color, egui::TextureOptions::NEAREST);
        (tex, [img.width, img.height])
    });
    ctx.data_mut(|d| d.insert_temp(key, loaded.clone()));
    loaded
}

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let projects = nine_slice_panel::projects(app);
    let current = nine_slice_panel::current_project(app);
    let items: Vec<DropdownItem> = projects.iter().map(|p| DropdownItem { label: p.clone(), removable: p != DEFAULT_PROJECT }).collect();
    let sel_id = selected_id();
    let mut selected: Option<String> = ui.ctx().data(|d| d.get_temp(sel_id)).flatten();
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 72.0).max(80.0);
        let spec = DropdownSpec { id: "nsProject", selected: &current, placeholder: DEFAULT_PROJECT, tooltip: "Project category", width, custom: true };
        match dropdown(ui, spec, &items) {
            DropdownEvent::Select(i) => {
                if let Some(p) = projects.get(i) {
                    run_safe(app, "Project", |app| nine_slice_panel::set_current_project(app, p));
                    selected = None;
                }
            }
            DropdownEvent::Remove(i) => {
                if let Some(p) = projects.get(i) {
                    run_safe(app, "Project", |app| nine_slice_panel::remove_project(app, p));
                }
            }
            DropdownEvent::Custom => prompt(app, "새 프로젝트 이름", "", |app, name| {
                if !name.trim().is_empty() {
                    run_safe(app, "Project", |app| nine_slice_panel::add_project(app, name.trim()));
                }
            }),
            DropdownEvent::None => {}
        }
        if text_button(ui, "+", "Add images").clicked() {
            let project = current.clone();
            pick_files(app, &ALLOWED_EXTENSIONS, true, move |app, paths| {
                if let Some((_, skipped)) = run_safe(app, "Add images", |app| nine_slice_panel::register_files(app, &paths, &project))
                    && skipped > 0
                {
                    alert(app, nine_slice_panel::skipped_message(skipped));
                }
            });
        }
        if text_button(ui, "Del", "Remove image").clicked() {
            let library = nine_slice_panel::load_library(app);
            match selected.clone().filter(|id| library.images.iter().any(|m| &m.id == id)) {
                Some(id) => {
                    run_safe(app, "Remove image", |app| nine_slice_panel::delete_image(app, &id));
                    ui.ctx().data_mut(|d| d.remove::<Option<(TextureHandle, [u32; 2])>>(texture_id(&id)));
                    selected = None;
                }
                None => alert(app, "삭제할 이미지를 먼저 선택해주세요."),
            }
        }
    });
    ui.separator();
    let images: Vec<NineSliceImage> = nine_slice_panel::load_library(app).images.into_iter().filter(|m| m.project == current).collect();
    if images.is_empty() {
        ui.weak("+ 버튼으로 이미지를 등록하세요");
    }
    // 그리드: 최소 3열, 카드 최대 76px (CAT 과 같은 계산)
    let width = ui.available_width() - 2.0;
    let per_row = MIN_PER_ROW.max((width / (CARD_MAX_WIDTH + CARD_GAP)).floor());
    let card_w = CARD_MAX_WIDTH.min(((width - per_row * CARD_GAP) / per_row).floor()).max(30.0);
    ui.spacing_mut().item_spacing = vec2(CARD_GAP, CARD_GAP);
    ui.horizontal_wrapped(|ui| {
        for m in &images {
            let on = selected.as_deref() == Some(m.id.as_str());
            match card(app, ui, m, card_w, on) {
                CardEvent::Click => {
                    selected = Some(m.id.clone());
                    let id = m.id.clone();
                    // 선택 영역이 있으면 즉시 삽입, 없으면 선택만 (오류 문구는 CAT 처럼 그대로 알림)
                    if let Err(e) = nine_slice_panel::run(app, &id) {
                        alert(app, e);
                    }
                }
                CardEvent::Slice => {
                    selected = Some(m.id.clone());
                    open_editor(app, ui.ctx(), m);
                }
                CardEvent::Size => {
                    selected = Some(m.id.clone());
                    let id = m.id.clone();
                    prompt_size(app, false, ["W", "H"], move |app, w, h| {
                        if let Err(e) = nine_slice_panel::insert_with_size(app, &id, w, h) {
                            alert(app, e);
                        }
                    });
                }
                CardEvent::None => {}
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(sel_id, selected));
    slice_editor(app, ui.ctx());
}

enum CardEvent {
    None,
    Click,
    Slice,
    Size,
}

/// 카드: 썸네일 + 이름 + 슬라이스 버튼.
/// 카드 클릭 영역을 먼저 등록해야 나중에 그린 "Slice" 버튼이 위에서 클릭을 받는다.
fn card(app: &PhotocraftApp, ui: &mut egui::Ui, m: &NineSliceImage, size: f32, selected: bool) -> CardEvent {
    let tex = texture(app, ui.ctx(), m);
    let mut event = CardEvent::None;
    let thumb = (size - 10.0).max(24.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(size, thumb + 46.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Insert {}", m.name)));
    let resp = resp.on_hover_text(format!("{}\n클릭: 선택 영역에 삽입 · 우클릭: 크기 지정 삽입", m.name));
    let stroke = if selected { Stroke::new(1.0, Color32::from_rgb(20, 115, 230)) } else { Stroke::new(1.0, Color32::from_white_alpha(30)) };
    let fill = if selected { Color32::from_rgba_unmultiplied(20, 115, 230, 46) } else { Color32::TRANSPARENT };
    ui.painter().rect(rect, 4.0, fill, stroke, StrokeKind::Inside);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(4.0)).layout(egui::Layout::top_down(egui::Align::Center)));
    child.spacing_mut().item_spacing = vec2(2.0, 2.0);
    let (thumb_rect, _) = child.allocate_exact_size(vec2(thumb, thumb), Sense::hover());
    child.painter().rect_filled(thumb_rect, 0.0, Color32::from_black_alpha(50));
    if let Some((tex, [w, h])) = &tex {
        let s = (thumb / *w as f32).min(thumb / *h as f32);
        let r = Rect::from_center_size(thumb_rect.center(), vec2(*w as f32 * s, *h as f32 * s));
        child.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    let name = if tex.is_some() { m.name.clone() } else { format!("{} (파일 없음)", m.name) };
    child.add(egui::Label::new(egui::RichText::new(name).size(10.0)).truncate().selectable(false).sense(Sense::hover()));
    let has_values = m.l + m.r + m.t + m.b > 0;
    let label = if has_values { format!("{}·{}·{}·{}", m.l, m.r, m.t, m.b) } else { "Slice".into() };
    if child.add(egui::Button::new(egui::RichText::new(label).size(10.0)).small()).on_hover_text("Edit 9-slice guides").clicked() {
        event = CardEvent::Slice;
    }
    if matches!(event, CardEvent::None) {
        if resp.clicked() {
            event = CardEvent::Click;
        } else if resp.secondary_clicked() {
            event = CardEvent::Size;
        }
    }
    event
}

fn open_editor(app: &mut PhotocraftApp, ctx: &egui::Context, m: &NineSliceImage) {
    let Some((_, [w, h])) = texture(app, ctx, m) else {
        alert(app, "이미지 파일을 찾을 수 없습니다. 이미지를 다시 등록해주세요.");
        return;
    };
    if w == 0 || h == 0 {
        alert(app, "이미지 크기를 읽을 수 없습니다.");
        return;
    }
    // 반대편 가이드와 최소 1px 간격 유지
    let l = m.l.min(w - 1);
    let r = m.r.min(w - 1 - l);
    let t = m.t.min(h - 1);
    let b = m.b.min(h - 1 - t);
    let st = EditorState { id: m.id.clone(), name: m.name.clone(), w, h, l, r, t, b, dragging: None };
    ctx.data_mut(|d| d.insert_temp(editor_id(), Some(st)));
}

/// 가이드 위치 갱신 (이미지 픽셀 좌표 x/y)
fn move_guide(st: &mut EditorState, key: char, x: f32, y: f32) {
    let (w, h) = (st.w as i64, st.h as i64);
    let (x, y) = (x.round() as i64, y.round() as i64);
    let (l, r, t, b) = (i64::from(st.l), i64::from(st.r), i64::from(st.t), i64::from(st.b));
    match key {
        'l' => st.l = x.min(w - r - 1).max(0) as u32,
        'r' => st.r = (w - x).min(w - l - 1).max(0) as u32,
        't' => st.t = y.min(h - b - 1).max(0) as u32,
        _ => st.b = (h - y).min(h - t - 1).max(0) as u32,
    }
}

/// 슬라이스 편집 모달 (룰러 가이드 드래그 방식)
fn slice_editor(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let key = editor_id();
    let Some(mut st) = ctx.data(|d| d.get_temp::<Option<EditorState>>(key)).flatten() else { return };
    let Some(meta) = nine_slice_panel::load_library(app).images.into_iter().find(|m| m.id == st.id) else {
        ctx.data_mut(|d| d.remove::<Option<EditorState>>(key));
        return;
    };
    let tex = texture(app, ctx, &meta);
    // 표시 배율: 큰 이미지는 축소, 작은 이미지는 확대
    let scale = (360.0 / st.w as f32).min(280.0 / st.h as f32).min(8.0);
    let disp = vec2((st.w as f32 * scale).round().max(1.0), (st.h as f32 * scale).round().max(1.0));
    let mut close: Option<bool> = None;
    egui::Modal::new(egui::Id::unique("cat-nine-slice-editor-modal")).show(ctx, |ui| {
        ui.set_width((RULER + disp.x + 24.0).max(320.0));
        ui.label(format!("{} ({}x{}px)", st.name, st.w, st.h));
        ui.weak("룰러에서 안쪽으로 드래그하여 가이드 위치를 설정하세요");
        ui.vertical_centered(|ui| {
            let (outer, _) = ui.allocate_exact_size(vec2(RULER + disp.x, RULER + disp.y), Sense::hover());
            let stage = Rect::from_min_size(outer.min + vec2(RULER, RULER), disp);
            let ruler_top = Rect::from_min_size(pos2(stage.min.x, outer.min.y), vec2(disp.x, RULER));
            let ruler_left = Rect::from_min_size(pos2(outer.min.x, stage.min.y), vec2(RULER, disp.y));
            let p = ui.painter_at(outer);
            p.rect_filled(ruler_top, 0.0, Color32::from_gray(64));
            p.rect_filled(ruler_left, 0.0, Color32::from_gray(64));
            p.rect_filled(stage, 0.0, Color32::from_gray(46));
            if let Some((tex, _)) = &tex {
                p.image(tex.id(), stage, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            let to_img = |pos: Pos2| ((pos.x - stage.min.x) / scale, (pos.y - stage.min.y) / scale);
            let lx = stage.min.x + (st.l as f32 * scale).round();
            let rx = stage.max.x - (st.r as f32 * scale).round();
            let ty = stage.min.y + (st.t as f32 * scale).round();
            let by = stage.max.y - (st.b as f32 * scale).round();

            // 드래그 영역: 가이드 선(HIT 폭), 핸들, 룰러
            let hit_v = |x: f32| Rect::from_min_max(pos2(x - HIT / 2.0, stage.min.y), pos2(x + HIT / 2.0, stage.max.y));
            let hit_h = |y: f32| Rect::from_min_max(pos2(stage.min.x, y - HIT / 2.0), pos2(stage.max.x, y + HIT / 2.0));
            let clamp_x = |x: f32| stage.min.x + (x - stage.min.x - HANDLE / 2.0).clamp(0.0, (disp.x - HANDLE).max(0.0));
            let clamp_y = |y: f32| stage.min.y + (y - stage.min.y - HANDLE / 2.0).clamp(0.0, (disp.y - HANDLE).max(0.0));
            let handles = [
                ('l', Rect::from_min_size(pos2(clamp_x(lx), outer.min.y), vec2(HANDLE, RULER - 1.0)), hit_v(lx)),
                ('r', Rect::from_min_size(pos2(clamp_x(rx), outer.min.y), vec2(HANDLE, RULER - 1.0)), hit_v(rx)),
                ('t', Rect::from_min_size(pos2(outer.min.x, clamp_y(ty)), vec2(RULER - 1.0, HANDLE)), hit_h(ty)),
                ('b', Rect::from_min_size(pos2(outer.min.x, clamp_y(by)), vec2(RULER - 1.0, HANDLE)), hit_h(by)),
            ];
            for (k, handle, line) in handles {
                for (i, r) in [handle, line].into_iter().enumerate() {
                    let resp = ui.interact(r, ui.scope_id().with(("ns-guide", k, i)), Sense::drag());
                    let resp = resp.on_hover_cursor(if matches!(k, 'l' | 'r') { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
                    if resp.drag_started() {
                        st.dragging = Some(k);
                    }
                }
            }
            // 룰러를 누르면 가까운 쪽 가이드를 잡아 그 위치부터 드래그
            for (vertical, r) in [(true, ruler_top), (false, ruler_left)] {
                let resp = ui.interact(r, ui.scope_id().with(("ns-ruler", vertical)), Sense::click_and_drag());
                if (resp.drag_started() || resp.clicked())
                    && let Some(pos) = resp.interact_pointer_pos()
                {
                    let (x, y) = to_img(pos);
                    let k = if vertical {
                        if (x - st.l as f32).abs() <= (st.w as f32 - st.r as f32 - x).abs() { 'l' } else { 'r' }
                    } else if (y - st.t as f32).abs() <= (st.h as f32 - st.b as f32 - y).abs() {
                        't'
                    } else {
                        'b'
                    };
                    move_guide(&mut st, k, x, y);
                    if resp.drag_started() {
                        st.dragging = Some(k);
                    }
                }
            }
            if let Some(k) = st.dragging {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                    let (x, y) = to_img(pos);
                    move_guide(&mut st, k, x, y);
                }
                if !ui.input(|i| i.pointer.any_down()) {
                    st.dragging = None;
                }
            }

            // 다시 계산한 위치로 그리기
            let lx = stage.min.x + (st.l as f32 * scale).round();
            let rx = stage.max.x - (st.r as f32 * scale).round();
            let ty = stage.min.y + (st.t as f32 * scale).round();
            let by = stage.max.y - (st.b as f32 * scale).round();
            let guide = Stroke::new(1.0, GUIDE_COLOR);
            for x in [lx, rx] {
                p.line_segment([pos2(x, stage.min.y), pos2(x, stage.max.y)], guide);
            }
            for y in [ty, by] {
                p.line_segment([pos2(stage.min.x, y), pos2(stage.max.x, y)], guide);
            }
            for (k, r) in [
                ('L', Rect::from_min_size(pos2(clamp_x(lx), outer.min.y), vec2(HANDLE, RULER - 1.0))),
                ('R', Rect::from_min_size(pos2(clamp_x(rx), outer.min.y), vec2(HANDLE, RULER - 1.0))),
                ('T', Rect::from_min_size(pos2(outer.min.x, clamp_y(ty)), vec2(RULER - 1.0, HANDLE))),
                ('B', Rect::from_min_size(pos2(outer.min.x, clamp_y(by)), vec2(RULER - 1.0, HANDLE))),
            ] {
                p.rect(r, 2.0, HANDLE_COLOR, Stroke::NONE, StrokeKind::Inside);
                p.text(r.center(), egui::Align2::CENTER_CENTER, k, egui::FontId::proportional(9.0), Color32::WHITE);
            }
        });
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            ui.colored_label(Color32::from_rgb(127, 178, 229), format!("L: {}   R: {}   T: {}   B: {}", st.l, st.r, st.t, st.b));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                close = Some(false);
            }
            if ui.button("Save").clicked() {
                close = Some(true);
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = Some(false);
        }
    });
    match close {
        Some(save) => {
            if save {
                let (l, r, t, b) = (st.l, st.r, st.t, st.b);
                run_safe(app, "9-Slice", |app| nine_slice_panel::update_slice_values(app, &meta.id, l, r, t, b));
            }
            ctx.data_mut(|d| d.remove::<Option<EditorState>>(key));
        }
        None => {
            ctx.data_mut(|d| d.insert_temp(key, Some(st)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guides_keep_one_pixel_gap() {
        let mut st = EditorState { id: "x".into(), name: "x".into(), w: 30, h: 20, l: 0, r: 0, t: 0, b: 0, dragging: None };
        move_guide(&mut st, 'l', 12.4, 0.0);
        move_guide(&mut st, 'r', 5.0, 0.0);
        assert_eq!((st.l, st.r), (12, 17));
        move_guide(&mut st, 'r', -10.0, 0.0);
        assert_eq!(st.r, 17, "왼쪽 가이드를 넘지 않음");
        move_guide(&mut st, 't', 0.0, 30.0);
        assert_eq!(st.t, 19);
        move_guide(&mut st, 'b', 0.0, 0.0);
        assert_eq!(st.b, 0, "위쪽 가이드와 1px 간격");
    }

    #[test]
    fn nine_slice_tab_edits_slices_and_inserts_on_card_click() {
        use egui_kittest::kittest::Queryable;
        use serde_json::json;

        use crate::cat::CatState;
        use crate::cat::features::nine_slice_panel::tests::write_sample_png;
        use crate::cat::lib::{layer, select};

        let dir = tempfile::tempdir().unwrap();
        let cat_dir = dir.path().join("cat");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(64).build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.cat = CatState::load(Some(cat_dir.clone()));
            app.ui.panels.color = false;
            app.ui.panels.properties = false;
            app.ui.dock.heights.insert(crate::dock::Group::Cat, 760.0);
            let png = write_sample_png(&mut app, &src_dir);
            nine_slice_panel::register_files(&mut app, &[png], DEFAULT_PROJECT).unwrap();
            app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
            app
        });
        h.ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
        h.run_steps(6);
        h.get_by_label("9-Slice Images").click();
        h.run_steps(4);

        // 슬라이스 편집기 열기 → 저장
        h.get_by_label("Slice").click();
        h.run_steps(4);
        assert!(h.ctx.data(|d| d.get_temp::<Option<EditorState>>(editor_id())).flatten().is_some(), "편집기 열림");
        if let Ok(out) = std::env::var("CAT_SNAPSHOT_DIR") {
            h.render().unwrap().save(std::path::Path::new(&out).join("cat-nine-slice-editor.png")).unwrap();
        }
        h.get_by_label("Save").click();
        h.run_steps(4);
        assert!(h.ctx.data(|d| d.get_temp::<Option<EditorState>>(editor_id())).flatten().is_none(), "편집기 닫힘");

        // 선택 영역이 있을 때 카드 클릭 → 그 크기로 삽입
        select::select_rect(h.state_mut(), 10, 10, 80, 40).unwrap();
        h.get_by_label("Insert btn").click();
        h.run_steps(4);
        let res = layer::active_layer(h.state()).unwrap();
        assert_eq!(res.name, "btn_80x40");
        assert_eq!(res.bounds, Some(layer::Bounds { x: 10, y: 10, w: 80, h: 40 }));
        if let Ok(out) = std::env::var("CAT_SNAPSHOT_DIR") {
            h.render().unwrap().save(std::path::Path::new(&out).join("cat-nine-slice-panel.png")).unwrap();
        }
    }
}
