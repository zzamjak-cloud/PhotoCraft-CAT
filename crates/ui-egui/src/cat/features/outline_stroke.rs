//! Outline stroke / Expand path (CAT outlineStroke.js 대응).
//! VectorCraft 패스 연산(`vectorcraft_pathops::{outline_stroke, offset_path}`)으로 계산하고, 셰이프 조작은 `lib::shape` 로 한다.
//! 결과는 겹침이 정리된(normalize) 단일 셰이프 레이어다.

use photocraft_doc::LayerId;
use vectorcraft_geom::{FillRule, PathData};
use vectorcraft_pathops::{BoolOp, Cap, Join, boolean, offset_path, outline_stroke};

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer::{active_layer, delete_layer};
use crate::cat::lib::shape::{self, ShapeOptions, StrokeSettings};

/// 마이터 한계 (Photoshop 스트로크 기본값)
pub const MITER_LIMIT: f64 = 4.0;

/// 스트로크 캡 문자열 → kurbo 캡
pub fn cap_of(cap: &str) -> Cap {
    match cap {
        "round" => Cap::Round,
        "square" => Cap::Square,
        _ => Cap::Butt,
    }
}

/// 스트로크 조인 문자열 → kurbo 조인
pub fn join_of(join: &str) -> Join {
    match join {
        "round" => Join::Round,
        "bevel" => Join::Bevel,
        _ => Join::Miter,
    }
}

/// 스트로크가 칠하는 영역. 정렬(inside/outside)은 닫힌 패스 영역과 교차/차집합으로 처리한다.
pub fn stroke_area(path: &shape::ShapePath, stroke: &StrokeSettings) -> PathData {
    let spine = shape::shape_path_to_path_data(path);
    let (cap, join) = (cap_of(&stroke.cap), join_of(&stroke.join));
    match stroke.alignment.as_str() {
        a @ ("inside" | "inner" | "outside" | "outer") => {
            // 안/밖 정렬: 두 배 폭으로 그린 뒤 채우기 영역으로 자른다 (열린 패스는 가운데 정렬과 같다)
            let region = shape::shape_path_region(path);
            if region.is_empty() {
                return outline_stroke(&spine, stroke.width, cap, join, MITER_LIMIT);
            }
            let wide = outline_stroke(&spine, stroke.width * 2.0, cap, join, MITER_LIMIT);
            let op = if a.starts_with("in") { BoolOp::Intersect } else { BoolOp::Difference };
            boolean(&wide, FillRule::NonZero, &region, FillRule::NonZero, op)
        }
        _ => outline_stroke(&spine, stroke.width, cap, join, MITER_LIMIT),
    }
}

/// 셰이프 스트로크를 면으로 변환 → 새 레이어 ID. 원본 레이어는 삭제한다.
pub fn run_outline(app: &mut PhotocraftApp) -> CatResult<LayerId> {
    let layer = active_layer(app).ok_or("선택된 레이어가 없습니다.")?;
    with_history(app, |app| {
        let path = shape::get_shape_path(app, layer.id)?;
        let stroke = shape::get_stroke_settings(app, layer.id)?.filter(|s| s.enabled && s.width > 0.0).ok_or("Stroke가 활성화되지 않았거나 두께가 0입니다.")?;
        let out = stroke_area(&path, &stroke);
        if out.is_empty() {
            return Err("아웃라인 결과가 없습니다.".into());
        }
        let id = shape::create_shape_from_path_data(app, &out, &ShapeOptions { fill: Some(stroke.color), ..Default::default() })?;
        delete_layer(app, layer.id)?;
        Ok(id)
    })
}

/// 패스를 `amount` px 만큼 확장(음수면 축소)한 새 셰이프 레이어 → 레이어 ID. 원본 레이어는 유지한다.
pub fn run_expand(app: &mut PhotocraftApp, amount: f64) -> CatResult<LayerId> {
    run_offset(app, amount, Join::Miter)
}

