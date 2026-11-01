//! Image trace (CAT imageTrace.js 대응): 픽셀 → 색상별 셰이프 레이어.
//!
//! 공유 경계 아크 방식 (원본 알고리즘 그대로):
//! 1) 모든 비투명 픽셀을 색상 인덱스에 배정 → 작은 영역은 인접 색으로 병합
//! 2) 픽셀 코너 격자에서 색상 경계 에지 그래프 구축 (차수≠2 정점 = 정션)
//! 3) 정션 사이 에지 체인(아크)을 한 번만 스무딩 + 베지어 피팅 (정션 좌표 고정)
//! 4) 색상별 윤곽은 아크를 정/역방향으로 이어붙여 재구성 → 인접 색상이 같은 곡선을 공유해 틈이 없다
//!
//! 픽셀 입력은 문서 모델을 직접 읽는다(`lib::pixels`).

use std::collections::HashMap;

use photocraft_doc::LayerId;

use super::curve_fit::{CubicSegment, fit_path_segments, reverse_segments, segments_to_knots};
use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::doc::require_doc;
use crate::cat::lib::layer::{active_layer, group_selected_layers, select_layers};
use crate::cat::lib::pixels::get_layer_pixels;
use crate::cat::lib::shape::{PathOp, ShapeOptions, ShapePath, Subpath, create_shape_from_path};

/// 트레이스 설정 (UI 가 입력받는다)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceSettings {
    /// 유사 색상 병합 강도 0..=100
    pub color_tolerance: f64,
    /// 경로 단순화 0..=5
    pub path_tolerance: f64,
}

impl Default for TraceSettings {
    /// CAT 다이얼로그 기본값
    fn default() -> Self {
        Self { color_tolerance: 10.0, path_tolerance: 0.5 }
    }
}

const MAX_COLORS: usize = 256;

/// 팔레트 색 (평균 RGB + 픽셀 수)
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pal {
    r: i32,
    g: i32,
    b: i32,
    count: f64,
}

impl Pal {
    fn dist(&self, o: &Pal) -> f64 {
        let (dr, dg, db) = (f64::from(self.r - o.r), f64::from(self.g - o.g), f64::from(self.b - o.b));
        (dr * dr + dg * dg + db * db).sqrt()
    }
    /// 가중 평균으로 합치기
    fn absorb(&mut self, o: &Pal) {
        let total = self.count + o.count;
        let mix = |a: i32, b: i32| ((f64::from(a) * self.count + f64::from(b) * o.count) / total).round() as i32;
        (self.r, self.g, self.b) = (mix(self.r, o.r), mix(self.g, o.g), mix(self.b, o.b));
        self.count = total;
    }
}

/// 활성 레이어 트레이스 → 생성된 셰이프 레이어 ID (2개 이상이면 `<이름>_trace` 그룹으로 묶는다)
pub fn run(app: &mut PhotocraftApp, settings: TraceSettings) -> CatResult<Vec<LayerId>> {
    let doc = require_doc(app)?;
    let layer = active_layer(app).ok_or("선택된 레이어가 없습니다.")?;
    let (dw, dh) = (doc.width as i32, doc.height as i32);
    // 레이어 bounds 기준 (캔버스 밖은 잘라낸다)
    let (bx, by, bw, bh) = layer.bounds.map_or((0, 0, dw, dh), |b| (b.x, b.y, b.w, b.h));
    let (ox, oy) = (bx.max(0), by.max(0));
    let (w, h) = ((bx + bw).min(dw) - ox, (by + bh).min(dh) - oy);
    if w <= 0 || h <= 0 {
        return Err("레이어에 픽셀이 없습니다.".into());
    }
    let full = get_layer_pixels(app, layer.id)?;
    let (w, h) = (w as usize, h as usize);
    let mut raw = vec![0u8; w * h * 4];
    for y in 0..h {
        let so = ((y + oy as usize) * full.width as usize + ox as usize) * 4;
        if let (Some(dst), Some(src)) = (raw.get_mut(y * w * 4..(y + 1) * w * 4), full.data.get(so..so + w * 4)) {
            dst.copy_from_slice(src);
        }
    }
    let contours = trace_rgba(raw, w, h, settings);
    let origin = [f64::from(ox), f64::from(oy)];
    with_history(app, |app| {
        let mut ids = Vec::new();
        for (color, path) in contours {
            let path = offset_path(path, origin);
            ids.push(create_shape_from_path(app, &path, &ShapeOptions { fill: Some(color), ..Default::default() })?);
        }
        if ids.len() > 1 {
            select_layers(app, &ids)?;
            group_selected_layers(app, &format!("{}_trace", layer.name))?;
        }
        Ok(ids)
    })
}

fn offset_path(mut p: ShapePath, [ox, oy]: [f64; 2]) -> ShapePath {
    for k in p.subpaths.iter_mut().flat_map(|s| s.knots.iter_mut()) {
        for q in [&mut k.anchor, &mut k.backward, &mut k.forward] {
            q[0] += ox;
            q[1] += oy;
        }
    }
    p
}

