//! Image trace (CAT imageTrace.js 대응): 픽셀 → 색상별 셰이프 레이어. Phase 2.

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 트레이스 설정 (UI 가 입력받는다)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceSettings {
    /// 유사 색상 병합 강도 0..=100
    pub color_tolerance: f64,
    /// 경로 단순화 0..=5
    pub path_tolerance: f64,
}

impl Default for TraceSettings {
    fn default() -> Self {
        Self { color_tolerance: 30.0, path_tolerance: 1.0 }
    }
}

/// 활성 레이어 트레이스
pub fn run(_app: &mut PhotocraftApp, _settings: TraceSettings) -> CatResult<()> {
    unimplemented_feature()
}
