//! Sort all layers / Sort selected layers (CAT sortingLayer.js 대응):
//! 일반 레이어를 이름순(A→Z, 위→아래) 연속 블록으로, 그룹은 그 위에 이름순으로 모으고 그룹 내부는 재귀 정렬한다.

use std::cmp::Ordering;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer::{self, LayerInfo};

/// JS `localeCompare` 근사: 대소문자 무시 비교 후 원문 비교
pub fn locale_cmp(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| b.cmp(a))
}

fn sort_layers(app: &mut PhotocraftApp, list: Vec<LayerInfo>) -> CatResult<()> {
    let mut groups = Vec::new();
    let mut layers = Vec::new();
    for l in list {
        if l.is_group() {
            // 빈 그룹은 무시
            if !layer::children_of(app, l.id)?.is_empty() {
                groups.push(l);
            }
        } else if !layer::is_background_layer(app, l.id) {
            layers.push(l);
        }
    }
    layers.sort_by(|a, b| locale_cmp(&a.name, &b.name));
    groups.sort_by(|a, b| locale_cmp(&a.name, &b.name));

    // 각 레이어를 마지막 레이어 바로 위로 → 위에서부터 A→Z 연속 블록
    if let Some(last) = layers.last().map(|l| l.id) {
        for l in layers.iter().filter(|l| l.id != last) {
            layer::move_layers_to(app, &[l.id], last, "above")?;
        }
    }
    // 그룹을 첫 레이어 바로 위로 → 그룹 A→Z 가 일반 레이어 위에 쌓인다 (일반 레이어가 없으면 마지막 그룹 기준)
    if let Some(anchor) = layers.first().or(groups.last()).map(|l| l.id) {
        for g in groups.iter().filter(|g| g.id != anchor) {
            layer::move_layers_to(app, &[g.id], anchor, "above")?;
        }
    }
    for g in groups {
        let children = layer::children_of(app, g.id)?;
        sort_layers(app, children)?;
    }
    Ok(())
}

/// 모든(최상위) 레이어 정렬
pub fn run_all(app: &mut PhotocraftApp) -> CatResult<()> {
    let top = layer::top_level_layers(app)?;
    with_history(app, |app| sort_layers(app, top))
}

/// 선택 레이어 정렬
pub fn run_selected(app: &mut PhotocraftApp) -> CatResult<()> {
    let selected = layer::selected_layers(app)?;
    if selected.is_empty() {
        return Err("선택된 레이어가 없습니다.".into());
    }
    with_history(app, |app| sort_layers(app, selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn names(app: &PhotocraftApp) -> Vec<String> {
        layer::all_layers(app).unwrap().into_iter().map(|l| format!("{}{}", "  ".repeat(l.depth), l.name)).collect()
    }

    #[test]
    fn sorts_layers_alphabetically_with_groups_on_top() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 50, 50);
        let b = rect_layer(&mut app, "b", 0, 0, 5, 5, "#ff0000");
        let c = rect_layer(&mut app, "C", 0, 0, 5, 5, "#ff0000");
        layer::select_layers(&mut app, &[b, c]).unwrap();
        let g = layer::group_selected_layers(&mut app, "Grp").unwrap();
        rect_layer(&mut app, "z", 0, 0, 5, 5, "#ff0000");
        rect_layer(&mut app, "a", 0, 0, 5, 5, "#ff0000");
        rect_layer(&mut app, "M", 0, 0, 5, 5, "#ff0000");
        layer::new_layer(&mut app, "Empty Group").unwrap();
        assert!(layer::layer_by_id(&app, g).is_some());
        let before = app.session.active().unwrap().history.past_len();

        run_all(&mut app).unwrap();
        assert_eq!(names(&app), ["Grp", "  b", "  C", "a", "Empty Group", "M", "z", "Background"]);
        assert_eq!(app.session.active().unwrap().history.past_len(), before + 1, "Undo 한 단계");
    }

    #[test]
    fn sorts_only_selected_layers() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 50, 50);
        let y = rect_layer(&mut app, "y", 0, 0, 5, 5, "#ff0000");
        rect_layer(&mut app, "keep", 0, 0, 5, 5, "#ff0000");
        let x = rect_layer(&mut app, "x", 0, 0, 5, 5, "#ff0000");
        // 위→아래: x, keep, y
        layer::select_layers(&mut app, &[x, y]).unwrap();
        run_selected(&mut app).unwrap();
        assert_eq!(names(&app), ["keep", "x", "y", "Background"]);

        layer::select_no_layers(&mut app).unwrap();
        assert!(run_selected(&mut app).is_err());
    }
}
