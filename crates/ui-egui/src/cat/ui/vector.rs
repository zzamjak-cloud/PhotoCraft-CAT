//! VECTOR 섹션 (CAT 에 없던 신설 섹션): Pathfinder · Simplify · Smooth · Offset path · Image Trace 창.
//! 엔진은 벤더링한 VectorCraft(`vectorcraft_pathops`, `vectorcraft_trace`). 아이콘은 egui painter 로 그린다.

use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};
use vectorcraft_pathops::PathfinderOp;

use super::{BUTTON, section};
use crate::PhotocraftApp;
use crate::cat::features::image_trace::{self, Method, Mode, Palette, TraceParams, TraceSettings};
use crate::cat::features::vector_ops::{self, OFFSET_JOINS, PATHFINDER_OPS};
use crate::cat::lib::core::{notify, run_safe};

/// Image Trace 창 프리셋 (표시 이름, VectorCraft 프리셋 이름)
const TRACE_PRESETS: &[(&str, &str)] = &[
    ("Default (B&W)", "Default"),
    ("Logo", "Flat Logo"),
    ("B&W Logo", "Black and White Logo"),
    ("Line Art", "Line Art"),
    ("Outline", "Outline"),
    ("3 Colors", "3 Colors"),
    ("6 Colors", "6 Colors"),
    ("16 Colors", "16 Colors"),
    ("Auto Color", "Auto Color"),
    ("Shades of Gray", "Shades of Gray"),
];

/// Image Trace 창 상태
#[derive(Clone, Debug, PartialEq)]
pub struct TraceWindow {
    pub open: bool,
    /// CAT 원본 알고리즘 사용
    pub classic: bool,
    /// 선택한 프리셋 표시 이름 (값을 바꾸면 "Custom")
    pub preset: String,
    pub params: TraceParams,
    pub classic_settings: TraceSettings,
}

impl Default for TraceWindow {
    fn default() -> Self {
        Self { open: false, classic: false, preset: "Default (B&W)".into(), params: TraceParams::default(), classic_settings: TraceSettings::default() }
    }
}

/// VECTOR 섹션 상태 (저장하지 않는다)
#[derive(Clone, Debug, PartialEq)]
pub struct VectorState {
    /// Simplify 레벨 0(가장 적은 점)..100(원본에 가깝게)
    pub simplify_level: f64,
    /// Smooth 레벨 0..100
    pub smooth_level: f64,
    /// Offset 거리 (px, 음수면 안쪽)
    pub offset: f64,
    /// `OFFSET_JOINS` 인덱스
    pub offset_join: usize,
    pub trace: TraceWindow,
}

impl Default for VectorState {
    fn default() -> Self {
        Self { simplify_level: 50.0, smooth_level: 50.0, offset: 4.0, offset_join: 0, trace: TraceWindow::default() }
    }
}