/// RGBA(w×h) 를 색상별 패스로 트레이스 (좌표는 이미지 기준) → (색, 패스) 목록
pub fn trace_rgba(mut raw: Vec<u8>, w: usize, h: usize, settings: TraceSettings) -> Vec<([u8; 3], ShapePath)> {
    // 투명 픽셀 RGB 를 인접 불투명 색으로 — 확대 시 어두운 테두리 색이 별도 레이어로 생기는 것 방지
    bleed_transparent_rgb(&mut raw, w, h, 2);
    // 작은 이미지는 2배 확대 후 트레이스 (결과 기준 오차가 절반)
    let ss = if w.max(h) <= 1200 { 2 } else { 1 };
    let (tw, th) = (w * ss, h * ss);
    if ss == 2 {
        raw = upscale2x(&raw, w, h);
    }
    let scale = 1.0 / ss as f64;
    let pixels = preprocess_pixels(&raw);
    let mut colors = quantize_colors(&pixels, settings.color_tolerance);
    let min_area = ((tw * th) as f64 * 0.0001).max(4.0);
    let mut map = build_color_index_map(&pixels, &colors);
    absorb_mixture_colors(&mut map, tw, th, &colors, settings.color_tolerance, ss);
    colors = merge_gradient_banding(&mut map, tw, th, &colors, &pixels, settings.color_tolerance);
    merge_small_regions(&mut map, tw, th, min_area, 3, Some(&colors));

    // 피팅 허용 오차는 트레이스 공간 기준이므로 배율을 곱해 결과 기준 단순화 강도를 유지
    let fit_error = (0.4 + settings.path_tolerance * 0.6).max(0.3) * ss as f64;
    let mut graph = Graph::build(&map, tw, th, colors.len());
    graph.smooth(3);
    let mut arcs = graph.extract_arcs();
    for arc in &mut arcs.arcs {
        graph.fit_arc(arc, fit_error);
    }
    let mut out = Vec::new();
    for (ci, c) in colors.iter().enumerate() {
        let edges = graph.color_edges.get(ci).map(Vec::as_slice).unwrap_or_default();
        let contours: Vec<Contour> = link_color_loops(edges, graph.w1).iter().filter_map(|l| graph.loop_to_contour(l, edges, &arcs)).collect();
        if contours.is_empty() {
            continue;
        }
        let rgb = [c.r.clamp(0, 255) as u8, c.g.clamp(0, 255) as u8, c.b.clamp(0, 255) as u8];
        out.push((rgb, build_trace_path(contours, scale)));
    }
    out
}

// ==================== 픽셀 전처리 ====================

fn neighbors4(i: usize, w: usize, h: usize) -> [Option<usize>; 4] {
    let (x, y) = (i % w, i / w);
    [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)]
}

/// alpha=0 픽셀 RGB 를 8-이웃 불투명(또는 이전 패스에서 채운) 픽셀 평균으로 채운다
fn bleed_transparent_rgb(px: &mut [u8], w: usize, h: usize, passes: usize) {
    let mut filled: Vec<bool> = (0..w * h).map(|i| px[i * 4 + 3] > 0).collect();
    for _ in 0..passes {
        let mut updates: Vec<(usize, [u8; 3])> = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if filled[i] {
                    continue;
                }
                let (mut s, mut n) = ([0u32; 3], 0u32);
                for ny in y.saturating_sub(1)..(y + 2).min(h) {
                    for nx in x.saturating_sub(1)..(x + 2).min(w) {
                        let j = ny * w + nx;
                        if filled[j] {
                            for (k, v) in s.iter_mut().enumerate() {
                                *v += u32::from(px[j * 4 + k]);
                            }
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    updates.push((i, s.map(|v| (f64::from(v) / f64::from(n)).round() as u8)));
                }
            }
        }
        if updates.is_empty() {
            break;
        }
        for (i, c) in updates {
            px[i * 4..i * 4 + 3].copy_from_slice(&c);
            filled[i] = true;
        }
    }
}

