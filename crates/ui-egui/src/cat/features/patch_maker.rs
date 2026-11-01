//! Make 9-patch (CAT patchMaker.js 대응): 가이드 사이의 늘어나는 영역을 제거해 최소 9-patch 소스를 만든다.
//! CAT 은 영역을 잘라 이동·병합했지만, 인프로세스라 최상위 레이어 픽셀에서 가이드 사이 열/행을 직접 제거한다.

use crate::PhotocraftApp;
use crate::cat::features::nine_slice_resize::{crop, set_pixel};
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::doc::{require_doc, trim_transparent};
use crate::cat::lib::guide::{GuideAxis, clear_all_guides, get_guides};
use crate::cat::lib::layer::{delete_layer, layer_by_id, rename_layer, select_layer, top_level_layers};
use crate::cat::lib::pixels::{self, RgbaImage};

/// 제거 구간 (시작, 끝)
type Band = Option<(i64, i64)>;

/// 가이드 구성 → (수직 제거 구간, 수평 제거 구간) (레이어 기준 상대 좌표).
/// 가이드 1개는 크기 기준 대칭으로 2개를 만든다 (CAT analyzePatchType + calculateSymmetricGuides).
fn patch_ranges(vg: &[i64], hg: &[i64], w: i64, h: i64) -> CatResult<(Band, Band)> {
    fn sym(g: i64, size: i64) -> (i64, i64) {
        // 중심보다 앞이면 (g, size-g), 뒤면 (size-g, g)
        if (g as f64) < size as f64 / 2.0 { (g, size - g) } else { (size - g, g) }
    }
    let pair = |gs: &[i64], size: i64| -> Option<(i64, i64)> {
        match gs {
            [g] => Some(sym(*g, size)),
            [a, b] => Some((*a, *b)),
            _ => None,
        }
    };
    match (vg.len(), hg.len()) {
        (0..=2, 0..=2) if vg.len() + hg.len() > 0 => Ok((pair(vg, w), pair(hg, h))),
        (v, hc) => Err(format!("유효하지 않은 가이드 구성입니다. 현재 가이드: 수직 {v}개, 수평 {hc}개")),
    }
}

/// [a, b) 열과 행을 제거한 이미지
fn remove_bands(img: &RgbaImage, cols: Option<(u32, u32)>, rows: Option<(u32, u32)>) -> RgbaImage {
    let (c0, c1) = cols.unwrap_or((0, 0));
    let (r0, r1) = rows.unwrap_or((0, 0));
    let w = img.width - (c1 - c0);
    let h = img.height - (r1 - r0);
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        let sy = if y < r0 { y } else { y + (r1 - r0) };
        for x in 0..w {
            let sx = if x < c0 { x } else { x + (c1 - c0) };
            if let Some(p) = img.pixel(sx, sy) {
                set_pixel(&mut out, x, y, p);
            }
        }
    }
    out
}

/// 상대 구간을 [0, size] 로 자르고 비었으면 None
fn clamp_range(r: Option<(i64, i64)>, size: u32) -> Option<(u32, u32)> {
    let (a, b) = r?;
    let a = a.clamp(0, i64::from(size)) as u32;
    let b = b.clamp(0, i64::from(size)) as u32;
    (a < b).then_some((a, b))
}