/// Image Trace 창 열기 (EXPORT 의 Image trace 버튼도 이것을 쓴다)
pub fn open_trace_window(app: &mut PhotocraftApp) {
    app.cat.vector.trace.open = true;
}

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // 트레이스 창은 섹션이 접혀 있어도 그린다
    trace_window(app, ui.ctx());
    section(app, ui, "VECTOR", |app, ui| {
        ui.horizontal_wrapped(|ui| {
            for (op, label, tooltip) in PATHFINDER_OPS {
                if pathfinder_button(ui, *op, label, tooltip).clicked() {
                    run_safe(app, label, |app| vector_ops::run_pathfinder(app, *op));
                }
            }
        });
        ui.separator();
        // 좁은 도크에서도 Apply 버튼이 보이도록 슬라이더를 줄인다
        ui.spacing_mut().slider_width = 90.0;
        let v = &mut app.cat.vector;
        let mut action: Option<Action> = None;
        egui::Grid::new("cat-vector-grid").num_columns(3).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
            ui.label("Simplify");
            ui.add(egui::Slider::new(&mut v.simplify_level, 0.0..=100.0).max_decimals(0))
                .on_hover_text("0 = 기준점 최소, 100 = 원본에 가깝게 (50 = 1px 허용 오차)");
            if text_action(ui, "Apply", "Simplify").clicked() {
                action = Some(Action::Simplify(v.simplify_level));
            }
            ui.end_row();
            ui.label("Smooth");
            ui.add(egui::Slider::new(&mut v.smooth_level, 0.0..=100.0).max_decimals(0)).on_hover_text("0 = 거의 그대로, 100 = 가장 부드럽게");
            if text_action(ui, "Apply", "Smooth").clicked() {
                action = Some(Action::Smooth(v.smooth_level));
            }
            ui.end_row();
            ui.label("Offset");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut v.offset).speed(0.5).range(-500.0..=500.0).suffix(" px")).on_hover_text("오프셋 거리 (음수면 안쪽)");
                let join = OFFSET_JOINS.get(v.offset_join).map_or("Miter", |(n, _)| *n);
                craft_ui::ComboBox::from_id_salt("cat-offset-join").selected_text(join).width(64.0).show_ui(ui, |ui| {
                    for (i, (name, _)) in OFFSET_JOINS.iter().enumerate() {
                        ui.selectable_value(&mut v.offset_join, i, *name);
                    }
                });
            });
            if text_action(ui, "Apply", "Offset path").clicked() {
                action = Some(Action::Offset(v.offset, v.offset_join));
            }
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if super::icon_button(ui, "imagetracing", "Image trace").clicked() {
                open_trace_window(app);
            }
        });
        match action {
            Some(Action::Simplify(level)) => {
                run_safe(app, "Simplify", |app| vector_ops::run_simplify(app, level));
            }
            Some(Action::Smooth(level)) => {
                run_safe(app, "Smooth", |app| vector_ops::run_smooth(app, level));
            }
            Some(Action::Offset(d, j)) => {
                let join = OFFSET_JOINS.get(j).map_or(vectorcraft_pathops::Join::Miter, |(_, j)| *j);
                run_safe(app, "Offset path", |app| vector_ops::run_offset(app, d, join));
            }
            None => {}
        }
    });
}

enum Action {
    Simplify(f64),
    Smooth(f64),
    Offset(f64, usize),
}

/// 글자 버튼 (보이는 글자와 접근성 라벨·툴팁이 다를 수 있다)
fn text_action(ui: &mut egui::Ui, text: &str, label: &str) -> egui::Response {
    let resp = ui.add(egui::Button::new(text));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    resp.on_hover_text(label)
}

/// Pathfinder 버튼: 두 사각형(뒤=왼쪽 위, 앞=오른쪽 아래)의 결과 영역을 칠한 아이콘
fn pathfinder_button(ui: &mut egui::Ui, op: PathfinderOp, label: &str, tooltip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(BUTTON, BUTTON), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&resp);
        ui.painter().rect(rect, visuals.corner_radius, visuals.bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
        paint_pathfinder_icon(ui.painter(), rect.shrink(5.0), op, visuals.fg_stroke.color);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    resp.on_hover_text(tooltip)
}

