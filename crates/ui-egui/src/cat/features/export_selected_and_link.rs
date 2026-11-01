//! Export PNG & link (CAT exportSelectedAndLink.js 대응).
//! 선택한 그룹/스마트 오브젝트의 내용을 `<폴더>/<내용 이름>.png` · `.psd` 로 저장하고,
//! 원래 레이어 자리의 스마트 오브젝트를 내보낸 PSD 에 **링크**한다 (CAT placedLayerRelinkToFile 과 동일).
//! 그룹은 먼저 스마트 오브젝트로 변환한다. 레이어 변경은 Undo 한 단계.

use std::path::{Path, PathBuf};

use photocraft_doc::{DocId, LayerId};

use crate::PhotocraftApp;
use crate::cat::lib::core::{alert, with_history};
use crate::cat::lib::{CatResult, doc, export, layer};

/// 선택 그룹/스마트 오브젝트를 `folder` 에 PNG+PSD 로 저장하고 PSD 링크로 교체 → 저장한 PNG 경로들.
/// 스마트 오브젝트로 만들 수 없는 레이어는 알림 후 건너뛴다.
pub fn run(app: &mut PhotocraftApp, folder: &Path, keep_doc_size: bool) -> CatResult<Vec<PathBuf>> {
    let parent = doc::require_doc(app)?.id;
    let targets: Vec<_> = layer::selected_layers(app)?.into_iter().filter(|l| l.is_smart_object() || l.is_group()).collect();
    if targets.is_empty() {
        return Err("스마트 오브젝트 또는 그룹 레이어를 선택해주세요.".into());
    }
    let mut exported = Vec::new();
    with_history(app, |app| {
        for l in &targets {
            doc::activate_doc(app, parent)?;
            layer::select_layer(app, l.id, false)?;
            let so = if l.is_group() {
                // 그룹 → 스마트 오브젝트 (UXP newPlacedLayer). 변환 결과가 활성 레이어가 된다.
                layer::convert_to_smart_object(app, l.id)?;
                layer::active_layer(app).map(|a| a.id).ok_or("스마트 오브젝트 변환에 실패했습니다.")?
            } else {
                l.id
            };
            if !layer::layer_by_id(app, so).is_some_and(|i| i.is_smart_object()) {
                alert(app, format!("\"{}\": Smart Object가 아닙니다.", l.name));
                continue;
            }
            exported.push(export_and_link(app, parent, so, folder, keep_doc_size)?);
        }
        doc::activate_doc(app, parent)?;
        layer::select_no_layers(app)
    })?;
    Ok(exported)
}

/// 내용 문서를 열어 (트림) → PNG/PSD 저장 → 부모 스마트 오브젝트를 PSD 로 재링크 → 내용 문서 닫기 → PNG 경로
fn export_and_link(app: &mut PhotocraftApp, parent: DocId, so: LayerId, folder: &Path, keep_doc_size: bool) -> CatResult<PathBuf> {
    let contents = doc::open_smart_object_contents(app, so)?;
    let saved = save_contents(app, folder, keep_doc_size);
    // 내용 문서는 저장 없이 닫는다 (원본과 연결되지 않은 임시 문서라 스마트 오브젝트는 바뀌지 않는다)
    let closed = doc::close_doc_without_saving(app, contents);
    let (png, psd) = saved?;
    closed?;
    doc::activate_doc(app, parent)?;
    layer::relink_to_file(app, so, &psd)?;
    Ok(png)
}

