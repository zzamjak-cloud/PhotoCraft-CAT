//! Market screenshot (CAT exportMarketScreenshot.js · JP_Plugin exportMarketScreenshot.ts 대응).
//! 최상위 국가코드 그룹/스마트 오브젝트를 하나씩만 보이게 해 `<폴더>/<문서명>_<레이어명>.jpg` (품질 60) 로 저장한다.
//! 끝나면 국가코드 레이어는 모두 숨겨진 상태다 (CAT 과 동일). 표시 변경은 Undo 한 단계.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use crate::PhotocraftApp;
use crate::cat::features::folder_preset;
use crate::cat::lib::core::with_history;
use crate::cat::lib::layer::LayerInfo;
use crate::cat::lib::{CatResult, export, layer};

/// 국가코드 레이어가 없을 때의 오류
pub const NO_COUNTRY_LAYERS: &str = "국가코드 레이어가 존재하지 않습니다.";

/// 국가코드 레이어마다 JPG 저장 → 저장한 개수. `folder`: 미저장 문서일 때 UI 가 고른 폴더.
pub fn run(app: &mut PhotocraftApp, folder: Option<&Path>) -> CatResult<usize> {
    let countries: Vec<LayerInfo> = layer::top_level_layers(app)?.into_iter().filter(is_country_layer).collect();
    if countries.is_empty() {
        return Err(NO_COUNTRY_LAYERS.into());
    }
    let (folder, doc_name) = match folder_preset::doc_folder_and_name(app)? {
        Some(v) => v,
        None => match folder {
            Some(f) => (f.to_path_buf(), folder_preset::doc_stem(app)?),
            None => return Err("문서가 저장되지 않아 저장할 폴더가 필요합니다.".into()),
        },
    };
    with_history(app, |app| {
        for l in &countries {
            layer::set_visible(app, l.id, false)?;
        }
        for l in &countries {
            layer::set_visible(app, l.id, true)?;
            let saved = export::save_for_web_jpg(app, &folder, &format!("{doc_name}_{}", l.name), 60);
            layer::set_visible(app, l.id, false)?;
            saved?;
        }
        Ok(())
    })?;
    Ok(countries.len())
}

/// 그룹 또는 스마트 오브젝트이면서 이름이 국가코드인 레이어
fn is_country_layer(l: &LayerInfo) -> bool {
    (l.is_group() || l.is_smart_object()) && is_country_code(&l.name)
}

/// 레이어 이름이 국가코드(ISO 3166-1 alpha-2/alpha-3 또는 ko/en/uk)와 정확히 일치하는지 (대소문자·앞뒤 공백 무시)
pub fn is_country_code(name: &str) -> bool {
    static CODES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    CODES.get_or_init(|| ISO_ALPHA2.iter().chain(ISO_ALPHA3).chain(LEGACY).copied().collect()).contains(name.trim().to_lowercase().as_str())
}