fn paint_pathfinder_icon(p: &egui::Painter, r: Rect, op: PathfinderOp, fg: Color32) {
    let u = r.width() / 20.0;
    let at = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pos2(r.min.x + x0 * u, r.min.y + y0 * u), pos2(r.min.x + x1 * u, r.min.y + y1 * u));
    // 뒤 A = (0,0)-(13,13), 앞 B = (7,7)-(20,20), 겹침 = (7,7)-(13,13)
    let a_only = [at(0.0, 0.0, 13.0, 7.0), at(0.0, 7.0, 7.0, 13.0)];
    let b_only = [at(13.0, 7.0, 20.0, 20.0), at(7.0, 13.0, 13.0, 20.0)];
    let overlap = at(7.0, 7.0, 13.0, 13.0);
    let (full, dim) = (fg, fg.gamma_multiply(0.45));
    let none = Color32::TRANSPARENT;
    // (A 만, 겹침, B 만) 색
    let (ca, co, cb) = match op {
        PathfinderOp::Unite => (full, full, full),
        PathfinderOp::MinusFront => (full, none, none),
        PathfinderOp::Intersect => (none, full, none),
        PathfinderOp::Exclude => (full, none, full),
        PathfinderOp::MinusBack => (none, none, full),
        PathfinderOp::Crop => (none, full, none),
        PathfinderOp::Trim | PathfinderOp::Merge => (dim, full, full),
        _ => (dim, full, dim),
    };
    for (rects, c) in [(&a_only[..], ca), (std::slice::from_ref(&overlap), co), (&b_only[..], cb)] {
        for q in rects {
            p.rect_filled(*q, 0.0, c);
        }
    }
    let thin = Stroke::new(1.0, fg);
    let outline = |q: Rect| {
        p.rect_stroke(q, 0.0, thin, egui::StrokeKind::Inside);
    };
    match op {
        // 나뉜 조각 경계선
        PathfinderOp::Divide | PathfinderOp::Trim => {
            outline(at(0.0, 0.0, 13.0, 13.0));
            outline(at(7.0, 7.0, 20.0, 20.0));
        }
        // 잘라내는 쪽(앞) 윤곽만
        PathfinderOp::MinusFront | PathfinderOp::Crop => outline(at(7.0, 7.0, 20.0, 20.0)),
        PathfinderOp::MinusBack => outline(at(0.0, 0.0, 13.0, 13.0)),
        PathfinderOp::Intersect | PathfinderOp::Exclude => {
            outline(at(0.0, 0.0, 13.0, 13.0));
            outline(at(7.0, 7.0, 20.0, 20.0));
        }
        _ => {}
    }
}

/// 콤보 박스 (값 목록) → 바뀌었으면 true
fn combo<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, value: &mut T, items: &[(T, &str)]) -> bool {
    let before = *value;
    let text = items.iter().find(|(v, _)| v == value).map_or("", |(_, n)| *n);
    craft_ui::ComboBox::from_id_salt(("cat-trace", id)).selected_text(text).width(130.0).show_ui(ui, |ui| {
        for (v, n) in items {
            ui.selectable_value(value, *v, *n);
        }
    });
    *value != before
}

/// Image Trace 설정 창
fn trace_window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.cat.vector.trace.open {
        return;
    }
    let mut open = true;
    let mut run = false;
    let w = &mut app.cat.vector.trace;
    egui::Window::new("Image Trace").id(egui::Id::unique("cat-image-trace")).open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Engine");
            ui.radio_value(&mut w.classic, false, "VectorCraft").on_hover_text("VectorCraft Image Trace (기본)");
            ui.radio_value(&mut w.classic, true, "Classic").on_hover_text("CAT 원본 알고리즘");
        });
        ui.separator();
        if w.classic {
            egui::Grid::new("cat-trace-classic").num_columns(2).show(ui, |ui| {
                ui.label("Color Tolerance");
                ui.add(egui::Slider::new(&mut w.classic_settings.color_tolerance, 0.0..=100.0).max_decimals(0));
                ui.end_row();
                ui.label("Path Smoothness");
                ui.add(egui::Slider::new(&mut w.classic_settings.path_tolerance, 0.0..=5.0).max_decimals(1));
                ui.end_row();
            });
        } else if trace_params_ui(ui, &mut w.preset, &mut w.params) {
            w.preset = "Custom".into();
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Trace").on_hover_text("활성 레이어를 트레이스해 색별 셰이프 레이어를 만든다").clicked() {
                run = true;
            }
        });
    });
    let w = app.cat.vector.trace.clone();
    app.cat.vector.trace.open = open;
    if run {
        let r = if w.classic {
            run_safe(app, "Image trace", |app| image_trace::run(app, w.classic_settings))
        } else {
            run_safe(app, "Image trace", |app| image_trace::run_vector(app, &w.params))
        };
        if let Some(ids) = r {
            notify(app, format!("셰이프 레이어 {}개를 만들었습니다.", ids.len()));
        }
    }
}

