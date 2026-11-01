//! Import SVG as shape (CAT svgToShapeLayer.js 대응). Phase 2.

use std::path::Path;

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// SVG 파일을 셰이프 레이어로 가져오기 → 새 레이어 ID
pub fn run(_app: &mut PhotocraftApp, _svg_path: &Path) -> CatResult<Vec<LayerId>> {
    unimplemented_feature()
}
