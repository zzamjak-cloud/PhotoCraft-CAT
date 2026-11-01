//! CAT 패널 UI: 공용 위젯(섹션, 아이콘 버튼, 체크박스, 드롭다운, 사이즈 프리셋, 진행 바, 모달)과 패널 본문.
//! 섹션별 배치는 JP_Plugin `src/panels/*.tsx` (CAT index.html) 를 따른다.

pub mod export;
pub mod extra;
pub mod guide;
pub mod icon_search;
pub mod icons;
pub mod import;
pub mod nine_slice;
pub mod rename;

use egui::{Id, RichText, vec2};

use crate::PhotocraftApp;
use crate::cat::features::size_presets::{self, Size, SizePresetConfig};
use crate::cat::lib::core::prompt_size;
use crate::cat::{ModalAnswer, ModalKind};

/// 아이콘 버튼 크기 (CAT sp-icons-button 30x30)
const BUTTON: f32 = 30.0;
/// 아이콘 그림 크기
const ICON: f32 = 22.0;

/// "CAT Tools" 탭 본문
pub fn tools_panel(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    export::show(app, ui);
    import::show(app, ui);
    rename::show(app, ui);
    guide::show(app, ui);
    extra::show(app, ui);
    progress_bar(app, ui);
    show_modals(app, ui.ctx());
}

/// "9-Slice Images" 탭 본문
pub fn nine_slice_tab(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    nine_slice::show(app, ui);
    progress_bar(app, ui);
    show_modals(app, ui.ctx());
}

/// 접이식 섹션 ("EXPORT +" / "EXPORT -"). 펼침 상태는 설정에 저장된다 (CAT 처럼 처음엔 접힘).
pub fn section(app: &mut PhotocraftApp, ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut PhotocraftApp, &mut egui::Ui)) {
    let open = app.cat.settings.sections.get(title).copied().unwrap_or(false);
    let label = format!("{title} {}", if open { "-" } else { "+" });
    let header = ui.add_sized([ui.available_width(), 22.0], egui::Button::new(RichText::new(label).strong()).frame(false));
    if header.clicked() {
        app.cat.settings.sections.insert(title.to_string(), !open);
        app.cat.save_settings();
    }
    if open {
        body(app, ui);
    }
    ui.separator();
}

/// 아이콘 버튼 (툴팁 = 접근성 라벨). 아이콘을 못 읽으면 툴팁 첫 글자를 쓴다.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    let button = match icons::texture(ui.ctx(), icon) {
        Some(tex) => egui::Button::image(egui::Image::new((tex.id(), vec2(ICON, ICON)))),
        None => egui::Button::new(tooltip.chars().next().map(String::from).unwrap_or_default()),
    };
    let resp = ui.add_sized([BUTTON, BUTTON], button);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), tooltip));
    resp.on_hover_text(tooltip)
}

/// 글자 버튼 (9-slice 툴바의 "+", "Del" 등)
pub fn text_button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    let resp = ui.add_sized([BUTTON, BUTTON], egui::Button::new(label));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), tooltip));
    resp.on_hover_text(tooltip)
}

/// 체크박스. label 이 비면 tooltip 이 접근성 라벨이 된다. → 바뀌었으면 true
pub fn checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str, tooltip: &str) -> bool {
    let resp = ui.checkbox(value, label);
    if label.is_empty() {
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *value, tooltip));
    }
    resp.on_hover_text(tooltip).changed()
}

/// 드롭다운 항목
#[derive(Clone, Debug, PartialEq)]
pub struct DropdownItem {
    pub label: String,
    /// 사용자 추가 항목: × 버튼으로 삭제 가능
    pub removable: bool,
}

/// 드롭다운 결과
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropdownEvent {
    None,
    Select(usize),
    Remove(usize),
    /// 마지막 "custom..." 행
    Custom,
}

/// 드롭다운 표시 설정
#[derive(Clone, Copy, Debug)]
pub struct DropdownSpec<'a> {
    pub id: &'a str,
    /// 현재 선택 라벨 (비면 placeholder 를 흐리게)
    pub selected: &'a str,
    pub placeholder: &'a str,
    pub tooltip: &'a str,
    pub width: f32,
    /// 마지막에 "custom..." 행 표시
    pub custom: bool,
}

/// 프리셋 드롭다운 (CAT sizeDropdown / renamer 드롭다운 대응)
pub fn dropdown(ui: &mut egui::Ui, spec: DropdownSpec<'_>, items: &[DropdownItem]) -> DropdownEvent {
    let DropdownSpec { id: id_salt, selected, placeholder, tooltip, width, custom } = spec;
    let mut event = DropdownEvent::None;
    let text = if selected.is_empty() { RichText::new(placeholder).weak() } else { RichText::new(selected) };
    let combo = craft_ui::ComboBox::from_id_salt(("cat-dropdown", id_salt)).selected_text(text).width(width).show_ui(ui, |ui| {
        for (i, item) in items.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.selectable_label(item.label == selected, &item.label).clicked() {
                    event = DropdownEvent::Select(i);
                }
                if item.removable && ui.small_button("×").on_hover_text("Remove").clicked() {
                    event = DropdownEvent::Remove(i);
                }
            });
        }
        if custom && ui.selectable_label(false, "custom...").clicked() {
            event = DropdownEvent::Custom;
        }
    });
    combo.response.on_hover_text(tooltip);
    event
}

