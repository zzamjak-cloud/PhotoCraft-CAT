//! 문서 생성/조회/활성화/닫기, 트림·리사이즈 (CAT lib_doc.js 대응).
//!
//! 문서는 항상 `DocId` 로 특정한다 (인덱스는 닫기·열기로 밀리므로 보관하지 않는다).
//! 확인 없이 닫을 수 있는 문서는 CAT 이 만든 임시 문서(`register_temp_doc`)와 변경 사항 없는 문서뿐이다.

use std::path::PathBuf;

use photocraft_doc::DocId;
use serde_json::{Value, json};

use super::core::{CatResult, exec, try_exec};
use crate::PhotocraftApp;

/// 열린 문서 요약
#[derive(Clone, Debug, PartialEq)]
pub struct DocInfo {
    pub id: DocId,
    /// 현재 탭 순서 인덱스 (조회 시점 값; 보관하지 말 것)
    pub index: usize,
    pub name: String,
    /// 저장된 파일 경로 (없으면 미저장 문서)
    pub path: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
    pub active: bool,
}

fn info(app: &PhotocraftApp, index: usize) -> Option<DocInfo> {
    let st = app.session.documents().get(index)?;
    Some(DocInfo {
        id: st.doc.id,
        index,
        name: st.doc.name.clone(),
        path: st.path.as_deref().map(PathBuf::from),
        width: st.doc.size.width,
        height: st.doc.size.height,
        dirty: st.is_dirty(),
        active: app.session.active_index() == Some(index),
    })
}

/// 열린 문서 목록 (탭 순서)
pub fn list_documents(app: &PhotocraftApp) -> Vec<DocInfo> {
    (0..app.session.documents().len()).filter_map(|i| info(app, i)).collect()
}

/// 활성 문서 (없으면 None)
pub fn active_doc(app: &PhotocraftApp) -> Option<DocInfo> {
    app.session.active_index().and_then(|i| info(app, i))
}

/// 활성 문서가 없으면 오류
pub fn require_doc(app: &PhotocraftApp) -> CatResult<DocInfo> {
    active_doc(app).ok_or_else(|| "열린 문서가 없습니다.".into())
}

/// ID 로 문서 찾기
pub fn doc_by_id(app: &PhotocraftApp, id: DocId) -> Option<DocInfo> {
    doc_index(app, id).and_then(|i| info(app, i))
}

/// ID 의 현재 인덱스
pub fn doc_index(app: &PhotocraftApp, id: DocId) -> Option<usize> {
    app.session.documents().iter().position(|d| d.doc.id == id)
}

/// 문서 활성화
pub fn activate_doc(app: &mut PhotocraftApp, id: DocId) -> CatResult<()> {
    let index = doc_index(app, id).ok_or("활성화할 문서를 찾을 수 없습니다.")?;
    if app.session.active_index() != Some(index) {
        exec(app, "document.activate", json!({ "document": index }))?;
    }
    Ok(())
}

/// 명령 결과의 `document` 인덱스 → 문서 ID (없으면 활성 문서)
fn result_doc(app: &PhotocraftApp, r: &Value) -> CatResult<DocId> {
    let index = r.get("document").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok()).or_else(|| app.session.active_index());
    index.and_then(|i| app.session.documents().get(i)).map(|d| d.doc.id).ok_or_else(|| "새 문서를 찾을 수 없습니다.".into())
}

/// 새 문서 배경
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocBackground {
    White,
    Black,
    Transparent,
}

impl DocBackground {
    fn key(self) -> &'static str {
        match self {
            DocBackground::White => "white",
            DocBackground::Black => "black",
            DocBackground::Transparent => "transparent",
        }
    }
}

/// 새 RGB 8비트 문서 생성 (72dpi) → 문서 ID. 새 문서가 활성 문서가 된다.
pub fn create_doc(app: &mut PhotocraftApp, name: &str, width: u32, height: u32, background: DocBackground) -> CatResult<DocId> {
    let r = exec(
        app,
        "file.new",
        json!({"name": name, "width": width, "height": height, "resolution": 72, "mode": "rgb", "depth": 8, "background": background.key()}),
    )?;
    result_doc(app, &r)
}

/// CAT 작업용 임시 문서로 등록 (미저장이어도 `close_doc_without_saving` 으로 닫을 수 있게 된다)
pub fn register_temp_doc(app: &mut PhotocraftApp, id: DocId) {
    if !app.cat.temp_docs.contains(&id) {
        app.cat.temp_docs.push(id);
    }
}

/// 임시 문서 여부
pub fn is_temp_doc(app: &PhotocraftApp, id: DocId) -> bool {
    app.cat.temp_docs.contains(&id)
}

