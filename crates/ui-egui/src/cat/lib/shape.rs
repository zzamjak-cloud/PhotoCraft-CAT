//! Shape/Path 생성·읽기 (CAT lib_shape.js · JP_Plugin lib_shape.ts 대응).
//! 읽기는 문서 모델을 직접 보고, 변경은 엔진 명령(`shape.*`, `path.*`, Undo 가능)으로 한다.
//!
//! CAT pathContents 의 컴포넌트 하나(여러 subpath)는 여기서 첫 subpath 의 `op` + 나머지 `Join` 으로 표현한다
//! (`Join` subpath 는 같은 컴포넌트로 함께 채워져 역방향 내부 루프가 구멍이 된다).

use photocraft_doc::{Fill, LayerContent, LayerId, LineCap, LineJoin, StrokeAlign};
use photocraft_geom::Point;
use serde_json::{Value, json};

use super::core::{CatResult, exec, try_exec};
use super::layer::layer_id_of;
use crate::PhotocraftApp;

/// 베지어 정점 (anchor + 들어오는/나가는 핸들, 문서 픽셀)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knot {
    pub anchor: [f64; 2],
    /// 이전 점에서 들어오는 핸들 (CAT backward)
    pub backward: [f64; 2],
    /// 다음 점으로 나가는 핸들 (CAT forward)
    pub forward: [f64; 2],
    pub smooth: bool,
}

impl Knot {
    /// 핸들 없는 코너 정점
    pub fn corner(x: f64, y: f64) -> Self {
        Self { anchor: [x, y], backward: [x, y], forward: [x, y], smooth: false }
    }
}

/// 서브패스 결합 연산 (CAT shapeOperation 대응, `Join` = 앞 컴포넌트에 포함)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathOp {
    #[default]
    Combine,
    Subtract,
    Intersect,
    Exclude,
    Join,
}

impl PathOp {
    fn key(self) -> &'static str {
        match self {
            PathOp::Combine => "combine",
            PathOp::Subtract => "subtract",
            PathOp::Intersect => "intersect",
            PathOp::Exclude => "exclude",
            PathOp::Join => "join",
        }
    }
}

/// 서브패스
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subpath {
    pub closed: bool,
    pub knots: Vec<Knot>,
    pub op: PathOp,
}

/// 패스 (여러 서브패스)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapePath {
    pub subpaths: Vec<Subpath>,
    /// even-odd 채우기 규칙 (기본 nonzero)
    pub even_odd: bool,
}

impl ShapePath {
    /// 전체 정점 수
    pub fn knot_count(&self) -> usize {
        self.subpaths.iter().map(|s| s.knots.len()).sum()
    }

