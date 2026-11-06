//! Windows 자체 업데이트 (QuickFolder/Tauri 업데이터와 같은 방식).
//!
//! craft-update 는 Windows 에서 Authenticode 서명이 같은 파일만 설치하므로, 서명하지 않는 이 포크는
//! Windows 에서 이 모듈을 쓴다. 릴리스 워크플로가 설치 프로그램을 Ed25519 개인키(GitHub Secret
//! `UPDATE_SIGNING_KEY`)로 서명해 `<setup>.sig`(64바이트)를 함께 올리고, 앱은 내장 공개키
//! [`PUBLIC_KEY`] 로 확인한 뒤에만 실행한다. 도우미 PowerShell 스크립트가 앱 종료 후 설치
//! 프로그램을 조용히 실행하고 앱을 다시 띄운다.
//!
//! 포터블 사본은 이 경로로 설치형으로 전환된다: 도우미가 `PhotoCraftData` 의 설정을 설치형 위치로
//! 복사한 뒤 설치한다. 이전 버전 앱은 포터블 zip 만 받을 수 있으므로 릴리스에 zip 도 계속 올린다.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

/// 업데이트 서명 공개키 (Ed25519, 개인키는 GitHub Secret `UPDATE_SIGNING_KEY`).
pub const PUBLIC_KEY: [u8; 32] = [
    0xac, 0xf9, 0x09, 0xb0, 0x38, 0xbd, 0x0e, 0x7e, 0x22, 0x72, 0x13, 0x1c, 0xa6, 0x44, 0x33, 0x5a, 0x2b, 0x8a, 0xd4, 0xdb, 0xfe, 0x74, 0xb1, 0x9c, 0x91, 0xdb,
    0x60, 0xde, 0x66, 0x7f, 0xb0, 0xa2,
];

/// 받는 설치 프로그램의 최대 크기.
const MAX_SETUP_BYTES: u64 = 1 << 30;

/// 포터블 표시 파일 (`apps/photocraft/src/app_dirs.rs` 의 `PORTABLE_MARKERS` 와 같아야 한다).
const PORTABLE_MARKERS: [&str; 2] = ["portable.txt", "PhotoCraft.portable"];

/// 설치 프로그램이 까는 실행 파일 이름 (`packaging/windows/photocat.iss`).
const INSTALLED_EXE: &str = "PhotoCAT.exe";

const HELPER: &str = include_str!("win_update_helper.ps1");

/// 이 플랫폼에서 이 모듈이 업데이트를 맡는가 (Windows).
pub fn active() -> bool {
    cfg!(windows)
}

fn repo() -> &'static str {
    crate::UPDATE_APP.repo
}

/// 버전 `v` 의 설치 프로그램 주소.
pub fn setup_url(v: &str) -> String {
    format!("https://github.com/{}/releases/download/v{v}/{}-{v}-windows-x64-setup.exe", repo(), crate::UPDATE_APP.id)
}

/// 서명 파일 주소.
pub fn sig_url(v: &str) -> String {
    format!("{}.sig", setup_url(v))
}

/// `dir` 이 포터블 사본의 폴더인가.
fn portable_at(dir: &Path) -> bool {
    PORTABLE_MARKERS.iter().any(|m| dir.join(m).is_file())
}

/// 이 실행 파일이 아직 포터블 사본인가 (설치형 전환을 제안한다).
pub fn is_portable() -> bool {
    std::env::current_exe().ok().and_then(|exe| exe.parent().map(portable_at)).unwrap_or(false)
}

/// `data` 가 `key` 의 개인키로 서명되었는지 (Ed25519, 64바이트 서명).
pub fn verify(key: &[u8; 32], data: &[u8], sig: &[u8]) -> Result<(), String> {
    if sig.len() != 64 {
        return Err(format!("the update signature is {} bytes, expected 64", sig.len()));
    }
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(data, sig)
        .map_err(|_| "the update's signature does not match; it was not installed".to_string())
}

/// 업데이트 작업 폴더 (임시 폴더 아래).
fn work_dir() -> PathBuf {
    std::env::temp_dir().join(format!("{}-update", crate::UPDATE_APP.id))
}

/// 도우미가 실패를 남기는 파일.
fn failure_log() -> PathBuf {
    work_dir().join("failed.txt")
}

/// 지난 업데이트가 실패했다면 그 이유 (한 번만 돌려주고 지운다).
pub fn take_failure() -> Option<String> {
    let path = failure_log();
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let text = text.trim().trim_start_matches('\u{feff}').to_string();
    (!text.is_empty()).then_some(text)
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder().tls_config(tls).timeout_connect(Some(std::time::Duration::from_secs(20))).build().into()
}

fn download(agent: &ureq::Agent, url: &str, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut res = agent.get(url).call().map_err(|e| format!("couldn't download {url}: {e}"))?;
    let mut out = Vec::new();
    res.body_mut().as_reader().take(max + 1).read_to_end(&mut out).map_err(|e| format!("couldn't download {url}: {e}"))?;
    if out.len() as u64 > max {
        return Err(format!("{url} is larger than expected"));
    }
    Ok(out)
}

