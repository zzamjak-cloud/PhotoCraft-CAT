//! 9-Slice Images 패널 로직 (CAT nineSlicePanel.js 대응): 이미지 라이브러리·프로젝트·선택 영역에 삽입.
//! 이미지는 CAT 데이터 폴더의 `9slice_images/` 에 복사해 보관하므로 원본을 옮기거나 지워도 유지된다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use photocraft_doc::LayerId;
use serde::{Deserialize, Serialize};

use super::nine_slice_resize::{self, NineSliceParams};
use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::select::selection_bounds;
use crate::cat::store;

pub const DEFAULT_PROJECT: &str = "default";
pub const IMAGE_FOLDER: &str = "9slice_images";
pub const ALLOWED_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
/// 라이브러리 파일 (CAT nineSliceImagesV1 / nineSliceProjectsV1 / nineSliceCurrentProjectV1 대응)
pub const LIBRARY_FILE: &str = "nine_slice.json";

/// 등록된 이미지 (CAT nineSliceImagesV1 항목과 같은 필드)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NineSliceImage {
    pub id: String,
    pub name: String,
    /// 보관 폴더 안 파일명 (<id>.<ext>)
    pub file: String,
    pub t: u32,
    pub b: u32,
    pub l: u32,
    pub r: u32,
    pub project: String,
}

/// 라이브러리 (이미지 + 사용자 프로젝트 + 현재 프로젝트)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NineSliceLibrary {
    pub images: Vec<NineSliceImage>,
    /// "default" 제외 사용자 프로젝트
    pub projects: Vec<String>,
    pub current_project: String,
}

/// 데이터 폴더별 라이브러리 캐시 (UI 가 매 프레임 읽으므로 디스크는 처음 한 번만 읽는다)
static CACHE: Mutex<Option<HashMap<Option<PathBuf>, NineSliceLibrary>>> = Mutex::new(None);

fn normalize(mut lib: NineSliceLibrary) -> NineSliceLibrary {
    // 예전 데이터 마이그레이션: project 가 비었으면 default 소속
    for m in &mut lib.images {
        if m.project.is_empty() {
            m.project = DEFAULT_PROJECT.into();
        }
    }
    lib.projects.retain(|p| !p.is_empty() && p != DEFAULT_PROJECT);
    lib
}

/// 라이브러리 읽기
pub fn load_library(app: &PhotocraftApp) -> NineSliceLibrary {
    let key = app.cat.data_dir.clone();
    let Ok(mut guard) = CACHE.lock() else { return normalize(store::load_json(key.as_deref(), LIBRARY_FILE)) };
    let cache = guard.get_or_insert_with(HashMap::new);
    cache.entry(key.clone()).or_insert_with(|| normalize(store::load_json(key.as_deref(), LIBRARY_FILE))).clone()
}

/// 라이브러리 저장 (캐시 갱신 + 디스크)
fn save_library(app: &PhotocraftApp, lib: NineSliceLibrary) -> CatResult<()> {
    let key = app.cat.data_dir.clone();
    store::save_json(key.as_deref(), LIBRARY_FILE, &lib)?;
    if let Ok(mut guard) = CACHE.lock() {
        guard.get_or_insert_with(HashMap::new).insert(key, lib);
    }
    Ok(())
}

/// 프로젝트 목록 ("default" 포함)
pub fn projects(app: &PhotocraftApp) -> Vec<String> {
    let mut out = vec![DEFAULT_PROJECT.to_string()];
    out.extend(load_library(app).projects);
    out
}

/// 현재 프로젝트 (없는 프로젝트면 default)
pub fn current_project(app: &PhotocraftApp) -> String {
    let lib = load_library(app);
    let p = lib.current_project;
    if lib.projects.contains(&p) { p } else { DEFAULT_PROJECT.to_string() }
}

/// 현재 프로젝트 변경
pub fn set_current_project(app: &mut PhotocraftApp, name: &str) -> CatResult<()> {
    let mut lib = load_library(app);
    if name != DEFAULT_PROJECT && !lib.projects.iter().any(|p| p == name) {
        return Err(format!("\"{name}\" 프로젝트가 없습니다."));
    }
    lib.current_project = name.to_string();
    save_library(app, lib)
}

