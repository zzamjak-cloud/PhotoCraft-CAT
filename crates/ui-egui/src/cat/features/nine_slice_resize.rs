//! 9-slice 리사이즈 코어 (CAT nineSliceResize.js 대응). Phase 2.

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

/// 9-slice 입력
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NineSliceParams {
    /// 원본 이미지 경로
    pub source: std::path::PathBuf,
    /// 슬라이스 (원본 픽셀)
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
    /// 목표 위치·크기 (문서 픽셀)
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 9-slice 로 늘린 이미지를 활성 문서에 레이어로 배치 → 새 레이어 ID
pub fn run(_app: &mut PhotocraftApp, _params: &NineSliceParams) -> CatResult<Option<LayerId>> {
    unimplemented_feature()
}
