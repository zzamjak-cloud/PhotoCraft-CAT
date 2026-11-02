//! VECTOR 섹션 기능: Pathfinder · Simplify · Smooth · Offset path (VectorCraft `vectorcraft_pathops` 기반).
//! 셰이프 레이어 ⇄ PathData 변환은 `lib::shape` 가 맡고, 여기서는 연산과 레이어 생성/삭제 순서만 다룬다.

use photocraft_doc::LayerId;
use vectorcraft_geom::FillRule;
use vectorcraft_pathops::{Join, PathfinderOp, Shape, SimplifyOptions, pathfinder, simplify_level_tolerance, simplify_with, smooth_level};

use super::outline_stroke;
use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer::{LayerInfo, LayerKind, delete_layer, group_selected_layers, select_layer, select_layers, selected_layers};
use crate::cat::lib::shape::{self, ShapeOptions};

/// VECTOR 섹션의 Pathfinder 버튼 (Illustrator Pathfinder 패널 순서)
pub const PATHFINDER_OPS: &[(PathfinderOp, &str, &str)] = &[
    (PathfinderOp::Unite, "Unite", "Unite: 선택한 셰이프를 하나로 합친다 (맨 위 색)"),
    (PathfinderOp::MinusFront, "Minus Front", "Minus Front: 맨 아래 셰이프에서 위 셰이프들을 뺀다"),
    (PathfinderOp::Intersect, "Intersect", "Intersect: 모두 겹친 영역만 남긴다 (맨 위 색)"),
    (PathfinderOp::Exclude, "Exclude", "Exclude: 홀수 번 겹친 영역만 남긴다 (맨 위 색)"),
    (PathfinderOp::Divide, "Divide", "Divide: 겹친 면마다 조각으로 나눈다"),
    (PathfinderOp::Trim, "Trim", "Trim: 가려진 부분을 지운다 (셰이프별)"),
    (PathfinderOp::Merge, "Merge", "Merge: Trim 후 같은 색끼리 합친다"),
    (PathfinderOp::Crop, "Crop", "Crop: 맨 위 셰이프 안의 아래 셰이프만 남긴다"),
    (PathfinderOp::MinusBack, "Minus Back", "Minus Back: 맨 위 셰이프에서 아래 셰이프들을 뺀다"),
];

/// 선택한 셰이프 레이어 (아래→위). 하나도 없으면 오류.
fn selected_shapes(app: &PhotocraftApp) -> CatResult<Vec<LayerInfo>> {
    let mut v: Vec<LayerInfo> = selected_layers(app)?.into_iter().filter(|l| l.kind == LayerKind::Shape).collect();
    // selected_layers 는 패널 순서(위→아래)
    v.reverse();
    if v.is_empty() {
        return Err("셰이프 레이어를 선택해주세요.".into());
    }
    Ok(v)
}

/// Pathfinder: 선택한 셰이프 레이어들(아래→위)에 연산 → 결과 셰이프 레이어 ID (아래→위).
/// 원본 레이어는 삭제하고(Illustrator 와 같음), 결과가 여럿이면 연산 이름의 그룹으로 묶는다. Undo 1단계.
pub fn run_pathfinder(app: &mut PhotocraftApp, op: PathfinderOp) -> CatResult<Vec<LayerId>> {
    let layers = selected_shapes(app)?;
    if layers.len() < 2 {
        return Err("셰이프 레이어를 2개 이상 선택해주세요.".into());
    }
    let mut shapes = Vec::with_capacity(layers.len());
    let mut colors = Vec::with_capacity(layers.len());
    for (i, l) in layers.iter().enumerate() {
        shapes.push(Shape::new(shape::get_shape_region(app, l.id)?, FillRule::NonZero, i as u64));
        colors.push(shape::get_shape_fill_color(app, l.id).unwrap_or([0, 0, 0]));
    }
    let results = pathfinder(op, &shapes);
    if results.is_empty() {
        return Err("Pathfinder 결과가 없습니다.".into());
    }
    let label = PATHFINDER_OPS.iter().find(|(o, _, _)| *o == op).map_or("Pathfinder", |(_, l, _)| *l);
    let top = layers.last().map(|l| l.id).ok_or("셰이프 레이어를 선택해주세요.")?;
    with_history(app, |app| {
        // 새 레이어는 활성 레이어 위에 생기므로 맨 위 원본을 활성으로
        select_layer(app, top, false)?;
        let mut ids = Vec::with_capacity(results.len());
        for r in &results {
            let idx = usize::try_from(r.key).unwrap_or(0);
            let color = colors.get(idx).copied().unwrap_or([0, 0, 0]);
            let name = layers.get(idx).map(|l| l.name.clone());
            ids.push(shape::create_shape_from_path_data(app, &r.path, &ShapeOptions { name, fill: Some(color), ..Default::default() })?);
        }
        for l in &layers {
            delete_layer(app, l.id)?;
        }
        if ids.len() > 1 {
            select_layers(app, &ids)?;
            group_selected_layers(app, label)?;
        } else {
            select_layers(app, &ids)?;
        }
        Ok(ids)
    })
}

