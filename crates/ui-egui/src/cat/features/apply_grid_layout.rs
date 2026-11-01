//! Arrange in grid (CAT applyGridLayout.js 대응). Phase 2.

use crate::PhotocraftApp;
use crate::cat::features::size_presets::Size;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 선택 레이어를 `cell` 크기 칸, `columns` 열 그리드로 배치
pub fn run(_app: &mut PhotocraftApp, _cell: Size, _columns: u32) -> CatResult<()> {
    unimplemented_feature()
}
