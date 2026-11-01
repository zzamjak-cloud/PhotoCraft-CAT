//! IMPORT 섹션 (CAT index.html IMPORT / JP_Plugin ImportSection.tsx 와 같은 배치)

use super::{checkbox, icon_button, section, size_dropdown};
use crate::PhotocraftApp;
use crate::cat::features::import_files::ImportKind;
use crate::cat::features::size_presets::{self, ARRAY_COLUMNS, ARRAY_GRID_SIZE, IMPORT_DOC_SIZE};
use crate::cat::features::{apply_grid_layout, auto_paste_png, import_files};
use crate::cat::lib::core::{alert, pick_folder, run_safe};

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    section(app, ui, "IMPORT", |app, ui| {
        ui.horizontal(|ui| {
            size_dropdown(app, ui, &IMPORT_DOC_SIZE, 64.0);
            if icon_button(ui, "export_all_lays_import_psd", "Import PSD files").clicked() {
                import(app, ImportKind::Psd, "Import PSD files");
            }
            if icon_button(ui, "export_all_lays_import_png", "Import PNG files").clicked() {
                import(app, ImportKind::Png, "Import PNG files");
            }
            let mut on = auto_paste_png::is_enabled(&app.cat);
            if checkbox(ui, &mut on, "", "Drop PNG as pixel layer") {
                auto_paste_png::set_enabled(&mut app.cat, on);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            size_dropdown(app, ui, &ARRAY_GRID_SIZE, 64.0);
            size_dropdown(app, ui, &ARRAY_COLUMNS, 48.0);
            if icon_button(ui, "array", "Arrange in grid").clicked() {
                let cell = size_presets::selected_size(&app.cat, &ARRAY_GRID_SIZE);
                let columns = size_presets::selected_size(&app.cat, &ARRAY_COLUMNS).width.max(1);
                run_safe(app, "Arrange in grid", |app| apply_grid_layout::run(app, cell, columns));
            }
        });
    });
}

/// 폴더를 고른 뒤 그 폴더의 파일을 새 문서로 가져오기
fn import(app: &mut PhotocraftApp, kind: ImportKind, label: &'static str) {
    let size = size_presets::selected_size(&app.cat, &IMPORT_DOC_SIZE);
    pick_folder(app, move |app, folder| {
        if let Some(None) = run_safe(app, label, |app| import_files::run(app, kind, &folder, size)) {
            alert(app, format!("선택한 폴더에 {} 파일이 없습니다.", kind.extension().to_uppercase()));
        }
    });
}