// ISO 3166-1 alpha-2 국가코드
const ISO_ALPHA2: &[&str] = &[
    "ad", "ae", "af", "ag", "ai", "al", "am", "ao", "aq", "ar", "as", "at", "au", "aw", "ax", "az", "ba", "bb", "bd", "be", "bf", "bg", "bh", "bi", "bj", "bl",
    "bm", "bn", "bo", "bq", "br", "bs", "bt", "bv", "bw", "by", "bz", "ca", "cc", "cd", "cf", "cg", "ch", "ci", "ck", "cl", "cm", "cn", "co", "cr", "cu", "cv",
    "cw", "cx", "cy", "cz", "de", "dj", "dk", "dm", "do", "dz", "ec", "ee", "eg", "eh", "er", "es", "et", "fi", "fj", "fk", "fm", "fo", "fr", "ga", "gb", "gd",
    "ge", "gf", "gg", "gh", "gi", "gl", "gm", "gn", "gp", "gq", "gr", "gs", "gt", "gu", "gw", "gy", "hk", "hm", "hn", "hr", "ht", "hu", "id", "ie", "il", "im",
    "in", "io", "iq", "ir", "is", "it", "je", "jm", "jo", "jp", "ke", "kg", "kh", "ki", "km", "kn", "kp", "kr", "kw", "ky", "kz", "la", "lb", "lc", "li", "lk",
    "lr", "ls", "lt", "lu", "lv", "ly", "ma", "mc", "md", "me", "mf", "mg", "mh", "mk", "ml", "mm", "mn", "mo", "mp", "mq", "mr", "ms", "mt", "mu", "mv", "mw",
    "mx", "my", "mz", "na", "nc", "ne", "nf", "ng", "ni", "nl", "no", "np", "nr", "nu", "nz", "om", "pa", "pe", "pf", "pg", "ph", "pk", "pl", "pm", "pn", "pr",
    "ps", "pt", "pw", "py", "qa", "re", "ro", "rs", "ru", "rw", "sa", "sb", "sc", "sd", "se", "sg", "sh", "si", "sj", "sk", "sl", "sm", "sn", "so", "sr", "ss",
    "st", "sv", "sx", "sy", "sz", "tc", "td", "tf", "tg", "th", "tj", "tk", "tl", "tm", "tn", "to", "tr", "tt", "tv", "tw", "tz", "ua", "ug", "um", "us", "uy",
    "uz", "va", "vc", "ve", "vg", "vi", "vn", "vu", "wf", "ws", "xk", "ye", "yt", "za", "zm", "zw",
];

// ISO 3166-1 alpha-3 국가코드
const ISO_ALPHA3: &[&str] = &[
    "abw", "afg", "ago", "aia", "ala", "alb", "and", "are", "arg", "arm", "asm", "ata", "atf", "atg", "aus", "aut", "aze", "bdi", "bel", "ben", "bes", "bfa",
    "bgd", "bgr", "bhr", "bhs", "bih", "blm", "blr", "blz", "bmu", "bol", "bra", "brb", "brn", "btn", "bvt", "bwa", "caf", "can", "cck", "che", "chl", "chn",
    "civ", "cmr", "cod", "cog", "cok", "col", "com", "cpv", "cri", "cub", "cuw", "cxr", "cym", "cyp", "cze", "deu", "dji", "dma", "dnk", "dom", "dza", "ecu",
    "egy", "eri", "esh", "esp", "est", "eth", "fin", "fji", "flk", "fra", "fro", "fsm", "gab", "gbr", "geo", "ggy", "gha", "gib", "gin", "glp", "gmb", "gnb",
    "gnq", "grc", "grd", "grl", "gtm", "guf", "gum", "guy", "hkg", "hmd", "hnd", "hrv", "hti", "hun", "idn", "imn", "ind", "iot", "irl", "irn", "irq", "isl",
    "isr", "ita", "jam", "jey", "jor", "jpn", "kaz", "ken", "kgz", "khm", "kir", "kna", "kor", "kwt", "lao", "lbn", "lbr", "lby", "lca", "lie", "lka", "lso",
    "ltu", "lux", "lva", "mac", "maf", "mar", "mco", "mda", "mdg", "mdv", "mex", "mhl", "mkd", "mli", "mlt", "mmr", "mne", "mng", "mnp", "moz", "mrt", "msr",
    "mtq", "mus", "mwi", "mys", "myt", "nam", "ncl", "ner", "nfk", "nga", "nic", "niu", "nld", "nor", "npl", "nru", "nzl", "omn", "pak", "pan", "pcn", "per",
    "phl", "plw", "png", "pol", "pri", "prk", "prt", "pry", "pse", "pyf", "qat", "reu", "rou", "rus", "rwa", "sau", "sdn", "sen", "sgp", "sgs", "shn", "sjm",
    "slb", "sle", "slv", "smr", "som", "spm", "srb", "ssd", "stp", "sur", "svk", "svn", "swe", "swz", "sxm", "syc", "syr", "tca", "tcd", "tgo", "tha", "tjk",
    "tkl", "tkm", "tls", "ton", "tto", "tun", "tur", "tuv", "twn", "tza", "uga", "ukr", "umi", "ury", "usa", "uzb", "vat", "vct", "ven", "vgb", "vir", "vnm",
    "vut", "wlf", "wsm", "xxk", "yem", "zaf", "zmb", "zwe",
];

