//! Tool Presets 패널: 새 프리셋 이름 입력(확정·취소·한글 IME)과 끌어서 순서 바꾸기.
use egui::{Event, ImeEvent, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use crate::PhotocraftApp;

fn harness(names: &[&str]) -> Harness<'static, PhotocraftApp> {
    let mut s = photocraft_engine::Session::new();
    s.execute("file.new", json!({"width": 64, "height": 48})).unwrap();
    s.presets.tool_presets.clear();
    for n in names {
        s.execute("tool.presets.new", json!({"name": n, "tool": "move"})).unwrap();
    }
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(1.0 / 60.0).with_max_steps(64).build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(s, crate::Services::default())
    });
    // 테스트 폰트에 한글이 없다.
    h.ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    h.state_mut().ui.panels.presets = true;
    h.run_steps(8);
    h
}

fn names(h: &Harness<'_, PhotocraftApp>) -> Vec<String> {
    h.state().session.presets.tool_presets.iter().map(|p| p.name.clone()).collect()
}

fn key(k: Key, m: Modifiers) -> Event {
    Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: m }
}

fn row(h: &Harness<'_, PhotocraftApp>, name: &str) -> egui::Rect {
    h.get_by_label(name).rect()
}

fn button(h: &mut Harness<'_, PhotocraftApp>, at: Pos2, pressed: bool) {
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
}

/// + 를 누르면 이름 필드가 열리고 이름 전체가 선택된다. 입력은 그 선택을 덮어쓴다.
fn start_new(h: &mut Harness<'_, PhotocraftApp>) {
    h.get_by_label("Create new tool preset").click();
    h.run_steps(3);
}

/// 한글 조합(가) 직후 "(" 를 쳐도 둘 다 들어가고, 다른 행을 클릭하면 입력한 이름이 확정된다.
#[test]
fn a_new_preset_name_survives_ime_and_a_click_elsewhere() {
    let mut h = harness(&["01_Pen"]);
    start_new(&mut h);
    for frame in [
        vec![Event::Ime(ImeEvent::Preedit { text: "ㄱ".into(), active_range_chars: None })],
        vec![Event::Ime(ImeEvent::Preedit { text: "가".into(), active_range_chars: None })],
        vec![Event::Ime(ImeEvent::Commit("가".into())), key(Key::Num9, Modifiers::SHIFT), Event::Text("(".into())],
        vec![Event::Text("1)".into())],
    ] {
        for e in frame {
            h.event(e);
        }
        h.run_steps(1);
    }
    let at = row(&h, "01_Pen").center();
    h.hover_at(at);
    h.run_steps(1);
    button(&mut h, at, true);
    h.run_steps(1);
    button(&mut h, at, false);
    h.run_steps(3);
    assert_eq!(names(&h), ["01_Pen", "가(1)"]);
    assert!(h.state().ui.presets_ui.renaming.is_none());
}

/// Esc 는 이름 변경을 취소한다(기본 이름 유지).
#[test]
fn escape_cancels_the_new_preset_name() {
    let mut h = harness(&[]);
    start_new(&mut h);
    let before = names(&h);
    h.event(Event::Text("Ink".into()));
    h.run_steps(1);
    h.event(key(Key::Escape, Modifiers::NONE));
    h.run_steps(2);
    assert_eq!(names(&h), before);
    assert!(h.state().ui.presets_ui.renaming.is_none());
}

/// 행을 끌어 다른 행의 아래 절반에 놓으면 그 뒤로 옮겨진다.
#[test]
fn dragging_a_row_reorders_the_presets() {
    let mut h = harness(&["A", "B", "C"]);
    let (a, c) = (row(&h, "A"), row(&h, "C"));
    h.hover_at(a.center());
    h.run_steps(1);
    button(&mut h, a.center(), true);
    h.run_steps(1);
    for i in 1..=6 {
        h.hover_at(a.center().lerp(c.center() + egui::vec2(0.0, 6.0), i as f32 / 6.0));
        h.run_steps(1);
    }
    button(&mut h, c.center() + egui::vec2(0.0, 6.0), false);
    h.run_steps(3);
    assert_eq!(names(&h), ["B", "C", "A"]);
}

#[test]
fn reorder_index_accounts_for_the_removed_row() {
    let n: Vec<String> = ["A", "B", "C"].map(String::from).to_vec();
    assert_eq!(super::panels::reorder_index(&n, "A", "C", true), Some(2));
    assert_eq!(super::panels::reorder_index(&n, "C", "A", false), Some(0));
    assert_eq!(super::panels::reorder_index(&n, "A", "B", false), None);
    assert_eq!(super::panels::reorder_index(&n, "B", "A", true), None);
}
