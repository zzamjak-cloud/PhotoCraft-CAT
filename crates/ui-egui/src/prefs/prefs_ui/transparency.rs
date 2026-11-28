//! 환경설정 › 투명도 및 색상 범위 UI.
//!
//! 포토샵의 Transparency & Gamut 패널처럼 격자 크기·격자 색상·타일 색상 견본 옆에
//! 실제 사용되는 체크 무늬 미리보기를 배치하고, 프리셋 색상에서 타일 색을 바로 편집하면
//! 자동으로 사용자 정의(`custom`)로 전환되어 시각적으로 확인하며 색을 맞출 수 있게 한다.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2, pos2, vec2};
use serde_json::{Map, Value, json};

use super::fields::{PickRequest, choice_label, field_visible, hex_field, humanize, pref_group, request_pick};
use super::{Tokens, prefs};

/// 미리보기 영역 크기(포인트): 큰 격자(16 pt)도 여러 칸이 한눈에 보이면서 좌측 4행 높이와 맞춘다.
const PREVIEW_SIZE: Vec2 = vec2(148.0, 114.0);

/// 미리보기 2×2 반복 텍스처 캐시 키.
fn preview_texture_id() -> egui::Id {
    egui::Id::unique("prefs-transparency-checker-preview")
}

/// JSON 편집 초안에서 `TransparencyAndGamut` 값을 복원한다.
pub(super) fn from_object(obj: &Map<String, Value>) -> prefs::TransparencyAndGamut {
    serde_json::from_value(Value::Object(obj.clone())).unwrap_or_default()
}

fn format_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// 두 타일 색으로 2×2 최근접 반복 체크 텍스처를 캐시해 반환한다.
fn preview_texture(ctx: &egui::Context, colors: [[u8; 3]; 2]) -> TextureHandle {
    if let Some((cached, handle)) = ctx.data(|d| d.get_temp::<([[u8; 3]; 2], TextureHandle)>(preview_texture_id()))
        && cached == colors
    {
        return handle;
    }
    let a = Color32::from_rgb(colors[0][0], colors[0][1], colors[0][2]);
    let b = Color32::from_rgb(colors[1][0], colors[1][1], colors[1][2]);
    let image = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
    let options = TextureOptions {
        magnification: egui::TextureFilter::Nearest,
        minification: egui::TextureFilter::Nearest,
        wrap_mode: egui::TextureWrapMode::Repeat,
        mipmap_mode: None,
    };
    let handle = ctx.load_texture("prefs-transparency-checker-preview", image, options);
    ctx.data_mut(|d| d.insert_temp(preview_texture_id(), (colors, handle.clone())));
    handle
}

/// 현재 선택된 격자 크기와 두 타일 색을 실제 캔버스와 동일한 비율·색상으로 그린다.
fn checker_preview(ui: &mut egui::Ui, settings: &prefs::TransparencyAndGamut) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(PREVIEW_SIZE, Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, tl!("Preview")));
    let inner = rect.shrink(1.0);
    let colors = settings.colors();
    match settings.square() {
        Some(sq) => {
            // GPU 캔버스와 동일하게 물리 픽셀 단위로 칸 크기를 맞춰 경계가 번지지 않게 한다.
            let ppp = ui.ctx().pixels_per_point().max(1.0);
            let sq = (sq * ppp).round().max(1.0) / ppp;
            let tex = preview_texture(ui.ctx(), colors);
            let uv = Rect::from_min_max(Pos2::ZERO, pos2(inner.width() / (2.0 * sq), inner.height() / (2.0 * sq)));
            ui.painter().add(egui::epaint::RectShape::filled(inner, t.radius_sm, Color32::WHITE).with_texture(tex.id(), uv));
        }
        None => {
            ui.painter().rect_filled(inner, t.radius_sm, Color32::WHITE);
        }
    }
    ui.painter().rect_stroke(rect, t.radius_sm, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    let tip = match settings.square() {
        Some(_) => format!("{} / {}", format_hex(colors[0]), format_hex(colors[1])),
        None => tl!("None").to_string(),
    };
    resp.on_hover_text(tip);
}

