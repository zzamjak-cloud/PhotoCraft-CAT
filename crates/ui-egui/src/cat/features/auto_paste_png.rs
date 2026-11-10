//! Drop PNG as pixel layer (CAT autoPastePng.js 대응): PNG 를 캔버스에 드롭하거나 File › Place 하면
//! 배치된 스마트 오브젝트를 원본 해상도 픽셀 레이어로 바꾼다 (배치 중심 유지, 정수 좌표, 이름 = 파일명 stem).
//!
//! CAT 은 Photoshop placeEvent 를 구독했다. 포크에서는 앱의 배치 경로가 성공 직후 [`after_place`] 를 부른다
//! (`file_open.rs` 드롭 배치 `place_next_dropped`, `menus/view_cmds/dialogs.rs` File › Place Embedded/Linked).

use photocraft_doc::LayerId;
use serde_json::Value;

use crate::PhotocraftApp;
use crate::cat::CatState;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::{alert, with_history};
use crate::cat::lib::{layer, pixels};

/// 켜짐 여부 (저장값이 없으면 켜짐; CAT 과 같음)
pub fn is_enabled(cat: &CatState) -> bool {
    // 앱 기본값은 켜짐. 테스트 빌드는 업스트림 Place 테스트(자유 변형 시작)를 위해 꺼 두고, CAT 테스트가 직접 켠다.
    cat.settings.auto_paste_png.unwrap_or(!cfg!(test))
}

/// 켜기/끄기 저장
pub fn set_enabled(cat: &mut CatState, enabled: bool) {
    cat.settings.auto_paste_png = Some(enabled);
    cat.save_settings();
}

/// 원본 파일이 PNG 인 스마트 오브젝트인지
pub fn is_placed_png(app: &PhotocraftApp, id: LayerId) -> bool {
    layer::layer_by_id(app, id).and_then(|l| l.smart).is_some_and(|s| s.file_name.to_ascii_lowercase().ends_with(".png"))
}

/// JS `Math.round` (0.5 는 +∞ 방향)
fn js_round(v: f64) -> i32 {
    (v + 0.5).floor() as i32
}

