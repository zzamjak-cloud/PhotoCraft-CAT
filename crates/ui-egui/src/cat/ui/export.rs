//! EXPORT 섹션 (CAT index.html #exportExpandableUI / JP_Plugin ExportSection.tsx 와 같은 배치)

use std::path::PathBuf;

use super::{DropdownEvent, DropdownItem, DropdownSpec, checkbox, dropdown, icon_button, section};
use crate::PhotocraftApp;
use crate::cat::features::export_selected_layers::LayerFileType;
use crate::cat::features::export_speed_save::{NEEDS_FOLDER, SpeedFormat};
use crate::cat::features::image_trace::TraceSettings;
use crate::cat::features::{
    clean_psd, export_market_screenshot, export_selected_and_link, export_selected_layers, export_speed_save, folder_preset, image_trace,
    outline_stroke, patch_maker, sorting_layer, split_to_layers, svg_to_shape_layer,
};
use crate::cat::lib::CatResult;
use crate::cat::lib::core::{alert, confirm, notify, pick_files, pick_folder, prompt, run_safe};

pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // Icon Search 창은 섹션이 접혀 있어도 그린다
    super::icon_search::show(app, ui.ctx());
    section(app, ui, "EXPORT", |app, ui| {
        folder_dropdown(app, ui);
        ui.horizontal(|ui| {
            if icon_button(ui, "link_1", "Export PNG & link").clicked() {
                with_export_folder(app, "Export PNG & link", |app, folder| {
                    let keep = app.cat.settings.keep_doc_size;
                    let n = export_selected_and_link::run(app, &folder, keep)?.len();
                    notify(app, format!("{n}개 내보내고 링크했습니다."));
                    Ok(())
                });
            }
            if icon_button(ui, "export_all_lays_import_psd", "Export layers as PSD").clicked() {
                export_layers(app, LayerFileType::Psd);
            }
            if icon_button(ui, "export_all_lays_import_png", "Export layers as PNG").clicked() {
                export_layers(app, LayerFileType::Png);
            }
            let mut keep = app.cat.settings.keep_doc_size;
            if checkbox(ui, &mut keep, "", "Keep doc size") {
                folder_preset::set_keep_doc_size(&mut app.cat, keep);
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if icon_button(ui, "psd_cleaner", "Clean PSD").clicked() {
                clean_psd_flow(app);
            }
            if icon_button(ui, "split_to_layers", "Split to layers").clicked() {
                run_safe(app, "Split to layers", split_to_layers::run);
            }
            if icon_button(ui, "sort", "Sort all layers").clicked() {
                run_safe(app, "Sort all layers", sorting_layer::run_all);
            }
            if icon_button(ui, "sort_pick", "Sort selected layers").clicked() {
                run_safe(app, "Sort selected layers", sorting_layer::run_selected);
            }
            if icon_button(ui, "patch", "Make 9-patch").clicked() {
                run_safe(app, "Make 9-patch", patch_maker::run);
            }
        });
        ui.horizontal(|ui| {
            if icon_button(ui, "outlinestroke", "Outline stroke").clicked() {
                run_safe(app, "Outline stroke", outline_stroke::run_outline);
            }
            if icon_button(ui, "expandstroke", "Expand path").clicked() {
                prompt(app, "Expand Path (px)", "4", |app, text| match text.trim().parse::<f64>() {
                    Ok(v) if v.is_finite() && v > 0.0 => {
                        run_safe(app, "Expand path", |app| outline_stroke::run_expand(app, v));
                    }
                    _ => alert(app, "올바른 숫자를 입력해주세요."),
                });
            }
            if icon_button(ui, "imagetracing", "Image trace").clicked() {
                image_trace_prompts(app);
            }
            if icon_button(ui, "export_market_screenshop", "Market screenshot").clicked() {
                with_doc_or_picked_folder(app, "Market screenshot", |app, folder| {
                    let n = export_market_screenshot::run(app, folder.as_deref())?;
                    notify(app, format!("{n}개 JPG 내보내기가 완료되었습니다."));
                    Ok(())
                });
            }
            if icon_button(ui, "import_searchicon", "Search icons").clicked() {
                super::icon_search::open(ui.ctx());
            }
        });
        ui.horizontal(|ui| {
            if icon_button(ui, "import_svg", "Import SVG as shape").clicked() {
                pick_files(app, &["svg"], false, |app, paths| {
                    if let Some(path) = paths.first() {
                        run_safe(app, "Import SVG as shape", |app| svg_to_shape_layer::run(app, path));
                    }
                });
            }
            for (icon, tip, format) in
                [("export_gif", "Save GIF", SpeedFormat::Gif), ("export_jpg", "Save JPG", SpeedFormat::Jpg), ("export_png", "Save PNG", SpeedFormat::Png)]
            {
                if icon_button(ui, icon, tip).clicked() {
                    speed_save(app, tip, format);
                }
            }
        });
    });
}

