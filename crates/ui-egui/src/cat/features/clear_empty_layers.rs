//! Clear empty layers (CAT clearEmptyLayers.js 대응; 잠긴 레이어 제외). Phase 2.

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 빈 레이어·빈 그룹 삭제 → 삭제한 개수
pub fn run(_app: &mut PhotocraftApp) -> CatResult<usize> {
    unimplemented_feature()
}
