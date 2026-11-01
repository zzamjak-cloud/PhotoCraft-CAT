//! Outline stroke / Expand path (CAT outlineStroke.js 대응).
//! 폴리라인 오프셋·자기교차 분해·베지어 피팅 알고리즘은 원본을 그대로 옮기고, 셰이프 조작만 `lib::shape` 로 한다.
//! CAT 은 subpath 마다 셰이프를 만들어 병합했지만, 여기서는 같은 결과(컴포넌트 결합된 단일 셰이프)를 한 번에 만든다.

use std::f64::consts::PI;

use photocraft_doc::LayerId;

use super::curve_fit::{fit_path_segments, segments_to_knots};
use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer::{active_layer, delete_layer};
use crate::cat::lib::shape::{self, PathOp, ShapeOptions, ShapePath, StrokeSettings, Subpath};

type V = [f64; 2];

/// 셰이프 스트로크를 면으로 변환 → 새 레이어 ID. 원본 레이어는 삭제한다.
pub fn run_outline(app: &mut PhotocraftApp) -> CatResult<LayerId> {
    let layer = active_layer(app).ok_or("선택된 레이어가 없습니다.")?;
    with_history(app, |app| {
        let path = shape::get_shape_path(app, layer.id)?;
        let stroke = shape::get_stroke_settings(app, layer.id)?.filter(|s| s.enabled && s.width > 0.0).ok_or("Stroke가 활성화되지 않았거나 두께가 0입니다.")?;
        let mut out = ShapePath::default();
        for sp in &path.subpaths {
            let polyline = subpath_to_polyline(sp);
            if sp.knots.len() < 2 || polyline.len() < 2 {
                continue;
            }
            let (outer, inner) = compute_outline(&polyline, &stroke, sp.closed);
            if outer.is_empty() {
                continue;
            }
            push_ring_component(&mut out, &outer, &inner);
        }
        if out.subpaths.is_empty() {
            return Err("아웃라인 결과가 없습니다.".into());
        }
        let id = shape::create_shape_from_path(app, &out, &ShapeOptions { fill: Some(stroke.color), ..Default::default() })?;
        delete_layer(app, layer.id)?;
        Ok(id)
    })
}

/// 패스를 `amount` px 만큼 확장(음수면 축소)한 새 셰이프 레이어 → 레이어 ID. 원본 레이어는 유지한다.
pub fn run_expand(app: &mut PhotocraftApp, amount: f64) -> CatResult<LayerId> {
    if !amount.is_finite() || amount == 0.0 {
        return Err("올바른 숫자를 입력해주세요.".into());
    }
    let layer = active_layer(app).ok_or("선택된 레이어가 없습니다.")?;
    with_history(app, |app| {
        let color = shape::get_shape_fill_color(app, layer.id).unwrap_or([0, 0, 0]);
        let path = shape::get_shape_path(app, layer.id)?;
        let expanded = expand_subpaths(&path, amount);
        if expanded.is_empty() {
            return Err("오프셋 결과가 없습니다.".into());
        }
        shape::create_shape_from_path(app, &build_expanded_path(&expanded), &ShapeOptions { fill: Some(color), ..Default::default() })
    })
}

/// 각 subpath 를 오프셋한 결과 (closed, 점열)
fn expand_subpaths(path: &ShapePath, offset: f64) -> Vec<(bool, Vec<V>)> {
    // 모든 subpath 의 폴리라인과 면적을 미리 계산
    let data: Vec<(bool, Vec<V>, f64)> = path
        .subpaths
        .iter()
        .filter_map(|sp| {
            let mut pts = subpath_to_polyline(sp);
            if pts.len() < 2 {
                return None;
            }
            if sp.closed && pts.len() > 2 && dist(pts[0], pts[pts.len() - 1]) < 0.01 {
                pts.pop();
            }
            let area = if sp.closed && pts.len() >= 3 { signed_area(&pts) } else { 0.0 };
            Some((sp.closed, pts, area))
        })
        .collect();
    // 포함 관계(중첩 깊이)로 outer/hole 판별 — 감기 방향이 제각각인 병합 레이어에서도 안전
    let depths = nesting_depths(&data.iter().map(|(c, p, _)| (*c, p.as_slice())).collect::<Vec<_>>());
    let mut out = Vec::new();
    for ((closed, pts, area), depth) in data.iter().zip(depths) {
        let mut d = offset;
        if *closed && pts.len() >= 3 && *area != 0.0 {
            // 양수 면적(CCW) + outer → 양수 오프셋 = 확장. 홀수 깊이 = 구멍은 반대
            if *area > 0.0 {
                d = -d;
            }
            if depth % 2 == 1 {
                d = -d;
            }
        }
        let raw = offset_with_joins(pts, d, *closed, "round");
        if *closed {
            let sign = if *area >= 0.0 { 1.0 } else { -1.0 };
            out.extend(resolve_offset_loops(&raw, sign, Some((pts, true)), d.abs()).into_iter().filter(|l| l.len() >= 3).map(|l| (true, l)));
        } else if raw.len() >= 2 {
            out.push((false, raw));
        }
    }
    out
}

