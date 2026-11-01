//! Export layers as PNG / PSD (CAT exportSelectedLayers.js 대응). Phase 2 에서 구현.

use std::path::Path;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 내보낼 파일 형식
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFileType {
    Png,
    Psd,
}

/// 내보내기 결과
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportReport {
    pub success: usize,
    /// (레이어 이름, 오류)
    pub failed: Vec<(String, String)>,
}

/// 선택 레이어를 각각 `folder` 에 내보낸다. keep_doc_size: 트림하지 않고 문서 크기 유지.
pub fn run(_app: &mut PhotocraftApp, _file_type: LayerFileType, _folder: &Path, _keep_doc_size: bool) -> CatResult<ExportReport> {
    unimplemented_feature()
}