/// Simplify: 선택한 셰이프 레이어의 기준점을 줄인다. `level` 0(가장 적은 점)..100(원본에 가장 가깝게), 50 = 1px 허용 오차.
/// 레이어를 그 자리에서 고친다 → 고친 레이어 수.
pub fn run_simplify(app: &mut PhotocraftApp, level: f64) -> CatResult<usize> {
    let opts = SimplifyOptions { tolerance: simplify_level_tolerance(level), ..Default::default() };
    edit_paths(app, |p| shape::map_subpaths(p, |d| simplify_with(d, &opts)))
}

/// Smooth: 선택한 셰이프 레이어를 부드럽게 (`level` 0..100) → 고친 레이어 수.
pub fn run_smooth(app: &mut PhotocraftApp, level: f64) -> CatResult<usize> {
    edit_paths(app, |p| shape::map_subpaths(p, |d| smooth_level(d, level)))
}

fn edit_paths(app: &mut PhotocraftApp, f: impl Fn(&shape::ShapePath) -> shape::ShapePath) -> CatResult<usize> {
    let layers = selected_shapes(app)?;
    with_history(app, |app| {
        for l in &layers {
            let path = shape::get_shape_path(app, l.id)?;
            shape::set_shape_path(app, l.id, &f(&path))?;
        }
        Ok(layers.len())
    })
}

/// Offset path 조인 (UI 드롭다운)
pub const OFFSET_JOINS: &[(&str, Join)] = &[("Miter", Join::Miter), ("Round", Join::Round), ("Bevel", Join::Bevel)];