/// 방금 배치된 PNG 스마트 오브젝트를 픽셀 레이어로 변환 → 새 레이어 ID (PNG 스마트 오브젝트가 아니면 None)
pub fn run(app: &mut PhotocraftApp, id: LayerId) -> CatResult<Option<LayerId>> {
    if !is_placed_png(app, id) {
        return Ok(None);
    }
    let Some(b) = layer::layer_by_id(app, id).and_then(|l| l.bounds) else { return Ok(None) };
    let (cx, cy) = (f64::from(b.x) + f64::from(b.w) / 2.0, f64::from(b.y) + f64::from(b.h) / 2.0);
    let (img, file_name) = pixels::smart_object_source_pixels(app, id)?;
    let stem = std::path::Path::new(&file_name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(file_name);
    let x = js_round(cx - f64::from(img.width) / 2.0);
    let y = js_round(cy - f64::from(img.height) / 2.0);
    with_history(app, |app| {
        // 새 레이어가 스마트 오브젝트 바로 위에 오도록 먼저 선택
        layer::select_layer(app, id, false)?;
        let new_id = pixels::put_pixels_as_layer(app, &img, &stem, x, y)?;
        layer::delete_layer(app, id)?;
        layer::select_layer(app, new_id, false)?;
        Ok(Some(new_id))
    })
}

/// 배치 경로 훅: 배치가 성공했고 기능이 켜져 있으면 PNG 스마트 오브젝트를 바로 변환한다.
/// → 변환했으면 true (호출부는 배치용 자유 변형을 시작하지 않는다)
pub fn after_place(app: &mut PhotocraftApp, placed: &Result<Value, String>) -> bool {
    if !is_enabled(&app.cat) {
        return false;
    }
    let Some(id) = placed.as_ref().ok().and_then(layer::layer_id_of) else { return false };
    let background = std::mem::replace(&mut app.background_jobs, false);
    let r = run(app, id);
    app.background_jobs = background;
    match r {
        Ok(converted) => converted.is_some(),
        Err(e) => {
            alert(app, format!("Drop PNG as pixel layer 실패:\n{e}"));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cat::features::import_files::tests::write_png;
    use crate::cat::lib::core::exec;
    use crate::cat::lib::layer::{Bounds, LayerKind};
    use crate::cat::tests::{new_doc, test_app};

    #[test]
    fn placed_png_becomes_original_resolution_pixel_layer() {
        let (mut app, dir) = test_app();
        set_enabled(&mut app.cat, true);
        new_doc(&mut app, "doc", 100, 80);
        let png = dir.path().join("Hero Icon.png");
        // 캔버스보다 커서 배치 시 축소되는 PNG
        write_png(&png, 201, 41, [200, 10, 10, 255]);
        let placed = exec(&mut app, "file.placeEmbedded", json!({"path": png.to_str().unwrap()}));
        let so = layer::layer_id_of(placed.as_ref().unwrap()).unwrap();
        assert_eq!(layer::layer_by_id(&app, so).unwrap().kind, LayerKind::SmartObject);

        assert!(after_place(&mut app, &placed));
        assert!(layer::layer_by_id(&app, so).is_none(), "스마트 오브젝트는 삭제");
        let l = layer::active_layer(&app).unwrap();
        assert_eq!((l.name.as_str(), l.kind), ("Hero Icon", LayerKind::Pixel));
        // 원본 해상도, 배치 중심(50,40) 유지 → 정수 좌표 round(50-100.5)= -50, round(40-20.5)=20
        assert_eq!(l.bounds, Some(Bounds { x: -50, y: 20, w: 201, h: 41 }));
        let names: Vec<_> = layer::all_layers(&app).unwrap().into_iter().map(|l| l.name).collect();
        assert_eq!(names, ["Hero Icon", "Background"]);
    }

    /// 앱의 드롭 배치 경로(place_next_dropped)에 걸린 훅: 켜져 있으면 변형 없이 픽셀 레이어, 꺼져 있으면 기존 동작(자유 변형)
    #[test]
    fn drop_place_path_runs_the_hook() {
        let (mut app, dir) = test_app();
        set_enabled(&mut app.cat, true);
        new_doc(&mut app, "doc", 60, 60);
        let png = dir.path().join("drop.png");
        write_png(&png, 9, 7, [0, 255, 0, 255]);
        let bytes = std::fs::read(&png).unwrap();
        let ctx = egui::Context::default();

        let doc = app.session.active().unwrap().doc.id;
        app.drop_places.push_back((doc, crate::file_open::DropPlace::Bytes("drop.png".into(), bytes.clone())));
        app.place_next_dropped(&ctx);
        let l = layer::active_layer(&app).unwrap();
        assert_eq!((l.name.as_str(), l.kind), ("drop", LayerKind::Pixel));
        assert_eq!(l.bounds, Some(Bounds { x: 26, y: 27, w: 9, h: 7 }));
        assert!(app.ui.transform.is_none());

        set_enabled(&mut app.cat, false);
        app.drop_places.push_back((doc, crate::file_open::DropPlace::Bytes("drop.png".into(), bytes)));
        app.place_next_dropped(&ctx);
        assert_eq!(layer::active_layer(&app).unwrap().kind, LayerKind::SmartObject);
    }

    #[test]
    fn disabled_or_non_png_is_left_alone() {
        let (mut app, dir) = test_app();
        new_doc(&mut app, "doc", 50, 50);
        let png = dir.path().join("a.png");
        write_png(&png, 10, 10, [0, 0, 255, 255]);
        set_enabled(&mut app.cat, false);
        let placed = exec(&mut app, "file.placeEmbedded", json!({"path": png.to_str().unwrap()}));
        assert!(!after_place(&mut app, &placed));
        assert_eq!(layer::active_layer(&app).unwrap().kind, LayerKind::SmartObject);

        set_enabled(&mut app.cat, true);
        assert!(!after_place(&mut app, &Err("x".into())));
        let pixel = crate::cat::tests::rect_layer(&mut app, "p", 0, 0, 5, 5, "#ff0000");
        assert_eq!(run(&mut app, pixel).unwrap(), None);
    }
}