// ISO 외 기존 사용 코드 (ko: 한국어, en: 영어, uk: 영국/우크라이나어)
const LEGACY: &[&str] = &["ko", "en", "uk"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::lib::doc;
    use crate::cat::lib::export::{SaveOptions, save_doc_as};
    use crate::cat::tests::{new_doc, rect_layer, test_app};

    fn group_with(app: &mut PhotocraftApp, group: &str, inner: &str, x: i32, y: i32) -> photocraft_doc::LayerId {
        rect_layer(app, inner, x, y, 30, 30, "#ff0000");
        layer::group_selected_layers(app, group).unwrap()
    }

    fn visible(app: &PhotocraftApp, name: &str) -> bool {
        layer::top_level_layers(app).unwrap().into_iter().find(|l| l.name == name).unwrap().visible
    }

    #[test]
    fn country_codes_match_trimmed_case_insensitive_iso_and_legacy_codes() {
        assert!(is_country_code(" KR ") && is_country_code("jpn") && is_country_code("uk") && is_country_code("ko"));
        assert!(!is_country_code("xx") && !is_country_code("korea") && !is_country_code(""));
    }

    #[test]
    fn exports_one_jpg_per_top_level_country_group_or_smart_object_and_hides_them() {
        let (mut app, dir) = test_app();
        let out = dir.path().join("e4");
        new_doc(&mut app, "market", 100, 60);
        rect_layer(&mut app, "bg", 0, 0, 100, 60, "#ffffff");
        group_with(&mut app, "kr", "k", 0, 0);
        group_with(&mut app, "JPN", "j", 40, 0);
        group_with(&mut app, "notcode", "n", 0, 30);
        let us = rect_layer(&mut app, "us", 70, 30, 30, 30, "#00ffff");
        layer::convert_to_smart_object(&mut app, us).unwrap();
        // 중첩된 국가코드 그룹, 국가코드 이름의 픽셀 레이어는 대상 아님
        let fr = group_with(&mut app, "fr", "f", 50, 50);
        layer::select_layers(&mut app, &[fr]).unwrap();
        layer::group_selected_layers(&mut app, "wrapper").unwrap();
        rect_layer(&mut app, "de", 90, 0, 5, 5, "#000000");
        save_doc_as(&mut app, &out.join("Shot.psd"), &SaveOptions { adopt_path: true, ..Default::default() }).unwrap();
        let past = app.session.active().unwrap().history.past_len();

        assert_eq!(run(&mut app, None).unwrap(), 3);
        for c in ["kr", "JPN", "us"] {
            assert_eq!(&std::fs::read(out.join(format!("Shot_{c}.jpg"))).unwrap()[..2], &[0xff, 0xd8], "{c}");
            assert!(!visible(&app, c), "{c} 숨김");
        }
        assert!(!out.join("Shot_fr.jpg").exists() && !out.join("Shot_de.jpg").exists());
        assert!(visible(&app, "notcode"));
        assert_eq!(app.session.active().unwrap().history.past_len(), past + 1, "Undo 한 단계");

        // 미저장 문서: 고른 폴더에 문서 이름으로
        new_doc(&mut app, "unsaved", 50, 50);
        group_with(&mut app, "en", "e", 0, 0);
        let picked = dir.path().join("picked");
        assert!(run(&mut app, None).is_err());
        assert_eq!(run(&mut app, Some(&picked)).unwrap(), 1);
        let stem = crate::cat::features::folder_preset::doc_stem(&app).unwrap();
        assert!(picked.join(format!("{stem}_en.jpg")).exists());

        // 국가코드 레이어 없음
        new_doc(&mut app, "none", 50, 50);
        rect_layer(&mut app, "x", 0, 0, 5, 5, "#000000");
        assert_eq!(run(&mut app, Some(&picked)).unwrap_err(), NO_COUNTRY_LAYERS);
        assert!(doc::list_documents(&app).len() == 3);
    }
}