/// 투명도 타일 색상(밝은 타일 / 어두운 타일) 한 행을 그린다.
///
/// 현재 프리셋(예: Dark)이 선택되어 있어도 실제 화면에 쓰이는 타일 색을 견본과 hex에 표시하며,
/// 색을 편집하면 다른 쪽 타일 색을 유지한 채 `gridColors`를 `custom`으로 전환한다.
fn tile_color_row(ui: &mut egui::Ui, obj: &mut Map<String, Value>, key: &str, enabled: bool) {
    let t = Tokens::get(ui.ctx());
    let path = format!("transparencyAndGamut.{key}");
    let human = humanize(key);
    let label = tl!(&human).to_string();
    let settings = from_object(obj);
    let [active_light, active_dark] = settings.colors();
    let is_custom = settings.grid_colors == prefs::CheckerColors::Custom;
    let is_light = key == "customLight";
    let active = if is_light { active_light } else { active_dark };
    let other_key = if is_light { "customDark" } else { "customLight" };
    let other_active = if is_light { active_dark } else { active_light };

    ui.label(RichText::new(&label).color(if enabled { t.text_dim } else { t.text_faint }));
    ui.add_enabled_ui(enabled, |ui| {
        let mut rgb = active;
        ui.horizontal(|ui| {
            if crate::widgets::color_swatch_button(ui, Color32::from_rgb(rgb[0], rgb[1], rgb[2]), &label).clicked() {
                if is_custom {
                    request_pick(ui, PickRequest::simple(path.clone(), label.clone()));
                } else {
                    // 프리셋에서 색상 피커를 열면 현재 활성 타일 색으로 시작하고,
                    // 실시간 미리보기·확정 시에만 custom으로 전환해 취소 시 프리셋을 보존한다.
                    let also = vec![
                        ("/values/transparencyAndGamut/gridColors".into(), json!("custom")),
                        (format!("/values/transparencyAndGamut/{other_key}"), json!(format_hex(other_active))),
                    ];
                    request_pick(ui, PickRequest { path: path.clone(), label: label.clone(), initial: Some(format_hex(active)), also });
                }
            }
            hex_field(ui, &path, &mut rgb);
        });
        if rgb != active {
            obj.insert("gridColors".into(), json!("custom"));
            obj.insert(key.into(), json!(format_hex(rgb)));
            if !is_custom {
                obj.insert(other_key.into(), json!(format_hex(other_active)));
            }
        }
    });
    ui.end_row();
}

/// 투명도 설정 좌측 컨트롤 그리드를 그린다.
fn transparency_controls(ui: &mut egui::Ui, obj: &mut Map<String, Value>, rows: &[&str]) {
    let t = Tokens::get(ui.ctx());
    egui::Grid::new("prefs-grid-transparency").num_columns(2).spacing([14.0, 7.0]).min_col_width(130.0).show(ui, |ui| {
        for &k in rows {
            match k {
                "gridSize" => {
                    let human = humanize(k);
                    let label = tl!(&human).to_string();
                    let path = "transparencyAndGamut.gridSize";
                    ui.label(RichText::new(&label).color(t.text_dim));
                    let opts = prefs::choices(path).unwrap_or(&[]);
                    let labels: Vec<String> = opts.iter().map(|o| choice_label(o)).collect();
                    let pairs: Vec<(String, &str)> = opts.iter().map(|o| o.to_string()).zip(labels.iter().map(String::as_str)).collect();
                    let mut cur = obj.get(k).and_then(Value::as_str).unwrap_or("medium").to_string();
                    crate::widgets::dropdown(ui, "pref-transparencyAndGamut.gridSize", &label, &mut cur, &pairs, 220.0);
                    obj.insert(k.into(), json!(cur));
                    ui.end_row();
                }
                "gridColors" => {
                    let enabled = obj.get("gridSize").and_then(Value::as_str) != Some("none");
                    let human = humanize(k);
                    let label = tl!(&human).to_string();
                    let path = "transparencyAndGamut.gridColors";
                    ui.label(RichText::new(&label).color(if enabled { t.text_dim } else { t.text_faint }));
                    ui.add_enabled_ui(enabled, |ui| {
                        let opts = prefs::choices(path).unwrap_or(&[]);
                        let labels: Vec<String> = opts.iter().map(|o| choice_label(o)).collect();
                        let pairs: Vec<(String, &str)> = opts.iter().map(|o| o.to_string()).zip(labels.iter().map(String::as_str)).collect();
                        let mut cur = obj.get(k).and_then(Value::as_str).unwrap_or("light").to_string();
                        crate::widgets::dropdown(ui, "pref-transparencyAndGamut.gridColors", &label, &mut cur, &pairs, 220.0);
                        obj.insert(k.into(), json!(cur));
                    });
                    ui.end_row();
                }
                "customLight" | "customDark" => {
                    let enabled = obj.get("gridSize").and_then(Value::as_str) != Some("none");
                    tile_color_row(ui, obj, k, enabled);
                }
                _ => {}
            }
        }
    });
}

