//! 파일 저장/내보내기/열기 (CAT lib_export.js 대응). 모두 **동기** 실행이다.
//!
//! 저장은 활성 문서를 `photocraft_io` 인코더로 바로 바이트로 만들고, 같은 폴더의 임시 파일에 쓴 뒤
//! rename 으로 교체한다 → 인코딩·쓰기가 실패해도 기존 파일은 그대로 남는다.
//! 기본은 사본 저장(문서 경로·저장 상태 불변)이며, `adopt_path` 면 Save As 처럼 문서가 저장 파일을 가리킨다.

use std::path::{Path, PathBuf};

use photocraft_doc::DocId;

use super::core::CatResult;
use super::doc;
use crate::PhotocraftApp;

/// 저장 옵션
#[derive(Clone, Debug, PartialEq)]
pub struct SaveOptions {
    /// JPG 품질 1..=100 (CAT Save for Web 기본 60)
    pub jpeg_quality: u8,
    /// 저장 후 문서가 이 파일을 자기 경로로 삼는다 (Save As). 기본은 사본 저장.
    pub adopt_path: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self { jpeg_quality: 60, adopt_path: false }
    }
}

/// 활성 문서를 `path` 에 저장 (확장자로 포맷 결정: png/jpg/gif/psd/…).
/// 기존 파일은 인코딩·쓰기가 모두 성공한 뒤에만 교체한다.
pub fn save_doc_as(app: &mut PhotocraftApp, path: &Path, opts: &SaveOptions) -> CatResult<()> {
    let path_str = path.to_str().ok_or_else(|| format!("경로에 사용할 수 없는 문자가 있습니다: {}", path.display()))?;
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let io = photocraft_io::ExportOptions {
        encode: photocraft_codecs::EncodeOptions {
            jpeg_quality: opts.jpeg_quality.clamp(1, 100),
            jpeg_format: photocraft_codecs::JpegFormat::Optimized,
            ..Default::default()
        },
        // CAT saveAsPSD 는 maximizeCompatibility: true
        psd_maximize_compatibility: true,
        document_path: st.path.as_deref().map(PathBuf::from),
        ..Default::default()
    };
    let result = photocraft_io::export(&st.doc, path_str, &io).map_err(|e| format!("{} 저장 실패: {e}", file_name(path)))?;
    for w in &result.warnings {
        log::info!("CAT 저장 경고 ({}): {w}", path.display());
    }
    crate::cat::store::write_atomic(path, &result.bytes)?;
    if opts.adopt_path
        && let Some(st) = app.session.active_mut()
    {
        st.path = Some(path_str.to_string());
        st.saved_revision = st.revision;
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// 열린 문서들의 원본 파일 경로
pub fn open_document_paths(app: &PhotocraftApp) -> Vec<PathBuf> {
    doc::list_documents(app).into_iter().filter_map(|d| d.path).collect()
}

/// 내보내기 대상이 열린 문서의 원본 파일이면 거부 (원본 PSD 덮어쓰기 방지)
pub fn ensure_not_open_document_path(app: &PhotocraftApp, path: &Path) -> CatResult<()> {
    if open_document_paths(app).iter().any(|p| p == path) {
        return Err(format!("\"{}\"은(는) 열려 있는 문서의 원본 파일이라 덮어쓰지 않았습니다. 다른 내보내기 폴더를 선택하세요.", file_name(path)));
    }
    Ok(())
}

/// `folder/name.png` 로 PNG-24(투명) 저장 (UXP saveForWebPNG 대응) → 저장 경로
pub fn save_for_web_png(app: &mut PhotocraftApp, folder: &Path, name: &str) -> CatResult<PathBuf> {
    let path = folder.join(format!("{name}.png"));
    ensure_not_open_document_path(app, &path)?;
    save_doc_as(app, &path, &SaveOptions::default())?;
    Ok(path)
}

/// `folder/name.jpg` 로 JPG 저장 (기본 품질 60, UXP saveForWebJPG 대응) → 저장 경로
pub fn save_for_web_jpg(app: &mut PhotocraftApp, folder: &Path, name: &str, quality: u8) -> CatResult<PathBuf> {
    let path = folder.join(format!("{name}.jpg"));
    ensure_not_open_document_path(app, &path)?;
    save_doc_as(app, &path, &SaveOptions { jpeg_quality: quality, ..Default::default() })?;
    Ok(path)
}

/// `folder/name.gif` 로 GIF 저장 (UXP saveForWebGIF 대응) → 저장 경로
pub fn save_for_web_gif(app: &mut PhotocraftApp, folder: &Path, name: &str) -> CatResult<PathBuf> {
    let path = folder.join(format!("{name}.gif"));
    ensure_not_open_document_path(app, &path)?;
    save_doc_as(app, &path, &SaveOptions::default())?;
    Ok(path)
}

/// PSD 저장 (UXP saveAsPSD 대응; 확장자가 없으면 .psd). 열린 다른 문서의 원본은 덮어쓰지 않는다 → 저장 경로
pub fn save_as_psd(app: &mut PhotocraftApp, path: &Path) -> CatResult<PathBuf> {
    let path = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("psd")) { path.to_path_buf() } else { path.with_extension("psd") };
    ensure_not_open_document_path(app, &path)?;
    save_doc_as(app, &path, &SaveOptions::default())?;
    Ok(path)
}

/// 파일을 새 문서로 **동기** 열기 → 새 문서 ID (활성 문서가 된다). 문서 경로는 열린 파일.
pub fn open_file(app: &mut PhotocraftApp, path: &Path) -> CatResult<DocId> {
    let bytes = std::fs::read(path).map_err(|e| format!("파일을 읽을 수 없습니다 ({}): {e}", path.display()))?;
    let name = file_name(path);
    let opts = photocraft_engine::jobs::import_options(app.session.prefs());
    let mut r = photocraft_io::import_with_options(&name, &bytes, &opts).map_err(|e| format!("{name} 열기 실패: {e}"))?;
    photocraft_engine::file_cmds::raw_defaults(&app.session, &r.warnings, &mut r.document);
    let id = r.document.id;
    app.session.add_document(r.document, path.to_str().map(str::to_string));
    app.sync_views();
    if doc::doc_index(app, id).is_none() {
        return Err(format!("{name} 을(를) 새 문서로 열지 못했습니다."));
    }
    doc::activate_doc(app, id)?;
    Ok(id)
}

/// 활성 문서의 저장 경로 (미저장이면 None)
pub fn active_doc_path(app: &PhotocraftApp) -> Option<PathBuf> {
    doc::active_doc(app).and_then(|d| d.path)
}

/// 파일을 활성 문서에 **링크된** 스마트 오브젝트로 배치 (UXP newPlacedLayer + relinkToFile 대응) → 새 레이어 ID.
/// 원본 크기(100%), 캔버스 가운데. 레이어 이름은 파일명(확장자 제외).
pub fn place_linked(app: &mut PhotocraftApp, path: &Path) -> CatResult<photocraft_doc::LayerId> {
    let path_str = path.to_str().ok_or_else(|| format!("경로에 사용할 수 없는 문자가 있습니다: {}", path.display()))?;
    let r = super::core::exec(app, "file.placeLinked", serde_json::json!({"path": path_str, "scale": 100, "fit": false}))?;
    super::layer::layer_id_of(&r).ok_or_else(|| format!("{} 을(를) 배치하지 못했습니다.", file_name(path)))
}
