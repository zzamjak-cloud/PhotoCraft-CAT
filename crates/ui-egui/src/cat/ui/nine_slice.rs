//! 9-Slice Images 탭 (CAT nineSlicePanel.js / JP_Plugin NineSlicePanel.tsx 대응).
//! 프로젝트 드롭다운 + "+"(이미지 추가) / "Del"(이미지 삭제) + 이미지 그리드. 카드 클릭 = 선택 영역에 삽입.
//! 썸네일·슬라이스 편집기는 Phase 2.

use super::{DropdownEvent, DropdownItem, DropdownSpec, dropdown, text_button};
use crate::PhotocraftApp;
use crate::cat::features::nine_slice_panel::{self, ALLOWED_EXTENSIONS, DEFAULT_PROJECT};
use crate::cat::lib::core::{alert, pick_files, prompt, run_safe};

/// 선택된 카드 ID 를 보관하는 egui 메모리 키
fn selected_id() -> egui::Id {
    egui::Id::unique("cat-nine-slice-selected")
}

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let projects = nine_slice_panel::projects(app);
    let current = nine_slice_panel::current_project(app);
    let items: Vec<DropdownItem> = projects.iter().map(|p| DropdownItem { label: p.clone(), removable: p != DEFAULT_PROJECT }).collect();
    let sel_id = selected_id();
    let mut selected: Option<String> = ui.ctx().data(|d| d.get_temp(sel_id));
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 72.0).max(80.0);
        let spec = DropdownSpec { id: "nsProject", selected: &current, placeholder: DEFAULT_PROJECT, tooltip: "Project category", width, custom: true };
        match dropdown(ui, spec, &items) {
            DropdownEvent::Select(i) => {
                if let Some(p) = projects.get(i) {
                    run_safe(app, "Project", |app| nine_slice_panel::set_current_project(app, p));
                }
            }
            DropdownEvent::Remove(i) => {
                if let Some(p) = projects.get(i) {
                    run_safe(app, "Project", |app| nine_slice_panel::remove_project(app, p));
                }
            }
            DropdownEvent::Custom => prompt(app, "새 프로젝트 이름", "", |app, name| {
                if !name.trim().is_empty() {
                    run_safe(app, "Project", |app| nine_slice_panel::add_project(app, name.trim()));
                }
            }),
            DropdownEvent::None => {}
        }
        if text_button(ui, "+", "Add images").clicked() {
            let project = current.clone();
            pick_files(app, &ALLOWED_EXTENSIONS, true, move |app, paths| {
                if let Some((_, skipped)) = run_safe(app, "Add images", |app| nine_slice_panel::register_files(app, &paths, &project))
                    && skipped > 0
                {
                    alert(app, format!("{skipped}개 파일을 건너뛰었습니다. (지원 형식: {})", ALLOWED_EXTENSIONS.join(", ")));
                }
            });
        }
        if text_button(ui, "Del", "Remove image").clicked() {
            match selected.clone() {
                Some(id) => {
                    run_safe(app, "Remove image", |app| nine_slice_panel::delete_image(app, &id));
                    selected = None;
                }
                None => alert(app, "삭제할 이미지를 선택해주세요."),
            }
        }
    });
    ui.separator();
    let images: Vec<_> = nine_slice_panel::load_library(app).images.into_iter().filter(|m| m.project == current).collect();
    if images.is_empty() {
        ui.weak("이미지가 없습니다. + 로 추가하세요.");
    }
    ui.horizontal_wrapped(|ui| {
        for m in &images {
            let on = selected.as_deref() == Some(m.id.as_str());
            let card = ui.add_sized([72.0, 72.0], egui::Button::new(egui::RichText::new(&m.name).small()).selected(on));
            if card.clicked() {
                selected = Some(m.id.clone());
                let id = m.id.clone();
                run_safe(app, "9-Slice", |app| nine_slice_panel::run(app, &id));
            }
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(sel_id, selected));
}