// ==================== 베지어 → 폴리라인 ====================

/// subpath 를 폴리라인으로 (직선 구간은 끝점만, 곡선은 적응적 분할)
pub fn subpath_to_polyline(sp: &Subpath) -> Vec<V> {
    let k = &sp.knots;
    let n = k.len();
    let Some(first) = k.first() else { return Vec::new() };
    let mut out = vec![first.anchor];
    let count = if sp.closed { n } else { n - 1 };
    for i in 0..count {
        let (cur, next) = (k[i], k[(i + 1) % n]);
        let (p0, p1, p2, p3) = (cur.anchor, cur.forward, next.backward, next.anchor);
        if dist(p0, p1) < 0.01 && dist(p2, p3) < 0.01 {
            out.push(p3);
        } else {
            // 허용 오차를 작게 유지해야 오프셋이 균일하다
            subdivide_bezier([p0, p1, p2, p3], &mut out, 0.15, 0);
        }
    }
    out
}

fn subdivide_bezier(c: [V; 4], out: &mut Vec<V>, tol: f64, depth: u32) {
    let [p0, p1, p2, p3] = c;
    if depth > 12 || point_line_distance(p1, p0, p3) + point_line_distance(p2, p0, p3) <= tol {
        out.push(p3);
        return;
    }
    // De Casteljau 분할 (t=0.5)
    let (m01, m12, m23) = (mid(p0, p1), mid(p1, p2), mid(p2, p3));
    let (m012, m123) = (mid(m01, m12), mid(m12, m23));
    let m = mid(m012, m123);
    subdivide_bezier([p0, m01, m012, m], out, tol, depth + 1);
    subdivide_bezier([m, m123, m23, p3], out, tol, depth + 1);
}

// ==================== 폴리라인 오프셋 ====================

