//! Icon Search 창 (CAT iconSearch.js showIconSearchDialog / JP IconSearchDialog.tsx 대응).
//! 검색·프리뷰·SVG 다운로드는 작업 스레드에서 하고 결과는 채널로 받는다. 상태는 egui 메모리에 둔다.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use crate::PhotocraftApp;
use crate::cat::features::icon_search::{PAGE_SIZE, fetch_icon_svg, fetch_preview_svgs, import_icon_svg, search_icons, split_icon_id};
use crate::cat::lib::core::run_safe;

/// 프리뷰 렌더 크기 (px)
const PREVIEW_PX: u32 = 56;
/// 그리드 칸 크기
const CELL: f32 = 72.0;

enum Msg {
    Search(u64, Result<Vec<String>, String>),
    /// (아이콘 ID, RGBA 프리뷰 — 실패면 None)
    Preview(String, Option<(u32, u32, Vec<u8>)>),
    PreviewsDone(u64),
    Svg(String, Result<String, String>),
}

#[derive(Default)]
struct State {
    open: bool,
    focus: bool,
    query: String,
    all: Vec<String>,
    page: usize,
    /// 결과 영역 상태 메시지 (None 이면 그리드)
    status: Option<(String, bool)>,
    selected: Option<String>,
    /// 아이콘 ID → 프리뷰 텍스처 (None = 실패, "?" 표시)
    previews: HashMap<String, Option<egui::TextureHandle>>,
    /// 진행 중 작업 무효화용 토큰 (페이지 이동·재검색 시 증가)
    token: u64,
    importing: bool,
    tx: Option<Sender<Msg>>,
    rx: Option<Receiver<Msg>>,
}

type Shared = Arc<Mutex<State>>;

fn state(ctx: &egui::Context) -> Shared {
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with(egui::Id::unique("cat_icon_search"), || Arc::new(Mutex::new(State::default()))).clone())
}

/// 창 열기 ("Search icons" 버튼). 이전 검색 상태는 유지한다.
pub fn open(ctx: &egui::Context) {
    if let Ok(mut s) = state(ctx).lock() {
        s.open = true;
        s.focus = true;
    }
}

impl State {
    fn sender(&mut self) -> Sender<Msg> {
        if let Some(tx) = &self.tx {
            return tx.clone();
        }
        let (tx, rx) = channel();
        self.tx = Some(tx.clone());
        self.rx = Some(rx);
        tx
    }

    fn run_search(&mut self, ctx: &egui::Context) {
        let q = self.query.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.token += 1;
        self.selected = None;
        self.all.clear();
        self.page = 0;
        self.status = Some(("검색 중...".into(), false));
        let (tx, token, ctx) = (self.sender(), self.token, ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Search(token, search_icons(&q)));
            ctx.request_repaint();
        });
    }

    fn page_ids(&self) -> Vec<String> {
        self.all.iter().skip(self.page * PAGE_SIZE).take(PAGE_SIZE).cloned().collect()
    }

    fn total_pages(&self) -> usize {
        self.all.len().div_ceil(PAGE_SIZE).max(1)
    }

    /// 페이지 표시: 캐시에 없는 프리뷰를 prefix 별로 묶어 받는다
    fn show_page(&mut self, ctx: &egui::Context, page: usize) {
        self.token += 1;
        self.selected = None;
        self.page = page;
        let ids = self.page_ids();
        if ids.is_empty() {
            self.status = Some(("결과 없음".into(), false));
            return;
        }
        let missing: Vec<String> = ids.into_iter().filter(|id| !self.previews.contains_key(id)).collect();
        if missing.is_empty() {
            self.status = None;
            return;
        }
        self.status = Some(("불러오는 중...".into(), false));
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for id in missing {
            let Ok((prefix, name)) = split_icon_id(&id) else { continue };
            match groups.iter_mut().find(|(p, _)| p == prefix) {
                Some((_, names)) => names.push(name.to_string()),
                None => groups.push((prefix.to_string(), vec![name.to_string()])),
            }
        }
        let (tx, token, ctx) = (self.sender(), self.token, ctx.clone());
        std::thread::spawn(move || {
            for (prefix, names) in groups {
                let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                // 실패한 prefix 는 "?" 로 표시
                let svgs = fetch_preview_svgs(&prefix, &refs, PREVIEW_PX).unwrap_or_else(|_| names.iter().map(|n| (n.clone(), None)).collect());
                for (name, svg) in svgs {
                    let img = svg.and_then(|s| photocraft_io::svg::rasterize_rgba8(s.as_bytes()).ok());
                    let _ = tx.send(Msg::Preview(format!("{prefix}:{name}"), img));
                }
                ctx.request_repaint();
            }
            let _ = tx.send(Msg::PreviewsDone(token));
            ctx.request_repaint();
        });
    }

    fn start_import(&mut self, ctx: &egui::Context) {
        let Some(id) = self.selected.clone() else { return };
        self.importing = true;
        let (tx, ctx) = (self.sender(), ctx.clone());
        std::thread::spawn(move || {
            let r = fetch_icon_svg(&id);
            let _ = tx.send(Msg::Svg(id, r));
            ctx.request_repaint();
        });
    }

    /// 작업 결과 반영 → 가져올 SVG (있으면)
    fn poll(&mut self, ctx: &egui::Context) -> Option<(String, Result<String, String>)> {
        let msgs: Vec<Msg> = self.rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        let mut import = None;
        for m in msgs {
            match m {
                Msg::Search(t, r) if t == self.token => match r {
                    Ok(all) => {
                        self.all = all;
                        self.show_page(ctx, 0);
                    }
                    Err(e) => self.status = Some((format!("오류: {e}"), true)),
                },
                Msg::Preview(id, img) => {
                    let tex = img.map(|(w, h, px)| {
                        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
                        ctx.load_texture(format!("cat_icon_{id}"), image, egui::TextureOptions::LINEAR)
                    });
                    self.previews.insert(id, tex);
                }
                Msg::PreviewsDone(t) if t == self.token => self.status = None,
                Msg::Svg(id, r) => {
                    self.importing = false;
                    import = Some((id, r));
                }
                _ => {}
            }
        }
        import
    }
}

