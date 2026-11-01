//! 공용 코어: 동기 명령 실행, Undo 한 단계 그룹, 직접 편집, 알림/확인/입력 모달, 파일 다이얼로그.
//! (CAT lib/lib.js · JP_Plugin core/bridge.ts 대응)

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::cat::{Modal, ModalAnswer, ModalKind};
use crate::file_dialog::{FileDialogAnswer, FileDialogRequest};

/// CAT 라이브러리 공통 결과 (오류는 사용자에게 보여줄 한국어 문장)
pub type CatResult<T> = Result<T, String>;

/// 아직 구현하지 않은 기능의 오류
pub fn unimplemented_feature<T>() -> CatResult<T> {
    Err("미구현".into())
}

/// 문서 히스토리 밖 명령: coalesce 키를 붙이지 않는다 (문서 생성/닫기/활성화 등)
fn groupable(id: &str) -> bool {
    !["file.", "document.", "command.", "prefs.", "ui.", "window."].iter().any(|p| id.starts_with(p))
}

/// 엔진 명령을 **동기로** 실행한다 (백그라운드 잡으로 넘기지 않음).
/// 히스토리 그룹(`with_history`) 안이면 공통 coalesce 키를 붙여 Undo 한 단계로 합친다.
pub fn exec(app: &mut PhotocraftApp, id: &str, params: Value) -> CatResult<Value> {
    let params = if params.is_null() { json!({}) } else { params };
    let key = app.cat.history_key.clone().filter(|_| groupable(id));
    let background = std::mem::replace(&mut app.background_jobs, false);
    let r = match key {
        Some(key) if params.get("coalesce").is_none() => {
            let mut grouped = params.clone();
            if let Some(o) = grouped.as_object_mut() {
                o.insert("coalesce".into(), json!(key));
            }
            match app.run(id, grouped) {
                // coalesce 를 모르는 엄격한 명령: 파라미터 검증은 실행 전이라 재시도해도 중복 실행이 없다
                Err(e) if e.contains("coalesce") => app.run(id, params),
                r => r,
            }
        }
        _ => app.run(id, params),
    };
    app.background_jobs = background;
    r
}

/// 실패해도 오류를 돌려주지 않는 실행 (실패 시 None)
pub fn try_exec(app: &mut PhotocraftApp, id: &str, params: Value) -> Option<Value> {
    exec(app, id, params).ok()
}

/// 콜백 안의 모든 문서 변경을 Undo 한 단계로 묶는다 (UXP executeAsModal + suspendHistory 대응).
/// 중첩 호출은 바깥 그룹에 합류한다.
pub fn with_history<T>(app: &mut PhotocraftApp, f: impl FnOnce(&mut PhotocraftApp) -> CatResult<T>) -> CatResult<T> {
    if app.cat.history_key.is_some() {
        return f(app);
    }
    app.cat.history_seq += 1;
    app.cat.history_key = Some(format!("cat-{}", app.cat.history_seq));
    let r = f(app);
    app.cat.history_key = None;
    r
}

/// 활성 문서를 직접 편집한다 (엔진 명령이 없는 조작용). 히스토리 그룹 안이면 같은 단계로 합쳐진다.
pub fn edit<R>(
    app: &mut PhotocraftApp,
    label: &str,
    f: impl FnOnce(&mut photocraft_doc::Document, &mut Option<photocraft_doc::LayerId>) -> photocraft_engine::Result<R>,
) -> CatResult<R> {
    let key = app.cat.history_key.clone();
    let r = app.session.edit_coalesced(label, key.as_deref(), f).map_err(|e| e.to_string());
    app.sync_views();
    r
}

/// 상태 표시줄 메시지
pub fn notify(app: &mut PhotocraftApp, message: impl Into<String>) {
    app.ui.status = message.into();
    app.ui.status_error = false;
}

/// 알림 모달 (UXP showAlert 대응)
pub fn alert(app: &mut PhotocraftApp, message: impl Into<String>) {
    app.cat.modals.push_back(Modal { title: "CAT".into(), message: message.into(), kind: ModalKind::Alert, then: None });
}

