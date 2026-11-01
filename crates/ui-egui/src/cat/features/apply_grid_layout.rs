//! Arrange in grid (CAT applyGridLayout.js 대응): 선택 레이어를 이름순으로 셀 크기·열 수 격자의 셀 중심에 배치.

use crate::PhotocraftApp;
use crate::cat::features::size_presets::Size;
use crate::cat::features::sorting_layer::locale_cmp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer;

/// JS `Math.round` (0.5 는 +∞ 방향)
fn js_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

/// 선택 레이어를 `cell` 크기 칸, `columns` 열 그리드로 배치 (원점 0,0)
pub fn run(app: &mut PhotocraftApp, cell: Size, columns: u32) -> CatResult<()> {
    let mut layers = layer::selected_layers(app)?;
    if layers.is_empty() {
        return Err("Please select layers to arrange in grid layout.".into());
    }
    let cols = columns.max(1) as usize;
    let (cw, ch) = (f64::from(cell.width), f64::from(cell.height));
    layers.sort_by(|a, b| locale_cmp(&a.name, &b.name));
    with_history(app, |app| {
        for (i, l) in layers.iter().enumerate() {
            let Some(b) = l.bounds else { continue };
            let (row, col) = ((i / cols) as f64, (i % cols) as f64);
            // 셀 중심에 레이어 중심을 맞춘다 (PhotoCraft 이동은 정수 픽셀)
            let tx = col * cw + cw / 2.0;
            let ty = row * ch + ch / 2.0;
            let dx = js_round(tx - (f64::from(b.x) + f64::from(b.w) / 2.0));
            let dy = js_round(ty - (f64::from(b.y) + f64::from(b.h) / 2.0));
            if dx != 0 || dy != 0 {
                layer::move_layer_offset(app, l.id, dx, dy)?;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    #[test]
    fn arranges_selected_layers_by_name_into_cells() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 300, 300);
        let c = rect_layer(&mut app, "c", 200, 200, 20, 20, "#ff0000");
        let a = rect_layer(&mut app, "a", 150, 10, 10, 30, "#00ff00");
        let b = rect_layer(&mut app, "b", 7, 250, 40, 40, "#0000ff");
        layer::select_layers(&mut app, &[c, a, b]).unwrap();
        let before = app.session.active().unwrap().history.past_len();

        run(&mut app, Size { width: 100, height: 60 }, 2).unwrap();
        let bounds = |id| layer::layer_by_id(&app, id).unwrap().bounds.unwrap();
        // a → (0,0) 셀 중심 (50,30), b → (1,0) 셀 중심 (150,30), c → (0,1) 셀 중심 (50,90)
        assert_eq!(bounds(a), layer::Bounds { x: 45, y: 15, w: 10, h: 30 });
        assert_eq!(bounds(b), layer::Bounds { x: 130, y: 10, w: 40, h: 40 });
        assert_eq!(bounds(c), layer::Bounds { x: 40, y: 80, w: 20, h: 20 });
        assert_eq!(app.session.active().unwrap().history.past_len(), before + 1, "Undo 한 단계");
    }

    #[test]
    fn requires_selection() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 50, 50);
        layer::select_no_layers(&mut app).unwrap();
        assert!(run(&mut app, Size { width: 10, height: 10 }, 1).is_err());
    }
}
