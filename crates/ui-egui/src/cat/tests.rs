//! CAT 인프로세스 테스트: 실제 문서를 만들어 기능을 실행하고 결과(레이어·가이드·파일·Undo)를 검증한다.
//! 기대값은 JP_Plugin `scripts/tests/` (renamerLayers.ts, guide.ts, exportSpeedSave.ts) 와 같다.

use std::path::Path;

use photocraft_doc::LayerId;
use serde_json::json;

use super::features::add_guide::{self, EdgeDirection};
use super::features::export_speed_save::{self, NEEDS_FOLDER, SpeedFormat};
use super::features::folder_preset;
use super::features::renamer_layers::{self, RenameType, TextPresetKind};
use super::lib::core::{exec, with_history};
use super::lib::doc::{self, DocBackground};
use super::lib::export::{self, SaveOptions};
use super::lib::guide::{GuideAxis, get_guides};
use super::lib::{layer, pixels};
use super::{CatState, ui};
use crate::PhotocraftApp;

// ---- 테스트 헬퍼 (Phase 2 테스트에서도 재사용) ----

/// CAT 설정을 임시 폴더에 두는 앱 (디스크의 사용자 설정은 건드리지 않는다)
pub(crate) fn test_app() -> (PhotocraftApp, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.cat = CatState::load(Some(dir.path().join("cat")));
    (app, dir)
}

/// 흰 배경 새 문서
pub(crate) fn new_doc(app: &mut PhotocraftApp, name: &str, w: u32, h: u32) -> photocraft_doc::DocId {
    doc::create_doc(app, name, w, h, DocBackground::White).unwrap()
}

/// 새 레이어에 사각형을 칠한다 → 레이어 ID
pub(crate) fn rect_layer(app: &mut PhotocraftApp, name: &str, x: i32, y: i32, w: i32, h: i32, color: &str) -> LayerId {
    let id = layer::new_layer(app, name).unwrap();
    exec(app, "select.rect", json!({"x": x, "y": y, "width": w, "height": h})).unwrap();
    exec(app, "edit.fill", json!({"contents": "color", "color": color})).unwrap();
    exec(app, "select.deselect", json!({})).unwrap();
    id
}

fn undo(app: &mut PhotocraftApp) {
    exec(app, "edit.undo", json!({})).unwrap();
}

fn past_len(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.past_len()
}

fn names_top_down(app: &PhotocraftApp) -> String {
    layer::selected_layers(app).unwrap().iter().map(|l| l.name.clone()).collect::<Vec<_>>().join(",")
}

/// 가이드를 "h50,v100" 처럼 정렬한 키로
fn guide_key(app: &PhotocraftApp) -> String {
    let mut keys: Vec<String> =
        get_guides(app).unwrap().iter().map(|g| format!("{}{}", if g.axis == GuideAxis::Horizontal { "h" } else { "v" }, g.position)).collect();
    keys.sort();
    keys.join(",")
}

// ---- lib ----

#[test]
fn with_history_groups_commands_and_direct_edits_into_one_undo_step() {
    let (mut app, _dir) = test_app();
    new_doc(&mut app, "h", 40, 40);
    let a = layer::new_layer(&mut app, "A").unwrap();
    let before = past_len(&app);
    with_history(&mut app, |app| {
        layer::rename_layer(app, a, "B")?;
        layer::rename_layer(app, a, "C")?;
        pixels::put_pixels_as_layer(app, &pixels::RgbaImage::new(4, 4), "px", 0, 0)?;
        Ok(())
    })
    .unwrap();
    assert_eq!(past_len(&app), before + 1, "한 그룹 = 한 단계");
    assert_eq!(layer::layer_by_id(&app, a).unwrap().name, "C");
    undo(&mut app);
    assert_eq!(layer::layer_by_id(&app, a).unwrap().name, "A");
    assert!(layer::all_layers(&app).unwrap().iter().all(|l| l.name != "px"));
    // 그룹이 끝나면 다음 명령은 별도 단계
    layer::rename_layer(&mut app, a, "D").unwrap();
    layer::rename_layer(&mut app, a, "E").unwrap();
    assert_eq!(past_len(&app), before + 2);
}

