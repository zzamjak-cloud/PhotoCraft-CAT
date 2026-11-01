//! Import SVG as shape (CAT svgToShapeLayer.js 대응): 128×128 새 문서에 fill/stroke 스타일별 셰이프 레이어로 가져온다.
//! SVG 해석은 PhotoCraft 의 usvg 기반 가져오기(`lib::shape::parse_svg_shapes`)를 쓴다 (CAT 의 정규식 파서보다 transform·use·CSS 가 정확).

use std::path::Path;

use photocraft_doc::LayerId;

use crate::PhotocraftApp;
use crate::cat::lib::CatResult;
use crate::cat::lib::core::with_history;
use crate::cat::lib::doc::{DocBackground, create_doc};
use crate::cat::lib::shape::{PathOp, ShapeOptions, ShapePath, SvgShape, create_shape_from_path, parse_svg_shapes, rgb_hex};

/// 새 문서 크기 (CAT 과 동일)
pub const DOC_SIZE: u32 = 128;

/// SVG 파일을 셰이프 레이어로 가져오기 → 새 레이어 ID
pub fn run(app: &mut PhotocraftApp, svg_path: &Path) -> CatResult<Vec<LayerId>> {
    let bytes = std::fs::read(svg_path).map_err(|e| format!("SVG 파일을 읽을 수 없습니다: {e}"))?;
    let name = svg_path.file_stem().map_or_else(|| "svg".to_string(), |s| s.to_string_lossy().into_owned());
    import_svg_bytes(app, &bytes, &name)
}

/// 스타일(fill/stroke 조합)이 같은 셰이프 묶음 — 멀티컬러 SVG 는 컬러당 셰이프 레이어 하나
struct StyleGroup {
    name: String,
    options: ShapeOptions,
    path: ShapePath,
}

fn style_key(s: &SvgShape) -> String {
    let stroke = s.stroke.as_ref().map_or("none".to_string(), |st| format!("{}|{}|{}|{}", rgb_hex(st.color), st.width, st.cap, st.join));
    format!("{}|{}|{}", s.fill.map_or("none".to_string(), rgb_hex), stroke, s.path.even_odd)
}

/// SVG 등장 순서를 유지하며 스타일별로 묶고 `scale` 배 확대
fn group_by_style(shapes: &[SvgShape], scale: f64) -> Vec<StyleGroup> {
    let mut keys: Vec<String> = Vec::new();
    let mut groups: Vec<StyleGroup> = Vec::new();
    for s in shapes {
        let key = style_key(s);
        let i = match keys.iter().position(|k| *k == key) {
            Some(i) => i,
            None => {
                keys.push(key);
                // 레이어 이름: fill 색 우선, 없으면 stroke 색
                let name = s.fill.or(s.stroke.as_ref().map(|st| st.color)).map_or("shape".to_string(), rgb_hex);
                let stroke = s.stroke.clone().map(|mut st| {
                    st.width = (st.width * scale).round().max(1.0);
                    st
                });
                let options = ShapeOptions { name: None, fill: s.fill, no_fill: s.fill.is_none(), stroke };
                groups.push(StyleGroup { name, options, path: ShapePath { subpaths: Vec::new(), even_odd: s.path.even_odd } });
                groups.len() - 1
            }
        };
        let Some(g) = groups.get_mut(i) else { continue };
        // 원본 path 하나 = 컴포넌트 하나 (첫 subpath 는 결합, 나머지는 같은 컴포넌트)
        for (j, sp) in s.path.subpaths.iter().enumerate() {
            let mut sp = sp.clone();
            sp.op = if j == 0 { PathOp::Combine } else { PathOp::Join };
            for k in &mut sp.knots {
                for q in [&mut k.anchor, &mut k.backward, &mut k.forward] {
                    q[0] *= scale;
                    q[1] *= scale;
                }
            }
            g.path.subpaths.push(sp);
        }
    }
    groups
}

