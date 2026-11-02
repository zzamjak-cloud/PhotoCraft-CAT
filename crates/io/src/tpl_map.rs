//! Photoshop 툴 프리셋(`.tpl`) → 브러시 프리셋 + 툴 프리셋.
//!
//! [`photocraft_psd::abr::tpl`] 이 파일을 읽고, 여기서 프리셋마다 PhotoCraft 툴과 옵션을 정한다.
//! 브러시 계열 툴(브러시·연필·지우개·닷지 …)의 브러시는 디스크립터를 브러시 파일의
//! `brushPreset` 모양으로 바꿔 [`crate::abr_map`] 으로 매핑한다(팁·다이내믹스·텍스처를 그대로
//! 재사용). 그 밖의 툴은 옵션 바 값 중 대응하는 것만 `toolOptions`(셸의 `ToolOptions` 필드
//! 이름)로 옮긴다. 같은 이름·툴·설정의 중복 프리셋은 하나만 남긴다.

use std::collections::HashSet;

use photocraft_paint::{BrushPreset, BrushSettings};
use photocraft_psd::abr::tpl::{self, TplPreset};
use photocraft_psd::descriptor::{Descriptor, Id, UnicodeString, Value};
use serde_json::{Map, Value as Json, json};

use crate::abr_map::{KNOWN, boolean, enum_value, num, obj, text};

/// PhotoCraft 툴 프리셋 하나가 될 내용.
#[derive(Clone, Debug, Default)]
pub struct TplTool {
    pub name: String,
    /// 셸의 툴 이름 (`brush`, `eraser`, `paintBucket`, …).
    pub tool: String,
    /// [`TplImport::brushes`] 의 인덱스: 이 프리셋의 브러시.
    pub brush: Option<usize>,
    /// 셸 `ToolOptions` 필드 이름 → 값.
    pub tool_options: Map<String, Json>,
    /// 페인트통 패턴: 가져온 패턴의 id 또는 라이브러리에서 찾을 이름.
    pub pattern: Option<String>,
}

/// 읽은 결과.
#[derive(Clone, Debug, Default)]
pub struct TplImport {
    /// 브러시 계열 프리셋의 브러시 (group 은 호출자가 정한다).
    pub brushes: Vec<BrushPreset>,
    pub tools: Vec<TplTool>,
    /// 파일에 들어 있던 패턴 (페인트통·브러시 텍스처가 참조).
    pub patterns: Vec<photocraft_doc::Pattern>,
    /// 툴이 없거나 옵션을 옮기지 못한 것 등.
    pub warnings: Vec<String>,
    /// 건너뛴 중복 프리셋 수.
    pub duplicates: usize,
}

/// Photoshop 툴 클래스 → (셸 툴 이름, 브러시 계열 여부, 브러시 블렌드 모드를 쓰는지).
fn shell_tool(class: &str) -> Option<(&'static str, bool, bool)> {
    Some(match class {
        "PbTl" | "paintbrushTool" => ("brush", true, true),
        "PcTl" | "pencilTool" => ("pencil", true, true),
        "ErTl" | "eraserTool" => ("eraser", true, false),
        "MixB" | "wetBrushTool" => ("mixerBrush", true, false),
        "SETl" | "backgroundEraserTool" => ("backgroundEraser", true, false),
        "colorReplacementBrushTool" | "ClRT" => ("colorReplacement", true, false),
        "DdTl" | "dodgeTool" => ("dodge", true, false),
        "BrTl" | "burnInTool" => ("burn", true, false),
        "SrTl" | "saturationTool" => ("sponge", true, false),
        "BlTl" | "blurTool" => ("blur", true, false),
        "ShTl" | "sharpenTool" => ("sharpen", true, false),
        "SmTl" | "smudgeTool" => ("smudge", true, false),
        "HBTl" | "historyBrushTool" => ("historyBrush", true, true),
        "ABTl" | "artBrushTool" => ("artHistoryBrush", true, true),
        "magicStampTool" => ("healing", true, false),
        "spotHealingBrushTool" => ("spotHealing", true, false),
        "ClTl" | "cloneStampTool" => ("cloneStamp", true, true),
        "PaTl" | "patternStampTool" => ("patternStamp", true, true),
        "GrTl" | "gradientTool" => ("gradient", false, false),
        "bucketTool" => ("paintBucket", false, false),
        "cropTool" => ("crop", false, false),
        "perspectiveCropTool" => ("perspectiveCrop", false, false),
        "magneticLassoTool" => ("magneticLasso", false, false),
        "lassoTool" => ("lasso", false, false),
        "polySelTool" => ("polygonLasso", false, false),
        "marqueeRectTool" => ("rectMarquee", false, false),
        "marqueeEllipTool" => ("ellipseMarquee", false, false),
        "magicWandTool" => ("magicWand", false, false),
        "typeCreateOrEditTool" => ("type", false, false),
        "typeVerticalCreateOrEditTool" => ("verticalType", false, false),
        "penTool" => ("pen", false, false),
        "freeformPenTool" => ("freeformPen", false, false),
        "rectangleTool" | "roundedRectangleTool" => ("rectangle", false, false),
        "ellipseTool" => ("ellipseShape", false, false),
        "polygonTool" => ("polygon", false, false),
        "lineTool" => ("line", false, false),
        "customShapeTool" => ("customShape", false, false),
        _ => return None,
    })
}