/// 매 프레임 호출: 열려 있으면 창을 그리고, 다운로드가 끝난 아이콘을 가져온다
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let shared = state(ctx);
    let Ok(mut s) = shared.lock() else { return };
    let import = s.poll(ctx);
    if s.open {
        window(&mut s, ctx);
    }
    drop(s);
    if let Some((id, r)) = import {
        run_safe(app, "아이콘 가져오기", |app| import_icon_svg(app, &id, &r?).map(|_| ()));
    }
}

fn window(s: &mut State, ctx: &egui::Context) {
    let mut open = true;
    let mut close = false;
    egui::Window::new("Icon Search").id(egui::Id::unique("cat_icon_search_window")).open(&mut open).collapsible(false).default_size([600.0, 560.0]).show(
        ctx,
        |ui| {
            ui.horizontal(|ui| {
                let edit =
                    ui.add(egui::TextEdit::singleline(&mut s.query).hint_text("검색어 (예: home, arrow, cat)").desired_width(ui.available_width() - 80.0));
                if s.focus {
                    edit.request_focus();
                    s.focus = false;
                }
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Search").clicked() || enter {
                    s.run_search(ctx);
                }
            });
            results(s, ui);
            if !s.all.is_empty() {
                pager(s, ui, ctx);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(s.selected.clone().unwrap_or_default()).small().weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can = s.selected.is_some() && !s.importing;
                    if ui.add_enabled(can, egui::Button::new(if s.importing { "Importing..." } else { "Import" })).clicked() {
                        s.start_import(ctx);
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        },
    );
    if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        s.open = false;
    }
}

/// 결과 그리드 (상태 메시지가 있으면 메시지)
fn results(s: &mut State, ui: &mut egui::Ui) {
    let frame = egui::Frame::group(ui.style()).inner_margin(8.0);
    frame.show(ui, |ui| {
        ui.set_min_height(380.0);
        egui::ScrollArea::vertical().id_salt(("cat_icon_results", s.page)).max_height(420.0).auto_shrink([false, false]).show(ui, |ui| {
            if let Some((text, error)) = &s.status {
                let rich = egui::RichText::new(text).small();
                ui.label(if *error { rich.color(ui.visuals().error_fg_color) } else { rich.weak() });
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                for id in s.page_ids() {
                    if icon_cell(ui, &id, s.previews.get(&id).and_then(Option::as_ref), s.selected.as_deref() == Some(id.as_str())).clicked() {
                        s.selected = Some(id);
                    }
                }
            });
        });
    });
}

fn icon_cell(ui: &mut egui::Ui, id: &str, tex: Option<&egui::TextureHandle>, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(CELL, CELL), egui::Sense::click());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    if selected {
        painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.5, visuals.selection.stroke.color), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
    }
    match tex {
        Some(t) => {
            let r = egui::Rect::from_center_size(rect.center(), egui::vec2(PREVIEW_PX as f32, PREVIEW_PX as f32));
            painter.image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(10.0), visuals.weak_text_color());
        }
    }
    resp.on_hover_text(id)
}

/// 이전 / "n / 전체 (개수)" / 다음
fn pager(s: &mut State, ui: &mut egui::Ui, ctx: &egui::Context) {
    let total = s.total_pages();
    ui.horizontal(|ui| {
        let w = ui.available_width();
        ui.add_space((w / 2.0 - 140.0).max(0.0));
        if ui.add_enabled(s.page > 0, egui::Button::new("이전")).clicked() {
            let p = s.page - 1;
            s.show_page(ctx, p);
        }
        ui.label(egui::RichText::new(format!("{} / {} ({}개)", s.page + 1, total, s.all.len())).small().weak());
        if ui.add_enabled(s.page + 1 < total, egui::Button::new("다음")).clicked() {
            let p = s.page + 1;
            s.show_page(ctx, p);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paging_and_stale_results_are_ignored() {
        let ctx = egui::Context::default();
        let mut s = State { all: (0..130).map(|i| format!("mdi:i{i}")).collect(), ..Default::default() };
        assert_eq!(s.total_pages(), 3);
        s.page = 2;
        assert_eq!(s.page_ids().len(), 10);
        // 오래된 토큰의 검색 결과는 무시된다
        s.token = 5;
        let tx = s.sender();
        tx.send(Msg::Search(4, Ok(vec!["x:y".into()]))).unwrap();
        tx.send(Msg::Svg("mdi:home".into(), Err("x".into()))).unwrap();
        let import = s.poll(&ctx);
        assert_eq!(s.all.len(), 130);
        assert_eq!(import.map(|(id, _)| id).as_deref(), Some("mdi:home"));
        // 프리뷰 수신 → 텍스처
        tx.send(Msg::Preview("mdi:i0".into(), Some((2, 2, vec![255; 16])))).unwrap();
        tx.send(Msg::Preview("mdi:i1".into(), None)).unwrap();
        s.poll(&ctx);
        assert!(s.previews.get("mdi:i0").unwrap().is_some());
        assert!(s.previews.get("mdi:i1").unwrap().is_none());
    }
}
