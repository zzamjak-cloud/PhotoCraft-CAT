//! Import PSD files / Import PNG files (CAT importFiles.js 대응): 폴더의 PSD/PNG 파일을 새 문서 "Imported_Assets" 에
//! **링크된** 스마트 오브젝트로 원본 크기(100%)·캔버스 가운데에 배치한다. 레이어 이름은 파일명(확장자 제외).

use std::path::{Path, PathBuf};

use photocraft_doc::DocId;

use crate::PhotocraftApp;
use crate::cat::Progress;
use crate::cat::features::size_presets::Size;
use crate::cat::lib::CatResult;
use crate::cat::lib::doc::{self, DocBackground};
use crate::cat::lib::{export, layer};

/// 새 문서 이름 (CAT 과 같음)
pub const DOC_NAME: &str = "Imported_Assets";

/// 가져올 파일 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportKind {
    Psd,
    Png,
}

impl ImportKind {
    pub fn extension(self) -> &'static str {
        match self {
            ImportKind::Psd => "psd",
            ImportKind::Png => "png",
        }
    }
}

/// `folder` 바로 아래의 해당 확장자 파일 (이름순)
pub fn list_files(folder: &Path, kind: ImportKind) -> CatResult<Vec<PathBuf>> {
    let entries = std::fs::read_dir(folder).map_err(|e| format!("폴더를 읽을 수 없습니다 ({}): {e}", folder.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case(kind.extension())))
        .collect();
    files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    Ok(files)
}

/// `folder` 의 파일을 `doc_size` 새 문서에 링크로 가져오기 → 새 문서 ID (파일이 없으면 None)
pub fn run(app: &mut PhotocraftApp, kind: ImportKind, folder: &Path, doc_size: Size) -> CatResult<Option<DocId>> {
    let files = list_files(folder, kind)?;
    if files.is_empty() {
        return Ok(None);
    }
    let id = doc::create_doc(app, DOC_NAME, doc_size.width, doc_size.height, DocBackground::Transparent)?;
    let initial: Vec<_> = layer::all_layers(app)?.into_iter().map(|l| l.id).collect();
    let r = (|| {
        for (i, path) in files.iter().enumerate() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            app.cat.progress =
                Some(Progress { label: format!("Import {}/{}: {name}", i + 1, files.len()), percent: (i + 1) as f32 / files.len() as f32 * 100.0 });
            export::place_linked(app, path)?;
            // 첫 파일 이후 초기 빈 레이어("Layer 1") 제거
            if i == 0 {
                for l in &initial {
                    layer::delete_layer(app, *l)?;
                }
            }
        }
        Ok(Some(id))
    })();
    app.cat.progress = None;
    r
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cat::lib::layer::LayerKind;
    use crate::cat::tests::test_app;

    /// 단색 RGBA PNG 파일 (테스트 픽스처; 다른 CAT 테스트에서도 사용)
    pub(crate) fn write_png(path: &Path, w: u32, h: u32, rgba: [u8; 4]) {
        use photocraft_codecs::{ChannelLayout, EncodeOptions, Format, Image, SampleType};
        let img = Image::from_raw(w, h, ChannelLayout::Rgba, SampleType::U8, rgba.repeat((w * h) as usize)).unwrap();
        std::fs::write(path, photocraft_codecs::encode(&img, Format::Png, &EncodeOptions::default()).unwrap()).unwrap();
    }

    #[test]
    fn imports_pngs_as_linked_smart_objects_at_original_size() {
        let (mut app, dir) = test_app();
        let folder = dir.path().join("assets");
        std::fs::create_dir_all(&folder).unwrap();
        write_png(&folder.join("b_icon.png"), 40, 20, [10, 20, 30, 255]);
        write_png(&folder.join("A_big.PNG"), 300, 300, [10, 20, 30, 255]);
        std::fs::write(folder.join("note.txt"), "x").unwrap();

        let id = run(&mut app, ImportKind::Png, &folder, Size { width: 200, height: 100 }).unwrap().unwrap();
        let d = doc::active_doc(&app).unwrap();
        assert_eq!((d.id, d.name.as_str(), d.width, d.height), (id, DOC_NAME, 200, 100));
        let layers = layer::all_layers(&app).unwrap();
        let names: Vec<_> = layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["b_icon", "A_big"], "초기 빈 레이어는 제거");
        for l in &layers {
            assert_eq!(l.kind, LayerKind::SmartObject);
            let smart = l.smart.as_ref().unwrap();
            assert!(smart.linked, "링크로 가져온다");
            assert!(smart.path.as_deref().unwrap().ends_with(&format!("{}.{}", l.name, if l.name == "A_big" { "PNG" } else { "png" })));
        }
        // 원본 크기(100%), 가운데 (캔버스보다 커도 맞춰 줄이지 않는다)
        let icon = layers.iter().find(|l| l.name == "b_icon").unwrap();
        assert_eq!(icon.bounds, Some(layer::Bounds { x: 80, y: 40, w: 40, h: 20 }));
        let st = app.session.active().unwrap();
        let big = layers.iter().find(|l| l.name == "A_big").unwrap();
        let photocraft_doc::LayerContent::Smart(so) = &st.doc.layer(big.id).unwrap().content else { panic!() };
        assert_eq!(so.transform.m, [1.0, 0.0, 0.0, 1.0, -50.0, -100.0]);
    }

    #[test]
    fn empty_folder_returns_none() {
        let (mut app, dir) = test_app();
        assert_eq!(run(&mut app, ImportKind::Psd, dir.path(), Size { width: 10, height: 10 }).unwrap(), None);
        assert!(doc::active_doc(&app).is_none());
    }
}
