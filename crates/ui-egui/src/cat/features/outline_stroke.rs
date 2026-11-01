//! Outline stroke / Expand path (CAT outlineStroke.js 대응). Phase 2.

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 셰이프 스트로크를 면으로 변환
pub fn run_outline(_app: &mut PhotocraftApp) -> CatResult<()> {
    unimplemented_feature()
}

/// 패스를 `amount` px 만큼 확장 (UI 가 "Expand Path (px)" 입력으로 받는다)
pub fn run_expand(_app: &mut PhotocraftApp, _amount: f64) -> CatResult<()> {
    unimplemented_feature()
}