/// 도우미에 넘길 인자. 포터블 사본 `exe` 는 기본 설치 폴더 아래 `local_app_data` 로 전환한다.
fn helper_args(pid: u32, setup: &Path, exe: &Path, log: &Path, local_app_data: Option<&Path>) -> Result<Vec<OsString>, String> {
    let dir = exe.parent().ok_or_else(|| "couldn't find the program's folder".to_string())?;
    let mut args: Vec<OsString> = vec!["-AppPid".into(), pid.to_string().into(), "-Setup".into(), setup.into(), "-Log".into(), log.into()];
    if portable_at(dir) {
        let local = local_app_data.ok_or_else(|| "couldn't find the folder for installed programs".to_string())?;
        let install = local.join("Programs").join(crate::UPDATE_APP.name);
        args.extend(["-Exe".into(), install.join(INSTALLED_EXE).into(), "-InstallDir".into(), install.into()]);
        args.extend(["-PortableDir".into(), dir.into(), "-OldExe".into(), exe.into()]);
    } else {
        args.extend(["-Exe".into(), exe.into()]);
    }
    Ok(args)
}

/// 내려받고 확인하고 도우미를 띄운다. 성공하면 앱이 곧 종료되어야 한다.
fn prepare(version: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program ({e})"))?;
    let agent = agent();
    let sig = download(&agent, &sig_url(version), 1024)?;
    let setup = download(&agent, &setup_url(version), MAX_SETUP_BYTES)?;
    verify(&PUBLIC_KEY, &setup, &sig)?;
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let setup_path = dir.join(format!("{}-{version}-setup.exe", crate::UPDATE_APP.id));
    std::fs::write(&setup_path, &setup).map_err(|e| format!("{}: {e}", setup_path.display()))?;
    let script = dir.join("update.ps1");
    std::fs::write(&script, HELPER).map_err(|e| format!("{}: {e}", script.display()))?;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let args = helper_args(std::process::id(), &setup_path, &exe, &failure_log(), local.as_deref())?;
    launch_helper(&script, &args)
}

#[cfg(windows)]
fn launch_helper(script: &Path, args: &[OsString]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW: 앱이 끝나도 살아 있고 창이 없다.
    const FLAGS: u32 = 0x0000_0008 | 0x0000_0200 | 0x0800_0000;
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(script)
        .args(args)
        .creation_flags(FLAGS)
        .spawn()
        .map(drop)
        .map_err(|e| format!("couldn't start the update helper: {e}"))
}

#[cfg(not(windows))]
fn launch_helper(_script: &Path, _args: &[OsString]) -> Result<(), String> {
    Err("this update path is for Windows".into())
}

/// 업데이트 창의 상태.
#[derive(Debug, PartialEq)]
enum State {
    Offer,
    Working,
    Failed(String),
}

/// 화면 응답.
#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    OpenUrl(String),
    /// 도우미가 기다리고 있다: 앱을 정상 종료 경로로 닫는다.
    Close,
}

/// 새 버전 알림과 설치 진행.
#[derive(Default)]
pub struct WinUpdater {
    offer: Option<(String, String, State)>,
    job: Option<Receiver<Result<(), String>>>,
}

impl WinUpdater {
    /// 새 버전 `version` (릴리스 페이지 `url`) 을 알린다.
    pub fn offer(&mut self, version: &str, url: &str) {
        if self.job.is_none() {
            self.offer = Some((version.to_string(), url.to_string(), State::Offer));
        }
    }

