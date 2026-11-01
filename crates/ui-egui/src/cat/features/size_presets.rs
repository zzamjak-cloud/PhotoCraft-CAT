//! 사이즈 프리셋 드롭다운 데이터 (CAT sizeDropdown.js 대응). 기본 프리셋 + 사용자 커스텀 사이즈.

use serde::{Deserialize, Serialize};

use crate::cat::CatState;

/// 너비·높이 (단일 값 모드에서는 width = height)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn square(v: u32) -> Self {
        Self { width: v, height: v }
    }
}

/// 저장 슬롯: 커스텀 사이즈 목록 + 선택값
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SizeSlot {
    pub customs: Vec<Size>,
    pub selected: Option<Size>,
}

/// 프리셋 정의
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizePresetConfig {
    /// CAT localStorage 키 (설정 파일 안의 키)
    pub storage_key: &'static str,
    pub default_size: u32,
    pub sizes: &'static [u32],
    /// 항상 "AxB" 로 표시
    pub pair_format: bool,
    /// 단일 값 (width = height)
    pub single: bool,
    pub placeholders: [&'static str; 2],
    pub tooltip: &'static str,
}

const DEFAULT_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];

/// Import 문서 크기 (기본 1024)
pub const IMPORT_DOC_SIZE: SizePresetConfig = SizePresetConfig {
    storage_key: "importDocSizePresets",
    default_size: 1024,
    sizes: DEFAULT_SIZES,
    pair_format: false,
    single: false,
    placeholders: ["W", "H"],
    tooltip: "Document size",
};
/// Arrange in grid 셀 크기 (기본 128)
pub const ARRAY_GRID_SIZE: SizePresetConfig = SizePresetConfig {
    storage_key: "arrayGridSizePresets",
    default_size: 128,
    sizes: DEFAULT_SIZES,
    pair_format: false,
    single: false,
    placeholders: ["W", "H"],
    tooltip: "Grid cell size",
};
/// Arrange in grid 열 수 (기본 10)
pub const ARRAY_COLUMNS: SizePresetConfig = SizePresetConfig {
    storage_key: "arrayColumnPresets",
    default_size: 10,
    sizes: &[5, 8, 10, 20],
    pair_format: false,
    single: true,
    placeholders: ["Columns", ""],
    tooltip: "Columns per row",
};
/// 가이드 그리드 행x열 (기본 2x2; width = 행, height = 열)
pub const GUIDE_GRID: SizePresetConfig = SizePresetConfig {
    storage_key: "guideGridPresets",
    default_size: 2,
    sizes: &[2, 3, 4, 5, 6, 8, 16, 32, 64],
    pair_format: true,
    single: false,
    placeholders: ["Row", "Col"],
    tooltip: "Rows x Columns",
};
/// 가장자리 가이드 오프셋 (기본 30)
pub const GUIDE_OFFSET: SizePresetConfig = SizePresetConfig {
    storage_key: "guideOffsetPresets",
    default_size: 30,
    sizes: &[5, 10, 15, 20, 30, 40],
    pair_format: false,
    single: true,
    placeholders: ["Offset", ""],
    tooltip: "Offset",
};

/// 표시 문자열
pub fn format_size(cfg: &SizePresetConfig, s: Size) -> String {
    if cfg.single {
        format!("{}", s.width)
    } else if cfg.pair_format {
        format!("{}x{}", s.width, s.height)
    } else if s.width == s.height {
        format!("{}", s.width)
    } else {
        format!("{} x {}", s.width, s.height)
    }
}

fn slot<'a>(cat: &'a CatState, cfg: &SizePresetConfig) -> Option<&'a SizeSlot> {
    cat.settings.sizes.get(cfg.storage_key)
}

/// 커스텀 사이즈 목록
pub fn custom_sizes(cat: &CatState, cfg: &SizePresetConfig) -> Vec<Size> {
    slot(cat, cfg).map(|s| s.customs.clone()).unwrap_or_default()
}

/// 선택값 (없거나 잘못되면 기본값)
pub fn selected_size(cat: &CatState, cfg: &SizePresetConfig) -> Size {
    match slot(cat, cfg).and_then(|s| s.selected) {
        Some(s) if s.width > 0 && s.height > 0 => s,
        _ => Size::square(cfg.default_size),
    }
}

/// 선택 (저장)
pub fn select_size(cat: &mut CatState, cfg: &SizePresetConfig, s: Size) {
    cat.settings.sizes.entry(cfg.storage_key.to_string()).or_default().selected = Some(s);
    cat.save_settings();
}

/// 커스텀 사이즈 추가 (기본 프리셋·중복이면 추가 없이 선택만)
pub fn add_custom_size(cat: &mut CatState, cfg: &SizePresetConfig, s: Size) {
    let s = if cfg.single { Size::square(s.width) } else { s };
    let slot = cat.settings.sizes.entry(cfg.storage_key.to_string()).or_default();
    let is_default = s.width == s.height && cfg.sizes.contains(&s.width);
    if !is_default && !slot.customs.contains(&s) {
        slot.customs.push(s);
    }
    slot.selected = Some(s);
    cat.save_settings();
}

/// 커스텀 사이즈 제거. 선택 중이던 값이면 기본값으로 되돌린다.
pub fn remove_custom_size(cat: &mut CatState, cfg: &SizePresetConfig, index: usize) {
    let selected = selected_size(cat, cfg);
    let slot = cat.settings.sizes.entry(cfg.storage_key.to_string()).or_default();
    if index < slot.customs.len() {
        let removed = slot.customs.remove(index);
        if removed == selected {
            slot.selected = Some(Size::square(cfg.default_size));
        }
    }
    cat.save_settings();
}
