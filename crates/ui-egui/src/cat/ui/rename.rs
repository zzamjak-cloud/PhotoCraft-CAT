//! LAYERS RENAME 섹션 (CAT index.html Layers Renamer / JP_Plugin RenameSection.tsx 와 같은 배치)

use super::{DropdownEvent, DropdownItem, DropdownSpec, dropdown, icon_button, section};
use crate::PhotocraftApp;
use crate::cat::features::renamer_layers::{self, DIGIT_VALUES, RenameType, TextPresetKind};
use crate::cat::lib::core::{alert, prompt, run_safe};

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    section(app, ui, "LAYERS RENAME", |app, ui| {
        ui.horizontal(|ui| {
            text_preset_dropdown(app, ui, TextPresetKind::Enter);
            text_preset_dropdown(app, ui, TextPresetKind::Replace);
        });
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 24.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let digit = renamer_layers::digit_count(&app.cat);
            let items: Vec<DropdownItem> = DIGIT_VALUES.iter().map(|v| DropdownItem { label: v.to_string(), removable: false }).collect();
            let label = digit.to_string();
            let spec = DropdownSpec { id: "renamerDigit", selected: &label, placeholder: "", tooltip: "digit", width: 40.0, custom: false };
            if let DropdownEvent::Select(i) = dropdown(ui, spec, &items)
                && let Some(v) = DIGIT_VALUES.get(i)
            {
                renamer_layers::set_digit_count(&mut app.cat, *v);
            }
            ui.weak("digit");
        });
        ui.separator();
        ui.horizontal(|ui| {
            for (icon, tip, kind) in [
                ("rename_prefix", "Add prefix", RenameType::Prefix),
                ("rename_number", "Add numbers", RenameType::Number),
                ("rename_reversenumber", "Add reverse numbers", RenameType::ReverseNumber),
                ("rename_sufix", "Add suffix", RenameType::Suffix),
            ] {
                if icon_button(ui, icon, tip).clicked() {
                    run_safe(app, tip, |app| renamer_layers::run_with_settings(app, kind));
                }
            }
        });
        ui.horizontal(|ui| {
            for (icon, tip, kind) in [
                ("renametext", "Rename layers", RenameType::Rename),
                ("replacetext", "Replace text", RenameType::Replace),
                ("remove_text", "Remove text", RenameType::Remove),
            ] {
                if icon_button(ui, icon, tip).clicked() {
                    run_safe(app, tip, |app| renamer_layers::run_with_settings(app, kind));
                }
            }
        });
    });
}

/// Enter / Replace 텍스트 프리셋 드롭다운 (선택이 없으면 placeholder; custom... 은 "Add Preset" 입력)
fn text_preset_dropdown(app: &mut PhotocraftApp, ui: &mut egui::Ui, kind: TextPresetKind) {
    let presets = renamer_layers::text_presets(&app.cat, kind);
    let selected = renamer_layers::selected_text(&app.cat, kind);
    let items: Vec<DropdownItem> = presets.iter().map(|t| DropdownItem { label: t.clone(), removable: true }).collect();
    let id = format!("renamer-{}", kind.placeholder());
    let spec = DropdownSpec { id: &id, selected: &selected, placeholder: kind.placeholder(), tooltip: kind.tooltip(), width: 96.0, custom: true };
    match dropdown(ui, spec, &items) {
        DropdownEvent::Select(i) => {
            if let Some(t) = presets.get(i) {
                renamer_layers::set_selected_text(&mut app.cat, kind, t);
            }
        }
        DropdownEvent::Remove(i) => {
            if let Some(t) = presets.get(i) {
                renamer_layers::remove_text_preset(&mut app.cat, kind, t);
            }
        }
        DropdownEvent::Custom => prompt(app, "Add Preset", "", move |app, text| {
            if text.trim().is_empty() {
                return;
            }
            match renamer_layers::add_text_preset(&mut app.cat, kind, &text) {
                Ok(t) => renamer_layers::set_selected_text(&mut app.cat, kind, &t),
                Err(e) => alert(app, e),
            }
        }),
        DropdownEvent::None => {}
    }
}