/// outer/inner 오프셋 루프들 (여러 루프 허용 — 좁은 목이 잘리는 형태 등)
fn compute_outline(polyline: &[V], stroke: &StrokeSettings, closed: bool) -> (Vec<Vec<V>>, Vec<Vec<V>>) {
    let mut poly = polyline.to_vec();
    if closed && poly.len() > 2 && dist(poly[0], poly[poly.len() - 1]) < 0.01 {
        poly.pop();
    }
    if poly.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let w = stroke.width;
    let half = w / 2.0;
    let (d_outer, d_inner) = match stroke.alignment.as_str() {
        "inside" | "inner" => (0.0, w),
        "outside" | "outer" => (w, 0.0),
        _ => (half, half),
    };
    if closed {
        // 경로 방향과 무관하게 양수 = 바깥이 되도록 부호 정규화 (양수 면적 방향에서 +오프셋은 안쪽)
        let area = signed_area(&poly);
        let outward = if area > 0.0 { -1.0 } else { 1.0 };
        let sign = if area >= 0.0 { 1.0 } else { -1.0 };
        let outer_raw = offset_with_joins(&poly, outward * d_outer, true, &stroke.join);
        let inner_raw = offset_with_joins(&poly, -outward * d_inner, true, &stroke.join);
        return (resolve_offset_loops(&outer_raw, sign, Some((&poly, true)), d_outer), resolve_offset_loops(&inner_raw, sign, Some((&poly, true)), d_inner));
    }
    // 열린 경로: outer → endCap → inner(역순) → startCap 으로 하나의 폐곡선
    let d_o = if d_outer == 0.0 { half } else { d_outer };
    let d_i = if d_inner == 0.0 { half } else { d_inner };
    let outer = offset_with_joins(&poly, d_o, false, &stroke.join);
    let mut inner_rev = offset_with_joins(&poly, -d_i, false, &stroke.join);
    inner_rev.reverse();
    let n = poly.len();
    let end_dir = normalize(sub(poly[n - 1], poly[n - 2]));
    let start_dir = normalize(sub(poly[0], poly[1]));
    let (Some(&o_first), Some(&o_last), Some(&i_first), Some(&i_last)) = (outer.first(), outer.last(), inner_rev.first(), inner_rev.last()) else {
        return (Vec::new(), Vec::new());
    };
    let end_cap = make_cap(o_last, i_first, poly[n - 1], &stroke.cap, w, end_dir);
    let start_cap = make_cap(i_last, o_first, poly[0], &stroke.cap, w, start_dir);
    let combined: Vec<V> = outer.iter().chain(&end_cap).chain(&inner_rev).chain(&start_cap).copied().collect();
    let sign = if signed_area(&combined) >= 0.0 { 1.0 } else { -1.0 };
    // 열린 경로도 모든 구성점이 경로에서 최소 오프셋 거리를 유지해야 유효
    (resolve_offset_loops(&combined, sign, Some((&poly, false)), d_o.min(d_i)), Vec::new())
}

/// 반지름·각도에 비례한 원호 분할 수 (현 오차 ≈ 0.05px)
fn arc_steps(da: f64, r: f64) -> usize {
    (da.abs() * r.max(1.0).sqrt() * 1.5).ceil().clamp(3.0, 96.0) as usize
}

fn wrap_angle(mut da: f64) -> f64 {
    if da > PI {
        da -= 2.0 * PI;
    }
    if da < -PI {
        da += 2.0 * PI;
    }
    da
}

/// 정점별 join 처리로 오프셋 경로 생성
fn offset_with_joins(poly: &[V], d: f64, closed: bool, join: &str) -> Vec<V> {
    let n = poly.len();
    if d.abs() < 1e-9 {
        return poly.to_vec();
    }
    let normals = segment_normals(poly);
    let seg_count = if closed { n } else { n - 1 };
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let p = poly[i];
        if !closed && i == 0 {
            out.push(offset_point(p, normals[0], d));
            continue;
        }
        if !closed && i == n - 1 {
            out.push(offset_point(p, normals[seg_count - 1], d));
            continue;
        }
        let prev = if closed { (i + seg_count - 1) % seg_count } else { i - 1 };
        let next = if closed { i % seg_count } else { i };
        let (n1, n2) = (normals[prev], normals[next]);
        let cross = n1[0] * n2[1] - n1[1] * n2[0];
        let dt = n1[0] * n2[0] + n1[1] * n2[1];
        // 거의 평행: 평균 법선
        if cross.abs() < 0.05 && dt > 0.0 {
            out.push(offset_point(p, normalize([n1[0] + n2[0], n1[1] + n2[1]]), d));
            continue;
        }
        let (p1, p2) = (offset_point(p, n1, d), offset_point(p, n2, d));
        let outer_side = if d > 0.0 { cross < 0.0 } else { cross > 0.0 };
        if !outer_side {
            // 안쪽(오목): 두 점을 그대로 — 생기는 교차 루프는 resolve_offset_loops 가 트리밍
            out.push(p1);
            out.push(p2);
            continue;
        }
        let corner = JoinCorner { p, p1, p2, n1, n2, d, prev_pt: poly[(i + n - 1) % n], next_pt: poly[(i + 1) % n] };
        push_join(&mut out, &corner, join);
    }
    out
}

/// 볼록 꺾임의 join 계산 입력
struct JoinCorner {
    p: V,
    p1: V,
    p2: V,
    n1: V,
    n2: V,
    d: f64,
    prev_pt: V,
    next_pt: V,
}

