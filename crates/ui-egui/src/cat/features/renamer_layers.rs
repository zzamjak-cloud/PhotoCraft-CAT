//! Layers rename (CAT renamerLayers.js + lib_text_preset.js 대응): Enter/Replace 텍스트 프리셋, 자리수, 7가지 일괄 이름 변경

use crate::PhotocraftApp;
use crate::cat::CatState;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer;

/// 이름 변경 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameType {
    Prefix,
    Suffix,
    Rename,
    Replace,
    Number,
    ReverseNumber,
    Remove,
}

/// 텍스트 프리셋 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPresetKind {
    /// "Enter" (Text to apply)
    Enter,
    /// "Replace" (Replacement text)
    Replace,
}

impl TextPresetKind {
    pub fn placeholder(self) -> &'static str {
        match self {
            TextPresetKind::Enter => "Enter",
            TextPresetKind::Replace => "Replace",
        }
    }
    pub fn tooltip(self) -> &'static str {
        match self {
            TextPresetKind::Enter => "Text to apply",
            TextPresetKind::Replace => "Replacement text",
        }
    }
}

pub const MAX_PRESETS: usize = 10;
pub const DIGIT_VALUES: [u32; 6] = [1, 2, 3, 4, 5, 6];

fn list(cat: &CatState, kind: TextPresetKind) -> &Vec<String> {
    match kind {
        TextPresetKind::Enter => &cat.settings.enter_presets,
        TextPresetKind::Replace => &cat.settings.replace_presets,
    }
}

fn list_mut(cat: &mut CatState, kind: TextPresetKind) -> &mut Vec<String> {
    match kind {
        TextPresetKind::Enter => &mut cat.settings.enter_presets,
        TextPresetKind::Replace => &mut cat.settings.replace_presets,
    }
}

fn selected_mut(cat: &mut CatState, kind: TextPresetKind) -> &mut String {
    match kind {
        TextPresetKind::Enter => &mut cat.settings.enter_selected,
        TextPresetKind::Replace => &mut cat.settings.replace_selected,
    }
}

/// 프리셋 목록
pub fn text_presets(cat: &CatState, kind: TextPresetKind) -> Vec<String> {
    list(cat, kind).clone()
}

/// 프리셋 추가 (트림, 중복이면 추가 없음; 최대 개수 초과 시 오류) → 추가/기존 텍스트
pub fn add_text_preset(cat: &mut CatState, kind: TextPresetKind, text: &str) -> CatResult<String> {
    let t = text.trim().to_string();
    if t.is_empty() {
        return Err("프리셋 텍스트를 입력해주세요.".into());
    }
    let l = list_mut(cat, kind);
    if l.contains(&t) {
        return Ok(t);
    }
    if l.len() >= MAX_PRESETS {
        return Err(format!("최대 {MAX_PRESETS}개의 프리셋만 추가할 수 있습니다."));
    }
    l.push(t.clone());
    cat.save_settings();
    Ok(t)
}

/// 프리셋 제거. 선택 중이던 텍스트면 선택 해제
pub fn remove_text_preset(cat: &mut CatState, kind: TextPresetKind, text: &str) {
    list_mut(cat, kind).retain(|p| p != text);
    let sel = selected_mut(cat, kind);
    if sel == text {
        sel.clear();
    }
    cat.save_settings();
}

/// 선택 텍스트 (프리셋 목록에 남아 있을 때만 유효)
pub fn selected_text(cat: &CatState, kind: TextPresetKind) -> String {
    let sel = match kind {
        TextPresetKind::Enter => &cat.settings.enter_selected,
        TextPresetKind::Replace => &cat.settings.replace_selected,
    };
    if !sel.is_empty() && list(cat, kind).contains(sel) { sel.clone() } else { String::new() }
}

/// 선택 (빈 문자열 = 해제)
pub fn set_selected_text(cat: &mut CatState, kind: TextPresetKind, text: &str) {
    *selected_mut(cat, kind) = text.to_string();
    cat.save_settings();
}

/// 자리수 (1..=6, 기본 1)
pub fn digit_count(cat: &CatState) -> u32 {
    cat.settings.digit_count.filter(|d| DIGIT_VALUES.contains(d)).unwrap_or(1)
}

pub fn set_digit_count(cat: &mut CatState, n: u32) {
    if DIGIT_VALUES.contains(&n) {
        cat.settings.digit_count = Some(n);
        cat.save_settings();
    }
}

fn validate_inputs(kind: RenameType, text: &str, replace: &str) -> CatResult<()> {
    if matches!(kind, RenameType::Number | RenameType::ReverseNumber) {
        return Ok(());
    }
    if kind != RenameType::Rename && text.is_empty() {
        return Err("변경할 텍스트를 입력해주세요.".into());
    }
    if kind == RenameType::Replace && replace.is_empty() {
        return Err("대체할 텍스트를 입력해주세요.".into());
    }
    Ok(())
}

/// 새 이름 계산 (replace/remove 는 첫 번째 일치만 문자 그대로 치환; JS String.replace 와 같음)
pub fn generate_new_name(name: &str, kind: RenameType, text: &str, replace: &str, index: usize, total: usize, digits: usize) -> String {
    match kind {
        RenameType::Prefix => format!("{text}{name}"),
        RenameType::Suffix => format!("{name}{text}"),
        RenameType::Rename => {
            if text.is_empty() {
                name.to_string()
            } else {
                text.to_string()
            }
        }
        RenameType::Replace => name.replacen(text, replace, 1),
        RenameType::Remove => name.replacen(text, "", 1),
        RenameType::Number => format!("{name}{:0digits$}", total.saturating_sub(index)),
        RenameType::ReverseNumber => format!("{name}{:0digits$}", index + 1),
    }
}

/// 선택 레이어(위→아래) 일괄 이름 변경. Undo 한 단계, 원래 선택 복원.
pub fn run(app: &mut PhotocraftApp, kind: RenameType, text: &str, replace: &str, digits: u32) -> CatResult<()> {
    let layers = layer::selected_layers(app)?;
    if layers.is_empty() {
        return Err("선택된 레이어가 없습니다.".into());
    }
    validate_inputs(kind, text, replace)?;
    let total = layers.len();
    let ids: Vec<_> = layers.iter().map(|l| l.id).collect();
    with_history(app, |app| {
        for (i, l) in layers.iter().enumerate() {
            let new_name = generate_new_name(&l.name, kind, text, replace, i, total, digits as usize);
            if new_name != l.name {
                layer::rename_layer(app, l.id, &new_name)?;
            }
        }
        // 원래 선택 상태 복원 (첫 레이어가 활성)
        layer::select_layers(app, &ids)
    })
}

/// UI 용: 저장된 프리셋 선택값·자리수로 실행
pub fn run_with_settings(app: &mut PhotocraftApp, kind: RenameType) -> CatResult<()> {
    let text = selected_text(&app.cat, TextPresetKind::Enter);
    let replace = selected_text(&app.cat, TextPresetKind::Replace);
    let digits = digit_count(&app.cat);
    run(app, kind, &text, &replace, digits)
}