/// VectorCraft 트레이스 설정 UI → 사용자가 값을 바꿨으면 true (프리셋 선택은 false)
fn trace_params_ui(ui: &mut egui::Ui, preset: &mut String, p: &mut TraceParams) -> bool {
    let mut changed = false;
    egui::Grid::new("cat-trace-params").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
        ui.label("Preset");
        craft_ui::ComboBox::from_id_salt("cat-trace-preset").selected_text(preset.as_str()).width(130.0).show_ui(ui, |ui| {
            for (label, name) in TRACE_PRESETS {
                if ui.selectable_label(preset == label, *label).clicked()
                    && let Some(params) = vectorcraft_trace::preset(name)
                {
                    *p = params;
                    *preset = (*label).into();
                }
            }
        });
        ui.end_row();
        ui.label("Mode");
        changed |= combo(
            ui,
            "mode",
            &mut p.mode,
            &[(Mode::BlackAndWhite, "Black and White"), (Mode::Grayscale, "Grayscale"), (Mode::Color, "Color"), (Mode::Logo, "Logo")],
        );
        ui.end_row();
        match p.mode {
            Mode::BlackAndWhite => {
                ui.label("Threshold");
                changed |= ui.add(egui::Slider::new(&mut p.threshold, 0..=255)).on_hover_text("이 밝기보다 어두운 픽셀이 검정").changed();
                ui.end_row();
            }
            Mode::Grayscale => {
                ui.label("Grays");
                changed |= ui.add(egui::Slider::new(&mut p.colors, 2..=64)).changed();
                ui.end_row();
            }
            Mode::Color => {
                ui.label("Palette");
                changed |=
                    combo(ui, "palette", &mut p.palette, &[(Palette::Limited, "Limited"), (Palette::FullTone, "Full Tone"), (Palette::Automatic, "Automatic")]);
                ui.end_row();
                if p.palette == Palette::Limited {
                    ui.label("Colors");
                    changed |= ui.add(egui::Slider::new(&mut p.colors, 2..=30)).changed();
                } else {
                    ui.label("Color Detail");
                    changed |= ui.add(egui::Slider::new(&mut p.color_detail, 0.0..=100.0).max_decimals(0)).changed();
                }
                ui.end_row();
            }
            Mode::Logo => {}
        }
        ui.label("Paths");
        changed |= ui.add(egui::Slider::new(&mut p.paths, 0.0..=100.0).max_decimals(0)).on_hover_text("높을수록 픽셀을 더 정확히 따른다").changed();
        ui.end_row();
        ui.label("Corners");
        changed |= ui.add(egui::Slider::new(&mut p.corners, 0.0..=100.0).max_decimals(0)).on_hover_text("높을수록 모서리를 더 많이 유지").changed();
        ui.end_row();
        ui.label("Noise");
        changed |= ui.add(egui::Slider::new(&mut p.noise, 1..=500).logarithmic(true).suffix(" px")).on_hover_text("이보다 작은 영역은 무시").changed();
        ui.end_row();
        ui.label("Method");
        changed |= combo(ui, "method", &mut p.method, &[(Method::Abutting, "Abutting"), (Method::Overlapping, "Overlapping")]);
        ui.end_row();
        ui.label("Create");
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut p.fills, "Fills").changed();
            changed |= ui.checkbox(&mut p.strokes, "Strokes").changed();
        });
        ui.end_row();
        if p.strokes {
            ui.label("Stroke");
            changed |=
                ui.add(egui::Slider::new(&mut p.stroke_width, 1.0..=100.0).max_decimals(1).suffix(" px")).on_hover_text("이보다 가는 부분을 선으로").changed();
            ui.end_row();
        }
        ui.label("Options");
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut p.ignore_white, "Ignore White").changed();
            changed |= ui.checkbox(&mut p.snap_curves_to_lines, "Snap Lines").on_hover_text("거의 직선인 곡선을 직선으로").changed();
        });
        ui.end_row();
    });
    // Fills/Strokes 둘 다 끄면 결과가 없다
    if !p.fills && !p.strokes {
        p.fills = true;
    }
    changed
}