/// Offset path: 활성 셰이프 레이어를 `distance` px 오프셋한 새 셰이프 (원본 유지) → 레이어 ID
pub fn run_offset(app: &mut PhotocraftApp, distance: f64, join: Join) -> CatResult<LayerId> {
    outline_stroke::run_offset(app, distance, join)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vectorcraft_geom::FillRule;
    use vectorcraft_pathops::area;

    use super::*;
    use crate::cat::lib::core::exec;
    use crate::cat::lib::layer::{all_layers, layer_id_of};
    use crate::cat::tests::{new_doc, test_app};

    fn rect(app: &mut PhotocraftApp, r: [f64; 4], color: &str) -> LayerId {
        let res = exec(app, "shape.create", json!({"kind": "rect", "rect": r, "fill": color})).unwrap();
        layer_id_of(&res).unwrap()
    }

    fn region_area(app: &PhotocraftApp, id: LayerId) -> f64 {
        area(&shape::get_shape_region(app, id).unwrap(), FillRule::NonZero)
    }

    /// 겹친 두 사각형: A(빨강, 아래) 0..40 x 0..40, B(파랑, 위) 20..60 x 20..60 → 겹침 20x20
    fn two_rects(app: &mut PhotocraftApp) -> (LayerId, LayerId) {
        new_doc(app, "doc", 100, 100);
        let a = rect(app, [0.0, 0.0, 40.0, 40.0], "#ff0000");
        let b = rect(app, [20.0, 20.0, 40.0, 40.0], "#0000ff");
        select_layers(app, &[a, b]).unwrap();
        (a, b)
    }

    /// (연산, 결과 레이어 수, 면적 합, 결과 색 목록)
    fn check(op: PathfinderOp, count: usize, total: f64, colors: &[[u8; 3]]) {
        let (mut app, _dir) = test_app();
        let (a, b) = two_rects(&mut app);
        let before = app.session.active().unwrap().history.past_len();
        let ids = run_pathfinder(&mut app, op).unwrap();
        assert_eq!(ids.len(), count, "{op:?}");
        let sum: f64 = ids.iter().map(|id| region_area(&app, *id)).sum();
        assert!((sum - total).abs() < 1.0, "{op:?} 면적 {sum} != {total}");
        let got: Vec<[u8; 3]> = ids.iter().map(|id| shape::get_shape_fill_color(&app, *id).unwrap()).collect();
        assert_eq!(got, colors, "{op:?}");
        let layers = all_layers(&app).unwrap();
        assert!(layers.iter().all(|l| l.id != a && l.id != b), "{op:?} 원본 삭제");
        assert_eq!(app.session.active().unwrap().history.past_len(), before + 1, "{op:?} Undo 1단계");
        exec(&mut app, "edit.undo", json!({})).unwrap();
        let layers = all_layers(&app).unwrap();
        assert!(layers.iter().any(|l| l.id == a) && layers.iter().any(|l| l.id == b), "{op:?} Undo 복원");
    }

    const RED: [u8; 3] = [255, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    #[test]
    fn pathfinder_shape_modes() {
        check(PathfinderOp::Unite, 1, 1600.0 + 1600.0 - 400.0, &[BLUE]);
        check(PathfinderOp::MinusFront, 1, 1200.0, &[RED]);
        check(PathfinderOp::Intersect, 1, 400.0, &[BLUE]);
        check(PathfinderOp::Exclude, 1, 2400.0, &[BLUE]);
        check(PathfinderOp::MinusBack, 1, 1200.0, &[BLUE]);
    }

    #[test]
    fn pathfinder_splitting_ops() {
        // Divide: A 만, 겹침(B 색), B 만 → 3조각
        let (mut app, _dir) = test_app();
        two_rects(&mut app);
        let ids = run_pathfinder(&mut app, PathfinderOp::Divide).unwrap();
        assert_eq!(ids.len(), 3);
        let sum: f64 = ids.iter().map(|id| region_area(&app, *id)).sum();
        assert!((sum - 2800.0).abs() < 1.0, "{sum}");
        // 결과는 연산 이름 그룹으로 묶인다
        let layers = all_layers(&app).unwrap();
        let g = layers.iter().find(|l| l.name == "Divide").unwrap();
        assert!(ids.iter().all(|id| layers.iter().any(|l| l.id == *id && l.parent == Some(g.id))));

        // Trim: A 의 보이는 부분 + B 전체
        check(PathfinderOp::Trim, 2, 1200.0 + 1600.0, &[RED, BLUE]);
        // Merge: 색이 달라 Trim 과 같다
        check(PathfinderOp::Merge, 2, 2800.0, &[RED, BLUE]);
        // Crop: 맨 위(B) 안의 A 부분만
        check(PathfinderOp::Crop, 1, 400.0, &[RED]);
    }

    #[test]
    fn pathfinder_needs_two_shapes() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        let a = rect(&mut app, [0.0, 0.0, 40.0, 40.0], "#ff0000");
        select_layers(&mut app, &[a]).unwrap();
        assert!(run_pathfinder(&mut app, PathfinderOp::Unite).unwrap_err().contains("2개"));
    }

    /// 점이 많은 원 (64개 코너 점)
    fn polygon_circle(app: &mut PhotocraftApp) -> LayerId {
        let knots = (0..64)
            .map(|i| {
                let t = f64::from(i) / 64.0 * std::f64::consts::TAU;
                shape::Knot::corner(50.0 + 30.0 * t.cos(), 50.0 + 30.0 * t.sin())
            })
            .collect();
        let path = shape::ShapePath { subpaths: vec![shape::Subpath { closed: true, knots, op: shape::PathOp::Combine }], even_odd: false };
        shape::create_shape_from_path(app, &path, &ShapeOptions { fill: Some([0, 128, 0]), ..Default::default() }).unwrap()
    }

    #[test]
    fn simplify_reduces_anchors_and_keeps_area() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        let id = polygon_circle(&mut app);
        let a0 = region_area(&app, id);
        select_layers(&mut app, &[id]).unwrap();
        assert_eq!(run_simplify(&mut app, 50.0).unwrap(), 1);
        let p = shape::get_shape_path(&app, id).unwrap();
        assert!(p.knot_count() < 20, "{}", p.knot_count());
        let a1 = region_area(&app, id);
        assert!((a1 - a0).abs() / a0 < 0.02, "{a0} {a1}");
        assert_eq!(shape::get_shape_fill_color(&app, id).unwrap(), [0, 128, 0]);
    }

    #[test]
    fn smooth_keeps_shape_and_fill() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        let id = polygon_circle(&mut app);
        select_layers(&mut app, &[id]).unwrap();
        run_smooth(&mut app, 50.0).unwrap();
        let p = shape::get_shape_path(&app, id).unwrap();
        assert!(p.knot_count() <= 64 && p.subpaths[0].closed);
        assert!(p.subpaths[0].knots.iter().any(|k| k.forward != k.anchor), "곡선 핸들이 생긴다");
    }

    #[test]
    fn offset_grows_bounds_and_keeps_original() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 100, 100);
        let id = rect(&mut app, [20.0, 20.0, 40.0, 40.0], "#00ff00");
        select_layers(&mut app, &[id]).unwrap();
        let out = run_offset(&mut app, 5.0, Join::Round).unwrap();
        let b = shape::get_shape_region(&app, out).unwrap().bounds().unwrap();
        assert!((b.x0 - 15.0).abs() < 0.1 && (b.y1 - 65.0).abs() < 0.1, "{b:?}");
        assert!(all_layers(&app).unwrap().iter().any(|l| l.id == id));
        // 둥근 조인: 면적 = 50x50 - 모서리 (4 - π)·25
        let a = region_area(&app, out);
        let expect = 2500.0 - (4.0 - std::f64::consts::PI) * 25.0;
        assert!((a - expect).abs() < 1.0, "{a} {expect}");
    }
}