    /// 엔진 compact JSON (`shape.create` / `path.set` 의 path)
    pub fn to_json(&self) -> Value {
        json!({
            "subpaths": self.subpaths.iter().map(|s| json!({
                "closed": s.closed,
                "op": s.op.key(),
                "knots": s.knots.iter().map(|k| json!({"anchor": k.anchor, "in": k.backward, "out": k.forward, "smooth": k.smooth})).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "fillRule": if self.even_odd { "evenodd" } else { "nonzero" },
        })
    }
}

impl From<&photocraft_doc::Path> for ShapePath {
    fn from(p: &photocraft_doc::Path) -> Self {
        let pt = |q: Point| [q.x, q.y];
        ShapePath {
            subpaths: p
                .subpaths
                .iter()
                .map(|s| Subpath {
                    closed: s.closed,
                    knots: s.knots.iter().map(|k| Knot { anchor: pt(k.anchor), backward: pt(k.in_ctrl), forward: pt(k.out_ctrl), smooth: k.smooth }).collect(),
                    op: match s.op {
                        photocraft_doc::PathOp::Combine => PathOp::Combine,
                        photocraft_doc::PathOp::Subtract => PathOp::Subtract,
                        photocraft_doc::PathOp::Intersect => PathOp::Intersect,
                        photocraft_doc::PathOp::Exclude => PathOp::Exclude,
                        photocraft_doc::PathOp::Join => PathOp::Join,
                    },
                })
                .collect(),
            even_odd: p.fill_rule == photocraft_doc::FillRule::EvenOdd,
        }
    }
}

/// 셰이프 레이어 생성 옵션 (기본: 검정 채우기, 스트로크 없음 — CAT 과 동일)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeOptions {
    pub name: Option<String>,
    /// 채우기 RGB (None 이면 검정)
    pub fill: Option<[u8; 3]>,
    /// 채우기 끄기 (스트로크만)
    pub no_fill: bool,
    /// 스트로크 (enabled 이고 width > 0 일 때만 적용)
    pub stroke: Option<StrokeSettings>,
}

/// 스트로크 설정 (CAT getStrokeSettings 대응)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrokeSettings {
    pub enabled: bool,
    pub width: f64,
    /// "inside" | "center" | "outside"
    pub alignment: String,
    /// "butt" | "round" | "square"
    pub cap: String,
    /// "miter" | "round" | "bevel"
    pub join: String,
    pub color: [u8; 3],
}

/// RGB → "#rrggbb"
pub fn rgb_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// 단색 Fill 의 RGB (그라디언트·패턴이면 None)
pub fn solid_rgb(f: &Fill) -> Option<[u8; 3]> {
    match f {
        Fill::Solid(c) => {
            let [r, g, b, _] = c.to_rgba8();
            Some([r, g, b])
        }
        _ => None,
    }
}

fn stroke_json(s: &StrokeSettings) -> Value {
    let align = match s.alignment.as_str() {
        "inside" | "inner" => "inside",
        "outside" | "outer" => "outside",
        _ => "center",
    };
    json!({"width": s.width, "color": rgb_hex(s.color), "opacity": 100, "align": align, "cap": s.cap, "join": s.join})
}

/// 패스로 셰이프 레이어 생성 → 레이어 ID (CAT createShapeFromPathContents). 새 레이어는 활성 레이어 위에 생긴다.
pub fn create_shape_from_path(app: &mut PhotocraftApp, path: &ShapePath, options: &ShapeOptions) -> CatResult<LayerId> {
    if path.subpaths.iter().all(|s| s.knots.is_empty()) {
        return Err("패스가 비어 있습니다.".into());
    }
    let fill = if options.no_fill { Value::Null } else { json!(rgb_hex(options.fill.unwrap_or([0, 0, 0]))) };
    let stroke = options.stroke.as_ref().filter(|s| s.enabled && s.width > 0.0).map_or(Value::Null, stroke_json);
    let mut p = json!({"kind": "path", "path": path.to_json(), "fill": fill, "stroke": stroke});
    if let Some(name) = &options.name {
        p["name"] = json!(name);
    }
    let r = exec(app, "shape.create", p)?;
    layer_id_of(&r).ok_or_else(|| "셰이프 레이어를 만들지 못했습니다.".into())
}

/// 셰이프 채우기 색 변경 (CAT setShapeColor)
pub fn set_shape_color(app: &mut PhotocraftApp, layer: LayerId, rgb: [u8; 3]) -> CatResult<()> {
    exec(app, "shape.edit", json!({"layer": layer.0, "fill": rgb_hex(rgb)})).map(|_| ())
}

fn with_layer<T>(app: &PhotocraftApp, layer: LayerId, f: impl FnOnce(&photocraft_doc::Layer) -> CatResult<T>) -> CatResult<T> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let l = st.doc.layer(layer).ok_or("레이어를 찾을 수 없습니다.")?;
    f(l)
}

/// 셰이프 레이어(또는 벡터 마스크) 패스 읽기 (CAT getShapePathData)
pub fn get_shape_path(app: &PhotocraftApp, layer: LayerId) -> CatResult<ShapePath> {
    with_layer(app, layer, |l| match (&l.content, &l.vector_mask) {
        (LayerContent::Shape(sh), _) => Ok(ShapePath::from(&sh.path)),
        (_, Some(vm)) => Ok(ShapePath::from(&vm.path)),
        _ => Err(format!("\"{}\" 레이어에 Vector Mask 데이터가 없습니다.", l.name)),
    })
}

/// 셰이프 채우기 색 읽기 (CAT getShapeFillColor). 단색이 아니면 흰색.
pub fn get_shape_fill_color(app: &PhotocraftApp, layer: LayerId) -> CatResult<[u8; 3]> {
    with_layer(app, layer, |l| match &l.content {
        LayerContent::Shape(sh) => Ok(sh.fill.as_ref().and_then(solid_rgb).unwrap_or([255, 255, 255])),
        _ => Err(format!("\"{}\" 레이어는 셰이프 레이어가 아닙니다.", l.name)),
    })
}