fn push_join(out: &mut Vec<V>, c: &JoinCorner, join: &str) {
    match join {
        "round" => {
            let r = c.d.abs();
            let a1 = (c.p1[1] - c.p[1]).atan2(c.p1[0] - c.p[0]);
            let a2 = (c.p2[1] - c.p[1]).atan2(c.p2[0] - c.p[0]);
            let da = wrap_angle(a2 - a1);
            out.push(c.p1);
            let steps = arc_steps(da, r);
            for s in 1..steps {
                let a = a1 + da * s as f64 / steps as f64;
                out.push([c.p[0] + r * a.cos(), c.p[1] + r * a.sin()]);
            }
            out.push(c.p2);
        }
        "bevel" => {
            out.push(c.p1);
            out.push(c.p2);
        }
        _ => {
            let inter = line_intersection(offset_point(c.prev_pt, c.n1, c.d), c.p1, c.p2, offset_point(c.next_pt, c.n2, c.d));
            match inter.filter(|q| dist(*q, c.p) < c.d.abs() * 4.0) {
                Some(q) => out.push(q),
                None => {
                    out.push(c.p1);
                    out.push(c.p2);
                }
            }
        }
    }
}

/// 각 세그먼트 법선 (마지막은 닫힌 경로용 끝→처음)
fn segment_normals(poly: &[V]) -> Vec<V> {
    let n = poly.len();
    let mut normals: Vec<V> = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt();
        let nn = if l > 0.0 { [-dy / l, dx / l] } else { normals.last().copied().unwrap_or([0.0, -1.0]) };
        normals.push(nn);
    }
    normals
}

/// fromPt → toPt 를 잇는 캡 (out_dir = 경로 바깥 방향)
fn make_cap(from: V, to: V, center: V, cap: &str, width: f64, out_dir: V) -> Vec<V> {
    match cap {
        "round" => {
            let r = width / 2.0;
            let a1 = (from[1] - center[1]).atan2(from[0] - center[0]);
            let a2 = (to[1] - center[1]).atan2(to[0] - center[0]);
            let mut da = wrap_angle(a2 - a1);
            // 호의 중간점이 바깥 방향 쪽에 있도록 회전 방향 결정
            let alt = if da > 0.0 { da - 2.0 * PI } else { da + 2.0 * PI };
            let (m1, m2) = (a1 + da / 2.0, a1 + alt / 2.0);
            if m2.cos() * out_dir[0] + m2.sin() * out_dir[1] > m1.cos() * out_dir[0] + m1.sin() * out_dir[1] {
                da = alt;
            }
            let steps = arc_steps(da, r);
            (1..steps)
                .map(|i| {
                    let a = a1 + da * i as f64 / steps as f64;
                    [center[0] + r * a.cos(), center[1] + r * a.sin()]
                })
                .collect()
        }
        "square" => {
            let e = width / 2.0;
            vec![[from[0] + out_dir[0] * e, from[1] + out_dir[1] * e], [to[0] + out_dir[0] * e, to[1] + out_dir[1] * e]]
        }
        _ => Vec::new(),
    }
}

// ==================== 자기교차 분해 및 유효 루프 선별 ====================

/// 원시 오프셋 폴리곤의 자기교차를 모두 분해한 뒤 유효 루프만 고른다.
/// 1) 방향 필터: 원본과 회전 방향이 같아야 한다 2) 거리 필터: 모든 정점이 원본에서 min_dist 이상
fn resolve_offset_loops(polygon: &[V], expected_sign: f64, src: Option<(&[V], bool)>, min_dist: f64) -> Vec<Vec<V>> {
    let simple = split_self_intersections(polygon);
    let mut valid: Vec<Vec<V>> = simple
        .into_iter()
        .filter(|l| {
            let a = signed_area(l);
            a.abs() > 0.5 && (if a >= 0.0 { 1.0 } else { -1.0 }) == expected_sign
        })
        .collect();
    if let Some((src, src_closed)) = src
        && min_dist > 0.01
    {
        let tol = (min_dist * 0.08).max(0.3);
        valid.retain(|l| l.iter().all(|p| min_dist_to_polyline(*p, src, src_closed) >= min_dist - tol));
    }
    valid
}

