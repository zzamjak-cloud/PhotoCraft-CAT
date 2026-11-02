//! CAT 패널: Photoshop UXP 플러그인 CAT(그리고 그 PhotoCraft 외부판 JP_Plugin)을 네이티브 도크 패널로 이식.
//!
//! - `lib/`      PhotoCraft 조작 라이브러리 (엔진 명령 문자열은 여기에만 둔다)
//! - `features/` 기능 로직 (UI 없음, `lib` 만 사용)
//! - `ui/`       패널 UI (섹션별 파일)
//! - `store`     영속 데이터 (설정 폴더 하위 `cat/`)

pub mod features;
pub mod lib;
pub mod store;
pub mod ui;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::PhotocraftApp;
use features::folder_preset::FolderPreset;
use features::size_presets::SizeSlot;
use lib::guide::Guide;

pub use store::set_data_dir;

/// 설정 파일 이름 (CAT localStorage + selectedFolder.txt/keepDocSizeState.txt 대응).
const SETTINGS_FILE: &str = "settings.json";

/// 영속 설정. 키 구성은 CAT localStorage 키를 따른다.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CatSettings {
    /// 섹션 펼침 상태 (섹션 제목 → 열림)
    pub sections: BTreeMap<String, bool>,
    /// 내보내기 폴더 프리셋 (exportFolderPresets)
    pub folder_presets: Vec<FolderPreset>,
    /// 선택된 폴더 프리셋 (selectedFolder.txt)
    pub selected_folder: Option<FolderPreset>,
    /// Keep doc size 체크 (keepDocSizeState.txt)
    pub keep_doc_size: bool,
    /// 사이즈 프리셋 (storageKey → 커스텀 목록 + 선택)
    pub sizes: BTreeMap<String, SizeSlot>,
    /// Layers rename: Enter 텍스트 프리셋 (layerRenamerPresets)
    pub enter_presets: Vec<String>,
    /// Layers rename: Replace 텍스트 프리셋 (layerRenamerReplacePresets)
    pub replace_presets: Vec<String>,
    pub enter_selected: String,
    pub replace_selected: String,
    /// 자리수 1..=6 (renamerDigitCount)
    pub digit_count: Option<u32>,
    /// Auto Paste PNG (autoPastePngEnabledV1). CAT 과 같이 저장값이 없으면 켜짐.
    pub auto_paste_png: Option<bool>,
}

/// 진행률 표시 (라벨, 0..=100)
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub label: String,
    pub percent: f32,
}

/// 모달 종류
#[derive(Clone, Debug, PartialEq)]
pub enum ModalKind {
    /// 확인 버튼만 있는 알림
    Alert,
    /// 확인/취소
    Confirm,
    /// 텍스트 한 줄 입력
    Prompt { value: String },
    /// 사이즈 입력 (single 이면 값 하나)
    Size { w: String, h: String, single: bool, placeholders: [String; 2] },
}

/// 모달 응답 (확인을 눌렀을 때만 전달된다)
#[derive(Clone, Debug, PartialEq)]
pub enum ModalAnswer {
    Ok,
    Text(String),
    Size(u32, u32),
}

/// 모달 확인 후 실행할 작업
pub type ModalThen = Box<dyn FnOnce(&mut PhotocraftApp, ModalAnswer)>;

/// 큐에 쌓이는 모달 하나
pub struct Modal {
    pub title: String,
    pub message: String,
    pub kind: ModalKind,
    pub then: Option<ModalThen>,
}

/// CAT 전역 UI 상태 (앱 상태에 붙는다; 영속 부분은 `settings`)
#[derive(Default)]
pub struct CatState {
    /// 영속 데이터 폴더 (None = 디스크에 쓰지 않음: 테스트·웹)
    pub data_dir: Option<PathBuf>,
    pub settings: CatSettings,
    /// 진행 중인 히스토리 그룹의 coalesce 키 (`lib::core::with_history`)
    pub history_key: Option<String>,
    pub(crate) history_seq: u64,
    /// CAT 이 만든 임시 문서 (미저장이어도 확인 없이 닫을 수 있는 문서는 이 목록뿐)
    pub temp_docs: Vec<photocraft_doc::DocId>,
    /// Copy guides 로 복사한 가이드 (세션 동안 유지)
    pub copied_guides: Vec<Guide>,
    /// GUIDE "Only selected layer" 체크 (CAT 과 같이 저장하지 않음)
    pub guide_layer_bound: bool,
    pub progress: Option<Progress>,
    /// 알림/확인/입력 모달 큐 (앞에서부터 하나씩 표시)
    pub modals: VecDeque<Modal>,
    /// 모달을 그린 마지막 패스 (두 패널이 같은 프레임에 중복으로 그리지 않도록)
    pub(crate) modal_pass: Option<u64>,
    /// VECTOR 섹션 입력값·Image Trace 창 (저장하지 않음)
    pub vector: ui::vector::VectorState,
}

impl CatState {
    /// `data_dir` 의 설정을 읽어 상태를 만든다.
    pub fn load(data_dir: Option<PathBuf>) -> Self {
        let settings = store::load_json(data_dir.as_deref(), SETTINGS_FILE);
        Self { data_dir, settings, ..Default::default() }
    }

    /// 설정 저장 (실패는 로그만 남긴다; 패널 동작을 막지 않는다)
    pub fn save_settings(&self) {
        if let Err(e) = store::save_json(self.data_dir.as_deref(), SETTINGS_FILE, &self.settings) {
            log::warn!("CAT 설정을 저장하지 못했습니다: {e}");
        }
    }
}

/// 도크 "CAT Tools" 탭 본문
pub fn tools_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui::tools_panel(app, ui);
}

/// 도크 "9-Slice Images" 탭 본문
pub fn nine_slice_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui::nine_slice_tab(app, ui);
}
