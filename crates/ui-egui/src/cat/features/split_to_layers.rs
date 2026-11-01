//! Split to layers (CAT SplitToLayers.js 대응): 연결 영역별 레이어 분리. Phase 2.

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 활성 레이어를 영역별 레이어로 분리 → 새 레이어 ID
pub fn run(_app: &mut PhotocraftApp) -> CatResult<Vec<LayerId>> {
    unimplemented_feature()
}
