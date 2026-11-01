//! Search icons (CAT iconSearch.js 대응): Iconify 검색 → SVG 를 셰이프로 가져오기. Phase 2.

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 검색어로 아이콘 ID 목록 검색 ("prefix:name")
pub fn run(_app: &mut PhotocraftApp, _query: &str) -> CatResult<Vec<String>> {
    unimplemented_feature()
}

/// 아이콘을 셰이프 레이어로 가져오기 → 새 레이어 ID
pub fn import_icon(_app: &mut PhotocraftApp, _icon_id: &str) -> CatResult<Vec<LayerId>> {
    unimplemented_feature()
}