/// Brushes 패널에서 툴별로 묶는 폴더 이름.
fn folder_label(tool: &str) -> String {
    let mut out = String::new();
    for (i, c) in tool.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else if c.is_ascii_uppercase() {
            out.push(' ');
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

/// 툴 프리셋 디스크립터 → 브러시 파일의 `brushPreset`: 브러시 키는 그대로, 툴 옵션 키는
/// `toolOptions` 아래로 (블렌드 모드는 호출자가 따로 적용).
fn brush_descriptor(p: &TplPreset) -> Descriptor {
    let mut d = Descriptor::new("brushPreset").with("Nm  ", Value::Text(UnicodeString::new_nul(&p.name)));
    let mut to = Descriptor::new("null");
    for (k, v) in &p.descriptor.items {
        let key = String::from_utf8_lossy(k.as_bytes());
        if KNOWN.contains(&key.as_ref()) {
            d.items.push((k.clone(), v.clone()));
        } else if !k.is("Md  ") {
            to.items.push((k.clone(), v.clone()));
        }
    }
    d.items.push((Id::new("toolOptions"), Value::Descriptor(to)));
    d
}

fn blend(d: &Descriptor) -> Option<photocraft_color::BlendMode> {
    enum_value(d, "Md  ").and_then(|m| crate::effects_map::blend_from_id(m.as_bytes()))
}

fn set(o: &mut Map<String, Json>, k: &str, v: Option<Json>) {
    if let Some(v) = v {
        o.insert(k.to_string(), v);
    }
}

/// Photoshop 길이 단위 코드 → 셸 단위 표기.
fn length_unit(code: f64) -> &'static str {
    match code as i32 {
        1 => "in",
        2 => "cm",
        3 => "mm",
        4 => "pt",
        5 => "pica",
        _ => "px",
    }
}

/// 브러시 계열이 아닌 툴의 옵션 바 값. 옮긴 것이 하나도 없으면 false.
fn other_options(tool: &str, d: &Descriptor, t: &mut TplTool, imported: &[photocraft_doc::Pattern]) -> bool {
    let o = &mut t.tool_options;
    match tool {
        "paintBucket" => {
            set(o, "tolerance", num(d, "BckT").map(|v| json!(v.clamp(0.0, 255.0))));
            set(o, "anti_alias", boolean(d, "BckA").map(Json::Bool));
            set(o, "contiguous", boolean(d, "Cntg").map(Json::Bool));
            set(o, "fill_opacity", num(d, "Opct").map(|v| json!(v.clamp(0.0, 100.0))));
            set(o, "bucket_blend_mode", blend(d).and_then(|m| serde_json::to_value(m).ok()));
            let pattern = obj(d, "Ptrn");
            set(o, "bucket_fill_pattern", boolean(d, "BckF").or(pattern.map(|_| true)).map(Json::Bool));
            if let Some(p) = pattern {
                let id = text(p, "Idnt").unwrap_or_default();
                t.pattern = if imported.iter().any(|q| q.id == id) { Some(id) } else { text(p, "Nm  ").map(|n| display(&n)) };
            }
            true
        }
        "gradient" => {
            let style = num(d, "GrdT").map(|v| match v as i32 {
                1 => "radial",
                2 => "angle",
                3 => "reflected",
                4 => "diamond",
                _ => "linear",
            });
            set(o, "gradient_style", style.map(|s| json!(s)));
            set(o, "gradient_reverse", num(d, "GrdR").map(|v| json!(v != 0.0)).or(boolean(d, "GrdR").map(Json::Bool)));
            set(o, "gradient_dither", num(d, "GrdD").map(|v| json!(v != 0.0)).or(boolean(d, "GrdD").map(Json::Bool)));
            set(o, "gradient_blend_mode", blend(d).and_then(|m| serde_json::to_value(m).ok()));
            true
        }
        "crop" | "perspectiveCrop" => {
            let Some(c) = obj(d, "CrpO") else { return false };
            let (w, h) = (num(c, "CrWV").unwrap_or(0.0), num(c, "CrHV").unwrap_or(0.0));
            if w <= 0.0 || h <= 0.0 {
                return false;
            }
            let fmt = |v: f64| {
                let s = format!("{:.3}", v);
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            };
            o.insert("crop_ratio".into(), json!("whr"));
            o.insert("crop_width".into(), json!(format!("{} {}", fmt(w), length_unit(num(c, "CrWS").unwrap_or(0.0)))));
            o.insert("crop_height".into(), json!(format!("{} {}", fmt(h), length_unit(num(c, "CrHS").unwrap_or(0.0)))));
            // 해상도는 16.16 고정소수점.
            let res = num(c, "CrRV").map(|v| v / 65536.0).filter(|v| *v > 0.0);
            o.insert("crop_resolution".into(), json!(res.map(fmt).unwrap_or_default()));
            o.insert("crop_resolution_unit".into(), json!(if num(c, "CrRS") == Some(2.0) { "px/cm" } else { "px/in" }));
            true
        }
        "magneticLasso" => {
            set(o, "magnetic_width", num(d, "MgWd").map(|v| json!(v.clamp(1.0, 256.0))));
            set(o, "magnetic_contrast", num(d, "MgEd").map(|v| json!(v.clamp(1.0, 100.0))));
            set(o, "anti_alias", boolean(d, "DrwA").map(Json::Bool));
            !o.is_empty()
        }
        _ => false,
    }
}

/// 로컬라이즈 키(`$$$/…=이름`)를 뗀 이름.
fn display(raw: &str) -> String {
    let raw = raw.trim_end_matches('\0');
    if raw.starts_with("$$$/") { raw.split_once('=').map_or(raw, |(_, n)| n).trim().to_string() } else { raw.trim().to_string() }
}

/// `.tpl` 을 읽어 매핑한다. `ctl` 은 브러시마다 확인한다(취소되면 "cancelled").
pub fn read_tpl_with(bytes: &[u8], ctl: &photocraft_raster::Interrupt) -> Result<TplImport, String> {
    let f = tpl::parse(bytes).map_err(|e| format!("not a readable tool preset file (.tpl): {e}"))?;
    ctl.progress(0.1);
    let mut out = TplImport { warnings: f.warnings.clone(), ..Default::default() };
    for p in &f.patterns {
        match crate::pattern_map::try_from_psd(p) {
            Ok(Some(pat)) => out.patterns.push(pat),
            Ok(None) => out.warnings.push(format!("pattern \"{}\" could not be read; skipped", p.name)),
            Err(e) => out.warnings.push(format!("pattern \"{}\": {e}; skipped", p.name)),
        }
    }
    // 중복 제거 (Photoshop 이 같은 기본 프리셋을 여러 번 저장해 둔 파일이 흔하다).
    let mut seen = HashSet::new();
    let mut presets: Vec<&TplPreset> = Vec::new();
    for p in &f.presets {
        if seen.insert((p.name.clone(), p.tool.clone(), p.descriptor.to_bytes())) {
            presets.push(p);
        } else {
            out.duplicates += 1;
        }
    }
    // 브러시 계열 프리셋의 브러시를 한 번에 매핑 (팁 변환 캐시 공유).
    let brushy: Vec<usize> =
        presets.iter().enumerate().filter(|(_, p)| shell_tool(&p.tool).is_some_and(|t| t.1) && obj(&p.descriptor, "Brsh").is_some()).map(|(i, _)| i).collect();
    let file = f.brush_file(brushy.iter().filter_map(|&i| presets.get(i)).map(|p| brush_descriptor(p)).collect());
    let cancel = || ctl.cancelled();
    let progress = |p: f32| ctl.progress(0.1 + 0.9 * p);
    let (mapped, warnings) = crate::abr_map::map_each_with(&file, &photocraft_raster::Interrupt::new(&cancel, &progress)).map_err(|e| e.to_string())?;
    out.warnings.extend(warnings);
    let mut brush_of: Vec<Option<BrushSettings>> = vec![None; presets.len()];
    for (&i, b) in brushy.iter().zip(mapped) {
        if let Some(slot) = brush_of.get_mut(i) {
            *slot = b;
        }
    }
    let mut unknown_tools = std::collections::BTreeSet::new();
    let mut tool_only = std::collections::BTreeSet::new();
    for (i, p) in presets.iter().enumerate() {
        let Some((tool, is_brushy, uses_mode)) = shell_tool(&p.tool) else {
            unknown_tools.insert(p.tool.clone());
            continue;
        };
        let name = if p.name.trim().is_empty() { folder_label(tool) } else { p.name.trim().to_string() };
        let mut t = TplTool { name: name.clone(), tool: tool.to_string(), ..Default::default() };
        if is_brushy {
            match brush_of.get_mut(i).and_then(Option::take) {
                Some(mut b) => {
                    if uses_mode && let Some(m) = blend(&p.descriptor) {
                        b.mode = m;
                    }
                    t.brush = Some(out.brushes.len());
                    out.brushes.push(BrushPreset {
                        name,
                        brush: b,
                        builtin: false,
                        group: String::new(),
                        folder: vec![folder_label(tool)],
                        captures: Default::default(),
                    });
                }
                None => {
                    out.warnings.push(format!("\"{}\": its brush could not be read; the preset only selects the tool", p.name));
                }
            }
        } else if !other_options(tool, &p.descriptor, &mut t, &out.patterns) {
            tool_only.insert(folder_label(tool));
        }
        out.tools.push(t);
    }
    if !unknown_tools.is_empty() {
        out.warnings.push(format!("presets of tools PhotoCraft does not have were skipped: {}", unknown_tools.into_iter().collect::<Vec<_>>().join(", ")));
    }
    if !tool_only.is_empty() {
        out.warnings.push(format!("these presets only select their tool (their options have no equivalent): {}", tool_only.into_iter().collect::<Vec<_>>().join(", ")));
    }
    ctl.progress(1.0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abr_map::pct;

    fn pct_v(v: f64) -> Value {
        Value::UnitFloat { unit: *b"#Prc", value: v }
    }

    fn preset(name: &str, d: Descriptor) -> TplPreset {
        TplPreset { name: name.into(), tool: String::from_utf8_lossy(d.class_id.as_bytes()).to_string(), descriptor: d }
    }

    #[test]
    fn brush_keys_stay_and_tool_options_move_under_tool_options() {
        let d = Descriptor::new("PbTl")
            .with("Opct", Value::Integer(80))
            .with("Md  ", Value::Enumerated { type_id: Id::new("BlnM"), value: Id::new("Mltp") })
            .with("Brsh", Value::Descriptor(Descriptor::new("computedBrush").with("Hrdn", pct_v(50.0))))
            .with("Wtdg", Value::Boolean(false));
        let b = brush_descriptor(&preset("x", d));
        assert!(b.get("Brsh").is_some() && b.get("Wtdg").is_some());
        assert!(b.get("Opct").is_none() && b.get("Md  ").is_none());
        let to = obj(&b, "toolOptions").unwrap();
        assert_eq!(pct(to, "Opct"), Some(0.8));
    }

    #[test]
    fn crop_presets_become_width_height_resolution() {
        let c = Descriptor::new("CrOC")
            .with("CrWV", Value::Double(4.0))
            .with("CrWS", Value::Integer(1))
            .with("CrHV", Value::Double(6.0))
            .with("CrHS", Value::Integer(1))
            .with("CrRV", Value::Integer(300 * 65536))
            .with("CrRS", Value::Integer(1));
        let d = Descriptor::new("cropTool").with("CrpO", Value::Descriptor(c));
        let mut t = TplTool::default();
        assert!(other_options("crop", &d, &mut t, &[]));
        assert_eq!(t.tool_options["crop_ratio"], "whr");
        assert_eq!(t.tool_options["crop_width"], "4 in");
        assert_eq!(t.tool_options["crop_height"], "6 in");
        assert_eq!(t.tool_options["crop_resolution"], "300");
    }

    #[test]
    fn bucket_presets_keep_fill_settings_and_the_pattern() {
        let pat = Descriptor::new("Ptrn").with("Nm  ", Value::Text(UnicodeString::new_nul("벽지"))).with("Idnt", Value::Text(UnicodeString::new_nul("abc")));
        let d = Descriptor::new("bucketTool")
            .with("Opct", Value::Integer(70))
            .with("BckT", Value::Integer(32))
            .with("BckA", Value::Boolean(true))
            .with("Cntg", Value::Boolean(false))
            .with("BckF", Value::Boolean(true))
            .with("Ptrn", Value::Descriptor(pat));
        let mut t = TplTool::default();
        assert!(other_options("paintBucket", &d, &mut t, &[]));
        assert_eq!(t.tool_options["fill_opacity"], 70.0);
        assert_eq!(t.tool_options["contiguous"], false);
        assert_eq!(t.tool_options["bucket_fill_pattern"], true);
        assert_eq!(t.pattern.as_deref(), Some("벽지"), "not in the file: looked up by name");
    }

    #[test]
    fn folder_labels_read_like_tool_names() {
        assert_eq!(folder_label("brush"), "Brush");
        assert_eq!(folder_label("artHistoryBrush"), "Art History Brush");
    }
}