/// 프로젝트 추가 후 그 프로젝트로 전환 (중복·default 면 오류)
pub fn add_project(app: &mut PhotocraftApp, name: &str) -> CatResult<()> {
    let n = name.trim();
    if n.is_empty() {
        return Ok(());
    }
    let mut lib = load_library(app);
    if n == DEFAULT_PROJECT || lib.projects.iter().any(|p| p == n) {
        return Err("이미 존재하는 프로젝트 이름입니다.".into());
    }
    lib.projects.push(n.to_string());
    lib.current_project = n.to_string();
    save_library(app, lib)
}

/// 프로젝트 삭제: 소속 이미지는 default 로 이동 (파일은 유지)
pub fn remove_project(app: &mut PhotocraftApp, name: &str) -> CatResult<()> {
    if name == DEFAULT_PROJECT {
        return Err("default 프로젝트는 삭제할 수 없습니다.".into());
    }
    let mut lib = load_library(app);
    for m in lib.images.iter_mut().filter(|m| m.project == name) {
        m.project = DEFAULT_PROJECT.into();
    }
    lib.projects.retain(|p| p != name);
    if lib.current_project == name {
        lib.current_project = DEFAULT_PROJECT.into();
    }
    save_library(app, lib)
}

/// 이미지 보관 폴더 (데이터 폴더가 없으면 오류)
pub fn image_folder(app: &PhotocraftApp) -> CatResult<PathBuf> {
    app.cat.data_dir.as_ref().map(|d| d.join(IMAGE_FOLDER)).ok_or_else(|| "CAT 데이터 폴더가 설정되지 않았습니다.".into())
}

/// 보관 이미지 경로
pub fn image_path(app: &PhotocraftApp, image: &NineSliceImage) -> Option<PathBuf> {
    image_folder(app).ok().map(|d| d.join(&image.file))
}

/// 새 이미지 ID (ns_<밀리초>_<순번>)
fn new_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("ns_{ms}_{}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// 이미지 파일들을 보관 폴더에 복사하고 프로젝트에 등록 → (추가된 이미지, 건너뛴 개수)
pub fn register_files(app: &mut PhotocraftApp, paths: &[PathBuf], project: &str) -> CatResult<(Vec<NineSliceImage>, usize)> {
    let folder = image_folder(app)?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("폴더를 만들 수 없습니다 ({}): {e}", folder.display()))?;
    let mut lib = load_library(app);
    let mut added = Vec::new();
    let mut skipped = 0;
    for p in paths {
        let Some(ext) = p.extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| ALLOWED_EXTENSIONS.contains(&e.as_str())) else {
            skipped += 1;
            continue;
        };
        let id = new_id();
        let file = format!("{id}.{ext}");
        if std::fs::copy(p, folder.join(&file)).is_err() {
            skipped += 1;
            continue;
        }
        let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let meta = NineSliceImage { id, name, file, project: project.to_string(), ..Default::default() };
        lib.images.push(meta.clone());
        added.push(meta);
    }
    if !added.is_empty() {
        save_library(app, lib)?;
    }
    Ok((added, skipped))
}

/// 건너뛴 파일 알림 문구
pub fn skipped_message(n: usize) -> String {
    format!("{n}개 파일을 건너뛰었습니다. (지원 형식: {})", ALLOWED_EXTENSIONS.join(", "))
}

/// 이미지 삭제 (파일이 이미 없어도 메타는 제거)
pub fn delete_image(app: &mut PhotocraftApp, id: &str) -> CatResult<()> {
    let mut lib = load_library(app);
    let Some(meta) = lib.images.iter().find(|m| m.id == id).cloned() else { return Ok(()) };
    if let Some(p) = image_path(app, &meta) {
        let _ = std::fs::remove_file(p);
    }
    lib.images.retain(|m| m.id != id);
    save_library(app, lib)
}

/// 슬라이스 값 변경
pub fn update_slice_values(app: &mut PhotocraftApp, id: &str, l: u32, r: u32, t: u32, b: u32) -> CatResult<()> {
    let mut lib = load_library(app);
    let meta = lib.images.iter_mut().find(|m| m.id == id).ok_or("이미지를 찾을 수 없습니다.")?;
    (meta.l, meta.r, meta.t, meta.b) = (l, r, t, b);
    save_library(app, lib)
}

