//! Clear empty layers (CAT clearEmptyLayers.js 대응): 비어 있는 픽셀 레이어와 빈 그룹을 삭제한다.
//! 잠긴 레이어·그룹과 배경 레이어는 지우지 않는다 (전체 잠금 그룹의 내부도 건드리지 않음).

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer::{self, LayerInfo, LayerKind};

fn is_empty_pixel(l: &LayerInfo) -> bool {
    l.kind == LayerKind::Pixel && l.bounds.is_none_or(|b| b.w == 0 || b.h == 0)
}

/// `list` 의 빈 레이어·빈 하위 그룹을 `out` 에 모으고(안쪽 항목이 먼저), 목록이 통째로 비었는지 반환.
/// 픽셀 레이어 외 종류(텍스트·스마트 오브젝트·셰이프·조정 등)와 잠긴 빈 레이어는 유효한 콘텐츠로 본다.
fn collect(app: &PhotocraftApp, list: &[LayerInfo], out: &mut Vec<LayerId>) -> CatResult<bool> {
    let mut has_content = false;
    for l in list {
        if l.is_group() {
            // 전체 잠금 그룹은 내부까지 보존
            let empty = !l.locked_all && collect(app, &layer::children_of(app, l.id)?, out)?;
            if empty && !l.locked {
                out.push(l.id);
            } else {
                has_content = true;
            }
        } else if is_empty_pixel(l) && !l.locked && !layer::is_background_layer(app, l.id) {
            out.push(l.id);
        } else {
            has_content = true;
        }
    }
    Ok(!has_content)
}

/// 삭제 대상 ID (그룹 안 항목이 그룹보다 먼저)
pub fn collect_empty_layers(app: &PhotocraftApp) -> CatResult<Vec<LayerId>> {
    let mut out = Vec::new();
    collect(app, &layer::top_level_layers(app)?, &mut out)?;
    Ok(out)
}

/// 빈 레이어·빈 그룹 삭제 → 삭제한 개수 (0 이면 지울 것이 없었음)
pub fn run(app: &mut PhotocraftApp) -> CatResult<usize> {
    if layer::top_level_layers(app)?.len() <= 1 {
        return Err("문서에 최소 2개 이상의 레이어가 필요합니다.".into());
    }
    let ids = collect_empty_layers(app)?;
    if ids.is_empty() {
        return Ok(0);
    }
    with_history(app, |app| {
        let mut removed = 0;
        for id in ids {
            // 상위 그룹과 함께 이미 사라진 항목은 건너뛴다
            if layer::layer_by_id(app, id).is_some() && layer::delete_layer(app, id).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cat::lib::core::exec;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn names(app: &PhotocraftApp) -> Vec<String> {
        layer::all_layers(app).unwrap().into_iter().map(|l| format!("{}{}", "  ".repeat(l.depth), l.name)).collect()
    }

    fn lock_all(app: &mut PhotocraftApp, id: LayerId) {
        exec(app, "layer.setProps", json!({"layer": id.0, "locks": {"all": true}})).unwrap();
        assert!(layer::layer_by_id(app, id).unwrap().locked_all);
    }

    #[test]
    fn deletes_empty_layers_and_groups_but_keeps_locked() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 50, 50);
        let full = rect_layer(&mut app, "full", 0, 0, 5, 5, "#ff0000");
        let inner_empty = layer::new_layer(&mut app, "inner empty").unwrap();
        layer::select_layers(&mut app, &[full, inner_empty]).unwrap();
        layer::group_selected_layers(&mut app, "G content").unwrap();
        let e1 = layer::new_layer(&mut app, "e1").unwrap();
        layer::select_layers(&mut app, &[e1]).unwrap();
        layer::group_selected_layers(&mut app, "G empty").unwrap();
        let locked = layer::new_layer(&mut app, "locked empty").unwrap();
        lock_all(&mut app, locked);
        let e2 = layer::new_layer(&mut app, "e2").unwrap();
        layer::select_layers(&mut app, &[e2]).unwrap();
        let locked_group = layer::group_selected_layers(&mut app, "G locked").unwrap();
        lock_all(&mut app, locked_group);
        layer::new_layer(&mut app, "top empty").unwrap();
        let before = app.session.active().unwrap().history.past_len();

        let n = run(&mut app).unwrap();
        assert_eq!(n, 4, "{:?}", names(&app));
        assert_eq!(names(&app), ["G locked", "  e2", "locked empty", "G content", "  full", "Background"]);
        assert_eq!(app.session.active().unwrap().history.past_len(), before + 1, "Undo 한 단계");

        // 더 지울 것이 없으면 0
        assert_eq!(run(&mut app).unwrap(), 0);
    }

    #[test]
    fn needs_two_layers() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 20, 20);
        assert!(run(&mut app).is_err());
    }
}
