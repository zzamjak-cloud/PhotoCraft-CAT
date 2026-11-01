//! Drop PNG as pixel layer (CAT autoPastePng.js 대응): PNG 를 드롭/Place 하면 픽셀 레이어로 변환.
//! Phase 2 에서 앱의 드롭·place 처리 경로에 훅을 건다.

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::CatState;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 켜짐 여부 (저장값이 없으면 켜짐; CAT 과 같음)
pub fn is_enabled(cat: &CatState) -> bool {
    cat.settings.auto_paste_png.unwrap_or(true)
}

/// 켜기/끄기 저장
pub fn set_enabled(cat: &mut CatState, enabled: bool) {
    cat.settings.auto_paste_png = Some(enabled);
    cat.save_settings();
}

/// 방금 배치된 PNG 스마트 오브젝트를 픽셀 레이어로 변환 → 새 레이어 ID
pub fn run(_app: &mut PhotocraftApp, _layer: LayerId) -> CatResult<Option<LayerId>> {
    unimplemented_feature()
}
