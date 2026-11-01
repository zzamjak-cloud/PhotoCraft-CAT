//! 9-slice 리사이즈 코어 (CAT nineSliceResize.js 대응).
//! CAT 은 임시 문서에서 영역을 잘라 변형·병합했지만, 인프로세스라 픽셀을 직접 합성한다 (같은 최근접 이웃 스케일).

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::pixels::{self, RgbaImage};

/// 슬라이스 값 (원본 픽셀)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SliceValues {
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

/// 9-slice 입력
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NineSliceParams {
    /// 원본 이미지 경로
    pub source: std::path::PathBuf,
    /// 슬라이스 (원본 픽셀)
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
    /// 목표 위치·크기 (문서 픽셀)
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// 결과 레이어 이름 (비면 "9slice_<W>x<H>")
    pub name: String,
}

/// T/B/L/R 보정 및 검증 (CAT adjustAndValidateSliceValues 와 같은 규칙·문구)
pub fn adjust_and_validate(v: SliceValues, width: u32, height: u32, src_w: u32, src_h: u32) -> CatResult<SliceValues> {
    let SliceValues { top, mut bottom, left, mut right } = v;
    // 내부 보정: T/B 또는 L/R 합이 원본 크기 이상이면 1px 보정
    if top + bottom >= src_h && top > 0 && bottom > 0 {
        bottom -= 1;
    }
    if left + right >= src_w && left > 0 && right > 0 {
        right -= 1;
    }
    if left + right >= src_w {
        return Err(format!("Left({left}) + Right({right}) 값이 원본 너비({src_w})보다 크거나 같습니다."));
    }
    if top + bottom >= src_h {
        return Err(format!("Top({top}) + Bottom({bottom}) 값이 원본 높이({src_h})보다 크거나 같습니다."));
    }
    if width < left + right {
        return Err(format!("목표 너비({width})가 Left({left}) + Right({right}) 합보다 작습니다."));
    }
    if height < top + bottom {
        return Err(format!("목표 높이({height})가 Top({top}) + Bottom({bottom}) 합보다 작습니다."));
    }
    Ok(SliceValues { top, bottom, left, right })
}

/// 이미지의 (x, y, w, h) 영역 (범위 밖은 투명)
pub fn crop(img: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    let mut out = RgbaImage::new(w, h);
    for yy in 0..h {
        for xx in 0..w {
            if let Some(p) = img.pixel(x + xx, y + yy) {
                set_pixel(&mut out, xx, yy, p);
            }
        }
    }
    out
}

/// (x, y) 에 RGBA 쓰기 (범위 밖이면 무시)
pub fn set_pixel(img: &mut RgbaImage, x: u32, y: u32, p: [u8; 4]) {
    if x >= img.width || y >= img.height {
        return;
    }
    let i = (y as usize * img.width as usize + x as usize) * 4;
    if let Some(dst) = img.data.get_mut(i..i + 4) {
        dst.copy_from_slice(&p);
    }
}

/// 소스 사각형 [x, y, w, h] 를 목표 사각형으로 최근접 이웃 스케일 복사
fn blit_scaled(src: &RgbaImage, s: [u32; 4], dst: &mut RgbaImage, d: [u32; 4]) {
    let [sx, sy, sw, sh] = s;
    let [dx, dy, dw, dh] = d;
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return;
    }
    for y in 0..dh {
        // 픽셀 중심 기준 샘플링 (같은 크기면 1:1 복사)
        let src_y = sy + ((2 * u64::from(y) + 1) * u64::from(sh) / (2 * u64::from(dh))) as u32;
        for x in 0..dw {
            let src_x = sx + ((2 * u64::from(x) + 1) * u64::from(sw) / (2 * u64::from(dw))) as u32;
            if let Some(p) = src.pixel(src_x, src_y) {
                set_pixel(dst, dx + x, dy + y, p);
            }
        }
    }
}

/// 9-slice 합성: 코너는 고정, 엣지는 한 방향, 센터는 양방향 스트레칭. 슬라이스가 모두 0이면 전체 스케일.
pub fn compose(src: &RgbaImage, v: SliceValues, width: u32, height: u32) -> RgbaImage {
    let mut out = RgbaImage::new(width, height);
    let (sw, sh) = (src.width, src.height);
    let SliceValues { top: t, bottom: b, left: l, right: r } = v;
    if t == 0 && b == 0 && l == 0 && r == 0 {
        blit_scaled(src, [0, 0, sw, sh], &mut out, [0, 0, width, height]);
        return out;
    }
    let src_cw = sw.saturating_sub(l + r);
    let src_ch = sh.saturating_sub(t + b);
    let cw = width.saturating_sub(l + r);
    let ch = height.saturating_sub(t + b);
    let (sr, sb) = (sw.saturating_sub(r), sh.saturating_sub(b));
    let (dr, db) = (width.saturating_sub(r), height.saturating_sub(b));
    // (소스 [x,y,w,h], 목표 [x,y,w,h]) — 크기 0 영역은 blit_scaled 가 건너뛴다
    let slices = [
        // 코너: 고정 크기
        ([0, 0, l, t], [0, 0, l, t]),
        ([sr, 0, r, t], [dr, 0, r, t]),
        ([0, sb, l, b], [0, db, l, b]),
        ([sr, sb, r, b], [dr, db, r, b]),
        // 엣지: 한 방향 스트레칭
        ([l, 0, src_cw, t], [l, 0, cw, t]),
        ([l, sb, src_cw, b], [l, db, cw, b]),
        ([0, t, l, src_ch], [0, t, l, ch]),
        ([sr, t, r, src_ch], [dr, t, r, ch]),
        // 센터: 양방향 스트레칭
        ([l, t, src_cw, src_ch], [l, t, cw, ch]),
    ];
    for (s, d) in slices {
        blit_scaled(src, s, &mut out, d);
    }
    out
}