/// 폴더 프리셋 드롭다운 ("No Folder" / 프리셋 × 삭제 / custom... 폴더 선택)
fn folder_dropdown(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let presets = app.cat.settings.folder_presets.clone();
    let selected = app.cat.settings.selected_folder.clone();
    let items: Vec<DropdownItem> = presets.iter().map(|p| DropdownItem { label: p.name.clone(), removable: true }).collect();
    let label = selected.as_ref().map_or("No Folder".to_string(), |p| p.name.clone());
    let tooltip = selected.as_ref().map_or("Select export folder".to_string(), |p| p.path.clone());
    let width = (ui.available_width() - 24.0).max(80.0);
    let spec = DropdownSpec { id: "exportFolder", selected: &label, placeholder: "No Folder", tooltip: &tooltip, width, custom: true };
    match dropdown(ui, spec, &items) {
        DropdownEvent::Select(i) => {
            if let Some(p) = presets.get(i) {
                folder_preset::set_selected_folder(&mut app.cat, Some(p.clone()));
            }
        }
        DropdownEvent::Remove(i) => {
            if let Some(p) = presets.get(i) {
                folder_preset::remove_folder_preset(&mut app.cat, &p.id);
            }
        }
        DropdownEvent::Custom => pick_folder(app, |app, folder| {
            if let Err(e) = folder_preset::add_folder_preset_path(&mut app.cat, &folder) {
                alert(app, e);
            }
        }),
        DropdownEvent::None => {}
    }
}

fn export_layers(app: &mut PhotocraftApp, file_type: LayerFileType) {
    let label = if file_type == LayerFileType::Psd { "Export layers as PSD" } else { "Export layers as PNG" };
    with_export_folder(app, label, move |app, folder| {
        let keep = app.cat.settings.keep_doc_size;
        let report = export_selected_layers::run(app, file_type, &folder, keep)?;
        if !report.failed.is_empty() {
            let lines: Vec<String> = report.failed.iter().map(|(n, e)| format!("{n}: {e}")).collect();
            alert(app, format!("{}개 성공, {}개 실패\n{}", report.success, report.failed.len(), lines.join("\n")));
        }
        Ok(())
    });
}

/// 내보내기 폴더(프리셋 → 문서 폴더 → 폴더 선택 다이얼로그)를 정해 `f` 실행
fn with_export_folder(app: &mut PhotocraftApp, label: &'static str, f: impl FnOnce(&mut PhotocraftApp, PathBuf) -> CatResult<()> + 'static) {
    match run_safe(app, label, |app| folder_preset::resolve_export_folder(app)) {
        Some(Some(folder)) => {
            run_safe(app, label, |app| f(app, folder));
        }
        Some(None) => pick_folder(app, move |app, folder| {
            run_safe(app, label, |app| f(app, folder));
        }),
        None => {}
    }
}

/// 문서 폴더가 있으면 None 으로, 미저장 문서면 폴더 선택 후 Some(폴더) 로 `f` 실행
fn with_doc_or_picked_folder(app: &mut PhotocraftApp, label: &'static str, f: impl FnOnce(&mut PhotocraftApp, Option<PathBuf>) -> CatResult<()> + 'static) {
    match run_safe(app, label, |app| folder_preset::doc_folder_and_name(app)) {
        Some(Some(_)) => {
            run_safe(app, label, |app| f(app, None));
        }
        Some(None) => pick_folder(app, move |app, folder| {
            run_safe(app, label, |app| f(app, Some(folder)));
        }),
        None => {}
    }
}

/// Clean PSD: 미저장 변경이 있으면 확인 후 진행 (변경 내용은 정리본에 포함되어 원본 경로에 저장된다)
fn clean_psd_flow(app: &mut PhotocraftApp) {
    let done = |app: &mut PhotocraftApp, path: PathBuf| notify(app, format!("Clean PSD 완료: {}", path.display()));
    let background = std::mem::replace(&mut app.background_jobs, false);
    let first = clean_psd::run(app, false);
    app.background_jobs = background;
    match first {
        Ok(path) => done(app, path),
        Err(e) if e == clean_psd::NEEDS_CONFIRM => confirm(
            app,
            "저장되지 않은 변경 사항을 포함해 원본 경로에 정리된 PSD를 저장하고 문서를 닫습니다. 계속할까요?",
            move |app| {
                if let Some(path) = run_safe(app, "Clean PSD", |app| clean_psd::run(app, true)) {
                    done(app, path);
                }
            },
        ),
        Err(e) => alert(app, format!("Clean PSD 실패:\n{e}")),
    }
}

/// Save GIF/JPG/PNG: 미저장 문서면 폴더를 묻는다
fn speed_save(app: &mut PhotocraftApp, label: &'static str, format: SpeedFormat) {
    let background = std::mem::replace(&mut app.background_jobs, false);
    let first = export_speed_save::run(app, format, None);
    app.background_jobs = background;
    match first {
        Ok(path) => crate::cat::lib::core::notify(app, format!("저장 완료: {}", path.display())),
        Err(e) if e == NEEDS_FOLDER => pick_folder(app, move |app, folder| {
            if let Some(path) = run_safe(app, label, |app| export_speed_save::run(app, format, Some(&folder))) {
                crate::cat::lib::core::notify(app, format!("저장 완료: {}", path.display()));
            }
        }),
        Err(e) => alert(app, format!("{label} 실패:\n{e}")),
    }
}

/// Image trace: 색상 허용치 → 경로 부드러움 입력 후 실행
fn image_trace_prompts(app: &mut PhotocraftApp) {
    let d = TraceSettings::default();
    prompt(app, "Image Trace — Color Tolerance (0-100)", d.color_tolerance.to_string(), move |app, c| {
        prompt(app, "Image Trace — Path Smoothness (0-5)", d.path_tolerance.to_string(), move |app, p| {
            let settings = TraceSettings {
                color_tolerance: c.trim().parse().unwrap_or(d.color_tolerance).clamp(0.0, 100.0),
                path_tolerance: p.trim().parse().unwrap_or(d.path_tolerance).clamp(0.0, 5.0),
            };
            run_safe(app, "Image trace", |app| image_trace::run(app, settings));
        });
    });
}
