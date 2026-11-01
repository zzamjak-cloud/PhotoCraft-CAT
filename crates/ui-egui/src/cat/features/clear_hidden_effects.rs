//! Clear hidden effects (CAT clearHiddenEffects.js 대응; 잠긴 레이어 제외). Phase 2.

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 선택 레이어의 숨겨진 효과 삭제 → 삭제한 효과 개수
pub fn run(_app: &mut PhotocraftApp) -> CatResult<usize> {
    unimplemented_feature()
}
