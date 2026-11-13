//! macOS 자체 업데이트 (포크 전용, `win_update` 와 같은 서명 키·창·실패 기록을 쓴다).
//!
//! craft-update 는 실행 파일이 `ai.storyteller.<id>` 가 아닌 번들 안에 있으면 자체 업데이트를 하지
//! 않는데, 포크 번들 id 는 Developer ID 서명 경로를 피하려고 `com.zzamjak.photocraft-cat` 이다.
//! 그래서 macOS 는 이 모듈이 맡는다: 릴리스의 `<id>-<버전>-macos-aarch64.tar.gz` 와 그 Ed25519 서명
//! `.tar.gz.sig` 를 받아 [`crate::win_update::PUBLIC_KEY`] 로 확인하고, 안의 실행 파일을 꺼낸 뒤
//! 도우미 sh 스크립트가 앱 종료 후 번들의 `Contents/MacOS` 실행 파일을 원자적으로 교체하고 다시 띄운다.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::win_update::{PUBLIC_KEY, agent, download, failure_log, verify, work_dir};

/// 받는 tarball 의 최대 크기.
const MAX_TARBALL_BYTES: u64 = 1 << 30;

const HELPER: &str = include_str!("mac_update_helper.sh");

/// 버전 `v` 의 자체 업데이트용 tarball 주소 (`packaging/macos/package-cat.sh`).
pub fn tarball_url(v: &str) -> String {
    format!("https://github.com/{}/releases/download/v{v}/{}-{v}-macos-aarch64.tar.gz", crate::UPDATE_APP.repo, crate::UPDATE_APP.id)
}

/// 서명 파일 주소.
pub fn sig_url(v: &str) -> String {
    format!("{}.sig", tarball_url(v))
}

/// 실행 파일이 `X.app/Contents/MacOS/` 안에 있으면 그 번들 경로.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let is_app = app.extension().is_some_and(|e| e == "app");
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && is_app).then(|| app.to_path_buf())
}

/// Mach-O(64비트 또는 유니버설) 실행 파일처럼 보이는가.
fn looks_like_mach_o(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some([0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe]))
}

/// tarball 에서 이름이 앱 id 인 실행 파일을 꺼낸다 (macOS tar 가 넣는 `._*` 등은 건너뛴다).
fn extract_binary(tarball: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let bad = |e: std::io::Error| format!("couldn't read the update archive: {e}");
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tarball));
    for entry in archive.entries().map_err(bad)? {
        let entry = entry.map_err(bad)?;
        let path: PathBuf = entry.path().map_err(bad)?.components().filter(|c| !matches!(c, Component::CurDir)).collect();
        if !entry.header().entry_type().is_file() || path != Path::new(crate::UPDATE_APP.id) {
            continue;
        }
        let mut out = Vec::new();
        entry.take(MAX_TARBALL_BYTES + 1).read_to_end(&mut out).map_err(bad)?;
        if !looks_like_mach_o(&out) {
            return Err("the update archive doesn't contain a macOS program".into());
        }
        return Ok(out);
    }
    Err(format!("the update archive has no {}", crate::UPDATE_APP.id))
}

/// 도우미 인자 (순서는 `mac_update_helper.sh` 의 사용법과 같다).
fn helper_args(pid: u32, new: &Path, exe: &Path, app: &Path, version: &str, log: &Path) -> Vec<OsString> {
    vec![pid.to_string().into(), new.into(), exe.into(), app.into(), version.into(), log.into()]
}