/// 원본 이미지를 읽어 투명 영역을 트림한 소스 → (트림된 소스, 원본 너비, 원본 높이). CAT 은 소스 문서를 트림한다.
pub fn load_source(path: &std::path::Path) -> CatResult<(RgbaImage, u32, u32)> {
    let img = pixels::load_image_file(path)?;
    let (w, h) = (img.width, img.height);
    let [x, y, bw, bh] = pixels::alpha_bounds(&img, 0).ok_or("소스 이미지가 비어 있습니다.")?;
    let trimmed = if [x, y, bw, bh] == [0, 0, w, h] { img } else { crop(&img, x, y, bw, bh) };
    Ok((trimmed, w, h))
}

/// 9-slice 로 늘린 이미지를 활성 문서에 레이어로 배치 → 새 레이어 ID (Undo 한 단계)
pub fn run(app: &mut PhotocraftApp, params: &NineSliceParams) -> CatResult<Option<LayerId>> {
    if app.session.active().is_none() {
        return Err("열린 문서가 없습니다.".into());
    }
    let (width, height) = (params.width, params.height);
    if width == 0 || height == 0 {
        return Ok(None);
    }
    let (src, src_w, src_h) = load_source(&params.source)?;
    let v = SliceValues { top: params.top, bottom: params.bottom, left: params.left, right: params.right };
    let v = adjust_and_validate(v, width, height, src_w, src_h)?;
    let out = compose(&src, v, width, height);
    let name = if params.name.is_empty() { format!("9slice_{width}x{height}") } else { params.name.clone() };
    with_history(app, |app| pixels::put_pixels_as_layer(app, &out, &name, params.x, params.y)).map(Some)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 30x30: 코너(10px) TL 빨강/TR 초록/BL 파랑/BR 노랑, 엣지 회색, 중앙 흰색 (JP_Plugin nineSlice.ts 와 같은 소스)
    pub(crate) fn sample() -> RgbaImage {
        let mut img = RgbaImage::new(30, 30);
        for y in 0..30 {
            for x in 0..30 {
                let cx = if x < 10 {
                    0
                } else if x >= 20 {
                    2
                } else {
                    1
                };
                let cy = if y < 10 {
                    0
                } else if y >= 20 {
                    2
                } else {
                    1
                };
                let c = match (cx, cy) {
                    (1, 1) => [255, 255, 255, 255],
                    (0, 0) => [255, 0, 0, 255],
                    (2, 0) => [0, 255, 0, 255],
                    (0, 2) => [0, 0, 255, 255],
                    (2, 2) => [255, 255, 0, 255],
                    _ => [128, 128, 128, 255],
                };
                set_pixel(&mut img, x, y, c);
            }
        }
        img
    }

    #[test]
    fn compose_keeps_corners_and_stretches_center() {
        let v = SliceValues { top: 10, bottom: 10, left: 10, right: 10 };
        let out = compose(&sample(), v, 100, 60);
        assert_eq!((out.width, out.height), (100, 60));
        // 코너 영역은 원본과 동일
        for (x, y, c) in [(0, 0, [255, 0, 0, 255]), (9, 9, [255, 0, 0, 255]), (90, 0, [0, 255, 0, 255]), (99, 9, [0, 255, 0, 255])] {
            assert_eq!(out.pixel(x, y), Some(c), "({x},{y})");
        }
        assert_eq!(out.pixel(0, 50), Some([0, 0, 255, 255]));
        assert_eq!(out.pixel(99, 59), Some([255, 255, 0, 255]));
        // 엣지·센터
        assert_eq!(out.pixel(50, 5), Some([128, 128, 128, 255]));
        assert_eq!(out.pixel(5, 30), Some([128, 128, 128, 255]));
        assert_eq!(out.pixel(10, 10), Some([255, 255, 255, 255]));
        assert_eq!(out.pixel(89, 49), Some([255, 255, 255, 255]));
        assert_eq!(out.pixel(90, 50), Some([255, 255, 0, 255]));
    }

    #[test]
    fn compose_without_slices_scales_whole_image() {
        let out = compose(&sample(), SliceValues::default(), 60, 60);
        assert_eq!(out.pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(out.pixel(19, 19), Some([255, 0, 0, 255]));
        assert_eq!(out.pixel(20, 20), Some([255, 255, 255, 255]));
        assert_eq!(out.pixel(59, 59), Some([255, 255, 0, 255]));
    }

    #[test]
    fn validation_matches_cat_rules() {
        let v = |t, b, l, r| SliceValues { top: t, bottom: b, left: l, right: r };
        // 합이 원본 크기와 같으면 1px 보정
        assert_eq!(adjust_and_validate(v(15, 15, 0, 0), 50, 50, 30, 30), Ok(v(15, 14, 0, 0)));
        assert!(adjust_and_validate(v(0, 0, 30, 0), 50, 50, 30, 30).unwrap_err().contains("Left(30) + Right(0)"));
        assert!(adjust_and_validate(v(10, 10, 10, 10), 15, 50, 30, 30).unwrap_err().contains("목표 너비(15)"));
        assert!(adjust_and_validate(v(10, 10, 10, 10), 50, 15, 30, 30).unwrap_err().contains("목표 높이(15)"));
    }
}
