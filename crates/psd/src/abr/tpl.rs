//! Photoshop 툴 프리셋 파일(`.tpl`, Tool Presets 패널 › Save Tool Presets) 읽기.
//!
//! ```text
//! file    := "8BTP" version u32 (3) sections u32, section*
//! section := "8BIM" key[4] length u32 data[length] (4바이트 패딩)
//! "tppa"  := 브러시 파일의 `patt` 와 같은 패턴 블록 (페인트통·텍스처가 참조)
//! "tpbd"  := 브러시 파일의 `samp` 와 같은 샘플 팁 (서브버전 1 또는 2 레이아웃)
//! "tptp"  := count u32, (name: u32 길이 + UTF-16, version u32 (16), ActionDescriptor)*
//! "tpsh", "tpst" := 셰이프·스타일 (셰이프 툴 프리셋용, 읽지 않음)
//! ```
//!
//! 프리셋 디스크립터의 클래스가 툴(`PbTl` 브러시, `ErTl` 지우개, `cropTool` …)이고, 브러시
//! 계열은 브러시 파일의 `brushPreset` 키(`Brsh`, `szVr`, …)에 툴 옵션(`Opct`, `flow`, …)이
//! 같은 수준에 섞여 있다.

use super::{AbrFile, AbrSample, Budget, MAX_TOTAL_BYTES, Rd, read_samp};
use crate::descriptor::{Descriptor, VersionedDescriptor};
use crate::error::{PsdError, Result};
use crate::patterns::{PatternBudget, PsdPattern, parse_pattern_block_with};

/// 한 파일에서 읽는 최대 프리셋 수.
pub const MAX_PRESETS: usize = 20_000;

/// 툴 프리셋 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct TplPreset {
    /// 표시 이름 (`$$$/Presets/Tools/…=이름` 의 `이름`).
    pub name: String,
    /// 툴 클래스 (`PbTl`, `cropTool`, …).
    pub tool: String,
    /// 프리셋 디스크립터 (클래스 = 툴).
    pub descriptor: Descriptor,
}

/// 읽은 파일.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TplFile {
    /// 파일 버전 (3).
    pub version: u32,
    /// 파일 순서의 프리셋.
    pub presets: Vec<TplPreset>,
    /// `tpbd` 샘플 팁.
    pub samples: Vec<AbrSample>,
    /// `tppa` 패턴.
    pub patterns: Vec<PsdPattern>,
    /// 읽기를 멈추지 않은 문제들.
    pub warnings: Vec<String>,
}

impl TplFile {
    /// 샘플 팁과 패턴을 담은 브러시 파일 형태(프리셋 없음): 브러시 매퍼가 팁·텍스처를 찾는 데 쓴다.
    pub fn brush_file(&self, presets: Vec<Descriptor>) -> AbrFile {
        AbrFile { version: 6, subversion: 1, samples: self.samples.clone(), patterns: self.patterns.clone(), presets, ..Default::default() }
    }
}

/// `$$$/Presets/Tools/HealingBrush21pixels=Healing Brush 21 pixels` → `Healing Brush 21 pixels`.
fn display_name(raw: &str) -> String {
    let raw = raw.trim_end_matches('\0');
    let name = if raw.starts_with("$$$/") { raw.split_once('=').map_or(raw, |(_, n)| n) } else { raw };
    name.trim().to_string()
}

fn read_tptp(data: &[u8], out: &mut TplFile) {
    let mut r = Rd::new(data);
    let Ok(count) = r.u32() else {
        out.warnings.push("tool presets (tptp) truncated".into());
        return;
    };
    for i in 0..count as usize {
        if out.presets.len() >= MAX_PRESETS {
            out.warnings.push("too many tool presets; the rest were skipped".into());
            break;
        }
        let name = match super::read_ucs2(&mut r) {
            Ok(n) => display_name(&n),
            Err(e) => {
                out.warnings.push(format!("tool preset {}: {e}; the rest were skipped", i + 1));
                break;
            }
        };
        let rest = r.b.get(r.p..).unwrap_or_default();
        match VersionedDescriptor::parse_prefix(rest) {
            Ok((vd, used)) => {
                r.p += used;
                let tool = String::from_utf8_lossy(vd.descriptor.class_id.as_bytes()).to_string();
                out.presets.push(TplPreset { name, tool, descriptor: vd.descriptor });
            }
            Err(e) => {
                out.warnings.push(format!("tool preset \"{name}\": {e}; the rest were skipped"));
                break;
            }
        }
    }
}

