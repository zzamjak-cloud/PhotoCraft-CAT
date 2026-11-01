//! 곡선 맞춤 (CAT lib_curve_fit.js / JP curveFit.ts 대응): 폴리라인 → 3차 베지어. outline_stroke·image_trace 공용.
//! - detect_corners: 진행 방향 회전각 기반 코너 감지
//! - fit_curve / fit_path_segments: 최소자승 3차 베지어 피팅 (Schneider, Graphics Gems)
//! - segments_to_knots: 베지어 구간 → 셰이프 정점 변환
//! - reverse_segments: 구간 시퀀스 역방향 (공유 경계를 양쪽에서 재사용할 때)

use crate::cat::lib::CatResult;
use crate::cat::lib::shape::Knot;

/// 3차 베지어 구간 [p0, c1, c2, p3]
pub type CubicSegment = [[f64; 2]; 4];

type V = [f64; 2];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1]]
}
fn scale(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: V) -> f64 {
    dot(a, a).sqrt()
}
fn norm(a: V) -> V {
    let l = len(a);
    if l > 1e-12 { scale(a, 1.0 / l) } else { [0.0, 0.0] }
}

/// 인접 중복점 제거
fn dedupe_points(pts: &[V]) -> Vec<V> {
    let mut out: Vec<V> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_none_or(|l| (l[0] - p[0]).abs() > 1e-9 || (l[1] - p[1]).abs() > 1e-9) {
            out.push(p);
        }
    }
    out
}

/// 진행 방향 회전각이 angle_deg 이상인 점의 인덱스 (열린 경로는 끝점 제외). 근접 후보 중 가장 날카로운 점만 유지.
pub fn detect_corners(pts: &[V], closed: bool, angle_deg: f64, spacing: usize) -> Vec<usize> {
    let n = pts.len();
    if n < 2 * spacing + 1 {
        return Vec::new();
    }
    let cos_th = angle_deg.to_radians().cos();
    let (start, end) = if closed { (0, n) } else { (spacing, n - spacing) };
    let mut cand: Vec<(usize, f64)> = Vec::new();
    for i in start..end {
        let a = pts[(i + n - spacing) % n];
        let b = pts[i];
        let c = pts[(i + spacing) % n];
        let (v1, v2) = (sub(b, a), sub(c, b));
        let (l1, l2) = (len(v1), len(v2));
        if l1 < 1e-9 || l2 < 1e-9 {
            continue;
        }
        let cs = dot(v1, v2) / (l1 * l2);
        if cs < cos_th {
            cand.push((i, cs));
        }
    }
    cand.sort_by(|p, q| p.1.total_cmp(&q.1));
    let mut taken: Vec<usize> = Vec::new();
    for (i, _) in cand {
        let near = taken.iter().any(|&t| {
            let d = i.abs_diff(t);
            let d = if closed { d.min(n - d) } else { d };
            d <= spacing * 2
        });
        if !near {
            taken.push(i);
        }
    }
    taken.sort_unstable();
    taken
}

/// 직선 구간 (핸들을 1/3 지점에)
pub fn line_segment(a: V, b: V) -> CubicSegment {
    let d = scale(sub(b, a), 1.0 / 3.0);
    [a, add(a, d), sub(b, d), b]
}

/// 점열을 최대 오차 `max_error` 로 베지어 구간들에 맞춘다
pub fn fit_curve(points: &[[f64; 2]], max_error: f64) -> CatResult<Vec<CubicSegment>> {
    Ok(fit_curve_tangents(points, max_error, None, None))
}

/// 끝점 접선을 지정할 수 있는 fit_curve (닫힌 곡선 이음매 연속성용)
fn fit_curve_tangents(points: &[V], max_error: f64, left: Option<V>, right: Option<V>) -> Vec<CubicSegment> {
    let pts = dedupe_points(points);
    match pts.len() {
        0 | 1 => Vec::new(),
        2 => vec![line_segment(pts[0], pts[1])],
        n => {
            let t1 = left.unwrap_or_else(|| norm(sub(pts[1], pts[0])));
            let t2 = right.unwrap_or_else(|| norm(sub(pts[n - 2], pts[n - 1])));
            let mut out = Vec::new();
            fit_cubic_rec(&pts, t1, t2, max_error * max_error, 0, &mut out);
            out
        }
    }
}