/// SVG 바이트 → `doc_name` 128×128 투명 새 문서 + 셰이프 레이어 → 생성된 레이어 ID (파일/아이콘 검색 공용)
pub fn import_svg_bytes(app: &mut PhotocraftApp, bytes: &[u8], doc_name: &str) -> CatResult<Vec<LayerId>> {
    let svg = parse_svg_shapes(bytes)?;
    if svg.shapes.is_empty() {
        return Err("SVG에서 유효한 path를 찾을 수 없습니다.".into());
    }
    // 균등 스케일 (비율 유지)
    let scale = f64::from(DOC_SIZE) / svg.width.max(svg.height).max(1e-6);
    let groups = group_by_style(&svg.shapes, scale);
    create_doc(app, doc_name, DOC_SIZE, DOC_SIZE, DocBackground::Transparent)?;
    let multi = groups.len() > 1;
    with_history(app, |app| {
        let mut ids = Vec::new();
        // 먼저 등장한 그룹부터 생성 → 나중 path 가 위에 쌓인다
        for g in groups {
            let mut options = g.options;
            // 단일 컬러면 레이어 이름은 기본값 유지
            if multi {
                options.name = Some(g.name);
            }
            ids.push(create_shape_from_path(app, &g.path, &options)?);
        }
        Ok(ids)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::lib::doc::active_doc;
    use crate::cat::lib::layer::all_layers;
    use crate::cat::lib::shape::{get_shape_fill_color, get_shape_path, get_stroke_settings};
    use crate::cat::tests::test_app;

    #[test]
    fn imports_multicolor_svg_into_128_doc() {
        let (mut app, dir) = test_app();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <path fill="#ff0000" d="M2 2h10v10H2z"/>
            <path fill="#ff0000" d="M14 2h8v8h-8z"/>
            <path fill="#0000ff" d="M12 22a6 6 0 1 1 0.01 0z"/>
            <path fill="none" d="M0 0h24v24H0z"/>
        </svg>"##;
        let file = dir.path().join("logo.svg");
        std::fs::write(&file, svg).unwrap();
        let ids = run(&mut app, &file).unwrap();
        let doc = active_doc(&app).unwrap();
        assert_eq!((doc.name.as_str(), doc.width, doc.height), ("logo", 128, 128));
        assert_eq!(ids.len(), 2);
        assert_eq!(get_shape_fill_color(&app, ids[0]).unwrap(), [255, 0, 0]);
        assert_eq!(get_shape_fill_color(&app, ids[1]).unwrap(), [0, 0, 255]);
        let names: Vec<String> = all_layers(&app).unwrap().iter().map(|l| l.name.clone()).collect();
        assert!(names.contains(&"#ff0000".to_string()) && names.contains(&"#0000ff".to_string()), "{names:?}");
        // 빨강: 사각형 2개 = 컴포넌트 2개, 정점 4+4, 128/24 배율
        let red = get_shape_path(&app, ids[0]).unwrap();
        assert_eq!(red.subpaths.len(), 2);
        assert_eq!(red.knot_count(), 8);
        let k = 128.0 / 24.0;
        let first = red.subpaths[0].knots[0].anchor;
        assert!((first[0] - 2.0 * k).abs() < 1e-3 && (first[1] - 2.0 * k).abs() < 1e-3, "{first:?}");
    }

    #[test]
    fn stroke_only_icon_keeps_stroke_without_fill() {
        let (mut app, _dir) = test_app();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path fill="none" stroke="#00ff00" stroke-width="2" stroke-linecap="round" d="M4 12h16"/></svg>"##;
        let ids = import_svg_bytes(&mut app, svg.as_bytes(), "line").unwrap();
        assert_eq!(ids.len(), 1);
        let st = get_stroke_settings(&app, ids[0]).unwrap().unwrap();
        assert!(st.enabled);
        assert_eq!(st.color, [0, 255, 0]);
        assert_eq!(st.cap, "round");
        assert!((st.width - (2.0f64 * 128.0 / 24.0).round()).abs() < 1e-6);
        let p = get_shape_path(&app, ids[0]).unwrap();
        assert_eq!(p.knot_count(), 2);
        assert!(!p.subpaths[0].closed);
    }

    #[test]
    fn rejects_svg_without_paths() {
        let (mut app, _dir) = test_app();
        let err = import_svg_bytes(&mut app, br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"></svg>"#, "x").unwrap_err();
        assert!(err.contains("path"), "{err}");
    }
}