/// 셰이프 스트로크 설정 읽기 (CAT getStrokeSettings). 셰이프가 아니면 None, 스트로크가 없으면 enabled=false.
pub fn get_stroke_settings(app: &PhotocraftApp, layer: LayerId) -> CatResult<Option<StrokeSettings>> {
    with_layer(app, layer, |l| {
        let LayerContent::Shape(sh) = &l.content else { return Ok(None) };
        let Some(s) = &sh.stroke else {
            return Ok(Some(StrokeSettings {
                enabled: false,
                width: 0.0,
                alignment: "center".into(),
                cap: "butt".into(),
                join: "miter".into(),
                color: [255, 255, 255],
            }));
        };
        Ok(Some(StrokeSettings {
            enabled: s.width > 0.0,
            width: f64::from(s.width),
            alignment: match s.align {
                StrokeAlign::Inside => "inside",
                StrokeAlign::Center => "center",
                StrokeAlign::Outside => "outside",
            }
            .into(),
            cap: match s.cap {
                LineCap::Butt => "butt",
                LineCap::Round => "round",
                LineCap::Square => "square",
            }
            .into(),
            join: match s.join {
                LineJoin::Miter => "miter",
                LineJoin::Round => "round",
                LineJoin::Bevel => "bevel",
            }
            .into(),
            color: solid_rgb(&s.paint).unwrap_or([255, 255, 255]),
        }))
    })
}

/// 선택 영역 → Work Path (CAT makeWorkPath, tolerance 0.5~10 px)
pub fn make_work_path(app: &mut PhotocraftApp, tolerance: f64) -> CatResult<()> {
    exec(app, "select.toWorkPath", json!({"tolerance": tolerance.clamp(0.5, 10.0)})).map(|_| ())
}

/// Work Path 읽기 (CAT getWorkPath, 없으면 None)
pub fn get_work_path(app: &PhotocraftApp) -> CatResult<Option<ShapePath>> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    Ok(st.doc.work_path.as_ref().map(ShapePath::from))
}

/// Work Path 삭제 (CAT deleteWorkPath, 없으면 무시)
pub fn delete_work_path(app: &mut PhotocraftApp) -> CatResult<()> {
    try_exec(app, "path.delete", json!({"name": "work"}));
    Ok(())
}

/// Work Path → 단색 셰이프 레이어 (CAT makeLayerFromPath) → 새 레이어 ID (Work Path 없으면 None)
pub fn make_layer_from_work_path(app: &mut PhotocraftApp, rgb: [u8; 3]) -> CatResult<Option<LayerId>> {
    let Some(path) = get_work_path(app)? else { return Ok(None) };
    create_shape_from_path(app, &path, &ShapeOptions { fill: Some(rgb), ..Default::default() }).map(Some)
}

/// 선택 영역 → Work Path → 셰이프 레이어 (CAT convertPathToShape) → 새 레이어 ID
pub fn convert_path_to_shape(app: &mut PhotocraftApp, rgb: [u8; 3], tolerance: f64) -> CatResult<Option<LayerId>> {
    make_work_path(app, tolerance)?;
    let id = make_layer_from_work_path(app, rgb)?;
    delete_work_path(app)?;
    Ok(id)
}

/// SVG 에서 읽은 셰이프 하나 (페인트 순서, SVG 캔버스 픽셀 좌표)
#[derive(Clone, Debug, PartialEq)]
pub struct SvgShape {
    pub path: ShapePath,
    /// 채우기 RGB (fill="none" 이면 None, 그라디언트는 첫 정지점 색)
    pub fill: Option<[u8; 3]>,
    /// 스트로크 (enabled=true)
    pub stroke: Option<StrokeSettings>,
}

/// SVG 파싱 결과
#[derive(Clone, Debug, PartialEq)]
pub struct SvgShapes {
    /// SVG 캔버스 크기 (viewBox 를 width/height 로 맞춘 픽셀)
    pub width: f64,
    pub height: f64,
    pub shapes: Vec<SvgShape>,
}

fn fill_rgb(f: &Fill) -> [u8; 3] {
    match f {
        Fill::Gradient { stops, .. } => stops.first().map_or([0, 0, 0], |(_, c)| {
            let [r, g, b, _] = c.to_rgba8();
            [r, g, b]
        }),
        f => solid_rgb(f).unwrap_or([0, 0, 0]),
    }
}

