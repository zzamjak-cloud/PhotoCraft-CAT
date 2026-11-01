//! Split to layers (CAT SplitToLayers.js 대응): 활성 레이어를 알파 기준 연결 영역별 레이어로 분리하고 원본은 숨긴다.

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::Progress;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer::{self, LayerKind};
use crate::cat::lib::pixels::{self, RgbaImage};

/// 이 값 미만의 알파는 배경으로 간주 (AA 찌꺼기로 인한 과분할 방지)
const ALPHA_THRESHOLD: u8 = 8;
/// 이 거리(px) 이내로 떨어진 조각은 같은 이미지로 병합
const MERGE_GAP: usize = 3;
/// 이 픽셀 수 미만의 고립 조각은 무시
const MIN_REGION_PIXELS: usize = 8;

/// 연결 영역 (bbox 는 팽창 전 실제 픽셀 기준, 양끝 포함)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub label: u32,
    pub min_x: usize,
    pub min_y: usize,
    pub max_x: usize,
    pub max_y: usize,
    pub count: usize,
}

/// 정사각 팽창 (가로 → 세로 2패스)
fn dilate(mask: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    if r == 0 || w == 0 || h == 0 {
        return mask.to_vec();
    }
    let mut tmp = vec![false; w * h];
    for y in 0..h {
        for x in (0..w).filter(|x| mask.get(y * w + x) == Some(&true)) {
            for k in x.saturating_sub(r)..=(x + r).min(w - 1) {
                if let Some(v) = tmp.get_mut(y * w + k) {
                    *v = true;
                }
            }
        }
    }
    let mut out = vec![false; w * h];
    for y in 0..h {
        for x in (0..w).filter(|x| tmp.get(y * w + x) == Some(&true)) {
            for k in y.saturating_sub(r)..=(y + r).min(h - 1) {
                if let Some(v) = out.get_mut(k * w + x) {
                    *v = true;
                }
            }
        }
    }
    out
}

/// 연결 요소 라벨링: 팽창 마스크에서 8-연결 라벨을 매기고, 영역 통계는 원본 픽셀로만 계산한다.
/// → (픽셀별 라벨, 읽기 순서(min_y → min_x)로 정렬된 유효 영역)
pub fn label_regions(img: &RgbaImage) -> (Vec<u32>, Vec<Region>) {
    let (w, h) = (img.width as usize, img.height as usize);
    let orig: Vec<bool> = img.data.chunks_exact(4).map(|p| p.get(3).is_some_and(|a| *a >= ALPHA_THRESHOLD)).collect();
    let mask = dilate(&orig, w, h, MERGE_GAP);
    let mut labels = vec![0u32; w * h];
    let mut regions = Vec::new();
    let mut stack = Vec::new();
    let mut next = 0u32;
    for start in 0..w * h {
        if mask.get(start) != Some(&true) || labels.get(start) != Some(&0) {
            continue;
        }
        next += 1;
        let mut reg = Region { label: next, min_x: w, min_y: h, max_x: 0, max_y: 0, count: 0 };
        if let Some(l) = labels.get_mut(start) {
            *l = next;
        }
        stack.push(start);
        while let Some(p) = stack.pop() {
            let (px, py) = (p % w, p / w);
            // 통계는 실제 픽셀(팽창 전)만 반영 — bbox 가 팽창 영역만큼 커지지 않도록
            if orig.get(p) == Some(&true) {
                reg.count += 1;
                reg.min_x = reg.min_x.min(px);
                reg.max_x = reg.max_x.max(px);
                reg.min_y = reg.min_y.min(py);
                reg.max_y = reg.max_y.max(py);
            }
            for ny in py.saturating_sub(1)..=(py + 1).min(h - 1) {
                for nx in px.saturating_sub(1)..=(px + 1).min(w - 1) {
                    let q = ny * w + nx;
                    if mask.get(q) == Some(&true)
                        && let Some(l) = labels.get_mut(q)
                        && *l == 0
                    {
                        *l = next;
                        stack.push(q);
                    }
                }
            }
        }
        if reg.count >= MIN_REGION_PIXELS {
            regions.push(reg);
        }
    }
    regions.sort_by_key(|r| (r.min_y, r.min_x));
    (labels, regions)
}

/// 영역 bbox 크기의 이미지에 같은 라벨 픽셀만 복사 (이웃 이미지 픽셀이 딸려오지 않는다)
fn extract_region(img: &RgbaImage, labels: &[u32], reg: &Region) -> RgbaImage {
    let (rw, rh) = (reg.max_x - reg.min_x + 1, reg.max_y - reg.min_y + 1);
    let mut out = RgbaImage::new(rw as u32, rh as u32);
    let w = img.width as usize;
    for y in reg.min_y..=reg.max_y {
        for x in reg.min_x..=reg.max_x {
            let idx = y * w + x;
            if labels.get(idx) != Some(&reg.label) {
                continue;
            }
            let d = ((y - reg.min_y) * rw + (x - reg.min_x)) * 4;
            if let (Some(src), Some(dst)) = (img.data.get(idx * 4..idx * 4 + 4), out.data.get_mut(d..d + 4)) {
                dst.copy_from_slice(src);
            }
        }
    }
    out
}

