//! 내보내기 폴더 프리셋 + Keep doc size (CAT lib_folder_preset.js / exportSelectedLayers.js 대응)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::PhotocraftApp;
use crate::cat::CatState;
use crate::cat::lib::CatResult;
use crate::cat::lib::{doc, export};

/// 폴더 프리셋 (CAT 과 같은 필드)
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderPreset {
    pub path: String,
    pub name: String,
    pub id: String,
}

pub const MAX_PRESETS: usize = 10;

/// 프리셋 추가 (CAT FolderPresetManager.addPreset 과 같은 검증·문구) → 추가된 프리셋. 자동 선택된다.
pub fn add_folder_preset(cat: &mut CatState, path: &str, name: &str) -> CatResult<FolderPreset> {
    let presets = &cat.settings.folder_presets;
    if presets.len() >= MAX_PRESETS {
        return Err(format!("최대 {MAX_PRESETS}개의 폴더 프리셋만 추가할 수 있습니다."));
    }
    if path.is_empty() || name.is_empty() {
        return Err("폴더 경로와 이름을 모두 입력해주세요.".into());
    }
    if presets.iter().any(|p| p.path == path) {
        return Err("이미 존재하는 폴더 경로입니다.".into());
    }
    let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default();
    let mut id = format!("folder_{millis}");
    while presets.iter().any(|p| p.id == id) {
        id.push('_');
    }
    let preset = FolderPreset { path: path.to_string(), name: name.to_string(), id };
    cat.settings.folder_presets.push(preset.clone());
    cat.settings.selected_folder = Some(preset.clone());
    cat.save_settings();
    Ok(preset)
}

/// 폴더 경로로 프리셋 추가 (이름 = 폴더 이름)
pub fn add_folder_preset_path(cat: &mut CatState, folder: &std::path::Path) -> CatResult<FolderPreset> {
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    add_folder_preset(cat, &folder.to_string_lossy(), &name)
}

/// 프리셋 제거. 선택 중이던 프리셋이면 선택도 해제
pub fn remove_folder_preset(cat: &mut CatState, id: &str) {
    cat.settings.folder_presets.retain(|p| p.id != id);
    if cat.settings.selected_folder.as_ref().is_some_and(|p| p.id == id) {
        cat.settings.selected_folder = None;
    }
    cat.save_settings();
}

/// 프리셋 선택 (None = No Folder)
pub fn set_selected_folder(cat: &mut CatState, preset: Option<FolderPreset>) {
    cat.settings.selected_folder = preset;
    cat.save_settings();
}

/// Keep doc size 저장
pub fn set_keep_doc_size(cat: &mut CatState, on: bool) {
    cat.settings.keep_doc_size = on;
    cat.save_settings();
}

/// 활성 문서의 저장 폴더와 파일명(확장자 제외). 미저장 문서면 None (UI 가 폴더 다이얼로그로 대체한다).
pub fn doc_folder_and_name(app: &PhotocraftApp) -> CatResult<Option<(PathBuf, String)>> {
    doc::require_doc(app)?;
    let Some(path) = export::active_doc_path(app) else { return Ok(None) };
    let folder = path.parent().map(PathBuf::from).unwrap_or_default();
    let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Some((folder, name)))
}

/// 활성 문서 이름에서 확장자를 뺀 이름
pub fn doc_stem(app: &PhotocraftApp) -> CatResult<String> {
    let d = doc::require_doc(app)?;
    Ok(std::path::Path::new(&d.name).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or(d.name))
}

/// 내보내기 폴더: 선택된 프리셋 → 없으면 문서 저장 폴더 → 미저장 문서면 None (UI 가 다이얼로그로 묻는다)
pub fn resolve_export_folder(app: &PhotocraftApp) -> CatResult<Option<PathBuf>> {
    if let Some(p) = app.cat.settings.selected_folder.as_ref().filter(|p| !p.path.is_empty()) {
        return Ok(Some(PathBuf::from(&p.path)));
    }
    Ok(doc_folder_and_name(app)?.map(|(folder, _)| folder))
}