#[test]
fn closing_refuses_a_users_unsaved_document_but_closes_temp_documents_by_id() {
    let (mut app, _dir) = test_app();
    let user = new_doc(&mut app, "user", 20, 20);
    rect_layer(&mut app, "dirty", 0, 0, 5, 5, "#ff0000");
    let err = doc::close_doc_without_saving(&mut app, user).unwrap_err();
    assert!(err.contains("저장되지 않은"), "{err}");
    assert!(doc::doc_by_id(&app, user).is_some());
    // 선택 레이어를 새 문서로 → 임시 문서. 앞의 문서를 하나 더 열어 인덱스가 밀려도 ID 로 닫는다.
    let temp = doc::create_doc_copy_layers(&mut app, "temp", None).unwrap();
    assert!(doc::is_temp_doc(&app, temp));
    assert_eq!(doc::active_doc(&app).unwrap().id, temp);
    exec(&mut app, "layer.new.layer", json!({})).unwrap();
    doc::activate_doc(&mut app, user).unwrap();
    doc::close_doc_without_saving(&mut app, temp).unwrap();
    assert!(doc::doc_by_id(&app, temp).is_none());
    assert_eq!(doc::active_doc(&app).unwrap().id, user, "닫기 전 활성 문서 유지");
    assert_eq!(doc::list_documents(&app).len(), 1);
}

#[test]
fn layer_tree_reports_kind_bounds_locks_and_selection() {
    let (mut app, _dir) = test_app();
    new_doc(&mut app, "t", 100, 50);
    let a = rect_layer(&mut app, "A", 10, 5, 20, 10, "#00ff00");
    exec(&mut app, "layer.setProps", json!({"layer": a.0, "locked": true})).ok();
    let ls = layer::all_layers(&app).unwrap();
    let row = ls.iter().find(|l| l.id == a).unwrap();
    assert_eq!(row.kind, layer::LayerKind::Pixel);
    assert_eq!(row.bounds, Some(layer::Bounds { x: 10, y: 5, w: 20, h: 10 }));
    assert!(row.active && row.selected);
    assert_eq!(ls.first().unwrap().id, a, "위→아래 순서");
    assert_eq!(ls.len(), 2);
}

#[test]
fn save_keeps_the_original_file_when_encoding_fails_and_open_is_synchronous() {
    let (mut app, dir) = test_app();
    new_doc(&mut app, "s", 30, 20);
    rect_layer(&mut app, "A", 0, 0, 10, 10, "#0000ff");
    let out = dir.path().join("out");
    // 인코딩할 수 없는 확장자: 기존 파일이 그대로 남아야 한다
    let bad = out.join("keep.unknownext");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(&bad, b"original").unwrap();
    assert!(export::save_doc_as(&mut app, &bad, &SaveOptions::default()).is_err());
    assert_eq!(std::fs::read(&bad).unwrap(), b"original");
    // PNG 저장 → 동기 열기
    let png = export::save_for_web_png(&mut app, &out, "s").unwrap();
    let docs = doc::list_documents(&app).len();
    let opened = export::open_file(&mut app, &png).unwrap();
    assert_eq!(doc::list_documents(&app).len(), docs + 1);
    let d = doc::active_doc(&app).unwrap();
    assert_eq!((d.id, d.width, d.height), (opened, 30, 20));
    assert_eq!(d.path.as_deref(), Some(png.as_path()));
    // 열린 문서의 원본 파일은 내보내기로 덮어쓰지 않는다
    let err = export::save_for_web_png(&mut app, &out, "s").unwrap_err();
    assert!(err.contains("열려 있는 문서의 원본"), "{err}");
}

