//! Shape/Path 생성·읽기 (CAT lib_shape.js · JP_Plugin lib_shape.ts 대응).
//! Phase 2(Outline stroke / Expand path / Image trace / Import SVG)에서 채운다. 지금은 시그니처만 확정한다.

use photocraft_doc::LayerId;

use super::core::{CatResult, unimplemented_feature};
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

/// 서브패스
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subpath {
    pub closed: bool,
    pub knots: Vec<Knot>,
}

/// 패스 (여러 서브패스, 같은 셰이프 연산)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapePath {
    pub subpaths: Vec<Subpath>,
}

/// 셰이프 레이어 생성 옵션
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeOptions {
    pub name: Option<String>,
    /// 채우기 RGB
    pub fill: Option<[u8; 3]>,
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

/// 패스로 셰이프 레이어 생성 → 레이어 ID (CAT createShapeFromPathContents)
pub fn create_shape_from_path(_app: &mut PhotocraftApp, _path: &ShapePath, _options: &ShapeOptions) -> CatResult<LayerId> {
    unimplemented_feature()
}

/// 셰이프 채우기 색 변경 (CAT setShapeColor)
pub fn set_shape_color(_app: &mut PhotocraftApp, _layer: LayerId, _rgb: [u8; 3]) -> CatResult<()> {
    unimplemented_feature()
}

/// 셰이프 레이어 패스 읽기 (CAT getShapePathData)
pub fn get_shape_path(_app: &PhotocraftApp, _layer: LayerId) -> CatResult<ShapePath> {
    unimplemented_feature()
}

/// 셰이프 채우기 색 읽기 (CAT getShapeFillColor)
pub fn get_shape_fill_color(_app: &PhotocraftApp, _layer: LayerId) -> CatResult<[u8; 3]> {
    unimplemented_feature()
}

/// 셰이프 스트로크 설정 읽기 (CAT getStrokeSettings)
pub fn get_stroke_settings(_app: &PhotocraftApp, _layer: LayerId) -> CatResult<Option<StrokeSettings>> {
    unimplemented_feature()
}

/// 선택 영역 → Work Path (CAT makeWorkPath)
pub fn make_work_path(_app: &mut PhotocraftApp, _tolerance: f64) -> CatResult<()> {
    unimplemented_feature()
}

/// Work Path 읽기 (CAT getWorkPath)
pub fn get_work_path(_app: &PhotocraftApp) -> CatResult<Option<ShapePath>> {
    unimplemented_feature()
}

/// Work Path 삭제 (CAT deleteWorkPath)
pub fn delete_work_path(_app: &mut PhotocraftApp) -> CatResult<()> {
    unimplemented_feature()
}