/// `.tpl` 파일을 읽는다. 손상된 섹션·프리셋은 경고와 함께 건너뛴다.
pub fn parse(data: &[u8]) -> Result<TplFile> {
    let mut r = Rd::new(data);
    if r.take(4)? != b"8BTP" {
        return Err(PsdError::invalid("not a tool preset file (.tpl)"));
    }
    let version = r.u32()?;
    if version > 16 {
        return Err(PsdError::Unsupported(format!("tool preset file version {version}")));
    }
    let _sections = r.u32()?;
    let mut out = TplFile { version, ..Default::default() };
    let mut budget = Budget(MAX_TOTAL_BYTES);
    let mut patterns = PatternBudget::default();
    while r.left() >= 12 {
        let sig = r.arr::<4>()?;
        if &sig != b"8BIM" {
            out.warnings.push(format!("unexpected data at offset {} ignored", r.p - 4));
            break;
        }
        let key = r.arr::<4>()?;
        let len = r.u32()? as usize;
        let data = match r.take(len) {
            Ok(d) => d,
            Err(_) => {
                out.warnings.push(format!("section {} truncated", String::from_utf8_lossy(&key)));
                r.take(r.left())?
            }
        };
        let pad = (4 - len % 4) % 4;
        if pad > 0 && r.b.get(r.p..r.p + 4) != Some(b"8BIM") && r.b.get(r.p + pad..r.p + pad + 4) == Some(b"8BIM") {
            r.p += pad;
        }
        match &key {
            b"tpbd" => {
                // 샘플 머리말 길이(10 또는 264)는 파일마다 다르다: 서브버전 1 로 읽고 안 되면 2.
                let mut tips = AbrFile::default();
                let mut b1 = Budget(budget.0);
                read_samp(data, 1, &mut tips, &mut b1)?;
                if tips.samples.is_empty() && !tips.warnings.is_empty() {
                    let mut retry = AbrFile::default();
                    let mut b2 = Budget(budget.0);
                    read_samp(data, 2, &mut retry, &mut b2)?;
                    if !retry.samples.is_empty() {
                        (tips, b1) = (retry, b2);
                    }
                }
                budget = b1;
                out.samples.extend(tips.samples);
                out.warnings.extend(tips.warnings);
            }
            b"tppa" => {
                let limit = u64::try_from(budget.0).unwrap_or(u64::MAX);
                patterns.cap_decoded_to(limit);
                let left = patterns.decoded_left();
                let res = parse_pattern_block_with(data, &mut patterns);
                let spent = usize::try_from(left.saturating_sub(patterns.decoded_left())).unwrap_or(usize::MAX);
                budget.spend(spent)?;
                match res {
                    Ok(p) => out.patterns.extend(p),
                    Err(e) => out.warnings.push(format!("embedded patterns unreadable: {e}")),
                }
            }
            b"tptp" => read_tptp(data, &mut out),
            // 셰이프·스타일 툴 프리셋의 부속 데이터: 쓰지 않는다.
            b"tpsh" | b"tpst" => {}
            k => out.warnings.push(format!("section {} ignored", String::from_utf8_lossy(k))),
        }
    }
    if out.presets.is_empty() {
        let why = out.warnings.first().map(|w| format!(" ({w})")).unwrap_or_default();
        return Err(PsdError::invalid(format!("the tool preset file holds no readable presets{why}")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{Id, UnicodeString, Value};

    fn put(o: &mut Vec<u8>, v: u32) {
        o.extend_from_slice(&v.to_be_bytes());
    }

    fn section(o: &mut Vec<u8>, key: &[u8; 4], data: &[u8]) {
        o.extend_from_slice(b"8BIM");
        o.extend_from_slice(key);
        put(o, data.len() as u32);
        o.extend_from_slice(data);
        while o.len() % 4 != 0 {
            o.push(0);
        }
    }

    fn preset(o: &mut Vec<u8>, name: &str, d: Descriptor) {
        let units: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        put(o, units.len() as u32);
        for u in units {
            o.extend_from_slice(&u.to_be_bytes());
        }
        o.extend(VersionedDescriptor::new(d).to_bytes());
    }

    /// 8×8 샘플 팁 하나(서브버전 1 레이아웃, 비압축).
    fn tip(uuid: &str) -> Vec<u8> {
        let mut body = vec![uuid.len() as u8];
        body.extend_from_slice(uuid.as_bytes());
        body.extend_from_slice(&[0u8; 10]);
        for v in [0i32, 0, 8, 8] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(&8u16.to_be_bytes());
        body.push(0);
        body.extend(std::iter::repeat_n(200u8, 64));
        let mut o = Vec::new();
        put(&mut o, body.len() as u32);
        o.extend(body);
        while o.len() % 4 != 0 {
            o.push(0);
        }
        o
    }

    pub(crate) fn sample_file() -> Vec<u8> {
        let pct = |v: f64| Value::UnitFloat { unit: *b"#Prc", value: v };
        let px = |v: f64| Value::UnitFloat { unit: *b"#Pxl", value: v };
        let brush = Descriptor::new("PbTl")
            .with("Opct", Value::Integer(80))
            .with("flow", Value::Integer(50))
            .with("Md  ", Value::Enumerated { type_id: Id::new("BlnM"), value: Id::new("Mltp") })
            .with(
                "Brsh",
                Value::Descriptor(
                    Descriptor::new("sampledBrush")
                        .with("Dmtr", px(40.0))
                        .with("Spcn", pct(25.0))
                        .with("sampledData", Value::Text(UnicodeString::new_nul("tip-1"))),
                ),
            );
        let crop = Descriptor::new("cropTool");
        let mut tptp = Vec::new();
        put(&mut tptp, 2);
        preset(&mut tptp, "$$$/Presets/Tools/Brush=기본 브러시", brush);
        preset(&mut tptp, "Crop", crop);
        let mut o = b"8BTP".to_vec();
        put(&mut o, 3);
        put(&mut o, 2);
        section(&mut o, b"tpbd", &tip("tip-1"));
        section(&mut o, b"tptp", &tptp);
        o
    }

    #[test]
    fn reads_presets_and_tips() {
        let f = parse(&sample_file()).unwrap();
        assert_eq!(f.presets.len(), 2, "{:?}", f.warnings);
        assert_eq!(f.presets[0].name, "기본 브러시");
        assert_eq!(f.presets[0].tool, "PbTl");
        assert_eq!(f.presets[1].tool, "cropTool");
        assert_eq!(f.samples.len(), 1);
        assert_eq!(f.samples[0].id, "tip-1");
        assert_eq!((f.samples[0].width, f.samples[0].height), (8, 8));
    }

    #[test]
    fn names_drop_the_localisation_key() {
        assert_eq!(display_name("$$$/Presets/Tools/HealingBrush21pixels=Healing Brush 21 pixels\0"), "Healing Brush 21 pixels");
        assert_eq!(display_name("이펙트용(갈필1)"), "이펙트용(갈필1)");
    }

    #[test]
    fn garbage_and_truncation_fail_cleanly() {
        assert!(parse(b"8BTP").is_err());
        assert!(parse(b"cush\0\0\0\x02").is_err());
        let full = sample_file();
        for cut in 0..full.len() {
            let _ = parse(&full[..cut]);
        }
    }
}