#[test]
fn settings_persist_in_the_cat_data_folder() {
    let (mut app, dir) = test_app();
    let folder = dir.path().join("exports");
    folder_preset::add_folder_preset_path(&mut app.cat, &folder).unwrap();
    folder_preset::set_keep_doc_size(&mut app.cat, true);
    let err = folder_preset::add_folder_preset_path(&mut app.cat, &folder).unwrap_err();
    assert_eq!(err, "이미 존재하는 폴더 경로입니다.");
    let back = CatState::load(Some(dir.path().join("cat")));
    assert_eq!(back.settings.folder_presets.len(), 1);
    assert_eq!(back.settings.selected_folder.as_ref().map(|p| p.name.as_str()), Some("exports"));
    assert!(back.settings.keep_doc_size);
}

#[test]
fn every_cat_icon_decodes() {
    let ctx = egui::Context::default();
    let names: Vec<_> = ui::icons::names().collect();
    assert!(names.len() >= 37);
    for name in names {
        assert!(ui::icons::texture(&ctx, name).is_some(), "{name}");
    }
    assert!(ui::icons::texture(&ctx, "no-such-icon").is_none());
}

// ---- Layers rename (renamerLayers.ts) ----

#[test]
fn rename_presets_trim_dedupe_cap_and_clear_selection() {
    let (mut app, _dir) = test_app();
    let cat = &mut app.cat;
    assert_eq!(renamer_layers::add_text_preset(cat, TextPresetKind::Enter, "  pre_ ").unwrap(), "pre_");
    renamer_layers::add_text_preset(cat, TextPresetKind::Enter, "pre_").unwrap();
    assert_eq!(renamer_layers::text_presets(cat, TextPresetKind::Enter).len(), 1, "중복 무시");
    for i in 0..9 {
        renamer_layers::add_text_preset(cat, TextPresetKind::Enter, &format!("p{i}")).unwrap();
    }
    assert_eq!(renamer_layers::add_text_preset(cat, TextPresetKind::Enter, "overflow").unwrap_err(), "최대 10개의 프리셋만 추가할 수 있습니다.");
    renamer_layers::set_selected_text(cat, TextPresetKind::Enter, "p0");
    renamer_layers::remove_text_preset(cat, TextPresetKind::Enter, "p0");
    assert_eq!(renamer_layers::selected_text(cat, TextPresetKind::Enter), "", "선택 프리셋 삭제 시 선택 해제");
}

#[test]
fn rename_all_seven_types_restore_selection_and_undo_in_one_step() {
    let (mut app, _dir) = test_app();
    new_doc(&mut app, "ren", 50, 50);
    let ids = [layer::new_layer(&mut app, "L1").unwrap(), layer::new_layer(&mut app, "L2").unwrap(), layer::new_layer(&mut app, "L3").unwrap()];
    let run = |app: &mut PhotocraftApp, kind: RenameType, text: &str, replace: &str, digits: u32| {
        layer::select_layers(app, &ids).unwrap();
        renamer_layers::run(app, kind, text, replace, digits).unwrap();
        names_top_down(app)
    };
    assert_eq!(run(&mut app, RenameType::Prefix, "pre_", "", 1), "pre_L3,pre_L2,pre_L1");
    assert_eq!(run(&mut app, RenameType::Replace, "pre_", "X", 1), "XL3,XL2,XL1", "첫 일치만");
    assert_eq!(run(&mut app, RenameType::Remove, "X", "", 1), "L3,L2,L1");
    assert_eq!(run(&mut app, RenameType::Suffix, "_s", "", 1), "L3_s,L2_s,L1_s");
    assert_eq!(run(&mut app, RenameType::Number, "", "", 3), "L3_s003,L2_s002,L1_s001", "위 레이어가 가장 큰 번호");
    assert_eq!(run(&mut app, RenameType::ReverseNumber, "", "", 1), "L3_s0031,L2_s0022,L1_s0013");
    let before = past_len(&app);
    assert_eq!(run(&mut app, RenameType::Rename, "icon", "", 1), "icon,icon,icon");
    assert_eq!(past_len(&app), before + 1, "세 레이어 이름 변경 = Undo 한 단계");
    assert_eq!(layer::selected_layers(&app).unwrap().len(), 3, "선택 복원");
    undo(&mut app);
    assert_eq!(names_top_down(&app), "L3_s0031,L2_s0022,L1_s0013");
    assert_eq!(renamer_layers::run(&mut app, RenameType::Prefix, "", "", 1).unwrap_err(), "변경할 텍스트를 입력해주세요.");
    assert_eq!(renamer_layers::run(&mut app, RenameType::Replace, "a", "", 1).unwrap_err(), "대체할 텍스트를 입력해주세요.");
}

