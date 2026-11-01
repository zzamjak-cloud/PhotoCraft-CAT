//! Export layers as PNG / PSD (CAT exportSelectedLayers.js · JP_Plugin exportSelectedLayers.ts 대응).
//! 선택 레이어를 하나씩 임시 새 문서로 복사 → (트림) → PNG(+PSD) 저장 → 임시 문서 닫기.

use std::path::Path;

use photocraft_doc::DocId;

use crate::PhotocraftApp;
use crate::cat::lib::core::sanitize_file_name;
use crate::cat::lib::layer::LayerInfo;
use crate::cat::lib::{CatResult, doc, export, layer};

/// 내보낼 파일 형식
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerFileType {
    /// 모든 선택 레이어 → PNG
    Png,
    /// 그룹/스마트 오브젝트만 → PNG + PSD
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
/// 실패한 레이어는 기록하고 다음 레이어로 진행한다.
pub fn run(app: &mut PhotocraftApp, file_type: LayerFileType, folder: &Path, keep_doc_size: bool) -> CatResult<ExportReport> {
    let original = doc::require_doc(app)?.id;
    let selected = layer::selected_layers(app)?;
    if selected.is_empty() {
        return Err("레이어를 선택해주세요.".into());
    }
    let valid: Vec<LayerInfo> = match file_type {
        LayerFileType::Png => selected,
        LayerFileType::Psd => selected.into_iter().filter(|l| l.is_group() || l.is_smart_object()).collect(),
    };
    if valid.is_empty() {
        return Err("그룹 레이어 또는 스마트 오브젝트를 선택해주세요.".into());
    }
    let mut report = ExportReport::default();
    for l in &valid {
        let r = export_layer(app, l, original, file_type, folder, keep_doc_size);
        // 성공·실패와 관계없이 원본 문서로 복귀
        match r.and(doc::activate_doc(app, original)) {
            Ok(()) => report.success += 1,
            Err(e) => report.failed.push((l.name.clone(), e)),
        }
    }
    Ok(report)
}

/// 레이어 하나를 임시 문서로 복사 → (트림) → 저장. 임시 문서는 성공·실패와 관계없이 닫는다.
fn export_layer(app: &mut PhotocraftApp, l: &LayerInfo, original: DocId, file_type: LayerFileType, folder: &Path, keep_doc_size: bool) -> CatResult<()> {
    doc::activate_doc(app, original)?;
    let temp = doc::create_doc_copy_layers(app, &l.name, Some(l.id))?;
    let saved = save_temp(app, &l.name, file_type, folder, keep_doc_size);
    let closed = doc::close_doc_without_saving(app, temp);
    saved.and(closed)
}

fn save_temp(app: &mut PhotocraftApp, layer_name: &str, file_type: LayerFileType, folder: &Path, keep_doc_size: bool) -> CatResult<()> {
    if !keep_doc_size {
        doc::trim_transparent(app);
    }
    let name = sanitize_file_name(layer_name);
    export::save_for_web_png(app, folder, &name)?;
    if file_type == LayerFileType::Psd {
        export::save_as_psd(app, &folder.join(format!("{name}.psd")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn png_size(path: &Path) -> (u32, u32) {
        let img = photocraft_codecs::decode(&std::fs::read(path).unwrap()).unwrap();
        (img.width(), img.height())
    }

    #[test]
    fn exports_each_selected_layer_trimmed_or_at_doc_size_and_psd_only_for_groups() {
        let (mut app, dir) = test_app();
        let out = dir.path().join("e1");
        let user = new_doc(&mut app, "e1doc", 200, 200);
        let a = rect_layer(&mut app, "A", 10, 10, 50, 40, "#ff0000");
        let bad = rect_layer(&mut app, "bad:name", 100, 20, 20, 30, "#0000ff");
        rect_layer(&mut app, "inner", 120, 120, 30, 25, "#00ff00");
        let g = layer::group_selected_layers(&mut app, "G").unwrap();
        let docs = doc::list_documents(&app).len();
        let layers_before = layer::all_layers(&app).unwrap().len();

        // PNG, 트림 + 파일명 정리
        layer::select_layers(&mut app, &[a, bad]).unwrap();
        let r = run(&mut app, LayerFileType::Png, &out, false).unwrap();
        assert_eq!(r, ExportReport { success: 2, failed: vec![] });
        assert_eq!(png_size(&out.join("A.png")), (50, 40));
        assert_eq!(png_size(&out.join("bad_name.png")), (20, 30));
        assert_eq!(doc::list_documents(&app).len(), docs, "임시 문서 정리");
        assert_eq!(doc::active_doc(&app).unwrap().id, user, "원본 문서로 복귀");

        // Keep doc size
        layer::select_layers(&mut app, &[a]).unwrap();
        run(&mut app, LayerFileType::Png, &out, true).unwrap();
        assert_eq!(png_size(&out.join("A.png")), (200, 200));

        // PSD: 픽셀 레이어만 → 오류
        layer::select_layers(&mut app, &[a]).unwrap();
        assert_eq!(run(&mut app, LayerFileType::Psd, &out, false).unwrap_err(), "그룹 레이어 또는 스마트 오브젝트를 선택해주세요.");

        // PSD: 그룹 + 픽셀 → 그룹만 PNG + PSD
        layer::select_layers(&mut app, &[g, a]).unwrap();
        let r = run(&mut app, LayerFileType::Psd, &out, false).unwrap();
        assert_eq!(r.success, 1);
        assert_eq!(png_size(&out.join("G.png")), (30, 25));
        let psd = std::fs::read(out.join("G.psd")).unwrap();
        assert_eq!(&psd[..4], b"8BPS");
        assert_eq!(layer::all_layers(&app).unwrap().len(), layers_before, "원본 레이어 유지");
        assert_eq!(doc::list_documents(&app).len(), docs);

        // 선택 없음
        layer::select_no_layers(&mut app).unwrap();
        assert_eq!(run(&mut app, LayerFileType::Png, &out, false).unwrap_err(), "레이어를 선택해주세요.");
    }
}