fn collect_svg_shapes(layers: &[photocraft_doc::Layer], out: &mut Vec<SvgShape>) {
    // 문서 레이어는 아래→위 저장 = SVG 페인트 순서
    for l in layers {
        if let Some(children) = l.children() {
            collect_svg_shapes(children, out);
            continue;
        }
        let LayerContent::Shape(sh) = &l.content else { continue };
        let stroke = sh.stroke.as_ref().filter(|s| s.width > 0.0).map(|s| StrokeSettings {
            enabled: true,
            width: f64::from(s.width),
            alignment: "center".into(),
            cap: match s.cap {
                LineCap::Butt => "butt",
                LineCap::Round => "round",
                LineCap::Square => "square",
            }
            .into(),
            join: match s.join {
                LineJoin::Miter => "miter",
                LineJoin::Round => "round",
                LineJoin::Bevel => "bevel",
            }
            .into(),
            color: fill_rgb(&s.paint),
        });
        let fill = sh.fill.as_ref().map(fill_rgb);
        // 채우기도 스트로크도 없는 path(배경 rect 등)는 버린다
        if fill.is_none() && stroke.is_none() {
            continue;
        }
        let mut path = ShapePath::from(&sh.path);
        // CAT 과 같이 핸들이 있는 점은 smooth 로 표시
        for k in path.subpaths.iter_mut().flat_map(|s| s.knots.iter_mut()) {
            k.smooth = k.backward != k.anchor || k.forward != k.anchor;
        }
        out.push(SvgShape { path, fill, stroke });
    }
}

/// SVG 바이트 → 셰이프 목록 (usvg 파싱: CSS·use·transform·단위 해석 포함). 셰이프로 표현할 수 없는 요소는 건너뛴다.
pub fn parse_svg_shapes(bytes: &[u8]) -> CatResult<SvgShapes> {
    let r = photocraft_io::svg::import_svg("svg", bytes).map_err(|e| format!("SVG를 읽을 수 없습니다: {e}"))?;
    let mut shapes = Vec::new();
    collect_svg_shapes(&r.document.layers, &mut shapes);
    Ok(SvgShapes { width: f64::from(r.document.size.width), height: f64::from(r.document.size.height), shapes })
}

// ==================== VectorCraft PathData 변환 (vc-geom / vc-pathops) ====================

/// 셰이프 서브패스 → VectorCraft 서브패스 (backward = h_in, forward = h_out)
pub fn subpath_to_vc(sp: &Subpath) -> vectorcraft_geom::SubPath {
    let pt = |q: [f64; 2]| vectorcraft_geom::Point::new(q[0], q[1]);
    let anchors = sp
        .knots
        .iter()
        .map(|k| vectorcraft_geom::Anchor {
            p: pt(k.anchor),
            h_in: pt(k.backward),
            h_out: pt(k.forward),
            kind: if k.smooth { vectorcraft_geom::AnchorKind::Smooth } else { vectorcraft_geom::AnchorKind::Corner },
        })
        .collect();
    vectorcraft_geom::SubPath::new(anchors, sp.closed)
}

/// VectorCraft 서브패스 → 셰이프 서브패스
pub fn subpath_from_vc(sp: &vectorcraft_geom::SubPath, op: PathOp) -> Subpath {
    let pt = |q: vectorcraft_geom::Point| [q.x, q.y];
    Subpath {
        closed: sp.closed,
        knots: sp
            .anchors
            .iter()
            .map(|a| Knot { anchor: pt(a.p), backward: pt(a.h_in), forward: pt(a.h_out), smooth: a.kind == vectorcraft_geom::AnchorKind::Smooth })
            .collect(),
        op,
    }
}

/// 셰이프 패스의 서브패스를 그대로 PathData 로 (op·채우기 규칙은 보지 않는다: 스트로크 아웃라인·편집용)
pub fn shape_path_to_path_data(path: &ShapePath) -> vectorcraft_geom::PathData {
    vectorcraft_geom::PathData::new(path.subpaths.iter().filter(|s| !s.knots.is_empty()).map(subpath_to_vc).collect())
}

/// PathData → 셰이프 패스 하나의 컴포넌트 (첫 서브패스 Combine, 나머지 Join: 역방향 루프가 구멍이 된다)
pub fn path_data_to_shape_path(path: &vectorcraft_geom::PathData, rule: vectorcraft_geom::FillRule) -> ShapePath {
    ShapePath {
        subpaths: path
            .subpaths
            .iter()
            .filter(|s| !s.anchors.is_empty())
            .enumerate()
            .map(|(i, s)| subpath_from_vc(s, if i == 0 { PathOp::Combine } else { PathOp::Join }))
            .collect(),
        even_odd: rule == vectorcraft_geom::FillRule::EvenOdd,
    }
}