fn fit_cubic_rec(pts: &[V], t1: V, t2: V, err_sq: f64, depth: u32, out: &mut Vec<CubicSegment>) {
    let n = pts.len();
    if n == 2 {
        out.push(line_segment(pts[0], pts[1]));
        return;
    }
    let mut u = chord_length_parameterize(pts);
    let mut bez = generate_bezier(pts, &u, t1, t2);
    let (mut max_sq, mut split) = compute_max_error(pts, &bez, &u);
    if max_sq < err_sq {
        out.push(bez);
        return;
    }
    // 오차가 크지 않으면 Newton-Raphson 파라미터 재조정 후 재피팅
    if max_sq < err_sq * 16.0 {
        for _ in 0..4 {
            u = u.iter().zip(pts).map(|(&ui, &p)| newton_raphson(&bez, p, ui)).collect();
            bez = generate_bezier(pts, &u, t1, t2);
            (max_sq, split) = compute_max_error(pts, &bez, &u);
            if max_sq < err_sq {
                out.push(bez);
                return;
            }
        }
    }
    if depth > 32 {
        out.push(bez);
        return;
    }
    // 최대 오차 지점에서 분할, 중앙 접선으로 곡률 연속성 유지
    let split = split.clamp(1, n - 2);
    let tc = center_tangent(pts, split);
    fit_cubic_rec(&pts[..=split], t1, tc, err_sq, depth + 1, out);
    fit_cubic_rec(&pts[split..], scale(tc, -1.0), t2, err_sq, depth + 1, out);
}

fn center_tangent(pts: &[V], i: usize) -> V {
    let prev = pts[i.saturating_sub(1)];
    let mut v = sub(prev, pts[(i + 1).min(pts.len() - 1)]);
    if len(v) < 1e-12 {
        v = sub(prev, pts[i]);
    }
    norm(v)
}

fn chord_length_parameterize(pts: &[V]) -> Vec<f64> {
    let mut u = vec![0.0];
    for w in pts.windows(2) {
        let last = u.last().copied().unwrap_or(0.0);
        u.push(last + len(sub(w[1], w[0])));
    }
    let total = u.last().copied().filter(|t| *t != 0.0).unwrap_or(1.0);
    u.iter().map(|v| v / total).collect()
}

