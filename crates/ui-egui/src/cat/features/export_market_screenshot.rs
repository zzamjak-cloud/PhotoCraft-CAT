//! Market screenshot (CAT exportMarketScreenshot.js 대응): 국가코드 레이어별 JPG 내보내기. Phase 2.

use std::path::Path;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 국가코드 레이어마다 JPG 저장 → 저장한 개수. `folder`: 미저장 문서일 때 UI 가 고른 폴더.
pub fn run(_app: &mut PhotocraftApp, _folder: Option<&Path>) -> CatResult<usize> {
    unimplemented_feature()
}
