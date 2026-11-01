//! GUIDE 섹션 (CAT index.html #guideExpandableUI / JP_Plugin GuideSection.tsx 와 같은 배치)

use super::{checkbox, icon_button, section, size_dropdown};
use crate::PhotocraftApp;
use crate::cat::features::add_guide::{self, EdgeDirection};
use crate::cat::features::patch_maker;
use crate::cat::features::size_presets::{self, GUIDE_GRID, GUIDE_OFFSET};
use crate::cat::lib::core::{notify, run_safe};

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    section(app, ui, "GUIDE", |app, ui| {
        ui.horizontal(|ui| {
            size_dropdown(app, ui, &GUIDE_GRID, 64.0);
            if icon_button(ui, "addnum", "Guides with numbers").clicked() {
                grid(app, true);
            }
            if icon_button(ui, "guidemake", "Make guides").clicked() {
                grid(app, false);
            }
        });
        ui.horizontal(|ui| {
            if icon_button(ui, "clearguide", "Clear guides").clicked() {
                run_safe(app, "Clear guides", add_guide::clear_guides);
            }
            if icon_button(ui, "copy", "Copy guides").clicked()
                && let Some(n) = run_safe(app, "Copy guides", add_guide::copy_guides)
            {
                notify(app, format!("가이드 {n}개를 복사했습니다."));
            }
            if icon_button(ui, "paste", "Paste guides").clicked()
                && let Some(n) = run_safe(app, "Paste guides", add_guide::paste_guides)
            {
                notify(app, format!("가이드 {n}개를 붙여넣었습니다."));
            }
            if icon_button(ui, "patch", "Make 9-patch").clicked() {
                run_safe(app, "Make 9-patch", patch_maker::run);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            size_dropdown(app, ui, &GUIDE_OFFSET, 64.0);
            let mut bound = app.cat.guide_layer_bound;
            if checkbox(ui, &mut bound, "Only selected layer", "Only selected layer") {
                app.cat.guide_layer_bound = bound;
            }
        });
        ui.horizontal(|ui| {
            for (icon, tip, dir) in [
                ("guide_top", "Guide from top", EdgeDirection::Top),
                ("guide_bottom", "Guide from bottom", EdgeDirection::Bottom),
                ("guide_left", "Guide from left", EdgeDirection::Left),
                ("guide_right", "Guide from right", EdgeDirection::Right),
            ] {
                if icon_button(ui, icon, tip).clicked() {
                    let offset = size_presets::selected_size(&app.cat, &GUIDE_OFFSET).width as f64;
                    let bound = app.cat.guide_layer_bound;
                    run_safe(app, tip, |app| add_guide::add_guide_from_edge(app, dir, offset, bound));
                }
            }
        });
    });
}

/// 행x열 그리드 가이드 (width = 행, height = 열)
fn grid(app: &mut PhotocraftApp, with_numbers: bool) {
    let s = size_presets::selected_size(&app.cat, &GUIDE_GRID);
    let label = if with_numbers { "Guides with numbers" } else { "Make guides" };
    run_safe(app, label, |app| add_guide::add_all_guides(app, with_numbers, s.width, s.height));
}