#[test]
fn rename_uses_the_saved_presets_and_digit_count() {
    let (mut app, _dir) = test_app();
    new_doc(&mut app, "ren", 20, 20);
    let a = layer::new_layer(&mut app, "A").unwrap();
    renamer_layers::add_text_preset(&mut app.cat, TextPresetKind::Enter, "x_").unwrap();
    renamer_layers::set_selected_text(&mut app.cat, TextPresetKind::Enter, "x_");
    renamer_layers::set_digit_count(&mut app.cat, 2);
    renamer_layers::run_with_settings(&mut app, RenameType::Prefix).unwrap();
    renamer_layers::run_with_settings(&mut app, RenameType::Number).unwrap();
    assert_eq!(layer::layer_by_id(&app, a).unwrap().name, "x_A01");
    assert_eq!(layer::selected_layers(&app).unwrap().len(), 1);
}

// ---- GUIDE (guide.ts) ----

#[test]
fn grid_guides_numbers_and_clear() {
    let (mut app, _dir) = test_app();
    new_doc(&mut app, "g1", 200, 100);
    add_guide::add_all_guides(&mut app, false, 2, 4).unwrap();
    assert_eq!(guide_key(&app), "h50,v100,v150,v50");
    assert!(layer::all_layers(&app).unwrap().iter().all(|l| l.name != add_guide::NUM_LAYER));

    exec(&mut app, "view.clearGuides", json!({})).unwrap();
    let before = past_len(&app);
    add_guide::add_all_guides(&mut app, true, 2, 2).unwrap();
    assert_eq!(past_len(&app), before + 1, "가이드 + 번호 레이어 = Undo 한 단계");
    assert_eq!(guide_key(&app), "h50,v100");
    let num = layer::top_level_layers(&app).unwrap().into_iter().find(|l| l.name == add_guide::NUM_LAYER).expect("Num 레이어");
    let px = pixels::get_layer_pixels(&app, num.id).unwrap();
    assert!(pixels::alpha_bounds(&px, 0).is_some());
    // 셀 0(좌상단) 중앙 (50,25) 근처에 숫자 0
    let found = (10..40).flat_map(|y| (35..65).map(move |x| (x, y))).filter(|(x, y)| px.pixel(*x, *y).is_some_and(|p| p[3] > 0)).count();
    assert!(found > 20, "digit 0 drawn in cell 0: {found}");

    undo(&mut app);
    assert_eq!(guide_key(&app), "", "Undo 한 번에 가이드와 번호 레이어 모두 취소");
    exec(&mut app, "edit.redo", json!({})).unwrap();
    add_guide::clear_guides(&mut app).unwrap();
    assert!(layer::all_layers(&app).unwrap().iter().all(|l| l.name != add_guide::NUM_LAYER));
    assert!(get_guides(&app).unwrap().is_empty());
}

#[test]
fn edge_guides_from_document_and_selected_layer() {
    let (mut app, _dir) = test_app();
    new_doc(&mut app, "g", 200, 100);
    for d in [EdgeDirection::Top, EdgeDirection::Bottom, EdgeDirection::Left, EdgeDirection::Right] {
        add_guide::add_guide_from_edge(&mut app, d, 30.0, false).unwrap();
    }
    assert_eq!(guide_key(&app), "h30,h70,v170,v30");
    exec(&mut app, "view.clearGuides", json!({})).unwrap();
    rect_layer(&mut app, "box", 20, 10, 60, 40, "#ff0000");
    for d in [EdgeDirection::Top, EdgeDirection::Bottom, EdgeDirection::Left, EdgeDirection::Right] {
        add_guide::add_guide_from_edge(&mut app, d, 5.0, true).unwrap();
    }
    assert_eq!(guide_key(&app), "h15,h45,v25,v75");
}