/// 교차점에서 두 루프로 나누기를 반복해 단순 폴리곤들로
fn split_self_intersections(polygon: &[V]) -> Vec<Vec<V>> {
    let mut stack = vec![dedupe_loop(polygon)];
    let mut simple = Vec::new();
    let mut guard = 400;
    while let Some(poly) = stack.pop() {
        if guard == 0 {
            break;
        }
        guard -= 1;
        if poly.len() < 3 {
            continue;
        }
        let Some((ea, eb, pt)) = first_self_intersection(&poly) else {
            simple.push(poly);
            continue;
        };
        let n = poly.len();
        let mut l1 = vec![pt];
        l1.extend_from_slice(&poly[ea + 1..=eb]);
        let mut l2 = vec![pt];
        let mut k = (eb + 1) % n;
        while k != (ea + 1) % n {
            l2.push(poly[k]);
            k = (k + 1) % n;
        }
        stack.push(dedupe_loop(&l1));
        stack.push(dedupe_loop(&l2));
    }
    simple
}

fn point_in_polygon(p: V, poly: &[V]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// outer 가 inner 를 포함하는지 — 여러 꼭짓점 다수결
fn contains(outer: &[V], inner: &[V]) -> bool {
    let samples = inner.len().min(9);
    let stride = (inner.len() / samples.max(1)).max(1);
    let tested: Vec<bool> = inner.iter().step_by(stride).take(samples).map(|p| point_in_polygon(*p, outer)).collect();
    tested.iter().filter(|b| **b).count() * 2 > tested.len()
}

/// 닫힌 subpath 들의 중첩 깊이 (홀수 = 구멍). 감기 방향과 무관하게 기하학적 포함 관계로 판별.
fn nesting_depths(data: &[(bool, &[V])]) -> Vec<usize> {
    data.iter()
        .enumerate()
        .map(|(i, (closed, pts))| {
            if !closed || pts.len() < 3 {
                return 0;
            }
            data.iter().enumerate().filter(|(j, (oc, op))| *j != i && *oc && op.len() >= 3 && contains(op, pts)).count()
        })
        .collect()
}

fn min_dist_to_polyline(p: V, poly: &[V], closed: bool) -> f64 {
    let n = poly.len();
    let cnt = if closed { n } else { n.saturating_sub(1) };
    let mut min = f64::INFINITY;
    for i in 0..cnt {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let ab = sub(b, a);
        let l2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
        min = min.min(dist(p, [a[0] + t * ab[0], a[1] + t * ab[1]]));
    }
    min
}

fn dedupe_loop(pts: &[V]) -> Vec<V> {
    const EPS: f64 = 1e-6;
    let same = |a: V, b: V| (a[0] - b[0]).abs() <= EPS && (a[1] - b[1]).abs() <= EPS;
    let mut out: Vec<V> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_none_or(|l| !same(*l, p)) {
            out.push(p);
        }
    }
    if out.len() > 1 && same(out[0], out[out.len() - 1]) {
        out.pop();
    }
    out
}

/// 첫 번째 자기교차 (edge_a, edge_b, 교점) — O(n²)
fn first_self_intersection(pts: &[V]) -> Option<(usize, usize, V)> {
    let n = pts.len();
    for i in 0..n {
        let (a1, a2) = (pts[i], pts[(i + 1) % n]);
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if let Some(p) = seg_seg_intersection(a1, a2, pts[j], pts[(j + 1) % n]) {
                return Some((i, j, p));
            }
        }
    }
    None
}

fn seg_seg_intersection(a1: V, a2: V, b1: V, b2: V) -> Option<V> {
    let (d1, d2) = (sub(a2, a1), sub(b2, b1));
    let cross = d1[0] * d2[1] - d1[1] * d2[0];
    if cross.abs() < 1e-10 {
        return None;
    }
    let d = sub(b1, a1);
    let ta = (d[0] * d2[1] - d[1] * d2[0]) / cross;
    let tb = (d[0] * d1[1] - d[1] * d1[0]) / cross;
    (ta > 0.001 && ta < 0.999 && tb > 0.001 && tb < 0.999).then(|| [a1[0] + ta * d1[0], a1[1] + ta * d1[1]])
}