/// 최소자승으로 양 끝 핸들 길이 결정
fn generate_bezier(pts: &[V], u: &[f64], t1: V, t2: V) -> CubicSegment {
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (&t, &p) in u.iter().zip(pts) {
        let mt = 1.0 - t;
        let a0 = scale(t1, 3.0 * t * mt * mt);
        let a1 = scale(t2, 3.0 * t * t * mt);
        c00 += dot(a0, a0);
        c01 += dot(a0, a1);
        c11 += dot(a1, a1);
        let (b0, b1, b2, b3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
        let tmp = sub(p, add(scale(first, b0 + b1), scale(last, b2 + b3)));
        x0 += dot(a0, tmp);
        x1 += dot(a1, tmp);
    }
    let det = c00 * c11 - c01 * c01;
    let (mut alpha1, mut alpha2) = (0.0, 0.0);
    if det.abs() > 1e-12 {
        alpha1 = (x0 * c11 - x1 * c01) / det;
        alpha2 = (c00 * x1 - c01 * x0) / det;
    }
    // 특이/비정상 해는 Wu-Barsky 휴리스틱(현 길이 1/3)으로 대체
    let seg = len(sub(last, first));
    let eps = 1e-6 * seg;
    if alpha1 < eps || alpha2 < eps || alpha1 > seg * 4.0 || alpha2 > seg * 4.0 {
        alpha1 = seg / 3.0;
        alpha2 = alpha1;
    }
    [first, add(first, scale(t1, alpha1)), add(last, scale(t2, alpha2)), last]
}

/// 3차 베지어 위치
pub fn eval_bezier(b: &CubicSegment, t: f64) -> V {
    let mt = 1.0 - t;
    let (a, bb, c, d) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
    [a * b[0][0] + bb * b[1][0] + c * b[2][0] + d * b[3][0], a * b[0][1] + bb * b[1][1] + c * b[2][1] + d * b[3][1]]
}

fn pt_seg_dist_sq(p: V, a: V, b: V) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    let t = if l2 > 1e-12 { (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let d = sub(p, add(a, scale(ab, t)));
    dot(d, d)
}

/// 피팅 곡선과 원본 점 사이 최대 제곱 오차와 위치
fn compute_max_error(pts: &[V], bez: &CubicSegment, u: &[f64]) -> (f64, usize) {
    let n = pts.len();
    let mut max_sq = 0.0;
    let mut split = n / 2;
    for i in 1..n - 1 {
        let d = sub(eval_bezier(bez, u[i]), pts[i]);
        let dsq = dot(d, d);
        if dsq > max_sq {
            max_sq = dsq;
            split = i;
        }
    }
    // 점 사이 중간 파라미터에서도 검사 — 성긴 구간에서 곡선이 활처럼 휘는 것을 놓치지 않도록
    for i in 0..n - 1 {
        let q = eval_bezier(bez, (u[i] + u[i + 1]) / 2.0);
        let dsq = pt_seg_dist_sq(q, pts[i], pts[i + 1]);
        if dsq > max_sq {
            max_sq = dsq;
            split = (i + 1).clamp(1, n - 2);
        }
    }
    (max_sq, split)
}

/// Newton-Raphson 으로 점 p 에 가장 가까운 곡선 파라미터 보정
fn newton_raphson(b: &CubicSegment, p: V, u: f64) -> f64 {
    let q = eval_bezier(b, u);
    let d1 = [scale(sub(b[1], b[0]), 3.0), scale(sub(b[2], b[1]), 3.0), scale(sub(b[3], b[2]), 3.0)];
    let d2 = [scale(sub(d1[1], d1[0]), 2.0), scale(sub(d1[2], d1[1]), 2.0)];
    let mt = 1.0 - u;
    let q1 = add(add(scale(d1[0], mt * mt), scale(d1[1], 2.0 * mt * u)), scale(d1[2], u * u));
    let q2 = add(scale(d2[0], mt), scale(d2[1], u));
    let diff = sub(q, p);
    let num = dot(diff, q1);
    let den = dot(q1, q1) + dot(diff, q2);
    if den.abs() < 1e-12 { u } else { (u - num / den).clamp(0.0, 1.0) }
}

/// 폴리라인 전체를 코너에서 분할해 구간별로 피팅한 베지어 구간 시퀀스.
/// closed 면 닫힌 윤곽으로 처리 (코너가 없으면 이음매도 매끄럽게 피팅).
pub fn fit_path_segments(pts_in: &[V], closed: bool, max_error: f64, corner_angle_deg: f64, corner_spacing: usize) -> Vec<CubicSegment> {
    let mut pts = dedupe_points(pts_in);
    if closed && pts.len() > 1 {
        let (f, l) = (pts[0], pts[pts.len() - 1]);
        if (f[0] - l[0]).abs() < 1e-9 && (f[1] - l[1]).abs() < 1e-9 {
            pts.pop();
        }
    }
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    if n == 2 {
        let seg = line_segment(pts[0], pts[1]);
        return if closed { vec![seg, line_segment(pts[1], pts[0])] } else { vec![seg] };
    }
    if !closed {
        let mut bounds = vec![0];
        bounds.extend(detect_corners(&pts, false, corner_angle_deg, corner_spacing));
        bounds.push(n - 1);
        return fit_sections(&pts, &bounds, max_error);
    }
    let corners = detect_corners(&pts, true, corner_angle_deg, corner_spacing);
    let Some(&s) = corners.first() else {
        // 코너 없는 닫힌 곡선: 시작점 접선을 중앙차분으로 맞춰 이음매를 매끄럽게
        let mut ring = pts.clone();
        ring.push(pts[0]);
        let t = norm(sub(pts[1], pts[n - 1]));
        return fit_curve_tangents(&ring, max_error, Some(t), Some(scale(t, -1.0)));
    };
    // 첫 코너가 시작점이 되도록 회전 후 코너 구간별 피팅
    let mut ring: Vec<V> = pts[s..].iter().chain(&pts[..s]).copied().collect();
    ring.push(ring[0]);
    let mut bounds = vec![0];
    let mut rest: Vec<usize> = corners.iter().skip(1).map(|&c| (c + n - s) % n).collect();
    rest.sort_unstable();
    bounds.extend(rest);
    bounds.push(n);
    fit_sections(&ring, &bounds, max_error)
}

fn fit_sections(pts: &[V], bounds: &[usize], max_error: f64) -> Vec<CubicSegment> {
    let mut segs = Vec::new();
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        let Some(section) = pts.get(a..=b).filter(|_| b > a) else { continue };
        if section.len() == 2 {
            segs.push(line_segment(section[0], section[1]));
        } else {
            segs.extend(fit_curve_tangents(section, max_error, None, None));
        }
    }
    segs
}

/// 두 핸들이 앵커 기준 거의 정반대 방향이면 smooth (사실상 일직선일 때만)
fn is_smooth_joint(h_in: V, anchor: V, h_out: V) -> bool {
    let (v1, v2) = (sub(h_in, anchor), sub(h_out, anchor));
    let (l1, l2) = (len(v1), len(v2));
    l1 >= 1e-9 && l2 >= 1e-9 && dot(v1, v2) / (l1 * l2) < -0.999
}

/// 베지어 구간 시퀀스 → 셰이프 정점 (closed 면 마지막 구간 끝이 첫 정점으로 이어진다)
pub fn segments_to_knots(segments: &[CubicSegment], closed: bool) -> Vec<Knot> {
    let mut knots = Vec::with_capacity(segments.len() + 1);
    for (i, seg) in segments.iter().enumerate() {
        let prev = if i > 0 {
            segments.get(i - 1)
        } else if closed {
            segments.last()
        } else {
            None
        };
        let backward = prev.map_or(seg[0], |p| p[2]);
        knots.push(Knot { anchor: seg[0], backward, forward: seg[1], smooth: prev.is_some() && is_smooth_joint(backward, seg[0], seg[1]) });
    }
    if let Some(last) = segments.last().filter(|_| !closed) {
        knots.push(Knot { anchor: last[3], backward: last[2], forward: last[3], smooth: false });
    }
    knots
}

/// 구간 시퀀스를 역방향으로
pub fn reverse_segments(segments: &[CubicSegment]) -> Vec<CubicSegment> {
    segments.iter().rev().map(|s| [s[3], s[2], s[1], s[0]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_circle_with_few_segments() {
        let pts: Vec<V> = (0..200).map(|i| (i as f64) / 200.0 * std::f64::consts::TAU).map(|a| [50.0 + 40.0 * a.cos(), 50.0 + 40.0 * a.sin()]).collect();
        let segs = fit_path_segments(&pts, true, 0.5, 60.0, 2);
        assert!((2..=8).contains(&segs.len()), "{}", segs.len());
        // 모든 구간 위의 점이 원 위에 있어야 한다
        for s in &segs {
            for k in 0..=10 {
                let p = eval_bezier(s, k as f64 / 10.0);
                let r = len(sub(p, [50.0, 50.0]));
                assert!((r - 40.0).abs() < 1.0, "r={r}");
            }
        }
        let knots = segments_to_knots(&segs, true);
        assert_eq!(knots.len(), segs.len());
        assert!(knots.iter().all(|k| k.smooth));
    }

    #[test]
    fn square_keeps_corners() {
        let mut pts = Vec::new();
        for i in 0..10 {
            pts.push([i as f64, 0.0]);
        }
        for i in 0..10 {
            pts.push([10.0, i as f64]);
        }
        for i in 0..10 {
            pts.push([10.0 - i as f64, 10.0]);
        }
        for i in 0..10 {
            pts.push([0.0, 10.0 - i as f64]);
        }
        let segs = fit_path_segments(&pts, true, 0.3, 45.0, 1);
        let knots = segments_to_knots(&segs, true);
        assert_eq!(knots.len(), 4);
        let mut anchors: Vec<V> = knots.iter().map(|k| k.anchor).collect();
        anchors.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        assert_eq!(anchors, vec![[0.0, 0.0], [0.0, 10.0], [10.0, 0.0], [10.0, 10.0]]);
        assert_eq!(reverse_segments(&segs).len(), 4);
        assert_eq!(fit_curve(&[[0.0, 0.0], [5.0, 0.0]], 1.0).unwrap().len(), 1);
    }
}