/// 투명도 및 색상 범위 섹션: 포토샵처럼 투명도 설정(실시간 체크 미리보기 포함)과 색상 범위 경고로 묶는다.
pub(super) fn transparency_and_gamut_groups(ui: &mut egui::Ui, obj: &mut Map<String, Value>, keys: &[String], query: &str) {
    const TRANSPARENCY: [&str; 4] = ["gridSize", "gridColors", "customLight", "customDark"];
    const GAMUT: [&str; 2] = ["gamutWarningColor", "gamutWarningOpacity"];

    let before = obj.clone();
    let shown = |k: &str| keys.iter().any(|key| key == k) && field_visible("transparencyAndGamut", obj, k, query);
    let transparency: Vec<&str> = TRANSPARENCY.into_iter().filter(|k| shown(k)).collect();
    let gamut: Vec<&str> = GAMUT.into_iter().filter(|k| shown(k)).collect();

    if !transparency.is_empty() {
        let t = Tokens::get(ui.ctx());
        pref_group(ui, "Transparency", |ui| {
            let side_by_side = ui.available_width() >= 520.0;
            if side_by_side {
                ui.horizontal_top(|ui| {
                    transparency_controls(ui, obj, &transparency);
                    ui.add_space(20.0);
                    let preview = from_object(obj);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tl!("Preview")).color(t.text_dim));
                        ui.add_space(2.0);
                        checker_preview(ui, &preview);
                    });
                });
            } else {
                transparency_controls(ui, obj, &transparency);
                ui.add_space(8.0);
                let preview = from_object(obj);
                ui.label(RichText::new(tl!("Preview")).color(t.text_dim));
                ui.add_space(2.0);
                checker_preview(ui, &preview);
            }
        });
    }

    if !gamut.is_empty() {
        let t = Tokens::get(ui.ctx());
        pref_group(ui, "Gamut Warning", |ui| {
            egui::Grid::new("prefs-grid-gamut").num_columns(2).spacing([14.0, 7.0]).min_col_width(130.0).show(ui, |ui| {
                for &k in &gamut {
                    let path = format!("transparencyAndGamut.{k}");
                    let human = humanize(k);
                    let label = tl!(&human).to_string();
                    match k {
                        "gamutWarningColor" => {
                            ui.label(RichText::new(&label).color(t.text_dim));
                            let s = obj.get(k).and_then(Value::as_str).unwrap_or("#808080");
                            let mut rgb = prefs::parse_hex(s).unwrap_or([128, 128, 128]);
                            ui.horizontal(|ui| {
                                if crate::widgets::color_swatch_button(ui, Color32::from_rgb(rgb[0], rgb[1], rgb[2]), &label).clicked() {
                                    request_pick(ui, PickRequest::simple(path.clone(), label.clone()));
                                }
                                hex_field(ui, &path, &mut rgb);
                            });
                            obj.insert(k.into(), json!(format_hex(rgb)));
                            ui.end_row();
                        }
                        "gamutWarningOpacity" => {
                            let value_label = ui.label(RichText::new(&label).color(t.text_dim));
                            let (lo, hi) = prefs::range(&path).unwrap_or((1.0, 100.0));
                            let mut x = obj.get(k).and_then(Value::as_i64).unwrap_or(100);
                            ui.add(egui::DragValue::new(&mut x).range(lo as i64..=hi as i64).suffix("%").custom_parser(crate::widgets::parse_num))
                                .labelled_by(value_label.id);
                            obj.insert(k.into(), json!(x));
                            ui.end_row();
                        }
                        _ => {}
                    }
                }
            });
        });
    }

    // 초안이 바뀌면 배경 캔버스 미리보기도 다음 프레임에 즉시 반영되도록 다시 그리기를 요청한다.
    if *obj != before {
        ui.ctx().request_repaint();
    }
}