/// 확인 모달: 확인을 누르면 `on_yes` 실행
pub fn confirm(app: &mut PhotocraftApp, message: impl Into<String>, on_yes: impl FnOnce(&mut PhotocraftApp) + 'static) {
    app.cat.modals.push_back(Modal { title: "CAT".into(), message: message.into(), kind: ModalKind::Confirm, then: Some(Box::new(move |app, _| on_yes(app))) });
}

/// 텍스트 입력 모달: 확인을 누르면 입력값으로 `on_ok` 실행
pub fn prompt(app: &mut PhotocraftApp, title: impl Into<String>, initial: impl Into<String>, on_ok: impl FnOnce(&mut PhotocraftApp, String) + 'static) {
    app.cat.modals.push_back(Modal {
        title: title.into(),
        message: String::new(),
        kind: ModalKind::Prompt { value: initial.into() },
        then: Some(Box::new(move |app, answer| {
            if let ModalAnswer::Text(t) = answer {
                on_ok(app, t);
            }
        })),
    });
}

/// 사이즈 입력 모달 ("Custom Value"): single 이면 값 하나(W=H)
pub fn prompt_size(app: &mut PhotocraftApp, single: bool, placeholders: [&str; 2], on_ok: impl FnOnce(&mut PhotocraftApp, u32, u32) + 'static) {
    app.cat.modals.push_back(Modal {
        title: "Custom Value".into(),
        message: String::new(),
        kind: ModalKind::Size { w: String::new(), h: String::new(), single, placeholders: placeholders.map(str::to_string) },
        then: Some(Box::new(move |app, answer| {
            if let ModalAnswer::Size(w, h) = answer {
                on_ok(app, w, h);
            }
        })),
    });
}

/// 기능 실행 래퍼: 실패하면 "<label> 실패:" 알림 (CAT runSafe 대응). 성공 값은 그대로 돌려준다.
pub fn run_safe<T>(app: &mut PhotocraftApp, label: &str, f: impl FnOnce(&mut PhotocraftApp) -> CatResult<T>) -> Option<T> {
    let background = std::mem::replace(&mut app.background_jobs, false);
    let r = f(app);
    app.background_jobs = background;
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            alert(app, format!("{label} 실패:\n{e}"));
            None
        }
    }
}

/// 파일 선택 다이얼로그 (확장자 필터, 여러 개 가능). 응답은 다음 프레임에 `then` 으로 온다. 취소하면 아무 것도 하지 않는다.
pub fn pick_files(app: &mut PhotocraftApp, extensions: &[&str], multiple: bool, then: impl FnOnce(&mut PhotocraftApp, Vec<PathBuf>) + 'static) {
    let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
    let extensions = Some(extensions.iter().map(|e| e.to_string()).collect());
    let asked = app.ask_file(FileDialogRequest::Open { multiple, initial_dir, extensions }, move |app, answer| {
        if let FileDialogAnswer::Paths(paths) = answer {
            then(app, paths.into_iter().map(PathBuf::from).collect());
        }
        Ok(Value::Null)
    });
    report_dialog_error(app, asked);
}

/// 폴더 선택 다이얼로그. 취소하면 아무 것도 하지 않는다.
pub fn pick_folder(app: &mut PhotocraftApp, then: impl FnOnce(&mut PhotocraftApp, PathBuf) + 'static) {
    let asked = app.ask_folder(move |app, path| {
        then(app, PathBuf::from(path));
        Ok(Value::Null)
    });
    report_dialog_error(app, asked);
}

fn report_dialog_error(app: &mut PhotocraftApp, asked: Result<Value, String>) {
    // 다이얼로그 서비스가 없을 때(테스트 등)의 "cancelled" 는 조용히 무시
    if let Err(e) = asked
        && e != crate::file_dialog::CANCELLED
    {
        alert(app, e);
    }
}

/// 파일명에서 사용할 수 없는 문자를 `_` 로 바꾼다 (UXP sanitizeFileName 대응)
pub fn sanitize_file_name(name: &str) -> String {
    name.chars().map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c }).collect::<String>().trim().to_string()
}
