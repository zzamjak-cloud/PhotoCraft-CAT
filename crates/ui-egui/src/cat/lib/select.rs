//! 선택 영역 조작 (CAT lib_select.js 대응)

use serde_json::json;

use super::core::{CatResult, exec};
use super::layer::Bounds;
use crate::PhotocraftApp;

/// 선택 해제
pub fn deselect_all(app: &mut PhotocraftApp) -> CatResult<()> {
    exec(app, "select.deselect", json!({})).map(|_| ())
}

/// 사각형 선택 (x, y, w, h) (UXP setBoundsRegion 대응)
pub fn select_rect(app: &mut PhotocraftApp, x: i32, y: i32, width: i32, height: i32) -> CatResult<()> {
    exec(app, "select.rect", json!({"x": x, "y": y, "width": width, "height": height})).map(|_| ())
}

/// 사각형 선택 (top, left, bottom, right) (UXP rectangularMarqueeTool 대응)
pub fn rectangular_marquee(app: &mut PhotocraftApp, top: i32, left: i32, bottom: i32, right: i32) -> CatResult<()> {
    select_rect(app, left, top, right - left, bottom - top)
}

/// 현재 선택 영역 bounds (없거나 비었으면 None) (UXP getSelectionBounds 대응)
pub fn selection_bounds(app: &PhotocraftApp) -> Option<Bounds> {
    let sel = app.session.active()?.doc.selection.as_ref()?;
    let r = sel.content_bounds();
    (!r.is_empty()).then(|| Bounds { x: r.x0, y: r.y0, w: r.width() as i32, h: r.height() as i32 })
}
