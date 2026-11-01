//! App icon maker (CAT appIconMaker.js · JP_Plugin appIconMaker.ts 대응; CAT 에서 버튼 미연결 → EXTRA 섹션).
//! 활성 문서(2048 템플릿)를 크기마다 임시 복제 → 캔버스 1332 중앙 크롭 → 리사이즈 → PNG 저장 → 닫기.
//! 적응형 아이콘은 최상위 "f"/"b" 레이어만 보이게 해 크롭 없이 리사이즈한다.

use std::path::Path;

use photocraft_doc::DocId;

use crate::PhotocraftApp;
use crate::cat::lib::{CatResult, doc, export, layer};

/// 플랫폼별 폴더 이름과 크기
pub struct IconSet {
    pub name: &'static str,
    pub sizes: &'static [u32],
}

pub const ANDROID: IconSet = IconSet { name: "android", sizes: &[512, 432, 324, 216, 192, 162, 108, 144, 96, 81, 72, 48, 36] };
pub const IOS: IconSet = IconSet { name: "iOS", sizes: &[1024, 180, 167, 152, 144, 120, 114, 87, 80, 76, 72, 60, 58, 57, 40, 29, 20] };
pub const ANDROID_ADAPTIVE: IconSet = IconSet { name: "android(Adaptive)", sizes: &[432, 324, 216, 162, 108, 81] };

/// 일반 아이콘의 중앙 크롭 크기 (템플릿 2048 기준)
const CROP_SIZE: u32 = 1332;

/// 활성 문서로 앱 아이콘 세트를 `folder` 하위 플랫폼 폴더에 생성 → 생성한 파일 수
pub fn run(app: &mut PhotocraftApp, folder: &Path) -> CatResult<usize> {
    let original = doc::require_doc(app)?.id;
    let mut count = 0;
    for set in [&IOS, &ANDROID] {
        let dir = folder.join(set.name);
        for &size in set.sizes {
            generate(app, original, size, &dir, &format!("{}_{size}", set.name), |app| doc::canvas_size(app, CROP_SIZE, CROP_SIZE, "center"))?;
            count += 1;
        }
    }
    let dir = folder.join(ANDROID_ADAPTIVE.name);
    for &size in ANDROID_ADAPTIVE.sizes {
        for part in ["f", "b"] {
            generate(app, original, size, &dir, &format!("android_{size}_{part}"), |app| {
                // 최상위 레이어 중 해당 이름만 표시
                for l in layer::top_level_layers(app)? {
                    layer::set_visible(app, l.id, l.name == part)?;
                }
                Ok(())
            })?;
            count += 1;
        }
    }
    Ok(count)
}

/// 원본 문서 임시 복제 → 준비(크롭/레이어 표시) → 리사이즈 → PNG 저장 → 복제본 닫기
fn generate(
    app: &mut PhotocraftApp,
    original: DocId,
    size: u32,
    dir: &Path,
    file_name: &str,
    prepare: impl FnOnce(&mut PhotocraftApp) -> CatResult<()>,
) -> CatResult<()> {
    doc::activate_doc(app, original)?;
    let dup = doc::duplicate_doc(app, file_name, false)?;
    let r = prepare(app).and_then(|_| doc::resize_image(app, size, size)).and_then(|_| export::save_for_web_png(app, dir, file_name).map(|_| ()));
    let closed = doc::close_doc_without_saving(app, dup);
    let back = doc::activate_doc(app, original);
    r.map_err(|e| format!("아이콘 생성 실패 ({size}px): {e}"))?;
    closed.and(back)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn png(path: &Path) -> photocraft_codecs::Image {
        photocraft_codecs::decode(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn writes_every_platform_size_and_adaptive_layers() {
        let (mut app, dir) = test_app();
        let out = dir.path().join("icons");
        let original = new_doc(&mut app, "icon", 256, 256);
        rect_layer(&mut app, "b", 0, 0, 256, 256, "#0000ff");
        rect_layer(&mut app, "f", 100, 100, 56, 56, "#ff0000");
        let layers = layer::all_layers(&app).unwrap();

        let n = run(&mut app, &out).unwrap();
        assert_eq!(n, IOS.sizes.len() + ANDROID.sizes.len() + ANDROID_ADAPTIVE.sizes.len() * 2);
        for set in [&IOS, &ANDROID] {
            for &s in set.sizes {
                let img = png(&out.join(set.name).join(format!("{}_{s}.png", set.name)));
                assert_eq!((img.width(), img.height()), (s, s));
            }
        }
        for &s in ANDROID_ADAPTIVE.sizes {
            for part in ["f", "b"] {
                let img = png(&out.join(ANDROID_ADAPTIVE.name).join(format!("android_{s}_{part}.png")));
                assert_eq!((img.width(), img.height()), (s, s));
            }
        }
        // 원본 문서는 그대로
        assert_eq!(doc::list_documents(&app).len(), 1);
        assert_eq!(doc::active_doc(&app).unwrap().id, original);
        assert_eq!(layer::all_layers(&app).unwrap(), layers);
        // 적응형 "f": 배경(b)·흰 배경 숨김 → 모서리 투명, 중앙 빨강
        crate::cat::lib::export::open_file(&mut app, &out.join(ANDROID_ADAPTIVE.name).join("android_432_f.png")).unwrap();
        let f = crate::cat::lib::pixels::get_document_pixels(&app).unwrap();
        assert_eq!(f.pixel(0, 0).unwrap()[3], 0);
        assert_eq!(f.pixel(216, 216).unwrap(), [255, 0, 0, 255]);
    }
}