#[test]
fn copy_and_paste_guides_skip_duplicates() {
    let (mut app, _dir) = test_app();
    new_doc(&mut app, "g", 200, 100);
    for (p, a) in [(15.0, GuideAxis::Horizontal), (45.0, GuideAxis::Horizontal), (25.0, GuideAxis::Vertical), (75.0, GuideAxis::Vertical)] {
        super::lib::guide::make_guide(&mut app, p, a).unwrap();
    }
    assert_eq!(add_guide::copy_guides(&mut app).unwrap(), 4);
    new_doc(&mut app, "g2", 200, 100);
    super::lib::guide::make_guide(&mut app, 15.0, GuideAxis::Horizontal).unwrap();
    let before = past_len(&app);
    assert_eq!(add_guide::paste_guides(&mut app).unwrap(), 3, "중복 1개 건너뜀");
    assert_eq!(past_len(&app), before + 1);
    assert_eq!(guide_key(&app), "h15,h45,v25,v75");
    assert_eq!(add_guide::paste_guides(&mut app).unwrap(), 0);
}

// ---- Save GIF / JPG / PNG (exportSpeedSave.ts) ----

fn png_size(path: &Path) -> (u32, u32) {
    let img = photocraft_codecs::decode(&std::fs::read(path).unwrap()).unwrap();
    (img.width(), img.height())
}

#[test]
fn speed_save_writes_next_to_the_psd_with_its_name() {
    let (mut app, dir) = test_app();
    new_doc(&mut app, "e3doc", 120, 80);
    rect_layer(&mut app, "A", 10, 10, 50, 40, "#ff0000");
    let psd = dir.path().join("e3").join("speed.psd");
    export::save_doc_as(&mut app, &psd, &SaveOptions { adopt_path: true, ..Default::default() }).unwrap();
    assert!(!doc::active_doc(&app).unwrap().dirty, "Save As 후 저장 상태");
    for f in [SpeedFormat::Png, SpeedFormat::Jpg, SpeedFormat::Gif] {
        export_speed_save::run(&mut app, f, None).unwrap();
    }
    let e3 = dir.path().join("e3");
    assert_eq!(png_size(&e3.join("speed.png")), (120, 80));
    assert_eq!(&std::fs::read(e3.join("speed.jpg")).unwrap()[..2], &[0xff, 0xd8]);
    assert_eq!(&std::fs::read(e3.join("speed.gif")).unwrap()[..3], b"GIF");
    // 덮어쓰기
    export_speed_save::run(&mut app, SpeedFormat::Png, None).unwrap();
    assert_eq!(png_size(&e3.join("speed.png")), (120, 80));
}

#[test]
fn speed_save_of_an_unsaved_document_needs_a_folder() {
    let (mut app, dir) = test_app();
    new_doc(&mut app, "unsaved", 64, 32);
    assert_eq!(export_speed_save::run(&mut app, SpeedFormat::Png, None).unwrap_err(), NEEDS_FOLDER);
    let out = dir.path().join("picked");
    let path = export_speed_save::run(&mut app, SpeedFormat::Png, Some(&out)).unwrap();
    assert_eq!(path, out.join("unsaved.png"));
    assert_eq!(png_size(&path), (64, 32));
    assert!(doc::active_doc(&app).unwrap().path.is_none(), "사본 저장: 문서 경로 불변");
}

// ---- UI ----