fn signed_area(poly: &[V]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i][0] * poly[(i + 1) % n][1] - poly[(i + 1) % n][0] * poly[i][1]).sum::<f64>() / 2.0
}

// ==================== 결과 → 셰이프 패스 ====================

/// 루프 → 베지어 피팅 → 정점 (오프셋 점 간격이 불균일하므로 코너 간격 1)
fn loop_subpath(points: &[V], closed: bool, op: PathOp) -> Subpath {
    Subpath { closed, knots: segments_to_knots(&fit_path_segments(points, closed, 0.22, 45.0, 1), closed), op }
}

/// outer 루프들 + inner 루프들(구멍, 역방향)을 하나의 컴포넌트로 추가
fn push_ring_component(out: &mut ShapePath, outer: &[Vec<V>], inner: &[Vec<V>]) {
    let mut first = true;
    let reversed: Vec<Vec<V>> = inner.iter().map(|l| l.iter().rev().copied().collect()).collect();
    for l in outer.iter().chain(&reversed) {
        let sp = loop_subpath(l, true, if first { PathOp::Combine } else { PathOp::Join });
        if sp.knots.len() >= 2 {
            out.subpaths.push(sp);
            first = false;
        }
    }
}

/// 확장 결과 → 패스. 깊이 홀수 = 구멍(subtract). 컨테이너가 구멍보다 먼저 오도록 깊이 오름차순 → 면적 내림차순.
fn build_expanded_path(subpaths: &[(bool, Vec<V>)]) -> ShapePath {
    let depths = nesting_depths(&subpaths.iter().map(|(c, p)| (*c, p.as_slice())).collect::<Vec<_>>());
    // (중첩 깊이, 부호 면적, subpath)
    type Item<'a> = (usize, f64, &'a (bool, Vec<V>));
    let mut items: Vec<Item> = subpaths.iter().zip(depths).map(|(sp, d)| (d, if sp.0 && sp.1.len() >= 3 { signed_area(&sp.1) } else { 0.0 }, sp)).collect();
    items.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.abs().total_cmp(&a.1.abs())));
    ShapePath {
        subpaths: items
            .into_iter()
            .map(|(depth, area, (closed, pts))| {
                let hole = *closed && area != 0.0 && depth % 2 == 1;
                loop_subpath(pts, *closed, if hole { PathOp::Subtract } else { PathOp::Combine })
            })
            .collect(),
        even_odd: false,
    }
}

// ==================== 벡터 유틸 ====================

fn mid(a: V, b: V) -> V {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn dist(a: V, b: V) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}
fn normalize(v: V) -> V {
    let l = v[0].hypot(v[1]);
    if l > 0.0 { [v[0] / l, v[1] / l] } else { [0.0, 0.0] }
}
fn offset_point(p: V, n: V, d: f64) -> V {
    [p[0] + n[0] * d, p[1] + n[1] * d]
}
fn point_line_distance(p: V, a: V, b: V) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx.hypot(dy);
    if l < 1e-10 { dist(p, a) } else { ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / l }
}
fn line_intersection(a: V, b: V, c: V, d: V) -> Option<V> {
    let (d1, d2) = (sub(b, a), sub(d, c));
    let den = d1[0] * d2[1] - d1[1] * d2[0];
    if den.abs() < 1e-10 {
        return None;
    }
    let t = ((c[0] - a[0]) * d2[1] - (c[1] - a[1]) * d2[0]) / den;
    Some([a[0] + t * d1[0], a[1] + t * d1[1]])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cat::lib::core::exec;
    use crate::cat::lib::layer::{all_layers, layer_id_of};
    use crate::cat::tests::{new_doc, test_app};

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
        assert!(p.subpaths.iter().all(|s| s.closed && s.knots.len() == 4), "{p:?}");
        let b = bounds_of(&app, id);
        for (v, e) in b.iter().zip([18.0, 18.0, 82.0, 62.0]) {
            assert!((v - e).abs() < 0.01, "{b:?}");
        }
        // Undo 한 단계
        exec(&mut app, "edit.undo", json!({})).unwrap();
        assert!(all_layers(&app).unwrap().iter().any(|l| l.id == src));
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
