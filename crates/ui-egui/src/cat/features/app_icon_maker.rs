//! App icon maker (CAT appIconMaker.js 대응; CAT 에서 버튼 미연결 → EXTRA 섹션). Phase 2.

use std::path::Path;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 활성 문서로 앱 아이콘 세트를 `folder` 에 생성
pub fn run(_app: &mut PhotocraftApp, _folder: &Path) -> CatResult<()> {
    unimplemented_feature()
}
