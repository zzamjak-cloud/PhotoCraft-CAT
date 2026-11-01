//! 곡선 맞춤 (CAT lib_curve_fit.js / JP curveFit.ts 대응): 폴리라인 → 3차 베지어. Phase 2 (outline_stroke·image_trace 공용).

use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 3차 베지어 구간 [p0, c1, c2, p3]
pub type CubicSegment = [[f64; 2]; 4];

/// 점열을 최대 오차 `max_error` 로 베지어 구간들에 맞춘다
pub fn fit_curve(_points: &[[f64; 2]], _max_error: f64) -> CatResult<Vec<CubicSegment>> {
    unimplemented_feature()
}