/// CAT 섹션을 모두 펼친 앱 하네스
fn harness(dir: &Path) -> egui_kittest::Harness<'static, PhotocraftApp> {
    let cat_dir = dir.join("cat");
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.cat = CatState::load(Some(cat_dir.clone()));
        for s in ["EXPORT", "IMPORT", "LAYERS RENAME", "GUIDE", "EXTRA"] {
            app.cat.settings.sections.insert(s.into(), true);
        }
        // 다른 그룹을 닫고 CAT 그룹을 크게: 모든 버튼이 화면 안에 보이도록
        app.ui.panels.color = false;
        app.ui.panels.properties = false;
        app.ui.dock.heights.insert(crate::dock::Group::Cat, 760.0);
        app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
        app
    });
    // 테스트 폰트에는 한글이 없다 (앱은 cjk_fonts 가 시스템 폰트를 지연 로드한다)
    h.ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    h.run_steps(6);
    h
}

#[test]
fn the_cat_panel_is_docked_by_default_and_its_buttons_run_features() {
    use egui_kittest::kittest::Queryable;
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(dir.path());
    assert!(h.state().ui.panels.cat, "기본 레이아웃에 CAT 표시");
    snapshot(&mut h, "cat-tools-panel.png");
    h.get_by_label("Make guides").click();
    h.run_steps(4);
    assert_eq!(guide_key(h.state()), "h50,v100", "기본 2x2");
    h.get_by_label("Guide from top").click();
    h.run_steps(4);
    assert_eq!(guide_key(h.state()), "h30,h50,v100", "기본 오프셋 30");
    // 기능 실패는 알림 모달로 알린다 (미저장 문서의 Clean PSD)
    h.get_by_label("Clean PSD").click();
    h.run_steps(4);
    assert!(h.state().cat.modals.front().is_some_and(|m| m.message.contains("Clean PSD 실패")));
    h.get_by_label("OK").click();
    h.run_steps(4);
    assert!(h.state().cat.modals.is_empty());
    snapshot(&mut h, "cat-tools-panel-after.png");
}

#[test]
fn the_cat_vector_section_and_trace_window() {
    use egui_kittest::kittest::Queryable;
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(dir.path());
    // VECTOR 섹션만 펼친다
    for (k, v) in h.state_mut().cat.settings.sections.iter_mut() {
        *v = k == "VECTOR";
    }
    h.state_mut().cat.settings.sections.insert("VECTOR".into(), true);
    h.run_steps(4);
    snapshot(&mut h, "cat-vector-section.png");
    // 셰이프가 선택되지 않으면 Pathfinder 는 실패 알림
    h.get_by_label("Unite").click();
    h.run_steps(4);
    assert!(h.state().cat.modals.front().is_some_and(|m| m.message.contains("Unite 실패")));
    h.get_by_label("OK").click();
    h.run_steps(4);
    // Image trace 버튼 → 설정 창
    h.get_by_label("Image trace").click();
    h.run_steps(6);
    assert!(h.state().cat.vector.trace.open);
    snapshot(&mut h, "cat-image-trace-window.png");
    h.get_by_label("Trace").click();
    h.run_steps(4);
    // 흰 배경(흑백 모드, Ignore White 꺼짐) → 흰 셰이프 하나
    assert!(h.state().cat.modals.is_empty());
    assert_eq!(layer::all_layers(h.state()).unwrap().len(), 2);
    assert!(h.state().ui.status.contains("1개"), "{}", h.state().ui.status);
}

/// `CAT_SNAPSHOT_DIR` 가 있으면 하네스 화면을 PNG 로 저장 (시각 확인용)
fn snapshot(h: &mut egui_kittest::Harness<'static, PhotocraftApp>, name: &str) {
    if let Ok(out) = std::env::var("CAT_SNAPSHOT_DIR") {
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        h.render().unwrap().save(out.join(name)).unwrap();
    }
}

#[test]
fn window_menu_toggles_the_cat_group() {
    let (mut app, _dir) = test_app();
    let ctx = egui::Context::default();
    assert!(app.ui.panels.cat);
    crate::menus::invoke(&mut app, &ctx, "window.panel.cat", json!({})).unwrap();
    assert!(!app.ui.panels.cat);
    crate::menus::invoke(&mut app, &ctx, "window.panel.cat", json!({})).unwrap();
    assert!(app.ui.panels.cat);
}
