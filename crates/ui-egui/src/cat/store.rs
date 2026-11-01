//! CAT 영속 데이터 저장소: PhotoCraft 설정 폴더 하위 `cat/` 에 JSON 파일로 보관한다.
//! (UXP localStorage·플러그인 데이터 폴더 대응)

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Serialize;
use serde::de::DeserializeOwned;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 데스크톱 앱이 시작할 때 한 번 지정한다 (`<설정 폴더>/cat`). 지정하지 않으면(테스트·웹) 디스크에 쓰지 않는다.
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

/// 지정된 CAT 데이터 폴더 (없으면 None).
pub fn default_dir() -> Option<PathBuf> {
    DATA_DIR.get().cloned()
}

/// `dir/file` 의 JSON 을 읽는다. 폴더가 없거나 파일이 없거나 깨졌으면 기본값.
pub fn load_json<T: DeserializeOwned + Default>(dir: Option<&Path>, file: &str) -> T {
    let Some(dir) = dir else { return T::default() };
    match std::fs::read(dir.join(file)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            log::warn!("CAT: {file} 을(를) 읽지 못해 기본값을 씁니다: {e}");
            T::default()
        }),
        Err(_) => T::default(),
    }
}

/// `dir/file` 에 JSON 저장. 임시 파일에 쓴 뒤 교체하므로 실패해도 기존 파일이 보존된다. 폴더가 없으면 아무것도 하지 않는다.
pub fn save_json<T: Serialize>(dir: Option<&Path>, file: &str, value: &T) -> Result<(), String> {
    let Some(dir) = dir else { return Ok(()) };
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    write_atomic(&dir.join(file), &bytes)
}

/// 같은 폴더의 임시 파일에 쓰고 rename 으로 교체 (실패 시 원본 보존).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or_else(|| format!("잘못된 경로입니다: {}", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("폴더를 만들 수 없습니다 ({}): {e}", dir.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.cat-tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("파일을 쓸 수 없습니다 ({}): {e}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("파일을 저장할 수 없습니다 ({}): {e}", path.display()));
    }
    Ok(())
}