/// 이미지를 (x, y) 에 width×height 로 9-slice 삽입 → 결과 레이어 ID (이름 "<이미지>_<W>x<H>")
pub fn insert_image(app: &mut PhotocraftApp, image_id: &str, x: i32, y: i32, width: u32, height: u32) -> CatResult<Option<LayerId>> {
    let meta = load_library(app).images.into_iter().find(|m| m.id == image_id).ok_or("이미지를 찾을 수 없습니다.")?;
    let source = image_path(app, &meta).filter(|p| p.is_file()).ok_or("등록된 이미지 파일을 찾을 수 없습니다. 이미지를 다시 등록해주세요.")?;
    let params = NineSliceParams {
        source,
        top: meta.t,
        bottom: meta.b,
        left: meta.l,
        right: meta.r,
        x,
        y,
        width,
        height,
        name: format!("{}_{width}x{height}", meta.name),
    };
    // 선택 영역은 그대로 두므로 연속 삽입이 가능하다 (CAT 은 해제 후 복원)
    nine_slice_resize::run(app, &params)
}

/// 카드 클릭: 활성 문서에 선택 영역이 있으면 그 위치·크기로 삽입 → 삽입 여부 (선택 영역이 없으면 선택만)
pub fn run(app: &mut PhotocraftApp, image_id: &str) -> CatResult<bool> {
    if app.session.active().is_none() {
        return Ok(false);
    }
    let Some(b) = selection_bounds(app) else { return Ok(false) };
    let (Ok(w), Ok(h)) = (u32::try_from(b.w), u32::try_from(b.h)) else { return Ok(false) };
    Ok(insert_image(app, image_id, b.x, b.y, w, h)?.is_some())
}

/// 크기를 지정해 삽입: 선택 영역이 있으면 그 좌상단, 없으면 캔버스 중앙 → 결과 레이어 ID
pub fn insert_with_size(app: &mut PhotocraftApp, image_id: &str, width: u32, height: u32) -> CatResult<Option<LayerId>> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let (dw, dh) = (st.doc.size.width as i32, st.doc.size.height as i32);
    let (x, y) = match selection_bounds(app) {
        Some(b) => (b.x, b.y),
        None => ((dw - width as i32) / 2, (dh - height as i32) / 2),
    };
    insert_image(app, image_id, x, y, width, height)
}

