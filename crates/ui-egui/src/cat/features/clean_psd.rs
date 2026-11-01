//! Clean PSD (CAT cleanPSD.js · JP_Plugin cleanPSD.ts 대응).
//! 모든 레이어를 새 문서로 옮겨 문서의 원본 경로에 다시 저장해 누적된 잔여 데이터를 털어낸다.
//! 완료 후에는 CAT 과 같이 원본·정리본 문서 모두 닫힌다.
//!
//! 데이터 손실 방지: 정리본을 원본 경로에 **먼저** 저장하고(임시 파일 → rename, 실패 시 원본 파일 보존),
//! 저장이 성공한 뒤에만 원본 문서를 닫는다. 원본 문서에 미저장 변경이 있으면 `confirmed_dirty` 가 필요하다.

use std::path::PathBuf;

use crate::PhotocraftApp;
use crate::cat::lib::export::SaveOptions;
use crate::cat::lib::{CatResult, doc, export, layer};

/// 미저장 변경이 있는 문서를 확인 없이 정리하려 할 때의 오류 (UI 는 이 경우 확인 모달을 띄운다)
pub const NEEDS_CONFIRM: &str = "문서에 저장되지 않은 변경 사항이 있습니다.";

/// 활성 문서를 정리해 원본 경로에 저장 → 저장 경로.
/// confirmed_dirty: 미저장 변경이 있는 문서도 진행 (변경 내용은 정리본에 포함되어 저장된다).
pub fn run(app: &mut PhotocraftApp, confirmed_dirty: bool) -> CatResult<PathBuf> {
    let original = doc::require_doc(app)?;
    let path = original.path.clone().ok_or("저장된 문서가 아닙니다. 먼저 PSD로 저장해주세요.")?;
    if original.dirty && !confirmed_dirty {
        return Err(NEEDS_CONFIRM.into());
    }
    // 1. 모든 최상위 레이어 선택 → 새 문서로 복사 (그룹은 자식과 함께 복사된다)
    let ids: Vec<_> = layer::top_level_layers(app)?.into_iter().map(|l| l.id).collect();
    if ids.is_empty() {
        return Err("레이어가 없습니다.".into());
    }
    layer::select_layers(app, &ids)?;
    let cleaned = doc::create_doc_copy_layers(app, &original.name, None)?;
    // 2. 정리본을 원본 경로에 저장 (Save As). 실패하면 정리본만 닫고 원본은 그대로 둔다.
    if let Err(e) = export::save_doc_as(app, &path, &SaveOptions { adopt_path: true, ..Default::default() }) {
        let _ = doc::close_doc_without_saving(app, cleaned);
        let _ = doc::activate_doc(app, original.id);
        return Err(e);
    }
    // 3. 저장이 끝난 뒤에만 원본 문서를 닫는다. 미저장 변경은 확인을 받았고 방금 저장한 파일에 들어 있다.
    if original.dirty {
        doc::register_temp_doc(app, original.id);
    }
    doc::close_doc_without_saving(app, original.id)?;
    // 4. 정리본 닫기 (이미 저장됨)
    doc::close_doc_without_saving(app, cleaned)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::lib::export::open_file;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    #[test]
    fn rewrites_the_original_path_with_all_layers_and_closes_both_documents() {
        let (mut app, dir) = test_app();
        let other = new_doc(&mut app, "other", 10, 10);
        new_doc(&mut app, "c", 80, 60);
        rect_layer(&mut app, "A", 5, 5, 20, 20, "#ff0000");
        rect_layer(&mut app, "inner", 30, 30, 10, 10, "#00ff00");
        layer::group_selected_layers(&mut app, "G").unwrap();
        let path = dir.path().join("clean").join("c.psd");
        export::save_doc_as(&mut app, &path, &SaveOptions { adopt_path: true, ..Default::default() }).unwrap();

        assert_eq!(run(&mut app, false).unwrap(), path);
        let docs = doc::list_documents(&app);
        assert_eq!(docs.len(), 1, "원본·정리본 모두 닫힘");
        assert_eq!(docs.first().unwrap().id, other);

        // 다시 열어 레이어 구조 확인
        open_file(&mut app, &path).unwrap();
        let names: Vec<String> = layer::all_layers(&app).unwrap().into_iter().map(|l| l.name).collect();
        assert_eq!(names, ["G", "inner", "A", "Background"]);
        assert_eq!((doc::active_doc(&app).unwrap().width, doc::active_doc(&app).unwrap().height), (80, 60));
    }

    #[test]
    fn unsaved_documents_are_refused_and_dirty_ones_need_confirmation() {
        let (mut app, dir) = test_app();
        let d = new_doc(&mut app, "u", 20, 20);
        assert!(run(&mut app, false).unwrap_err().contains("저장된 문서가 아닙니다"));
        let path = dir.path().join("u.psd");
        export::save_doc_as(&mut app, &path, &SaveOptions { adopt_path: true, ..Default::default() }).unwrap();
        let saved = std::fs::read(&path).unwrap();
        rect_layer(&mut app, "dirty", 0, 0, 5, 5, "#0000ff");
        // 확인 없이: 아무 것도 바꾸지 않는다
        assert_eq!(run(&mut app, false).unwrap_err(), NEEDS_CONFIRM);
        assert!(doc::doc_by_id(&app, d).is_some());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert_eq!(doc::list_documents(&app).len(), 1);
        // 확인 후: 미저장 변경을 포함해 저장하고 닫는다
        run(&mut app, true).unwrap();
        assert!(doc::list_documents(&app).is_empty());
        open_file(&mut app, &path).unwrap();
        assert!(layer::all_layers(&app).unwrap().iter().any(|l| l.name == "dirty"));
    }

    #[test]
    fn a_failed_save_keeps_the_original_document_and_file() {
        let (mut app, dir) = test_app();
        new_doc(&mut app, "f", 20, 20);
        rect_layer(&mut app, "A", 0, 0, 5, 5, "#0000ff");
        // 인코딩할 수 없는 확장자의 원본 경로 → 저장 실패
        let path = dir.path().join("f.unknownext");
        std::fs::write(&path, b"original").unwrap();
        app.session.active_mut().unwrap().path = path.to_str().map(str::to_string);
        assert!(run(&mut app, true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        let docs = doc::list_documents(&app);
        assert_eq!(docs.len(), 1, "정리본만 닫힘");
        assert_eq!(docs.first().unwrap().name, "f");
    }
}
