//! 9-Slice Images 패널 로직 (CAT nineSlicePanel.js 대응): 이미지 라이브러리·프로젝트·선택 영역에 삽입. Phase 2.
//! 이미지는 CAT 데이터 폴더의 `9slice_images/` 에 복사해 보관한다.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::unimplemented_feature;

pub const DEFAULT_PROJECT: &str = "default";
pub const IMAGE_FOLDER: &str = "9slice_images";
pub const ALLOWED_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// 등록된 이미지 (CAT nineSliceImagesV1 항목과 같은 필드)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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

/// 라이브러리 읽기
pub fn load_library(_app: &PhotocraftApp) -> NineSliceLibrary {
    NineSliceLibrary::default()
}

/// 프로젝트 목록 ("default" 포함)
pub fn projects(app: &PhotocraftApp) -> Vec<String> {
    let mut out = vec![DEFAULT_PROJECT.to_string()];
    out.extend(load_library(app).projects);
    out
}

/// 현재 프로젝트
pub fn current_project(app: &PhotocraftApp) -> String {
    let p = load_library(app).current_project;
    if p.is_empty() { DEFAULT_PROJECT.to_string() } else { p }
}

/// 현재 프로젝트 변경
pub fn set_current_project(_app: &mut PhotocraftApp, _name: &str) -> CatResult<()> {
    unimplemented_feature()
}

/// 프로젝트 추가
pub fn add_project(_app: &mut PhotocraftApp, _name: &str) -> CatResult<()> {
    unimplemented_feature()
}

/// 프로젝트 삭제
pub fn remove_project(_app: &mut PhotocraftApp, _name: &str) -> CatResult<()> {
    unimplemented_feature()
}

/// 이미지 파일들을 프로젝트에 등록 → (추가된 이미지, 건너뛴 개수)
pub fn register_files(_app: &mut PhotocraftApp, _paths: &[PathBuf], _project: &str) -> CatResult<(Vec<NineSliceImage>, usize)> {
    unimplemented_feature()
}

/// 이미지 삭제
pub fn delete_image(_app: &mut PhotocraftApp, _id: &str) -> CatResult<()> {
    unimplemented_feature()
}

/// 슬라이스 값 변경
pub fn update_slice_values(_app: &mut PhotocraftApp, _id: &str, _l: u32, _r: u32, _t: u32, _b: u32) -> CatResult<()> {
    unimplemented_feature()
}

/// 보관 이미지 경로
pub fn image_path(_app: &PhotocraftApp, _image: &NineSliceImage) -> Option<PathBuf> {
    None
}

/// 활성 문서에 선택 영역이 있으면 그 영역 크기로 9-slice 삽입 → 삽입 여부
pub fn run(_app: &mut PhotocraftApp, _image_id: &str) -> CatResult<bool> {
    unimplemented_feature()
}

/// 파일 확장자 허용 여부
pub fn is_allowed(path: &Path) -> bool {
    path.extension().is_some_and(|e| ALLOWED_EXTENSIONS.iter().any(|a| e.eq_ignore_ascii_case(a)))
}
