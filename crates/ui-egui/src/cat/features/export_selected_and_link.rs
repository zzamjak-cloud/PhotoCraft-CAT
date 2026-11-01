//! Export PNG & link (CAT exportSelectedAndLink.js 대응): 선택 스마트 오브젝트를 PNG 로 내보내고 그 PNG 로 **링크** 교체. Phase 2.

use std::path::{Path, PathBuf};

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 선택 스마트 오브젝트 레이어를 `folder` 에 PNG 로 저장하고 링크로 교체 → 저장한 PNG 경로들
pub fn run(_app: &mut PhotocraftApp, _folder: &Path, _keep_doc_size: bool) -> CatResult<Vec<PathBuf>> {
    unimplemented_feature()
}