/// Offset path: 채우기 영역을 `amount` px 오프셋(조인 지정)한 새 셰이프 레이어 → 레이어 ID. 원본은 유지한다.
pub fn run_offset(app: &mut PhotocraftApp, amount: f64, join: Join) -> CatResult<LayerId> {
    if !amount.is_finite() || amount == 0.0 {
        return Err("올바른 숫자를 입력해주세요.".into());
    }
    let layer = active_layer(app).ok_or("선택된 레이어가 없습니다.")?;
    with_history(app, |app| {
        let color = shape::get_shape_fill_color(app, layer.id).unwrap_or([0, 0, 0]);
        let region = shape::get_shape_region(app, layer.id)?;
        let out = offset_path(&region, amount, join, MITER_LIMIT);
        if out.is_empty() {
            return Err("오프셋 결과가 없습니다.".into());
        }
        shape::create_shape_from_path_data(app, &out, &ShapeOptions { fill: Some(color), ..Default::default() })
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cat::lib::core::exec;
    use crate::cat::lib::layer::{all_layers, layer_id_of};
    use crate::cat::lib::shape::{PathOp, ShapePath, Subpath};
    use crate::cat::tests::{new_doc, test_app};

    type V = [f64; 2];

    /// 사각 셰이프 (스트로크 옵션)
    fn rect_shape(app: &mut PhotocraftApp, stroke: Option<serde_json::Value>) -> LayerId {
        let r = exec(app, "shape.create", json!({"kind": "rect", "rect": [20, 20, 60, 40], "fill": "#00ff00", "stroke": stroke})).unwrap();
        layer_id_of(&r).unwrap()
    }

    fn bounds_of(app: &PhotocraftApp, id: LayerId) -> [f64; 4] {
        // 정점만이 아니라 곡선 위 점까지 샘플링한 bounds
        let p = shape::get_shape_path(app, id).unwrap();
        let mut pts: Vec<V> = Vec::new();
        for s in &p.subpaths {
            let n = s.knots.len();
            for i in 0..n {
                let (a, b) = (s.knots[i], s.knots[(i + 1) % n]);
                let seg = [a.anchor, a.forward, b.backward, b.anchor];
                pts.extend((0..=20).map(|t| super::super::curve_fit::eval_bezier(&seg, f64::from(t) / 20.0)));
            }
        }
        let f = |i: usize, max: bool| pts.iter().map(|p| p[i]).fold(if max { f64::MIN } else { f64::MAX }, |a, b| if max { a.max(b) } else { a.min(b) });
        [f(0, false), f(1, false), f(0, true), f(1, true)]
    }

    #[test]
    fn outline_stroke_makes_ring_shape_and_removes_original() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        let src = rect_shape(&mut app, Some(json!({"width": 4, "color": "#ff0000", "align": "center", "join": "miter"})));
        let before = all_layers(&app).unwrap().len();
        let id = run_outline(&mut app).unwrap();
        assert!(all_layers(&app).unwrap().iter().all(|l| l.id != src), "원본 레이어 삭제");
        assert_eq!(all_layers(&app).unwrap().len(), before);
        assert_eq!(shape::get_shape_fill_color(&app, id).unwrap(), [255, 0, 0]);
        let p = shape::get_shape_path(&app, id).unwrap();
        // 바깥 사각 + 안쪽 구멍 (같은 컴포넌트)
        assert_eq!(p.subpaths.len(), 2);
        assert_eq!(p.subpaths[1].op, PathOp::Join);
        assert!(p.subpaths.iter().all(|s| s.closed && (4..=8).contains(&s.knots.len())), "{p:?}");
        // 링 면적 = 64x44 - 56x36
        let area = vectorcraft_pathops::area(&shape::shape_path_region(&p), FillRule::NonZero);
        assert!((area - 800.0).abs() < 1.0, "{area}");
        let b = bounds_of(&app, id);
        for (v, e) in b.iter().zip([18.0, 18.0, 82.0, 62.0]) {
            assert!((v - e).abs() < 0.01, "{b:?}");
        }
        // Undo 한 단계
        exec(&mut app, "edit.undo", json!({})).unwrap();
        assert!(all_layers(&app).unwrap().iter().any(|l| l.id == src));
    }

    #[test]
    fn outline_inside_aligned_stroke_stays_inside() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        rect_shape(&mut app, Some(json!({"width": 4, "color": "#ff0000", "align": "inside", "join": "miter"})));
        let id = run_outline(&mut app).unwrap();
        let p = shape::get_shape_path(&app, id).unwrap();
        let region = shape::shape_path_region(&p);
        let b = region.bounds().unwrap();
        assert!((b.x0 - 20.0).abs() < 0.01 && (b.x1 - 80.0).abs() < 0.01, "{b:?}");
        // 60x40 - 52x32
        let area = vectorcraft_pathops::area(&region, FillRule::NonZero);
        assert!((area - 736.0).abs() < 1.0, "{area}");
    }

    #[test]
    fn outline_requires_stroke() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        rect_shape(&mut app, None);
        assert!(run_outline(&mut app).unwrap_err().contains("Stroke"));
    }

    #[test]
    fn expand_path_grows_rect_and_keeps_original() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        let src = rect_shape(&mut app, None);
        let id = run_expand(&mut app, 4.0).unwrap();
        assert!(all_layers(&app).unwrap().iter().any(|l| l.id == src));
        assert_eq!(shape::get_shape_fill_color(&app, id).unwrap(), [0, 255, 0]);
        let b = bounds_of(&app, id);
        for (v, e) in b.iter().zip([16.0, 16.0, 84.0, 64.0]) {
            assert!((v - e).abs() < 0.2, "{b:?}");
        }
        // 축소
        exec(&mut app, "layer.select", json!({"layer": src.0})).unwrap();
        let id = run_expand(&mut app, -4.0).unwrap();
        let b = bounds_of(&app, id);
        for (v, e) in b.iter().zip([24.0, 24.0, 76.0, 56.0]) {
            assert!((v - e).abs() < 0.2, "{b:?}");
        }
    }

    #[test]
    fn open_path_round_cap_outline() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        let path = ShapePath {
            subpaths: vec![Subpath { closed: false, knots: vec![shape::Knot::corner(20.0, 50.0), shape::Knot::corner(80.0, 50.0)], op: PathOp::Combine }],
            even_odd: false,
        };
        let stroke = StrokeSettings { enabled: true, width: 10.0, alignment: "center".into(), cap: "round".into(), join: "round".into(), color: [0, 0, 255] };
        shape::create_shape_from_path(&mut app, &path, &ShapeOptions { no_fill: true, stroke: Some(stroke), ..Default::default() }).unwrap();
        let id = run_outline(&mut app).unwrap();
        let b = bounds_of(&app, id);
        for (v, e) in b.iter().zip([15.0, 45.0, 85.0, 55.0]) {
            assert!((v - e).abs() < 0.3, "{b:?}");
        }
        let p = shape::get_shape_path(&app, id).unwrap();
        assert_eq!(p.subpaths.len(), 1);
        assert!(p.subpaths[0].closed);
    }
}