/// 사이즈 프리셋 드롭다운 (기본 프리셋 + 커스텀 사이즈(× 삭제) + "custom..." 입력 모달)
pub fn size_dropdown(app: &mut PhotocraftApp, ui: &mut egui::Ui, cfg: &'static SizePresetConfig, width: f32) {
    let selected = size_presets::selected_size(&app.cat, cfg);
    let customs = size_presets::custom_sizes(&app.cat, cfg);
    let defaults = cfg.sizes.len();
    let mut items: Vec<DropdownItem> =
        cfg.sizes.iter().map(|v| DropdownItem { label: size_presets::format_size(cfg, Size::square(*v)), removable: false }).collect();
    items.extend(customs.iter().map(|s| DropdownItem { label: size_presets::format_size(cfg, *s), removable: true }));
    let label = size_presets::format_size(cfg, selected);
    let spec = DropdownSpec { id: cfg.storage_key, selected: &label, placeholder: "", tooltip: cfg.tooltip, width, custom: true };
    match dropdown(ui, spec, &items) {
        DropdownEvent::Select(i) => {
            let size = cfg.sizes.get(i).map(|v| Size::square(*v)).or_else(|| customs.get(i - defaults).copied());
            if let Some(size) = size {
                size_presets::select_size(&mut app.cat, cfg, size);
            }
        }
        DropdownEvent::Remove(i) => size_presets::remove_custom_size(&mut app.cat, cfg, i.saturating_sub(defaults)),
        DropdownEvent::Custom => {
            prompt_size(app, cfg.single, cfg.placeholders, move |app, w, h| size_presets::add_custom_size(&mut app.cat, cfg, Size { width: w, height: h }));
        }
        DropdownEvent::None => {}
    }
}

/// 진행 바 (진행 중일 때만)
pub fn progress_bar(app: &PhotocraftApp, ui: &mut egui::Ui) {
    if let Some(p) = &app.cat.progress {
        ui.label(&p.label);
        ui.add(egui::ProgressBar::new((p.percent / 100.0).clamp(0.0, 1.0)).desired_height(6.0));
    }
}

/// 모달 큐의 맨 앞 모달을 그린다. 한 패스에 한 번만 (두 탭이 같은 프레임에 그려도 중복 없음).
pub fn show_modals(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    if app.cat.modal_pass == Some(pass) {
        return;
    }
    app.cat.modal_pass = Some(pass);
    let Some(modal) = app.cat.modals.front_mut() else { return };
    // None: 아직 / Some(None): 취소 / Some(Some(답)): 확인
    let mut result: Option<Option<ModalAnswer>> = None;
    egui::Modal::new(Id::unique("cat-modal")).show(ctx, |ui| {
        ui.set_width(260.0);
        ui.strong(&modal.title);
        if !modal.message.is_empty() {
            ui.label(&modal.message);
        }
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let answer = match &mut modal.kind {
            ModalKind::Alert | ModalKind::Confirm => Some(ModalAnswer::Ok),
            ModalKind::Prompt { value } => {
                ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY)).request_focus();
                Some(ModalAnswer::Text(value.clone()))
            }
            ModalKind::Size { w, h, single, placeholders } => {
                ui.horizontal(|ui| {
                    let first = ui.add(egui::TextEdit::singleline(w).hint_text(placeholders[0].as_str()).desired_width(if *single { 230.0 } else { 110.0 }));
                    if !first.has_focus() && w.is_empty() && h.is_empty() {
                        first.request_focus();
                    }
                    if !*single {
                        ui.add(egui::TextEdit::singleline(h).hint_text(placeholders[1].as_str()).desired_width(110.0));
                    }
                });
                parse_size(w, h, *single).map(|(w, h)| ModalAnswer::Size(w, h))
            }
        };
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let alert = matches!(modal.kind, ModalKind::Alert);
            if !alert && ui.button("Cancel").clicked() {
                result = Some(None);
            }
            if ui.button("OK").clicked() || enter {
                // 잘못된 사이즈 값은 취소로 처리 (JP_Plugin 과 같음)
                result = Some(answer.clone());
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            result = Some(None);
        }
    });
    if let Some(answer) = result
        && let Some(modal) = app.cat.modals.pop_front()
        && let (Some(answer), Some(then)) = (answer, modal.then)
    {
        then(app, answer);
    }
}

/// 사이즈 입력 해석: H 미입력이면 정사각형, 0 이하·숫자 아님이면 None
fn parse_size(w: &str, h: &str, single: bool) -> Option<(u32, u32)> {
    let width: u32 = w.trim().parse().ok().filter(|v| *v > 0)?;
    let height = if single { width } else { h.trim().parse().ok().filter(|v: &u32| *v > 0).unwrap_or(width) };
    Some((width, height))
}
