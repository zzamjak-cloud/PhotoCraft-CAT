//! 가이드: 그리드 가이드(+번호), 가이드 제거, 복사/붙여넣기, 가장자리 오프셋 가이드 (CAT addGuide.js 대응)

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::guide::{Guide, GuideAxis, clear_all_guides, get_guides, make_guide};
use crate::cat::lib::pixels::{RgbaImage, put_pixels_as_layer};
use crate::cat::lib::{doc, layer};

/// 번호 레이어 이름
pub const NUM_LAYER: &str = "Num";

/// 0~9 숫자 5x7 비트맵 글리프 (텍스트 엔진 없이 픽셀로 직접 그린다)
const DIGIT_GLYPHS: [[&str; 7]; 10] = [
    ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
    ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
    ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
    ["11111", "00010", "00100", "00010", "00001", "10001", "01110"],
    ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
    ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
    ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
    ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
    ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
    ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
];

/// 셀 번호(0..rows*cols-1, 행 우선)를 문서 크기 RGBA 에 검정 불투명 픽셀로 그린다
pub fn render_grid_numbers(doc_w: u32, doc_h: u32, rows: u32, cols: u32) -> RgbaImage {
    let mut img = RgbaImage::new(doc_w, doc_h);
    if rows == 0 || cols == 0 {
        return img;
    }
    let section_w = doc_w as f64 / cols as f64;
    let section_h = doc_h as f64 / rows as f64;
    let font_h = if rows < 10 {
        30
    } else if rows > 30 {
        14
    } else {
        18
    };
    let max_chars = (rows * cols).saturating_sub(1).to_string().len() as i64;
    let mut scale = ((font_h as f64 / 7.0).round() as i64).max(1);
    // 셀 안에 여유 있게 들어가도록 줄인다 (글리프 폭 5 + 자간 1)
    while scale > 1 && ((max_chars * 6 - 1) * scale) as f64 > section_w * 0.9 {
        scale -= 1;
    }
    while scale > 1 && (7 * scale) as f64 > section_h * 0.9 {
        scale -= 1;
    }
    let (w, h) = (doc_w as i64, doc_h as i64);
    let mut number = 0u32;
    for row in 0..rows {
        for col in 0..cols {
            let text = number.to_string();
            let text_w = (text.len() as i64 * 6 - 1) * scale;
            let mut pen_x = ((col as f64 + 0.5) * section_w - text_w as f64 / 2.0).round() as i64;
            let pen_y = ((row as f64 + 0.5) * section_h - (7 * scale) as f64 / 2.0).round() as i64;
            for ch in text.bytes() {
                let Some(glyph) = DIGIT_GLYPHS.get((ch - b'0') as usize) else { continue };
                for (gy, line) in glyph.iter().enumerate() {
                    for (gx, bit) in line.bytes().enumerate() {
                        if bit != b'1' {
                            continue;
                        }
                        for sy in 0..scale {
                            let py = pen_y + gy as i64 * scale + sy;
                            if py < 0 || py >= h {
                                continue;
                            }
                            for sx in 0..scale {
                                let px = pen_x + gx as i64 * scale + sx;
                                if px < 0 || px >= w {
                                    continue;
                                }
                                if let Some(a) = img.data.get_mut(((py * w + px) * 4 + 3) as usize) {
                                    *a = 255;
                                }
                            }
                        }
                    }
                }
                pen_x += 6 * scale;
            }
            number += 1;
        }
    }
    img
}

/// rows x cols 그리드 가이드 추가 (with_numbers: 셀 번호 "Num" 픽셀 레이어 포함). Undo 한 단계.
pub fn add_all_guides(app: &mut PhotocraftApp, with_numbers: bool, rows: u32, cols: u32) -> CatResult<()> {
    let d = doc::require_doc(app)?;
    let (rows, cols) = (rows.max(1), cols.max(1));
    with_history(app, |app| {
        let section_h = d.height as f64 / rows as f64;
        for i in 1..rows {
            make_guide(app, section_h * i as f64, GuideAxis::Horizontal)?;
        }
        let section_w = d.width as f64 / cols as f64;
        for i in 1..cols {
            make_guide(app, section_w * i as f64, GuideAxis::Vertical)?;
        }
        if with_numbers {
            let img = render_grid_numbers(d.width, d.height, rows, cols);
            put_pixels_as_layer(app, &img, NUM_LAYER, 0, 0)?;
        }
        Ok(())
    })
}

/// 가장자리 방향
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeDirection {
    Top,
    Bottom,
    Left,
    Right,
}

/// 가장자리 기준 오프셋 가이드 (layer_bound: 첫 선택 레이어 bounds 기준, 아니면 문서 기준).
/// CAT 과 같이 layer_bound 인데 선택 레이어(또는 픽셀)가 없으면 아무 것도 하지 않는다.
pub fn add_guide_from_edge(app: &mut PhotocraftApp, direction: EdgeDirection, value: f64, layer_bound: bool) -> CatResult<()> {
    let d = doc::require_doc(app)?;
    let (mut top, mut left, mut bottom, mut right) = (0.0, 0.0, d.height as f64, d.width as f64);
    if layer_bound {
        let Some(b) = layer::selected_layers(app)?.first().and_then(|l| l.bounds) else { return Ok(()) };
        (top, left, bottom, right) = (b.top() as f64, b.left() as f64, b.bottom() as f64, b.right() as f64);
    }
    match direction {
        EdgeDirection::Top => make_guide(app, top + value, GuideAxis::Horizontal),
        EdgeDirection::Bottom => make_guide(app, bottom - value, GuideAxis::Horizontal),
        EdgeDirection::Left => make_guide(app, left + value, GuideAxis::Vertical),
        EdgeDirection::Right => make_guide(app, right - value, GuideAxis::Vertical),
    }
}

/// "Num" 레이어와 모든 가이드 제거. Undo 한 단계.
pub fn clear_guides(app: &mut PhotocraftApp) -> CatResult<()> {
    doc::require_doc(app)?;
    with_history(app, |app| {
        layer::delete_layer_by_name(app, NUM_LAYER)?;
        clear_all_guides(app)
    })
}

/// 정수 좌표로 내린 현재 가이드
fn current_guides(app: &PhotocraftApp) -> CatResult<Vec<Guide>> {
    Ok(get_guides(app)?.into_iter().map(|g| Guide { position: g.position.trunc(), axis: g.axis }).collect())
}

/// 현재 문서의 모든 가이드 복사 (세션 동안 보관) → 복사한 개수
pub fn copy_guides(app: &mut PhotocraftApp) -> CatResult<usize> {
    let guides = current_guides(app)?;
    app.cat.copied_guides = guides;
    Ok(app.cat.copied_guides.len())
}

/// 복사한 가이드를 현재 문서에 붙여넣기 (같은 위치·방향 가이드는 건너뜀) → 추가한 개수. Undo 한 단계.
pub fn paste_guides(app: &mut PhotocraftApp) -> CatResult<usize> {
    if app.cat.copied_guides.is_empty() {
        return Ok(0);
    }
    let existing = current_guides(app)?;
    let new: Vec<Guide> = app.cat.copied_guides.iter().filter(|c| !existing.contains(c)).copied().collect();
    if new.is_empty() {
        return Ok(0);
    }
    with_history(app, |app| {
        for g in &new {
            make_guide(app, g.position, g.axis)?;
        }
        Ok(new.len())
    })
}
