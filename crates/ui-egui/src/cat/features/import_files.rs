//! Import PSD files / Import PNG files (CAT importFiles.js 대응): 폴더의 파일을 새 문서에 **링크**로 배치. Phase 2.

use std::path::Path;

use photocraft_doc::DocId;

use crate::PhotocraftApp;
use crate::cat::features::size_presets::Size;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 가져올 파일 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    Psd,
    Png,
}

/// `folder` 의 파일을 `doc_size` 새 문서에 링크로 가져오기 → 새 문서 ID (파일이 없으면 None)
pub fn run(_app: &mut PhotocraftApp, _kind: ImportKind, _folder: &Path, _doc_size: Size) -> CatResult<Option<DocId>> {
    unimplemented_feature()
}
