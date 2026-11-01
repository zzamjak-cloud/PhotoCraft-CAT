//! Save GIF / JPG / PNG: 활성 문서를 PSD 와 같은 폴더·같은 이름으로 저장 (CAT exportSpeedSave.js 대응)

use std::path::{Path, PathBuf};

use crate::PhotocraftApp;
use crate::cat::features::folder_preset;
use crate::cat::lib::CatResult;
use crate::cat::lib::export;

/// 빠른 저장 포맷
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedFormat {
    /// PNG-24 투명
    Png,
    /// JPG 품질 60
    Jpg,
    /// GIF
    Gif,
}

/// 미저장 문서에 폴더가 주어지지 않았을 때의 오류 (UI 는 이 경우 폴더 다이얼로그를 띄운다)
pub const NEEDS_FOLDER: &str = "문서가 저장되지 않아 저장할 폴더가 필요합니다.";

/// 저장된 문서: 문서 폴더/문서 이름.확장자 로 저장 (기존 파일 덮어씀).
/// 미저장 문서: `folder` 가 있으면 그 폴더/문서 이름, 없으면 `NEEDS_FOLDER` 오류. → 저장 경로
pub fn run(app: &mut PhotocraftApp, format: SpeedFormat, folder: Option<&Path>) -> CatResult<PathBuf> {
    let (folder, name) = match folder_preset::doc_folder_and_name(app)? {
        Some(v) => v,
        None => match folder {
            Some(f) => (f.to_path_buf(), folder_preset::doc_stem(app)?),
            None => return Err(NEEDS_FOLDER.into()),
        },
    };
    match format {
        SpeedFormat::Png => export::save_for_web_png(app, &folder, &name),
        SpeedFormat::Jpg => export::save_for_web_jpg(app, &folder, &name, 60),
        SpeedFormat::Gif => export::save_for_web_gif(app, &folder, &name),
    }
}
