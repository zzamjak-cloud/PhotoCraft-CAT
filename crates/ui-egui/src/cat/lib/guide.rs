//! 가이드 조작 (CAT lib_guide.js 대응). 조회는 문서 모델의 가이드를 직접 읽는다 (임시 PSD 불필요).

use serde_json::json;

use super::core::{CatResult, exec};
use crate::PhotocraftApp;

/// 가이드 방향
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}

impl GuideAxis {
    fn key(self) -> &'static str {
        match self {
            GuideAxis::Horizontal => "horizontal",
            GuideAxis::Vertical => "vertical",
        }
    }
}

/// 가이드 하나 (문서 픽셀 좌표)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub position: f64,
    pub axis: GuideAxis,
}

/// 활성 문서의 가이드 목록 (수평 → 수직 순). 문서가 없으면 오류.
pub fn get_guides(app: &PhotocraftApp) -> CatResult<Vec<Guide>> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let g = &st.doc.guides;
    let h = g.horizontal.iter().map(|&position| Guide { position, axis: GuideAxis::Horizontal });
    let v = g.vertical.iter().map(|&position| Guide { position, axis: GuideAxis::Vertical });
    Ok(h.chain(v).collect())
}

/// 가이드 추가 (UXP makeGuide 대응)
pub fn make_guide(app: &mut PhotocraftApp, position: f64, axis: GuideAxis) -> CatResult<()> {
    exec(app, "view.newGuide", json!({"orientation": axis.key(), "position": position})).map(|_| ())
}

/// 모든 가이드 제거 (UXP clearAllGuides 대응)
pub fn clear_all_guides(app: &mut PhotocraftApp) -> CatResult<()> {
    exec(app, "view.clearGuides", json!({})).map(|_| ())
}