/// 가이드로 9-patch 생성: 최상위 레이어 → "Source", 가이드 사이 영역 제거, 트림, 가이드 삭제 (Undo 한 단계)
pub fn run(app: &mut PhotocraftApp) -> CatResult<()> {
    require_doc(app)?;
    let guides = get_guides(app)?;
    if guides.is_empty() {
        return Err("가이드가 없습니다. 패치 메이커를 실행하기 전에 가이드를 추가해주세요.".into());
    }
    let first = top_level_layers(app)?.into_iter().next().ok_or("레이어가 없습니다.")?;
    let b = first.bounds.ok_or("레이어 영역을 읽을 수 없습니다.")?;
    // 가이드는 레이어 좌상단 기준 상대 좌표로 (CAT 은 트림 후 문서 좌표 = 레이어 기준)
    let axis = |a: GuideAxis, origin: i32| {
        let mut v: Vec<i64> = guides.iter().filter(|g| g.axis == a).map(|g| g.position.trunc() as i64 - i64::from(origin)).collect();
        v.sort_unstable();
        v
    };
    let (vr, hr) = patch_ranges(&axis(GuideAxis::Vertical, b.x), &axis(GuideAxis::Horizontal, b.y), i64::from(b.w), i64::from(b.h))?;
    let (w, h) = (b.w.max(0) as u32, b.h.max(0) as u32);
    let (cols, rows) = (clamp_range(vr, w), clamp_range(hr, h));

    with_history(app, |app| {
        select_layer(app, first.id, false)?;
        rename_layer(app, first.id, "Source")?;
        let full = pixels::get_layer_pixels(app, first.id)?;
        let src = crop(&full, b.x.max(0) as u32, b.y.max(0) as u32, w, h);
        let out = remove_bands(&src, cols, rows);
        let id = pixels::put_pixels_as_layer(app, &out, "Source", b.x, b.y)?;
        delete_layer(app, first.id)?;
        select_layer(app, id, false)?;
        trim_transparent(app);
        clear_all_guides(app)?;
        // 결과 레이어가 남아 있는지 확인
        layer_by_id(app, id).map(|_| ()).ok_or_else(|| "9-patch 결과 레이어를 만들 수 없습니다.".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::features::nine_slice_resize::tests::sample;
    use crate::cat::lib::doc::{self, DocBackground};
    use crate::cat::lib::guide::make_guide;
    use crate::cat::lib::layer::active_layer;
    use crate::cat::tests::test_app;

    #[test]
    fn ranges_follow_cat_patch_types() {
        assert_eq!(patch_ranges(&[10, 20], &[], 30, 30), Ok((Some((10, 20)), None)));
        assert_eq!(patch_ranges(&[], &[5], 30, 30), Ok((None, Some((5, 25)))));
        assert_eq!(patch_ranges(&[25], &[10, 20], 30, 30), Ok((Some((5, 25)), Some((10, 20)))));
        assert!(patch_ranges(&[1, 2, 3], &[], 30, 30).unwrap_err().contains("수직 3개"));
    }

    #[test]
    fn make_9patch_removes_stretch_area_and_keeps_corners() {
        let (mut app, _dir) = test_app();
        doc::create_doc(&mut app, "patch", 60, 60, DocBackground::Transparent).unwrap();
        pixels::put_pixels_as_layer(&mut app, &sample(), "btn", 10, 10).unwrap();
        // 소스(10..40) 안의 가이드: 수직 20/30, 수평 1개(15 → 대칭 5..25)
        make_guide(&mut app, 20.0, GuideAxis::Vertical).unwrap();
        make_guide(&mut app, 30.0, GuideAxis::Vertical).unwrap();
        make_guide(&mut app, 15.0, GuideAxis::Horizontal).unwrap();
        let before = app.session.active().unwrap().history.past_len();

        run(&mut app).unwrap();

        let st = app.session.active().unwrap();
        assert_eq!((st.doc.size.width, st.doc.size.height), (20, 10), "트림된 결과 크기");
        assert_eq!(st.history.past_len(), before + 1, "Undo 한 단계");
        assert!(get_guides(&app).unwrap().is_empty(), "가이드 제거");
        let res = active_layer(&app).unwrap();
        assert_eq!(res.name, "Source");
        let px = pixels::get_layer_pixels(&app, res.id).unwrap();
        // 남은 열: 0..10, 20..30 / 남은 행: 0..5, 25..30
        assert_eq!(px.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(px.pixel(9, 4), Some([255, 0, 0, 255]));
        assert_eq!(px.pixel(19, 0), Some([0, 255, 0, 255]));
        assert_eq!(px.pixel(0, 9), Some([0, 0, 255, 255]));
        assert_eq!(px.pixel(10, 5), Some([255, 255, 0, 255]));
        assert_eq!(px.pixel(19, 9), Some([255, 255, 0, 255]));
    }

    #[test]
    fn make_9patch_without_guides_fails() {
        let (mut app, _dir) = test_app();
        doc::create_doc(&mut app, "p", 20, 20, DocBackground::Transparent).unwrap();
        assert!(run(&mut app).unwrap_err().contains("가이드가 없습니다"));
    }
}