/// Catmull-Rom 바이큐빅 정확히 2배 확대 (경계에 서브픽셀 위치 정보가 생겨 매끄러운 경계 추출)
fn upscale2x(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    const W25: [f32; 4] = [-0.070_312_5, 0.867_187_5, 0.226_562_5, -0.023_437_5];
    const W75: [f32; 4] = [-0.023_437_5, 0.226_562_5, 0.867_187_5, -0.070_312_5];
    let taps = |n: usize| -> Vec<([usize; 4], [f32; 4])> {
        (0..n * 2)
            .map(|o| {
                let s = o as f64 * 0.5 - 0.25;
                let i0 = s.floor();
                let wt = if s - i0 < 0.5 { W25 } else { W75 };
                let idx = [0, 1, 2, 3].map(|k| (i0 as i64 - 1 + k).clamp(0, n as i64 - 1) as usize);
                (idx, wt)
            })
            .collect()
    };
    let (tx, ty) = (taps(w), taps(h));
    let (ww, hh) = (w * 2, h * 2);
    // 수평 패스
    let mut tmp = vec![0f32; ww * h * 4];
    for y in 0..h {
        for (x, (idx, wt)) in tx.iter().enumerate() {
            for c in 0..4 {
                tmp[(y * ww + x) * 4 + c] = (0..4).map(|k| wt[k] * f32::from(src[(y * w + idx[k]) * 4 + c])).sum();
            }
        }
    }
    // 수직 패스
    let stride = ww * 4;
    let mut out = vec![0u8; ww * hh * 4];
    for (y, (idx, wt)) in ty.iter().enumerate() {
        for i in 0..stride {
            let v: f32 = (0..4).map(|k| wt[k] * tmp[idx[k] * stride + i]).sum();
            out[y * stride + i] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// 알파 이진화 (반투명 → 투명, 불투명 → 원색 유지 + 알파 255)
fn preprocess_pixels(px: &[u8]) -> Vec<u8> {
    px.chunks_exact(4).flat_map(|p| if p[3] < 128 { [0, 0, 0, 0] } else { [p[0], p[1], p[2], 255] }).collect()
}

/// 색상 양자화: 스냅 버킷 → 원본 평균색 → 유사색 병합 (상위 MAX_COLORS)
fn quantize_colors(px: &[u8], tolerance: f64) -> Vec<Pal> {
    let snap = |v: u8| ((f64::from(v) / 4.0).round() * 4.0) as i32;
    let mut index: HashMap<(i32, i32, i32), usize> = HashMap::new();
    let mut buckets: Vec<(u64, [u64; 3])> = Vec::new();
    for p in px.chunks_exact(4).filter(|p| p[3] >= 10) {
        let key = (snap(p[0]), snap(p[1]), snap(p[2]));
        let i = *index.entry(key).or_insert_with(|| {
            buckets.push((0, [0; 3]));
            buckets.len() - 1
        });
        if let Some(b) = buckets.get_mut(i) {
            b.0 += 1;
            for (s, v) in b.1.iter_mut().zip(p) {
                *s += u64::from(*v);
            }
        }
    }
    let mut colors: Vec<Pal> = buckets
        .iter()
        .map(|(n, s)| {
            let avg = |v: u64| (v as f64 / *n as f64).round() as i32;
            Pal { r: avg(s[0]), g: avg(s[1]), b: avg(s[2]), count: *n as f64 }
        })
        .collect();
    colors.sort_by(|a, b| b.count.total_cmp(&a.count));
    let tol = tolerance * 2.55;
    let mut merged: Vec<Pal> = Vec::new();
    for c in colors {
        match merged.iter_mut().find(|m| c.dist(m) <= tol) {
            Some(m) => m.absorb(&c),
            None => merged.push(c),
        }
    }
    merged.sort_by(|a, b| b.count.total_cmp(&a.count));
    merged.truncate(MAX_COLORS);
    merged
}

/// 모든 비투명 픽셀을 가장 가까운 색상에 배정 (-1 = 투명)
fn build_color_index_map(px: &[u8], colors: &[Pal]) -> Vec<i32> {
    px.chunks_exact(4)
        .map(|p| {
            if p[3] < 10 {
                return -1;
            }
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            let mut best = (i32::MAX, 0usize);
            for (ci, c) in colors.iter().enumerate() {
                let d = (r - c.r).pow(2) + (g - c.g).pow(2) + (b - c.b).pow(2);
                if d < best.0 {
                    best = (d, ci);
                }
            }
            best.1 as i32
        })
        .collect()
}

/// RGB 공간에서 점 c 와 선분 a-b 사이 거리
fn dist_to_rgb_segment(c: &Pal, a: &Pal, b: &Pal) -> f64 {
    let ab = [f64::from(b.r - a.r), f64::from(b.g - a.g), f64::from(b.b - a.b)];
    let ac = [f64::from(c.r - a.r), f64::from(c.g - a.g), f64::from(c.b - a.b)];
    let l2: f64 = ab.iter().map(|v| v * v).sum();
    let t = if l2 > 0.0 { (ac.iter().zip(&ab).map(|(x, y)| x * y).sum::<f64>() / l2).clamp(0.0, 1.0) } else { 0.0 };
    ac.iter().zip(&ab).map(|(x, y)| (x - t * y).powi(2)).sum::<f64>().sqrt()
}

// ==================== 안티에일리어싱 혼합색 흡수 ====================

/// 두 상위 색상 사이(RGB 선분 근처)에 있으면서 얇은 색 영역을 전이색으로 판정해 인접 영역으로 재배정
fn absorb_mixture_colors(map: &mut [i32], w: usize, h: usize, colors: &[Pal], tolerance: f64, step: usize) {
    let n = colors.len();
    if n < 3 {
        return;
    }
    let mix_tol = (tolerance * 2.55).max(20.0);
    let Some(candidate) = mixture_candidates(map, colors, mix_tol) else { return };
    let weak = weak_components(map, w, h, colors, &candidate, mix_tol, step);
    if weak.iter().any(|b| *b) {
        regrow_weak(map, w, h, weak);
    }
}

/// 전역 프리필터: 상위 색 쌍의 RGB 선분 근처 색 = 혼합 후보. 후보가 없으면 None.
fn mixture_candidates(map: &[i32], colors: &[Pal], mix_tol: f64) -> Option<Vec<bool>> {
    let n = colors.len();
    let mut count = vec![0usize; n];
    for &c in map.iter().filter(|c| **c >= 0) {
        if let Some(v) = count.get_mut(c as usize) {
            *v += 1;
        }
    }
    let mut order: Vec<usize> = (0..n).filter(|&c| count[c] > 0).collect();
    order.sort_by(|a, b| count[*b].cmp(&count[*a]));
    let mut strong: Vec<usize> = Vec::new();
    let mut candidate = vec![false; n];
    for &ci in &order {
        let c = &colors[ci];
        let is_mix = strong.iter().enumerate().any(|(i, &a)| strong[i + 1..].iter().any(|&b| dist_to_rgb_segment(c, &colors[a], &colors[b]) < mix_tol));
        if is_mix {
            candidate[ci] = true;
        } else {
            strong.push(ci);
        }
    }
    (strong.len() != order.len()).then_some(candidate)
}

/// 후보 색의 연결 컴포넌트별로 (a) 얇음 (b) 실제 인접 두 색의 전이 를 검증해 흡수 대상 픽셀 표시
fn weak_components(map: &[i32], w: usize, h: usize, colors: &[Pal], candidate: &[bool], mix_tol: f64, step: usize) -> Vec<bool> {
    let total = w * h;
    let d = step * 2;
    let mut weak = vec![false; total];
    let mut labeled = vec![false; total];
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..total {
        let ci = map[start];
        if labeled[start] || ci < 0 || !candidate[ci as usize] {
            continue;
        }
        labeled[start] = true;
        stack.push(start);
        let mut comp = Vec::new();
        let mut deep = 0usize;
        let mut nbr: Vec<(i32, usize)> = Vec::new();
        while let Some(cur) = stack.pop() {
            comp.push(cur);
            let (x, y) = (cur % w, cur / w);
            if x >= d && y >= d && x + d < w && y + d < h && [cur - d, cur + d, cur - d * w, cur + d * w].iter().all(|&j| map[j] == ci) {
                deep += 1;
            }
            for nb in neighbors4(cur, w, h).into_iter().flatten() {
                let nc = map[nb];
                if nc == ci {
                    if !labeled[nb] {
                        labeled[nb] = true;
                        stack.push(nb);
                    }
                } else {
                    bump(&mut nbr, nc);
                }
            }
        }
        // 뭉쳐 있는 실제 디테일은 유지
        if deep as f64 / comp.len() as f64 >= 0.2 {
            continue;
        }
        if is_transition(&colors[ci as usize], &nbr, colors, mix_tol) {
            for i in comp {
                weak[i] = true;
            }
        }
    }
    weak
}

/// 삽입 순서를 유지하는 카운터 증가 (JS Map 순회 순서와 같게)
fn bump(counts: &mut Vec<(i32, usize)>, key: i32) {
    match counts.iter_mut().find(|(k, _)| *k == key) {
        Some(e) => e.1 += 1,
        None => counts.push((key, 1)),
    }
}

fn is_transition(c: &Pal, nbr: &[(i32, usize)], colors: &[Pal], mix_tol: f64) -> bool {
    // 인접 상위 두 색 (투명 제외)
    let (mut n1, mut c1, mut n2, mut c2, mut colored) = (-1i32, 0usize, -1i32, 0usize, 0usize);
    for &(nc, cnt) in nbr.iter().filter(|(nc, _)| *nc >= 0) {
        colored += cnt;
        if cnt > c1 {
            (n2, c2, n1, c1) = (n1, c1, nc, cnt);
        } else if cnt > c2 {
            (n2, c2) = (nc, cnt);
        }
    }
    let (Some(a), b) = (colors.get(n1.max(0) as usize).filter(|_| n1 >= 0), colors.get(n2.max(0) as usize).filter(|_| n2 >= 0)) else {
        return false;
    };
    match b {
        // 두 색 사이에 낀 띠: 그 두 색의 실제 혼합일 때만 전이
        Some(b) if (c1 + c2) as f64 >= colored as f64 * 0.7 => dist_to_rgb_segment(c, a, b) < mix_tol,
        Some(_) => false,
        // 한 색에만 둘러싸인 얇은 조각: 유사색 프린지만 흡수
        None => c.dist(a) < mix_tol,
    }
}

/// 웨이브 단위 다중 소스 성장으로 재배정 — 라운드마다 확정 이웃만 보고 일괄 배정해 대칭 성장
fn regrow_weak(map: &mut [i32], w: usize, h: usize, mut weak: Vec<bool>) {
    let total = w * h;
    let mut in_frontier = vec![false; total];
    let mut frontier: Vec<usize> = (0..total).filter(|&i| weak[i] && neighbors4(i, w, h).into_iter().flatten().any(|j| !weak[j])).collect();
    for &i in &frontier {
        in_frontier[i] = true;
    }
    let mut remaining = weak.iter().filter(|b| **b).count();
    let mut guard = w + h + 8;
    while !frontier.is_empty() && remaining > 0 && guard > 0 {
        guard -= 1;
        let mut assigned: Vec<(usize, i32)> = Vec::new();
        for &i in frontier.iter().filter(|&&i| weak[i]) {
            let mut counts: Vec<(i32, usize)> = Vec::new();
            let (mut best, mut best_cnt, mut transparent) = (None, 0usize, 0usize);
            for nb in neighbors4(i, w, h).into_iter().flatten().filter(|&j| !weak[j]) {
                let nc = map[nb];
                if nc == -1 {
                    transparent += 1;
                    continue;
                }
                bump(&mut counts, nc);
                let cnt = counts.iter().find(|(k, _)| *k == nc).map_or(0, |e| e.1);
                if cnt > best_cnt {
                    best_cnt = cnt;
                    best = Some(nc);
                }
            }
            match best {
                Some(b) => assigned.push((i, b)),
                None if transparent > 0 => assigned.push((i, -1)),
                None => {}
            }
        }
        if assigned.is_empty() {
            break;
        }
        for &(i, c) in &assigned {
            map[i] = c;
            weak[i] = false;
            remaining -= 1;
        }
        let mut next = Vec::new();
        for &(i, _) in &assigned {
            for nb in neighbors4(i, w, h).into_iter().flatten() {
                if weak[nb] && !in_frontier[nb] {
                    in_frontier[nb] = true;
                    next.push(nb);
                }
            }
        }
        frontier = next;
    }
}

// ==================== 그라데이션 밴딩 병합 ====================

/// 4-연결 영역 라벨링 → (픽셀별 영역, 영역별 색 인덱스, 영역 크기)
fn label_regions(map: &[i32], w: usize, h: usize) -> (Vec<i32>, Vec<usize>, Vec<usize>) {
    let total = w * h;
    let mut region = vec![-1i32; total];
    let (mut rcolor, mut rsize) = (Vec::new(), Vec::new());
    let mut stack = Vec::new();
    for start in 0..total {
        if region[start] >= 0 || map[start] < 0 {
            continue;
        }
        let ci = map[start];
        let rid = rcolor.len() as i32;
        region[start] = rid;
        stack.push(start);
        let mut size = 0;
        while let Some(cur) = stack.pop() {
            size += 1;
            for nb in neighbors4(cur, w, h).into_iter().flatten() {
                if region[nb] < 0 && map[nb] == ci {
                    region[nb] = rid;
                    stack.push(nb);
                }
            }
        }
        rcolor.push(ci as usize);
        rsize.push(size);
    }
    (region, rcolor, rsize)
}

/// 인접 영역 쌍별 경계 대비 (원본 픽셀 RGB 차 평균) — 삽입 순서 유지
fn region_pair_softness(region: &[i32], px: &[u8], w: usize, h: usize) -> Vec<((usize, usize), f64)> {
    let mut index: HashMap<(usize, usize), usize> = HashMap::new();
    let mut stats: Vec<((usize, usize), usize, f64)> = Vec::new();
    let mut measure = |i: usize, j: usize| {
        let (ra, rb) = (region[i], region[j]);
        if ra < 0 || rb < 0 || ra == rb {
            return;
        }
        let d = (0..3).map(|k| (f64::from(px[i * 4 + k]) - f64::from(px[j * 4 + k])).powi(2)).sum::<f64>().sqrt();
        let key = (ra.min(rb) as usize, ra.max(rb) as usize);
        let k = *index.entry(key).or_insert_with(|| {
            stats.push((key, 0, 0.0));
            stats.len() - 1
        });
        if let Some(s) = stats.get_mut(k) {
            s.1 += 1;
            s.2 += d;
        }
    };
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if region[i] < 0 {
                continue;
            }
            if x + 1 < w {
                measure(i, i + 1);
            }
            if y + 1 < h {
                measure(i, i + w);
            }
        }
    }
    stats.into_iter().map(|(k, cnt, sum)| (k, sum / cnt as f64)).collect()
}

/// 양자화로 잘게 쪼개진 그라데이션 밴드를 면 단위로 통합 (경계가 부드럽고 그룹 평균색 이동이 작을 때만) → 새 팔레트
fn merge_gradient_banding(map: &mut [i32], w: usize, h: usize, colors: &[Pal], px: &[u8], tolerance: f64) -> Vec<Pal> {
    const SOFT_EDGE: f64 = 10.0;
    let max_group_dist = (tolerance * 2.55).max(24.0);
    let (region, rcolor, rsize) = label_regions(map, w, h);
    let nr = rcolor.len();
    if nr < 2 {
        return colors.to_vec();
    }
    let mut edges: Vec<((usize, usize), f64)> = region_pair_softness(&region, px, w, h).into_iter().filter(|(_, s)| *s < SOFT_EDGE).collect();
    edges.sort_by(|a, b| a.1.total_cmp(&b.1));
    // union-find (그룹 합계 색으로 이동 폭 제한)
    let mut parent: Vec<usize> = (0..nr).collect();
    let mut sums: Vec<[f64; 4]> = (0..nr)
        .map(|r| {
            let c = &colors[rcolor[r]];
            let s = rsize[r] as f64;
            [s, f64::from(c.r) * s, f64::from(c.g) * s, f64::from(c.b) * s]
        })
        .collect();
    let find = |parent: &mut Vec<usize>, mut r: usize| {
        while parent[r] != r {
            parent[r] = parent[parent[r]];
            r = parent[r];
        }
        r
    };
    let mut merged = 0;
    for ((a, b), _) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra == rb {
            continue;
        }
        let (sa, sb) = (sums[ra], sums[rb]);
        let dd = (1..4).map(|k| (sa[k] / sa[0] - sb[k] / sb[0]).powi(2)).sum::<f64>().sqrt();
        if dd > max_group_dist {
            continue;
        }
        let (big, small) = if sa[0] >= sb[0] { (ra, rb) } else { (rb, ra) };
        parent[small] = big;
        let add = sums[small];
        for (s, v) in sums[big].iter_mut().zip(add) {
            *s += v;
        }
        merged += 1;
    }
    if merged == 0 {
        return colors.to_vec();
    }
    // 그룹 대표색 수집 (동일 색 dedupe)
    let mut key_index: HashMap<(i32, i32, i32), usize> = HashMap::new();
    let mut group_colors: Vec<Pal> = Vec::new();
    let mut region_group = vec![0usize; nr];
    for r in 0..nr {
        let root = find(&mut parent, r);
        let s = sums[root];
        let c = ((s[1] / s[0]).round() as i32, (s[2] / s[0]).round() as i32, (s[3] / s[0]).round() as i32);
        let idx = *key_index.entry(c).or_insert_with(|| {
            group_colors.push(Pal { r: c.0, g: c.1, b: c.2, count: 0.0 });
            group_colors.len() - 1
        });
        group_colors[idx].count += rsize[r] as f64;
        region_group[r] = idx;
    }
    // 팔레트 통합: 지각상 구분 안 되는 근접 그룹색 병합
    let consolidate = (tolerance * 2.55).max(8.0);
    let mut order: Vec<usize> = (0..group_colors.len()).collect();
    order.sort_by(|a, b| group_colors[*b].count.total_cmp(&group_colors[*a].count));
    let mut new_colors: Vec<Pal> = Vec::new();
    let mut to_final = vec![0usize; group_colors.len()];
    for gi in order {
        let c = group_colors[gi];
        match new_colors.iter().position(|m| c.dist(m) <= consolidate) {
            Some(f) => {
                new_colors[f].absorb(&c);
                to_final[gi] = f;
            }
            None => {
                to_final[gi] = new_colors.len();
                new_colors.push(c);
            }
        }
    }
    for (m, &r) in map.iter_mut().zip(&region) {
        if r >= 0 {
            *m = to_final[region_group[r as usize]] as i32;
        }
    }
    new_colors
}