/// CAT generateLayerName: `<원본>_<두 자리 번호>`
fn region_name(base: &str, n: usize) -> String {
    format!("{base}_{n:02}")
}

/// 활성 레이어를 영역별 레이어로 분리 → 새 레이어 ID (읽기 순서)
pub fn run(app: &mut PhotocraftApp) -> CatResult<Vec<LayerId>> {
    let base = layer::active_layer(app).or_else(|| layer::selected_layers(app).ok()?.into_iter().next()).ok_or("레이어를 선택해주세요.")?;
    if base.kind == LayerKind::Group || layer::is_background_layer(app, base.id) {
        return Err("지원되는 레이어 타입이 아닙니다.".into());
    }
    let r = with_history(app, |app| {
        layer::select_layer(app, base.id, false)?;
        // 래스터화(실패는 무시: 픽셀이 없으면 아래 픽셀 읽기가 알려준다) → 마스크 적용 후 제거
        if base.kind != LayerKind::Pixel {
            let _ = layer::rasterize_layer(app, base.id);
        }
        layer::apply_layer_mask(app, base.id);

        app.cat.progress = Some(Progress { label: "레이어 분할: 픽셀 분석".into(), percent: 0.0 });
        let img = pixels::get_layer_pixels(app, base.id)?;
        let (labels, regions) = label_regions(&img);
        if regions.is_empty() {
            return Err("분할할 영역을 찾을 수 없습니다.".into());
        }
        let mut created = Vec::with_capacity(regions.len());
        for (i, reg) in regions.iter().enumerate() {
            let percent = (i + 1) as f32 / regions.len() as f32 * 100.0;
            app.cat.progress = Some(Progress { label: format!("레이어 분할 {}/{}", i + 1, regions.len()), percent });
            let part = extract_region(&img, &labels, reg);
            created.push(pixels::put_pixels_as_layer(app, &part, &region_name(&base.name, i + 1), reg.min_x as i32, reg.min_y as i32)?);
        }
        layer::set_visible(app, base.id, false)?;
        Ok(created)
    });
    app.cat.progress = None;
    r
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cat::lib::core::exec;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn fill(app: &mut PhotocraftApp, x: i32, y: i32, w: i32, h: i32, color: &str) {
        exec(app, "select.rect", json!({"x": x, "y": y, "width": w, "height": h})).unwrap();
        exec(app, "edit.fill", json!({"contents": "color", "color": color})).unwrap();
        exec(app, "select.deselect", json!({})).unwrap();
    }

    #[test]
    fn splits_islands_into_named_layers_in_reading_order() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 80);
        let base = rect_layer(&mut app, "Icons", 60, 10, 20, 20, "#ff0000");
        fill(&mut app, 5, 10, 10, 10, "#00ff00");
        fill(&mut app, 10, 50, 30, 20, "#0000ff");
        let before = app.session.active().unwrap().history.past_len();

        let ids = run(&mut app).unwrap();
        assert_eq!(ids.len(), 3);
        let infos: Vec<_> = ids.iter().map(|id| layer::layer_by_id(&app, *id).unwrap()).collect();
        let names: Vec<_> = infos.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Icons_01", "Icons_02", "Icons_03"]);
        let bounds: Vec<_> = infos.iter().map(|l| l.bounds.unwrap()).collect();
        assert_eq!(bounds[0], layer::Bounds { x: 5, y: 10, w: 10, h: 10 });
        assert_eq!(bounds[1], layer::Bounds { x: 60, y: 10, w: 20, h: 20 });
        assert_eq!(bounds[2], layer::Bounds { x: 10, y: 50, w: 30, h: 20 });
        // 각 레이어에는 자기 영역 픽셀만
        let px = pixels::get_layer_pixels(&app, ids[1]).unwrap();
        assert_eq!(px.pixel(65, 15), Some([255, 0, 0, 255]));
        assert_eq!(px.pixel(8, 12).map(|p| p[3]), Some(0));
        assert!(!layer::layer_by_id(&app, base).unwrap().visible);
        assert_eq!(app.session.active().unwrap().history.past_len(), before + 1, "Undo 한 단계");
    }

    #[test]
    fn close_fragments_merge_and_specks_are_ignored() {
        let mut img = RgbaImage::new(40, 20);
        let mut set = |x: u32, y: u32| {
            let i = ((y * 40 + x) * 4) as usize;
            img.data[i..i + 4].copy_from_slice(&[1, 2, 3, 255]);
        };
        // 4x4 블록 두 개, 간격 2px → 병합
        for y in 0..4 {
            for x in 0..4 {
                set(x, y);
                set(x + 6, y);
            }
        }
        // 멀리 떨어진 2px 점 → 무시
        set(30, 15);
        set(31, 15);
        let (_, regions) = label_regions(&img);
        assert_eq!(regions.len(), 1);
        assert_eq!((regions[0].min_x, regions[0].max_x, regions[0].count), (0, 9, 32));
    }

    #[test]
    fn rejects_background_layer() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 20, 20);
        assert!(run(&mut app).is_err());
    }
}