/// 선택 레이어(또는 `layer`)를 새 문서로 복제 (UXP createDocCopyLayers 대응) → 임시 문서로 등록된 새 문서 ID.
/// 새 문서는 원본과 같은 캔버스 크기이며 활성 문서가 된다.
pub fn create_doc_copy_layers(app: &mut PhotocraftApp, doc_name: &str, layer: Option<photocraft_doc::LayerId>) -> CatResult<DocId> {
    let mut p = json!({"document": "new", "documentName": doc_name});
    if let Some(l) = layer {
        p["layer"] = json!(l.0);
    }
    let before = app.session.documents().len();
    let r = exec(app, "layer.duplicate", p)?;
    if app.session.documents().len() != before + 1 {
        return Err("레이어 복제 문서를 만들지 못했습니다.".into());
    }
    let id = result_doc(app, &r)?;
    register_temp_doc(app, id);
    activate_doc(app, id)?;
    Ok(id)
}

/// 활성 문서 복제 (merged_only: 병합 이미지 한 장) → 임시 문서로 등록된 새 문서 ID
pub fn duplicate_doc(app: &mut PhotocraftApp, name: &str, merged_only: bool) -> CatResult<DocId> {
    let before = app.session.documents().len();
    let r = exec(app, "image.duplicate", json!({"name": name, "mergedOnly": merged_only}))?;
    if app.session.documents().len() != before + 1 {
        return Err("문서 복제에 실패했습니다.".into());
    }
    let id = result_doc(app, &r)?;
    register_temp_doc(app, id);
    activate_doc(app, id)?;
    Ok(id)
}

/// 저장하지 않고 문서 닫기.
/// 데이터 손실 방지: CAT 임시 문서이거나 변경 사항이 없는 문서만 닫는다. 사용자의 미저장 문서는 오류.
pub fn close_doc_without_saving(app: &mut PhotocraftApp, id: DocId) -> CatResult<()> {
    let doc = doc_by_id(app, id).ok_or("닫을 문서를 찾을 수 없습니다.")?;
    if doc.dirty && !is_temp_doc(app, id) {
        return Err(format!("\"{}\" 문서에 저장되지 않은 변경 사항이 있어 닫지 않았습니다.", doc.name));
    }
    let active = active_doc(app).map(|d| d.id).filter(|a| *a != id);
    exec(app, "file.close", json!({ "document": doc.index }))?;
    if doc_index(app, id).is_some() {
        return Err(format!("\"{}\" 문서를 닫지 못했습니다.", doc.name));
    }
    app.cat.temp_docs.retain(|d| *d != id);
    // 닫기 전 활성 문서로 복귀 (닫은 문서가 활성 문서였으면 엔진이 고른 문서 유지)
    if let Some(a) = active
        && doc_index(app, a).is_some()
    {
        activate_doc(app, a)?;
    }
    Ok(())
}

/// 투명 영역 기준 캔버스 트림 (UXP layerTrim 대응). 트림할 것이 없으면 false.
pub fn trim_transparent(app: &mut PhotocraftApp) -> bool {
    try_exec(app, "image.trim", json!({"basedOn": "transparent"})).is_some()
}

/// 이미지 크기 변경 (픽셀)
pub fn resize_image(app: &mut PhotocraftApp, width: u32, height: u32) -> CatResult<()> {
    exec(app, "image.imageSize", json!({"width": width, "height": height, "resample": "bicubic"})).map(|_| ())
}

/// 퍼센트로 이미지 크기 변경
pub fn resize_image_percent(app: &mut PhotocraftApp, percent: f64) -> CatResult<()> {
    exec(app, "image.imageSize", json!({"percent": percent, "resample": "bicubic"})).map(|_| ())
}

/// 캔버스 크기 변경 (anchor: "center" | "top-left" …)
pub fn canvas_size(app: &mut PhotocraftApp, width: u32, height: u32, anchor: &str) -> CatResult<()> {
    exec(app, "image.canvasSize", json!({"width": width, "height": height, "anchor": anchor, "extensionColor": "transparent"})).map(|_| ())
}

/// 자르기
pub fn crop_doc(app: &mut PhotocraftApp, x: i32, y: i32, width: u32, height: u32) -> CatResult<()> {
    exec(app, "image.crop", json!({"x": x, "y": y, "width": width, "height": height, "deleteCroppedPixels": true})).map(|_| ())
}

/// 모든 레이어 병합
pub fn flatten(app: &mut PhotocraftApp) -> CatResult<()> {
    exec(app, "layer.flattenImage", json!({})).map(|_| ())
}
