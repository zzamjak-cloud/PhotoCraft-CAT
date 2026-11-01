//! Clear hidden effects (CAT clearHiddenEffects.js 대응): 선택 레이어에서 꺼진(숨김) 레이어 효과만 삭제한다.
//! 잠긴 레이어는 건너뛴다.

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer;

/// 선택 레이어의 숨겨진 효과 삭제 → 삭제한 효과 개수
pub fn run(app: &mut PhotocraftApp) -> CatResult<usize> {
    let layers = layer::selected_layers(app)?;
    if layers.is_empty() {
        return Err("선택된 레이어가 없습니다.".into());
    }
    with_history(app, |app| {
        let mut removed = 0;
        for l in layers.iter().filter(|l| !l.locked && l.has_effects) {
            // 뒤에서부터 지워야 앞쪽 인덱스가 유지된다
            for i in layer::hidden_effect_indices(app, l.id).into_iter().rev() {
                layer::remove_layer_effect(app, l.id, i)?;
                removed += 1;
            }
        }
        layer::select_no_layers(app)?;
        Ok(removed)
    })
}

#[cfg(test)]
mod tests {
    use photocraft_doc::LayerId;
    use serde_json::json;

    use super::*;
    use crate::cat::lib::core::exec;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn add_effects(app: &mut PhotocraftApp, id: LayerId) {
        exec(app, "layer.layerStyle.dropShadow", json!({"layer": id.0, "enabled": false})).unwrap();
        exec(app, "layer.layerStyle.stroke", json!({"layer": id.0, "size": 2})).unwrap();
        exec(app, "layer.layerStyle.colorOverlay", json!({"layer": id.0, "color": "#00ff00", "enabled": false})).unwrap();
    }

    fn effect_count(app: &PhotocraftApp, id: LayerId) -> (usize, usize) {
        let st = app.session.active().unwrap();
        let items = &st.doc.layer(id).unwrap().effects.items;
        (items.len(), items.iter().filter(|e| !e.enabled()).count())
    }

    #[test]
    fn removes_only_disabled_effects_and_skips_locked_layers() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 50, 50);
        let a = rect_layer(&mut app, "a", 0, 0, 10, 10, "#ff0000");
        add_effects(&mut app, a);
        let locked = rect_layer(&mut app, "locked", 20, 20, 10, 10, "#ff0000");
        add_effects(&mut app, locked);
        exec(&mut app, "layer.setProps", json!({"layer": locked.0, "locks": {"position": true}})).unwrap();
        assert!(layer::layer_by_id(&app, locked).unwrap().locked);
        assert_eq!(effect_count(&app, a), (3, 2));
        layer::select_layers(&mut app, &[a, locked]).unwrap();
        let before = app.session.active().unwrap().history.past_len();

        assert_eq!(run(&mut app).unwrap(), 2);
        assert_eq!(effect_count(&app, a), (1, 0));
        assert_eq!(effect_count(&app, locked), (3, 2), "잠긴 레이어는 그대로");
        assert!(layer::selected_layers(&app).unwrap().is_empty());
        assert_eq!(app.session.active().unwrap().history.past_len(), before + 1, "Undo 한 단계");
    }

    #[test]
    fn requires_selection() {
        let (mut app, _dir) = test_app();
        new_doc(&mut app, "doc", 50, 50);
        layer::select_no_layers(&mut app).unwrap();
        assert!(run(&mut app).is_err());
    }
}