    pub fn prompt_open(&self) -> bool {
        self.offer.is_some()
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some((version, _, state)) = &mut self.offer else { return };
        *state = State::Working;
        let (tx, rx) = channel();
        let version = version.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(prepare(&version));
            ctx.request_repaint();
        });
        self.job = Some(rx);
    }

    /// 창을 그린다. `clean` 이 아니면(저장 안 한 작업) 설치를 시작하지 않는다.
    pub fn show(&mut self, ctx: &egui::Context, clean: bool) -> Action {
        let mut action = Action::None;
        if let Some(rx) = &self.job
            && let Ok(result) = rx.try_recv()
        {
            self.job = None;
            match result {
                Ok(()) => {
                    self.offer = None;
                    return Action::Close;
                }
                Err(e) => {
                    if let Some((_, _, state)) = &mut self.offer {
                        *state = State::Failed(e);
                    }
                }
            }
        }
        let Some((version, url, state)) = &self.offer else { return action };
        let (version, url) = (version.clone(), url.clone());
        let working = *state == State::Working;
        let failed = match state {
            State::Failed(e) => Some(e.clone()),
            _ => None,
        };
        let mut start = false;
        let mut later = false;
        egui::Window::new(tl!("Software Update")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(crate::i18n::fmt(
                tl!("{app} {new} is available. You have {current}."),
                &[("app", crate::UPDATE_APP.name), ("new", &version), ("current", photocraft_engine::build_info::VERSION)],
            ));
            if is_portable() {
                ui.label(tl!("This update switches to the installed version. Your settings are kept."));
            }
            if working {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tl!("Downloading and verifying the update…"));
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            if let Some(e) = &failed {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            if !clean {
                ui.label(tl!("Save or close your documents to update now."));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(clean && !working, egui::Button::new(tl!("Update and Restart"))).clicked() {
                    start = true;
                }
                if ui.add_enabled(!working, egui::Button::new(tl!("Download"))).clicked() {
                    action = Action::OpenUrl(url.clone());
                }
                if ui.add_enabled(!working, egui::Button::new(tl!("Later"))).clicked() {
                    later = true;
                }
            });
        });
        if start {
            self.start(ctx);
        }
        if later || matches!(action, Action::OpenUrl(_)) {
            self.offer = None;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).filter_map(|i| s.get(i..i + 2)).map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    // openssl genpkey -algorithm ed25519 / pkeyutl -sign -rawin 으로 만든 시험용 키·서명.
    const TEST_KEY: &str = "bce39923120e0d0ef87113a16bf6881a3aa7b1fd879a3e50358fc4861f5bf061";
    const TEST_SIG: &str = "8570c0c149decdcfc45d2b3c3e087a955a0c208c62607d7d983cac377237fad35eff28e6be7d3d2dac4ebc0a8c0b96e279b0d35487a828b52687622b24bc7a0d";

    #[test]
    fn openssl_signatures_verify_and_tampering_is_refused() {
        let key: [u8; 32] = hex(TEST_KEY).try_into().unwrap();
        let sig = hex(TEST_SIG);
        assert!(verify(&key, b"photocraft-cat update test", &sig).is_ok());
        assert!(verify(&key, b"photocraft-cat update tesT", &sig).is_err());
        assert!(verify(&PUBLIC_KEY, b"photocraft-cat update test", &sig).is_err(), "another key's signature");
        assert!(verify(&key, b"x", &sig[..63]).is_err());
    }

    #[test]
    fn asset_urls_follow_the_release_names() {
        assert_eq!(
            setup_url("0.19.3"),
            "https://github.com/zzamjak-cloud/PhotoCraft-CAT/releases/download/v0.19.3/photocraft-cat-0.19.3-windows-x64-setup.exe"
        );
        assert!(sig_url("0.19.3").ends_with("-windows-x64-setup.exe.sig"));
    }

    #[test]
    fn helper_takes_the_arguments_we_pass() {
        for p in ["$AppPid", "$Setup", "$Exe", "$Log", "$PortableDir", "$InstallDir", "$OldExe"] {
            assert!(HELPER.contains(p), "{p}");
        }
    }

    /// Windows PowerShell 5.1 은 BOM 없는 스크립트를 시스템 코드 페이지로 읽는다. CP949 에서는 UTF-8
    /// 한글 주석이 줄바꿈을 삼켜 다음 줄까지 주석이 될 수 있어 도우미는 ASCII 만 쓴다.
    #[test]
    fn helper_is_ascii() {
        assert!(HELPER.is_ascii());
    }

    fn arg_after(args: &[OsString], name: &str) -> Option<OsString> {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
    }

    #[test]
    fn an_installed_copy_updates_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join(INSTALLED_EXE);
        let args = helper_args(7, Path::new("s.exe"), &exe, Path::new("l.txt"), None).unwrap();
        assert_eq!(arg_after(&args, "-Exe"), Some(exe.into()));
        for name in ["-PortableDir", "-InstallDir", "-OldExe"] {
            assert_eq!(arg_after(&args, name), None, "{name}");
        }
    }

    #[test]
    fn a_portable_copy_converts_to_the_default_install_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("portable.txt"), b"").unwrap();
        assert!(portable_at(dir.path()));
        let exe = dir.path().join("photocraft-cat.exe");
        let local = Path::new("local");
        let args = helper_args(7, Path::new("s.exe"), &exe, Path::new("l.txt"), Some(local)).unwrap();
        let install = local.join("Programs").join("PhotoCAT");
        assert_eq!(arg_after(&args, "-InstallDir"), Some(install.clone().into()));
        assert_eq!(arg_after(&args, "-Exe"), Some(install.join("PhotoCAT.exe").into()));
        assert_eq!(arg_after(&args, "-PortableDir"), Some(dir.path().into()));
        assert_eq!(arg_after(&args, "-OldExe"), Some(exe.into()));
        // 설치 폴더를 알 수 없으면 조용히 다른 곳에 깔지 않고 오류로 알린다.
        assert!(helper_args(7, Path::new("s.exe"), &dir.path().join("a.exe"), Path::new("l.txt"), None).is_err());
    }

    #[test]
    fn prompt_offers_and_later_dismisses() {
        let ctx = egui::Context::default();
        let mut u = WinUpdater::default();
        u.offer("9.9.9", "https://example.invalid");
        assert!(u.prompt_open());
        let mut action = Action::None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| action = u.show(ui.ctx(), true));
        output.textures_delta.clear();
        assert_eq!(action, Action::None);
        assert!(u.prompt_open());
    }
}