// ==================== 작은 영역 병합 ====================

/// min_area 미만 4-연결 영역을 인접 색으로 흡수 (colors 가 있으면 충분히 접촉한 이웃 중 RGB 최근접 색)
fn merge_small_regions(map: &mut [i32], w: usize, h: usize, min_area: f64, max_passes: usize, colors: Option<&[Pal]>) {
    let total = w * h;
    let cap = min_area.ceil() as usize;
    for _ in 0..max_passes {
        let mut labeled = vec![false; total];
        let mut changed = false;
        let mut stack = Vec::new();
        for start in 0..total {
            if labeled[start] {
                continue;
            }
            let ci = map[start];
            labeled[start] = true;
            stack.push(start);
            let mut comp = Vec::new();
            let (mut size, mut border) = (0usize, false);
            let mut nbr: Vec<(i32, usize)> = Vec::new();
            while let Some(cur) = stack.pop() {
                size += 1;
                if size <= cap {
                    comp.push(cur);
                }
                let (x, y) = (cur % w, cur / w);
                border |= x == 0 || y == 0 || x == w - 1 || y == h - 1;
                for nb in neighbors4(cur, w, h).into_iter().flatten() {
                    if map[nb] == ci {
                        if !labeled[nb] {
                            labeled[nb] = true;
                            stack.push(nb);
                        }
                    } else {
                        bump(&mut nbr, map[nb]);
                    }
                }
            }
            // 캔버스 경계에 닿은 투명 영역(외곽 배경)은 유지
            if size as f64 >= min_area || (ci == -1 && border) {
                continue;
            }
            if let Some(best) = pick_merge_target(ci, &nbr, colors) {
                for i in comp {
                    map[i] = best;
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

fn pick_merge_target(ci: i32, nbr: &[(i32, usize)], colors: Option<&[Pal]>) -> Option<i32> {
    // 최다 인접 색 (동률이면 먼저 만난 색)
    let mut best: Option<(i32, usize)> = None;
    for &(c, cnt) in nbr {
        if best.is_none_or(|b| cnt > b.1) {
            best = Some((c, cnt));
        }
    }
    let (best_c, best_cnt) = best?;
    let (Some(colors), true) = (colors, ci >= 0) else { return Some(best_c) };
    let c0 = colors.get(ci as usize)?;
    // 접촉이 최다치의 25% 이상인 이웃 중 RGB 최근접 색 우선
    let mut pick: Option<(i32, f64)> = None;
    for &(nc, cnt) in nbr {
        let Some(cc) = colors.get(nc.max(0) as usize).filter(|_| nc >= 0 && cnt as f64 >= best_cnt as f64 * 0.25) else { continue };
        let d = c0.dist(cc);
        if pick.is_none_or(|p| d < p.1) {
            pick = Some((nc, d));
        }
    }
    Some(pick.map_or(best_c, |p| p.0))
}

// ==================== 색상 경계 그래프 ====================
//
// 격자 정점 id: vid = y*(w+1)+x (픽셀 코너, x∈[0,w], y∈[0,h])
// 무방향 에지 id: 수평 (x,y)-(x+1,y) → vid*2, 수직 (x,y)-(x,y+1) → vid*2+1
// 방향 에지: 각 색상 영역이 진행 방향 오른쪽에 내부를 두도록 생성 → 인접 두 색은 같은 에지를 반대 방향으로 소유

struct Graph {
    w: usize,
    h: usize,
    w1: usize,
    /// 에지 존재 비트맵 (id → bool)
    is_edge: Vec<bool>,
    /// 삽입 순서 에지 목록
    edges: Vec<usize>,
    /// 색상별 방향 에지 [from0, to0, from1, to1, ...]
    color_edges: Vec<Vec<usize>>,
    deg: Vec<u8>,
    /// 스무딩된 경계 정점 좌표
    smooth_pos: HashMap<usize, [f64; 2]>,
}

struct Arc {
    vids: Vec<usize>,
    eids: Vec<usize>,
    closed: bool,
    segments: Vec<CubicSegment>,
    rev_segments: Vec<CubicSegment>,
}

struct Arcs {
    arcs: Vec<Arc>,
    /// eid → (아크 인덱스, 아크 내 위치)
    edge_to_arc: HashMap<usize, (usize, usize)>,
}

struct Contour {
    segments: Vec<CubicSegment>,
    hole: bool,
    area: f64,
}

impl Graph {
    fn build(map: &[i32], w: usize, h: usize, num_colors: usize) -> Graph {
        let w1 = w + 1;
        let at = |x: i64, y: i64| if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { -1 } else { map[y as usize * w + x as usize] };
        let mut g = Graph {
            w,
            h,
            w1,
            is_edge: vec![false; w1 * (h + 1) * 2],
            edges: Vec::new(),
            color_edges: vec![Vec::new(); num_colors],
            deg: vec![0; w1 * (h + 1)],
            smooth_pos: HashMap::new(),
        };
        for y in 0..h {
            for x in 0..w {
                let (xi, yi) = (x as i64, y as i64);
                let c = at(xi, yi);
                let tl = y * w1 + x;
                let (tr, bl) = (tl + 1, tl + w1);
                let br = bl + 1;
                // (이웃 방향, 무방향 에지, 방향 에지)
                let sides = [
                    ((xi, yi - 1), tl * 2, (tl, tr)),
                    ((xi + 1, yi), tr * 2 + 1, (tr, br)),
                    ((xi, yi + 1), bl * 2, (br, bl)),
                    ((xi - 1, yi), tl * 2 + 1, (bl, tl)),
                ];
                for ((nx, ny), eid, (from, to)) in sides {
                    if at(nx, ny) == c {
                        continue;
                    }
                    g.add_edge(eid);
                    if let Some(list) = g.color_edges.get_mut(c.max(0) as usize).filter(|_| c >= 0) {
                        list.push(from);
                        list.push(to);
                    }
                }
            }
        }
        for &e in &g.edges {
            let (a, b) = g.ends(e);
            g.deg[a] += 1;
            g.deg[b] += 1;
        }
        g
    }

    fn add_edge(&mut self, eid: usize) {
        if !self.is_edge[eid] {
            self.is_edge[eid] = true;
            self.edges.push(eid);
        }
    }

    fn ends(&self, eid: usize) -> (usize, usize) {
        let v1 = eid >> 1;
        (v1, if eid & 1 == 1 { v1 + self.w1 } else { v1 + 1 })
    }

    fn other_end(&self, eid: usize, v: usize) -> usize {
        let (a, b) = self.ends(eid);
        if v == a { b } else { a }
    }

    /// 정점에 인접한 경계 에지
    fn incident(&self, v: usize) -> Vec<usize> {
        let (x, y) = (v % self.w1, v / self.w1);
        let mut out = Vec::with_capacity(4);
        let mut push = |cond: bool, e: usize| {
            if cond && self.is_edge.get(e).copied().unwrap_or(false) {
                out.push(e);
            }
        };
        push(x < self.w, v * 2);
        push(x > 0, v.wrapping_sub(1).wrapping_mul(2));
        push(y < self.h, v * 2 + 1);
        push(y > 0, v.wrapping_sub(self.w1).wrapping_mul(2) + 1);
        out
    }

    fn pos(&self, v: usize) -> [f64; 2] {
        self.smooth_pos.get(&v).copied().unwrap_or([(v % self.w1) as f64, (v / self.w1) as f64])
    }

    /// 정션 포함 모든 경계 정점을 이웃 평균으로 이동 (캔버스 테두리 정점은 고정)
    fn smooth(&mut self, passes: usize) {
        let mut verts: Vec<usize> = Vec::new();
        let mut seen = HashMap::new();
        for &e in &self.edges {
            let (a, b) = self.ends(e);
            for v in [a, b] {
                seen.entry(v).or_insert_with(|| {
                    verts.push(v);
                });
            }
        }
        let nbrs: Vec<Vec<usize>> = verts.iter().map(|&v| self.incident(v).into_iter().map(|e| self.other_end(e, v)).collect()).collect();
        self.smooth_pos = verts.iter().map(|&v| (v, [(v % self.w1) as f64, (v / self.w1) as f64])).collect();
        for _ in 0..passes {
            let mut next = HashMap::with_capacity(verts.len());
            for (&v, list) in verts.iter().zip(&nbrs) {
                let cur = self.pos(v);
                let (x0, y0) = (v % self.w1, v / self.w1);
                if x0 == 0 || y0 == 0 || x0 == self.w || y0 == self.h {
                    next.insert(v, cur);
                    continue;
                }
                let (sx, sy) = list.iter().map(|&nb| self.pos(nb)).fold((0.0, 0.0), |(sx, sy), p| (sx + p[0], sy + p[1]));
                let n = list.len() as f64;
                next.insert(v, [(cur[0] * 2.0 + sx) / (2.0 + n), (cur[1] * 2.0 + sy) / (2.0 + n)]);
            }
            self.smooth_pos = next;
        }
    }

    /// 경계 그래프를 정션 사이 체인(아크)으로 분해
    fn extract_arcs(&self) -> Arcs {
        let mut st = Arcs { arcs: Vec::new(), edge_to_arc: HashMap::new() };
        let mut visited = vec![false; self.is_edge.len()];
        // 1) 정션에서 시작하는 열린 아크
        for &e in &self.edges {
            if visited[e] {
                continue;
            }
            let (a, b) = self.ends(e);
            if self.deg[a] != 2 {
                self.walk(a, e, false, &mut visited, &mut st);
            } else if self.deg[b] != 2 {
                self.walk(b, e, false, &mut visited, &mut st);
            }
        }
        // 2) 정션이 없는 순수 폐루프
        for &e in &self.edges {
            if !visited[e] {
                self.walk(e >> 1, e, true, &mut visited, &mut st);
            }
        }
        st
    }

    fn walk(&self, start_v: usize, start_e: usize, closed: bool, visited: &mut [bool], st: &mut Arcs) {
        let mut vids = vec![start_v];
        let mut eids = Vec::new();
        let (mut v, mut e) = (start_v, start_e);
        for _ in 0..=self.edges.len() {
            visited[e] = true;
            st.edge_to_arc.insert(e, (st.arcs.len(), eids.len()));
            eids.push(e);
            v = self.other_end(e, v);
            if closed && v == start_v {
                break;
            }
            vids.push(v);
            if !closed && self.deg[v] != 2 {
                break;
            }
            // 차수 2 정점: 남은 한 에지로 계속
            match self.incident(v).into_iter().find(|&c| c != e && !visited[c]) {
                Some(n) => e = n,
                None => break,
            }
        }
        st.arcs.push(Arc { vids, eids, closed, segments: Vec::new(), rev_segments: Vec::new() });
    }

    /// 아크 폴리라인 베지어 피팅 (스무딩된 공유 좌표 — 양쪽 영역이 정확히 일치)
    fn fit_arc(&self, arc: &mut Arc, fit_error: f64) {
        let pts: Vec<[f64; 2]> = arc.vids.iter().map(|&v| self.pos(v)).collect();
        arc.segments = fit_path_segments(&pts, arc.closed, fit_error, 60.0, 2);
        arc.rev_segments = reverse_segments(&arc.segments);
    }

    /// 방향 에지 루프를 아크 단위로 이어 베지어 윤곽으로
    fn loop_to_contour(&self, lp: &[usize], edges: &[usize], arcs: &Arcs) -> Option<Contour> {
        let mut seq: Vec<(usize, usize, usize)> = lp
            .iter()
            .map(|&i| {
                let (from, to) = (edges[i * 2], edges[i * 2 + 1]);
                let lo = from.min(to);
                (from, to, if from.abs_diff(to) == 1 { lo * 2 } else { lo * 2 + 1 })
            })
            .collect();
        // 정션 정점이 시작점이 되도록 회전 (아크 경계와 정렬)
        if let Some(k) = seq.iter().position(|s| self.deg[s.0] != 2) {
            seq.rotate_left(k);
        }
        // 격자 슈레이스 면적 — 바깥 윤곽 = 양수, 구멍 = 음수
        let area2: f64 = seq
            .iter()
            .map(|&(f, t, _)| {
                let (x1, y1, x2, y2) = ((f % self.w1) as f64, (f / self.w1) as f64, (t % self.w1) as f64, (t / self.w1) as f64);
                x1 * y2 - x2 * y1
            })
            .sum();
        let mut segments = Vec::new();
        let mut i = 0;
        for _ in 0..seq.len() + 4 {
            let Some(&(from, _, eid)) = seq.get(i) else { break };
            let &(ai, pos) = arcs.edge_to_arc.get(&eid)?;
            let arc = arcs.arcs.get(ai)?;
            let forward = arc.vids.get(pos) == Some(&from);
            segments.extend_from_slice(if forward { &arc.segments } else { &arc.rev_segments });
            i += arc.eids.len();
        }
        (!segments.is_empty()).then_some(Contour { segments, hole: area2 < 0.0, area: area2.abs() / 2.0 })
    }
}

/// 색상의 방향 에지들을 폐루프로 연결 (분기 정점에서는 우회전 우선 → 4-연결 유지)
fn link_color_loops(edges: &[usize], w1: usize) -> Vec<Vec<usize>> {
    let m = edges.len() / 2;
    let mut out_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..m {
        out_map.entry(edges[i * 2]).or_default().push(i);
    }
    let dir = |from: usize, to: usize| -> (i64, i64) {
        if to == from + 1 {
            (1, 0)
        } else if to + 1 == from {
            (-1, 0)
        } else if to == from + w1 {
            (0, 1)
        } else {
            (0, -1)
        }
    };
    let mut used = vec![false; m];
    let mut loops = Vec::new();
    for s in 0..m {
        if used[s] {
            continue;
        }
        let mut lp = Vec::new();
        let mut cur = s;
        for _ in 0..=m {
            if used[cur] {
                break;
            }
            used[cur] = true;
            lp.push(cur);
            let (from, to) = (edges[cur * 2], edges[cur * 2 + 1]);
            let cands: Vec<usize> = out_map.get(&to).map(|v| v.iter().copied().filter(|&i| !used[i]).collect()).unwrap_or_default();
            let (dx, dy) = dir(from, to);
            // 분기: 진입 방향 기준 우회전 > 직진 > 좌회전
            let score = |i: usize| {
                let (ex, ey) = dir(edges[i * 2], edges[i * 2 + 1]);
                let (cross, dt) = (dx * ey - dy * ex, dx * ex + dy * ey);
                if cross > 0 {
                    2
                } else if dt > 0 {
                    1
                } else if cross < 0 {
                    0
                } else {
                    -1
                }
            };
            let mut best: Option<(usize, i32)> = None;
            for &i in &cands {
                let sc = score(i);
                if best.is_none_or(|b| sc > b.1) {
                    best = Some((i, sc));
                }
            }
            match best {
                Some((i, _)) => cur = i,
                None => break,
            }
        }
        // 최소 폐루프 = 픽셀 1개 (에지 4개)
        if lp.len() >= 4 {
            loops.push(lp);
        }
    }
    loops
}

/// 윤곽들 → 패스. 컨테이너가 먼저 오도록 면적 내림차순, 구멍은 exclude(xor).
fn build_trace_path(mut contours: Vec<Contour>, scale: f64) -> ShapePath {
    contours.sort_by(|a, b| b.area.total_cmp(&a.area));
    let subpaths = contours
        .iter()
        .map(|ct| {
            let mut knots = segments_to_knots(&ct.segments, true);
            for k in &mut knots {
                for q in [&mut k.anchor, &mut k.backward, &mut k.forward] {
                    q[0] *= scale;
                    q[1] *= scale;
                }
            }
            Subpath { closed: true, knots, op: if ct.hole { PathOp::Exclude } else { PathOp::Combine } }
        })
        .collect();
    ShapePath { subpaths, even_odd: false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::lib::layer::{LayerKind, all_layers};
    use crate::cat::lib::shape::{get_shape_fill_color, get_shape_path};
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    #[test]
    fn traces_two_color_rects_into_grouped_shapes() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 80, 60);
        let src = rect_layer(&mut app, "Art", 10, 10, 30, 20, "#ff0000");
        crate::cat::lib::core::exec(&mut app, "select.rect", serde_json::json!({"x": 40, "y": 10, "width": 20, "height": 20})).unwrap();
        crate::cat::lib::core::exec(&mut app, "edit.fill", serde_json::json!({"contents": "color", "color": "#0000ff"})).unwrap();
        crate::cat::lib::core::exec(&mut app, "select.deselect", serde_json::json!({})).unwrap();
        crate::cat::lib::layer::select_layer(&mut app, src, false).unwrap();

        let ids = run(&mut app, TraceSettings::default()).unwrap();
        assert_eq!(ids.len(), 2);
        let mut colors: Vec<[u8; 3]> = ids.iter().map(|id| get_shape_fill_color(&app, *id).unwrap()).collect();
        colors.sort_unstable();
        assert_eq!(colors, vec![[0, 0, 255], [255, 0, 0]]);
        for id in &ids {
            let p = get_shape_path(&app, *id).unwrap();
            assert_eq!(p.subpaths.len(), 1, "{p:?}");
            // 사각형은 4 코너 정점 근처로 단순화된다
            assert!((4..=8).contains(&p.knot_count()), "{}", p.knot_count());
        }
        // 그룹으로 묶였다
        let layers = all_layers(&app).unwrap();
        let group = layers.iter().find(|l| l.name == "Art_trace").unwrap();
        assert_eq!(group.kind, LayerKind::Group);
        assert!(ids.iter().all(|id| layers.iter().any(|l| l.id == *id && l.parent == Some(group.id))));
        // 빨강 사각형 bounds 는 원본 (10,10)-(40,30) 근처
        let red = ids.iter().find(|id| get_shape_fill_color(&app, **id).unwrap() == [255, 0, 0]).unwrap();
        let p = get_shape_path(&app, *red).unwrap();
        let xs: Vec<f64> = p.subpaths[0].knots.iter().map(|k| k.anchor[0]).collect();
        let (lo, hi) = (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max));
        assert!((lo - 10.0).abs() < 1.0 && (hi - 40.0).abs() < 1.0, "{lo} {hi}");
    }

    #[test]
    fn ring_has_hole_subpath() {
        // 20x20 이미지: 검정 링 (바깥 2..18, 안쪽 6..14 투명)
        let (w, h) = (20, 20);
        let mut px = vec![0u8; w * h * 4];
        for y in 2..18 {
            for x in 2..18 {
                if !(6..14).contains(&x) || !(6..14).contains(&y) {
                    px[(y * w + x) * 4 + 3] = 255;
                }
            }
        }
        let out = trace_rgba(px, w, h, TraceSettings::default());
        assert_eq!(out.len(), 1);
        let (rgb, path) = &out[0];
        assert_eq!(*rgb, [0, 0, 0]);
        assert_eq!(path.subpaths.len(), 2);
        assert_eq!(path.subpaths[0].op, PathOp::Combine);
        assert_eq!(path.subpaths[1].op, PathOp::Exclude);
    }
}