/// 실행 파일 폴더에 쓸 수 있는지 미리 확인한다 (관리자 소유 설치, App Translocation 등).
fn check_writable(dir: &Path) -> Result<(), String> {
    let probe = dir.join(".photocat-update-check");
    std::fs::write(&probe, b"").map_err(|e| format!("can't write to {} ({e}); use Download to update", dir.display()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// 내려받고 확인하고 도우미를 띄운다. 성공하면 앱이 곧 종료되어야 한다.
pub(crate) fn prepare(version: &str) -> Result<(), String> {
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(format!("unexpected version {version}"));
    }
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program ({e})"))?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let app = bundle_of(&exe).ok_or_else(|| "this copy isn't inside an app bundle; use Download to update".to_string())?;
    check_writable(exe.parent().unwrap_or(app.as_path()))?;
    let agent = agent();
    let sig = download(&agent, &sig_url(version), 1024)?;
    let tarball = download(&agent, &tarball_url(version), MAX_TARBALL_BYTES)?;
    verify(&PUBLIC_KEY, &tarball, &sig)?;
    let binary = extract_binary(&tarball)?;
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let new = dir.join(format!("{}-{version}", crate::UPDATE_APP.id));
    std::fs::write(&new, &binary).map_err(|e| format!("{}: {e}", new.display()))?;
    let script = dir.join("update.sh");
    std::fs::write(&script, HELPER).map_err(|e| format!("{}: {e}", script.display()))?;
    launch_helper(&script, &helper_args(std::process::id(), &new, &exe, &app, version, &failure_log()))
}

#[cfg(target_os = "macos")]
fn launch_helper(script: &Path, args: &[OsString]) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    // 별도 프로세스 그룹: 앱이 끝나도 도우미는 살아 있다.
    std::process::Command::new("/bin/sh")
        .arg(script)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(drop)
        .map_err(|e| format!("couldn't start the update helper: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn launch_helper(_script: &Path, _args: &[OsString]) -> Result<(), String> {
    Err("this update path is for macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        for (name, data) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            b.append_data(&mut h, name, *data).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap()
    }

    const MACH_O: &[u8] = &[0xcf, 0xfa, 0xed, 0xfe, 1, 2, 3];

    #[test]
    fn asset_urls_follow_the_release_names() {
        assert_eq!(
            tarball_url("0.19.3"),
            "https://github.com/zzamjak-cloud/PhotoCraft-CAT/releases/download/v0.19.3/photocraft-cat-0.19.3-macos-aarch64.tar.gz"
        );
        assert!(sig_url("0.19.3").ends_with("-macos-aarch64.tar.gz.sig"));
    }

    #[test]
    fn signed_tarballs_verify_and_tampering_is_refused() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let pair = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        let mut data = tarball(&[("photocraft-cat", MACH_O)]);
        let sig = pair.sign(&data);
        assert!(verify(&key, &data, sig.as_ref()).is_ok());
        assert!(verify(&PUBLIC_KEY, &data, sig.as_ref()).is_err(), "another key's signature");
        if let Some(b) = data.last_mut() {
            *b ^= 1;
        }
        assert!(verify(&key, &data, sig.as_ref()).is_err());
    }

    #[test]
    fn the_binary_is_taken_from_the_tarball() {
        // macOS tar 는 확장 속성을 `._<이름>` 으로 함께 넣는다.
        let data = tarball(&[("._photocraft-cat", b"apple double"), ("./photocraft-cat", MACH_O)]);
        assert_eq!(extract_binary(&data).unwrap(), MACH_O);
        assert!(extract_binary(&tarball(&[("photocraft", MACH_O)])).is_err(), "official app's name");
        assert!(extract_binary(&tarball(&[("photocraft-cat", b"#!/bin/sh\n")])).is_err(), "not a program");
        assert!(extract_binary(b"not gzip").is_err());
    }

    #[test]
    fn only_a_bundled_executable_is_replaced() {
        let exe = Path::new("/Applications/PhotoCAT.app/Contents/MacOS/PhotoCraft");
        assert_eq!(bundle_of(exe), Some(PathBuf::from("/Applications/PhotoCAT.app")));
        assert_eq!(bundle_of(Path::new("/usr/local/bin/photocraft")), None);
        assert_eq!(bundle_of(Path::new("/x/Contents/MacOS/PhotoCraft")), None);
    }

    #[test]
    fn helper_takes_the_arguments_in_order() {
        let args = helper_args(7, Path::new("/t/new"), Path::new("/A.app/Contents/MacOS/PhotoCraft"), Path::new("/A.app"), "1.2.3", Path::new("/t/f.txt"));
        let args: Vec<&str> = args.iter().filter_map(|a| a.to_str()).collect();
        assert_eq!(args, ["7", "/t/new", "/A.app/Contents/MacOS/PhotoCraft", "/A.app", "1.2.3", "/t/f.txt"]);
        for p in ["pid=$1", "new=$2", "exe=$3", "app=$4", "version=$5", "log=$6"] {
            assert!(HELPER.contains(p), "{p}");
        }
        assert!(HELPER.contains("mv -f \"$staged\" \"$exe\""), "same-folder rename");
    }

    #[cfg(unix)]
    #[test]
    fn helper_is_valid_sh() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("update.sh");
        std::fs::write(&script, HELPER).unwrap();
        let status = std::process::Command::new("/bin/sh").arg("-n").arg(&script).status().unwrap();
        assert!(status.success());
    }
}