/// 활성(내용) 문서를 `<폴더>/<문서 이름에서 확장자 뺀 이름>.png/.psd` 로 저장 → (PNG, PSD)
fn save_contents(app: &mut PhotocraftApp, folder: &Path, keep_doc_size: bool) -> CatResult<(PathBuf, PathBuf)> {
    if !keep_doc_size {
        doc::trim_transparent(app);
    }
    let name = doc::require_doc(app)?.name;
    let base = Path::new(&name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(name);
    let base = crate::cat::lib::core::sanitize_file_name(&base);
    let png = export::save_for_web_png(app, folder, &base)?;
    let psd = export::save_as_psd(app, &folder.join(format!("{base}.psd")))?;
    Ok((png, psd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::lib::pixels;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn png_size(path: &Path) -> (u32, u32) {
        let img = photocraft_codecs::decode(&std::fs::read(path).unwrap()).unwrap();
        (img.width(), img.height())
    }

    fn linked_path(app: &PhotocraftApp, name: &str) -> Option<String> {
        let l = layer::all_layers(app).unwrap().into_iter().find(|l| l.name == name)?;
        l.smart.filter(|s| s.linked).and_then(|s| s.path)
    }

    #[test]
    fn exports_contents_and_relinks_groups_and_smart_objects_in_place() {
        let (mut app, dir) = test_app();
        let out = dir.path().join("e2");
        new_doc(&mut app, "e2doc", 200, 200);
        let a = rect_layer(&mut app, "A", 10, 10, 50, 40, "#ff0000");
        rect_layer(&mut app, "inner", 120, 120, 30, 25, "#00ff00");
        let g = layer::group_selected_layers(&mut app, "Grp").unwrap();
        let sm = rect_layer(&mut app, "Sm", 60, 100, 16, 12, "#0000ff");
        layer::convert_to_smart_object(&mut app, sm).unwrap();
        let so = layer::active_layer(&app).unwrap().id;
        assert!(layer::layer_by_id(&app, so).unwrap().is_smart_object());
        let before = pixels::get_document_pixels(&app).unwrap();
        let past = app.session.active().unwrap().history.past_len();

        // 픽셀 레이어만 → 오류
        layer::select_layers(&mut app, &[a]).unwrap();
        assert_eq!(run(&mut app, &out, false).unwrap_err(), "스마트 오브젝트 또는 그룹 레이어를 선택해주세요.");

        layer::select_layers(&mut app, &[g, so, a]).unwrap();
        let pngs = run(&mut app, &out, false).unwrap();
        assert_eq!(pngs.len(), 2, "{:?}", app.cat.modals.iter().map(|m| &m.message).collect::<Vec<_>>());
        assert_eq!(png_size(&out.join("Grp.png")), (30, 25));
        assert_eq!(png_size(&out.join("Sm.png")), (16, 12));
        for n in ["Grp", "Sm"] {
            assert_eq!(&std::fs::read(out.join(format!("{n}.psd"))).unwrap()[..4], b"8BPS");
        }
        // 링크된 스마트 오브젝트로 교체 (CAT: 내보낸 PSD 에 relink)
        assert_eq!(linked_path(&app, "Grp").as_deref(), out.join("Grp.psd").to_str());
        assert_eq!(linked_path(&app, "Sm").as_deref(), out.join("Sm.psd").to_str());
        // 원래 자리에 그대로 보인다
        let after = pixels::get_document_pixels(&app).unwrap();
        for (x, y) in [(125, 125), (149, 144), (60, 100), (75, 111), (20, 20)] {
            assert_eq!(after.pixel(x, y), before.pixel(x, y), "({x},{y})");
        }
        assert!(layer::selected_layers(&app).unwrap().is_empty(), "선택 해제");
        assert_eq!(doc::list_documents(&app).len(), 1, "내용 문서 닫힘");
        assert_eq!(app.session.active().unwrap().history.past_len(), past + 1, "Undo 한 단계");
    }

    #[test]
    fn keep_doc_size_exports_the_contents_canvas_untrimmed() {
        let (mut app, dir) = test_app();
        let out = dir.path().join("e2k");
        new_doc(&mut app, "e2k", 120, 90);
        rect_layer(&mut app, "inner", 10, 10, 20, 20, "#00ff00");
        let g = layer::group_selected_layers(&mut app, "K").unwrap();
        let before = pixels::get_document_pixels(&app).unwrap();
        layer::select_layers(&mut app, &[g]).unwrap();
        run(&mut app, &out, true).unwrap();
        // 그룹 → 스마트 오브젝트 변환 시 내용 캔버스는 레이어 bounds 크기 (Photoshop newPlacedLayer 와 같음)
        assert_eq!(png_size(&out.join("K.png")), (20, 20));
        assert_eq!(pixels::get_document_pixels(&app).unwrap(), before);
    }
}