/// 파일 확장자 허용 여부
pub fn is_allowed(path: &Path) -> bool {
    path.extension().is_some_and(|e| ALLOWED_EXTENSIONS.iter().any(|a| e.eq_ignore_ascii_case(a)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cat::features::nine_slice_resize::tests::sample;
    use crate::cat::lib::doc::{self, DocBackground};
    use crate::cat::lib::{export, layer, pixels, select};
    use crate::cat::tests::{new_doc, test_app};

    /// 샘플 30x30 PNG 를 dir 에 저장 → 경로
    pub(crate) fn write_sample_png(app: &mut PhotocraftApp, dir: &Path) -> PathBuf {
        let id = doc::create_doc(app, "src", 30, 30, DocBackground::Transparent).unwrap();
        pixels::put_pixels_as_layer(app, &sample(), "btn", 0, 0).unwrap();
        let path = export::save_for_web_png(app, dir, "btn").unwrap();
        doc::register_temp_doc(app, id);
        doc::close_doc_without_saving(app, id).unwrap();
        path
    }

    #[test]
    fn library_register_projects_slices_delete_persist() {
        let (mut app, dir) = test_app();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let png = write_sample_png(&mut app, &src_dir);
        let txt = src_dir.join("note.txt");
        std::fs::write(&txt, "x").unwrap();

        let (added, skipped) = register_files(&mut app, &[png.clone(), txt], DEFAULT_PROJECT).unwrap();
        assert_eq!((added.len(), skipped), (1, 1));
        assert_eq!(skipped_message(1), "1개 파일을 건너뛰었습니다. (지원 형식: png, jpg, jpeg, webp)");
        let meta = added[0].clone();
        assert_eq!(meta.name, "btn");
        let stored = image_path(&app, &meta).unwrap();
        assert!(stored.is_file() && stored.starts_with(dir.path().join("cat").join(IMAGE_FOLDER)), "{}", stored.display());

        // 프로젝트: 추가하면 전환, 중복 오류, 삭제하면 default 로 복귀하고 이미지도 default 로
        add_project(&mut app, "ui").unwrap();
        assert_eq!(current_project(&app), "ui");
        assert!(add_project(&mut app, "ui").is_err());
        assert!(add_project(&mut app, DEFAULT_PROJECT).is_err());
        assert_eq!(projects(&app), vec![DEFAULT_PROJECT.to_string(), "ui".into()]);
        let (in_ui, _) = register_files(&mut app, std::slice::from_ref(&png), "ui").unwrap();
        update_slice_values(&mut app, &meta.id, 10, 10, 10, 10).unwrap();

        // 디스크에서 다시 읽어도 같은 값 (캐시를 비우고 확인)
        if let Ok(mut g) = CACHE.lock()
            && let Some(cache) = g.as_mut()
        {
            cache.remove(&app.cat.data_dir);
        }
        let lib = load_library(&app);
        assert_eq!(lib.images.len(), 2);
        let m = lib.images.iter().find(|m| m.id == meta.id).unwrap();
        assert_eq!((m.l, m.r, m.t, m.b), (10, 10, 10, 10));
        assert_eq!(lib.current_project, "ui");

        remove_project(&mut app, "ui").unwrap();
        assert_eq!(current_project(&app), DEFAULT_PROJECT);
        assert!(load_library(&app).images.iter().all(|m| m.project == DEFAULT_PROJECT));

        let in_ui = in_ui[0].clone();
        let in_ui_path = image_path(&app, &in_ui).unwrap();
        delete_image(&mut app, &in_ui.id).unwrap();
        assert!(!in_ui_path.exists());
        assert_eq!(load_library(&app).images.len(), 1);
    }

    #[test]
    fn insert_at_selection_preserves_corners() {
        let (mut app, dir) = test_app();
        let png = write_sample_png(&mut app, dir.path());
        let (added, _) = register_files(&mut app, &[png], DEFAULT_PROJECT).unwrap();
        let id = added[0].id.clone();
        update_slice_values(&mut app, &id, 10, 10, 10, 10).unwrap();

        new_doc(&mut app, "dest", 200, 150);
        // 선택 영역이 없으면 선택만 (삽입 안 함)
        assert!(!run(&mut app, &id).unwrap());
        let before = app.session.active().unwrap().history.past_len();

        select::select_rect(&mut app, 20, 30, 100, 60).unwrap();
        let before_insert = app.session.active().unwrap().history.past_len();
        assert!(before_insert > before);
        assert!(run(&mut app, &id).unwrap());
        assert_eq!(app.session.active().unwrap().history.past_len(), before_insert + 1, "Undo 한 단계");

        let res = layer::active_layer(&app).unwrap();
        assert_eq!(res.name, "btn_100x60");
        assert_eq!(res.bounds, Some(layer::Bounds { x: 20, y: 30, w: 100, h: 60 }));
        let px = pixels::get_layer_pixels(&app, res.id).unwrap();
        assert_eq!(px.pixel(20, 30), Some([255, 0, 0, 255]));
        assert_eq!(px.pixel(29, 39), Some([255, 0, 0, 255]));
        assert_eq!(px.pixel(119, 30), Some([0, 255, 0, 255]));
        assert_eq!(px.pixel(20, 89), Some([0, 0, 255, 255]));
        assert_eq!(px.pixel(119, 89), Some([255, 255, 0, 255]));
        assert_eq!(px.pixel(70, 60), Some([255, 255, 255, 255]));
        assert_eq!(px.pixel(70, 32), Some([128, 128, 128, 255]));
        // 선택 영역 유지 (연속 삽입)
        assert_eq!(select::selection_bounds(&app), Some(layer::Bounds { x: 20, y: 30, w: 100, h: 60 }));

        // 목표가 Border 합보다 작으면 CAT 문구로 오류, 레이어 추가 없음
        select::select_rect(&mut app, 0, 0, 15, 50).unwrap();
        let n = layer::all_layers(&app).unwrap().len();
        assert!(run(&mut app, &id).unwrap_err().contains("목표 너비(15)"));
        assert_eq!(layer::all_layers(&app).unwrap().len(), n);

        // 크기 지정 삽입: 선택 없으면 캔버스 중앙
        select::deselect_all(&mut app).unwrap();
        let lid = insert_with_size(&mut app, &id, 40, 30).unwrap().unwrap();
        assert_eq!(layer::layer_by_id(&app, lid).unwrap().bounds, Some(layer::Bounds { x: 80, y: 60, w: 40, h: 30 }));

        // 보관 파일이 없으면 다시 등록 안내
        std::fs::remove_file(image_path(&app, &added[0]).unwrap()).unwrap();
        select::select_rect(&mut app, 0, 0, 50, 50).unwrap();
        assert!(run(&mut app, &id).unwrap_err().contains("다시 등록"));
    }
}
