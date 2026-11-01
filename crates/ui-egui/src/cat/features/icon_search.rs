//! Search icons (CAT iconSearch.js 대응): Iconify 검색 → SVG 를 셰이프로 가져오기. UI 는 `cat::ui::icon_search`.
//! 네트워크 함수는 블로킹이므로 UI 는 작업 스레드에서 부른다.

use photocraft_doc::LayerId;
use serde_json::Value;

use super::svg_to_shape_layer::import_svg_bytes;
use crate::PhotocraftApp;
use crate::cat::lib::CatResult;

const ICONIFY_API: &str = "https://api.iconify.design";
/// API 최대치 — 한 번에 받아 클라이언트에서 페이징
const SEARCH_LIMIT: u32 = 999;
/// 한 페이지에 표시할 아이콘 수
pub const PAGE_SIZE: usize = 60;
/// 다크 패널에서 보이도록 회색 프리뷰 (currentColor 대체)
const PREVIEW_COLOR: &str = "#cccccc";

/// URL 쿼리 값 퍼센트 인코딩 (RFC 3986 unreserved 외 모두)
fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn http_get(url: &str) -> CatResult<String> {
    use std::time::Duration;
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).timeout_global(Some(Duration::from_secs(20))).build().into();
    let mut res = agent.get(url).call().map_err(|e| format!("요청 실패: {e}"))?;
    res.body_mut().read_to_string().map_err(|e| format!("응답을 읽을 수 없습니다: {e}"))
}

#[cfg(target_arch = "wasm32")]
fn http_get(_url: &str) -> CatResult<String> {
    Err("웹 빌드에서는 아이콘 검색을 지원하지 않습니다.".into())
}

/// "prefix:name" → (prefix, name)
pub fn split_icon_id(icon_id: &str) -> CatResult<(&str, &str)> {
    icon_id.split_once(':').filter(|(p, n)| !p.is_empty() && !n.is_empty()).ok_or_else(|| format!("잘못된 아이콘 ID: {icon_id}"))
}

/// 아이콘 검색 → "prefix:name" 목록 (블로킹)
pub fn search_icons(query: &str) -> CatResult<Vec<String>> {
    let body = http_get(&format!("{ICONIFY_API}/search?query={}&limit={SEARCH_LIMIT}", encode(query.trim())))?;
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("검색 응답 해석 실패: {e}"))?;
    Ok(v.get("icons").and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default())
}

/// 개별 아이콘 SVG 텍스트 (블로킹)
pub fn fetch_icon_svg(icon_id: &str) -> CatResult<String> {
    let (prefix, name) = split_icon_id(icon_id)?;
    http_get(&format!("{ICONIFY_API}/{}/{}.svg", encode(prefix), encode(name)))
}