/// 셰이프 패스가 칠하는 영역 (컴포넌트별 op 를 불리언으로 풀어 nonzero 단일 PathData 로).
/// 컴포넌트 = op 가 Join 이 아닌 서브패스 + 뒤따르는 Join 서브패스들. 열린 서브패스는 닫아서 본다.
pub fn shape_path_region(path: &ShapePath) -> vectorcraft_geom::PathData {
    use vectorcraft_geom::{FillRule, PathData};
    use vectorcraft_pathops::{BoolOp, boolean, normalize};
    let rule = if path.even_odd { FillRule::EvenOdd } else { FillRule::NonZero };
    let mut components: Vec<(PathOp, PathData)> = Vec::new();
    for sp in path.subpaths.iter().filter(|s| s.knots.len() > 1) {
        let mut vc = subpath_to_vc(sp);
        vc.closed = true;
        match components.last_mut() {
            Some((_, d)) if sp.op == PathOp::Join => d.subpaths.push(vc),
            _ => components.push((if sp.op == PathOp::Join { PathOp::Combine } else { sp.op }, PathData::single(vc))),
        }
    }
    let mut acc: Option<PathData> = None;
    for (op, comp) in components {
        let comp = normalize(&comp, rule);
        acc = Some(match acc {
            // 첫 컴포넌트는 op 와 무관하게 시작 영역 (빈 영역에서 빼기·교차는 빈 결과라 Photoshop 도 같은 결과)
            None => {
                if matches!(op, PathOp::Subtract | PathOp::Intersect) {
                    PathData::default()
                } else {
                    comp
                }
            }
            Some(a) => {
                let bop = match op {
                    PathOp::Combine | PathOp::Join => BoolOp::Union,
                    PathOp::Subtract => BoolOp::Difference,
                    PathOp::Intersect => BoolOp::Intersect,
                    PathOp::Exclude => BoolOp::Xor,
                };
                boolean(&a, FillRule::NonZero, &comp, FillRule::NonZero, bop)
            }
        });
    }
    acc.unwrap_or_default()
}

/// 셰이프 레이어(또는 벡터 마스크)가 칠하는 영역 (`shape_path_region`)
pub fn get_shape_region(app: &PhotocraftApp, layer: LayerId) -> CatResult<vectorcraft_geom::PathData> {
    Ok(shape_path_region(&get_shape_path(app, layer)?))
}

/// PathData(nonzero)로 셰이프 레이어 생성 → 레이어 ID
pub fn create_shape_from_path_data(app: &mut PhotocraftApp, path: &vectorcraft_geom::PathData, options: &ShapeOptions) -> CatResult<LayerId> {
    create_shape_from_path(app, &path_data_to_shape_path(path, vectorcraft_geom::FillRule::NonZero), options)
}

/// 셰이프 레이어의 패스 교체 (채우기·스트로크 유지)
pub fn set_shape_path(app: &mut PhotocraftApp, layer: LayerId, path: &ShapePath) -> CatResult<()> {
    if path.subpaths.iter().all(|s| s.knots.is_empty()) {
        return Err("패스가 비어 있습니다.".into());
    }
    exec(app, "shape.edit", json!({"layer": layer.0, "path": path.to_json()})).map(|_| ())
}

/// 서브패스마다 `f` 를 적용 (op·채우기 규칙 유지; 결과가 빈 서브패스는 원본 유지)
pub fn map_subpaths(path: &ShapePath, f: impl Fn(&vectorcraft_geom::PathData) -> vectorcraft_geom::PathData) -> ShapePath {
    let subpaths = path
        .subpaths
        .iter()
        .map(|sp| {
            if sp.knots.len() < 2 {
                return sp.clone();
            }
            let out = f(&vectorcraft_geom::PathData::single(subpath_to_vc(sp)));
            match out.subpaths.first() {
                Some(first) if out.subpaths.len() == 1 && !first.anchors.is_empty() => subpath_from_vc(first, sp.op),
                _ => sp.clone(),
            }
        })
        .collect();
    ShapePath { subpaths, even_odd: path.even_odd }
}
