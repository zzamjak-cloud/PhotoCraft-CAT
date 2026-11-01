//! EXTRA 섹션: CAT 에서 UI 에 연결되지 않았던 기능 (Animation match layers 는 이식하지 않음)

use super::{icon_button, section};
use crate::PhotocraftApp;
use crate::cat::features::{app_icon_maker, clear_empty_layers, clear_hidden_effects};
use crate::cat::lib::core::{alert, notify, pick_folder, run_safe};

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    section(app, ui, "EXTRA", |app, ui| {
        ui.horizontal(|ui| {
            if icon_button(ui, "clear_empty_layers", "Clear empty layers").clicked()
                && let Some(n) = run_safe(app, "Clear empty layers", clear_empty_layers::run)
            {
                if n == 0 {
                    alert(app, "삭제할 빈 레이어가 없습니다.");
                } else {
                    notify(app, format!("빈 레이어 {n}개를 삭제했습니다."));
                }
            }
            if icon_button(ui, "clear_hiddenfx", "Clear hidden effects").clicked()
                && let Some(n) = run_safe(app, "Clear hidden effects", clear_hidden_effects::run)
            {
                notify(app, format!("숨겨진 효과 {n}개를 삭제했습니다."));
            }
            if icon_button(ui, "appicon", "App icon maker").clicked() {
                pick_folder(app, |app, folder| {
                    if let Some(n) = run_safe(app, "App icon maker", |app| app_icon_maker::run(app, &folder)) {
                        notify(app, format!("앱 아이콘 {n}개 생성이 완료되었습니다!"));
                    }
                });
            }
        });
    });
}