/// Iconify 아이콘 세트 JSON(`/{prefix}.json?icons=...`) → 이름별 프리뷰 SVG (없으면 None). 별칭은 부모 아이콘으로.
pub fn preview_svgs_from_json(data: &Value, names: &[&str], size: u32) -> Vec<(String, Option<String>)> {
    let icons = data.get("icons");
    let aliases = data.get("aliases");
    let num = |v: Option<&Value>, key: &str| v.and_then(|v| v.get(key)).and_then(Value::as_f64);
    names
        .iter()
        .map(|name| {
            let parent = aliases.and_then(|a| a.get(*name)).and_then(|a| a.get("parent")).and_then(Value::as_str);
            let icon = icons.and_then(|i| i.get(*name)).or_else(|| parent.and_then(|p| icons.and_then(|i| i.get(p))));
            let svg = icon.and_then(|icon| {
                let body = icon.get("body")?.as_str()?.replace("currentColor", PREVIEW_COLOR);
                let w = num(Some(icon), "width").or(num(Some(data), "width")).unwrap_or(16.0);
                let h = num(Some(icon), "height").or(num(Some(data), "height")).unwrap_or(16.0);
                let (l, t) = (num(Some(icon), "left").unwrap_or(0.0), num(Some(icon), "top").unwrap_or(0.0));
                Some(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="{l} {t} {w} {h}">{body}</svg>"#))
            });
            (name.to_string(), svg)
        })
        .collect()
}

/// 같은 prefix 아이콘들의 프리뷰 SVG 일괄 조회 (블로킹)
pub fn fetch_preview_svgs(prefix: &str, names: &[&str], size: u32) -> CatResult<Vec<(String, Option<String>)>> {
    let list = names.iter().map(|n| encode(n)).collect::<Vec<_>>().join(",");
    let body = http_get(&format!("{ICONIFY_API}/{}.json?icons={list}", encode(prefix)))?;
    let data: Value = serde_json::from_str(&body).map_err(|e| format!("프리뷰 응답 해석 실패: {e}"))?;
    Ok(preview_svgs_from_json(&data, names, size))
}

/// 검색어로 아이콘 ID 목록 검색 ("prefix:name", 블로킹)
pub fn run(_app: &mut PhotocraftApp, query: &str) -> CatResult<Vec<String>> {
    search_icons(query)
}

/// 받아 둔 SVG 를 아이콘 이름의 새 문서 + 셰이프 레이어로 → 새 레이어 ID
pub fn import_icon_svg(app: &mut PhotocraftApp, icon_id: &str, svg: &str) -> CatResult<Vec<LayerId>> {
    let (_, name) = split_icon_id(icon_id)?;
    import_svg_bytes(app, svg.as_bytes(), name)
}

/// 아이콘을 셰이프 레이어로 가져오기 → 새 레이어 ID (블로킹 다운로드)
pub fn import_icon(app: &mut PhotocraftApp, icon_id: &str) -> CatResult<Vec<LayerId>> {
    let svg = fetch_icon_svg(icon_id)?;
    import_icon_svg(app, icon_id, &svg)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cat::lib::doc::active_doc;
    use crate::cat::lib::shape::get_shape_path;
    use crate::cat::tests::test_app;

    #[test]
    fn preview_json_resolves_aliases_and_color() {
        let data = json!({"prefix": "mdi", "width": 24, "height": 24, "icons": {"home": {"body": "<path fill=\"currentColor\" d=\"M0 0h1v1z\"/>"}}, "aliases": {"house": {"parent": "home"}}});
        let r = preview_svgs_from_json(&data, &["home", "house", "nope"], 56);
        assert_eq!(r.len(), 3);
        let svg = r[0].1.as_deref().unwrap();
        assert!(svg.contains("viewBox=\"0 0 24 24\"") && svg.contains(PREVIEW_COLOR) && svg.contains("width=\"56\""));
        assert_eq!(r[1].1, r[0].1);
        assert!(r[2].1.is_none());
        // 프리뷰 SVG 는 래스터화 가능해야 한다
        let (w, h, px) = photocraft_io::svg::rasterize_rgba8(svg.as_bytes()).unwrap();
        assert_eq!((w, h), (56, 56));
        assert!(px.chunks_exact(4).any(|p| p[3] > 0));
    }

    #[test]
    fn encodes_query_and_splits_ids() {
        assert_eq!(encode("arrow left/ㄱ"), "arrow%20left%2F%E3%84%B1");
        assert_eq!(split_icon_id("mdi:home").unwrap(), ("mdi", "home"));
        assert!(split_icon_id("home").is_err());
    }

    /// Iconify 아이콘 SVG 형식(width=1em, currentColor) 그대로 오프라인 가져오기
    #[test]
    fn imports_iconify_style_svg_offline() {
        let (mut app, _dir) = test_app();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"><path fill="currentColor" d="M10 20v-6h4v6h5v-8h3L12 3L2 12h3v8z"/></svg>"#;
        let ids = import_icon_svg(&mut app, "mdi:home", svg).unwrap();
        assert_eq!(ids.len(), 1);
        let doc = active_doc(&app).unwrap();
        assert_eq!((doc.name.as_str(), doc.width), ("home", 128));
        let p = get_shape_path(&app, ids[0]).unwrap();
        assert_eq!(p.subpaths.len(), 1);
        assert_eq!(p.knot_count(), 11);
        // viewBox 24 → 128 배율: 꼭대기 점 (12,3) → (64,16)
        assert!(p.subpaths[0].knots.iter().any(|k| (k.anchor[0] - 64.0).abs() < 0.01 && (k.anchor[1] - 16.0).abs() < 0.01), "{p:?}");
    }

    #[test]
    #[ignore = "네트워크 필요 (Iconify API)"]
    fn searches_and_imports_from_iconify() {
        let ids = search_icons("home").unwrap();
        assert!(ids.iter().any(|i| i == "mdi:home"), "{}", ids.len());
        let (mut app, _dir) = test_app();
        let layers = import_icon(&mut app, "mdi:home").unwrap();
        assert!(!layers.is_empty());
        let pv = fetch_preview_svgs("mdi", &["home", "account"], 56).unwrap();
        assert!(pv.iter().all(|(_, s)| s.is_some()));
    }
}
