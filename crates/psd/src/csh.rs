//! Photoshop 사용자 정의 셰이프 파일(`.csh`, Shapes 패널 › Export Selected Shapes) 읽기.
//!
//! ```text
//! file   := "cush" version u32 (1|2) count u32 shape[count] ("8BIM" "phry" len u32 data)?
//! shape  := name (u32 길이 + UTF-16, NUL 포함) pad4, unknown u32, length u32, body[length] pad4
//! body   := [v2: Pascal uuid] bounds (top, left, bottom, right: i32 ×4), path records (26 B 단위)
//! ```
//!
//! 패스 레코드 좌표는 bounds 기준 0..1 비율이다. `phry` 는 브러시 파일과 같은 프리셋 계층
//! (Shapes 패널 폴더)이다.

use crate::abr;
use crate::error::{PsdError, Result};
use crate::path::{PathData, RECORD_LEN};

/// 한 파일에서 읽는 최대 셰이프 수.
pub const MAX_SHAPES: usize = 100_000;
/// 받아들이는 최대 파일 크기.
pub const MAX_FILE_BYTES: usize = 64 << 20;

/// 셰이프 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct CshShape {
    /// 셰이프 이름.
    pub name: String,
    /// v2 의 uuid (v1 은 빈 문자열).
    pub id: String,
    /// 원본 크기(px): top, left, bottom, right.
    pub bounds: [i32; 4],
    /// bounds 기준 0..1 비율 좌표의 패스 레코드.
    pub path: PathData,
}

impl CshShape {
    /// 원본 너비·높이(px, 최소 1).
    pub fn size(&self) -> (u32, u32) {
        let [t, l, b, r] = self.bounds;
        (r.saturating_sub(l).max(1).unsigned_abs(), b.saturating_sub(t).max(1).unsigned_abs())
    }
}

/// 읽은 파일.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CshFile {
    /// 파일 버전 (1 또는 2).
    pub version: u32,
    /// 파일 순서의 셰이프.
    pub shapes: Vec<CshShape>,
    /// [`CshFile::shapes`] 와 같은 순서의 폴더 경로(바깥부터). 계층이 없으면 비어 있다.
    pub folders: Vec<Vec<String>>,
    /// 읽기를 멈추지 않은 문제들.
    pub warnings: Vec<String>,
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(|| PsdError::invalid("custom shape file truncated"))?;
        let s = self.b.get(self.p..end).ok_or_else(|| PsdError::invalid("custom shape file truncated"))?;
        self.p = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        let s: [u8; 4] = self.take(4)?.try_into().map_err(|_| PsdError::invalid("custom shape file truncated"))?;
        Ok(u32::from_be_bytes(s))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    fn align4(&mut self) {
        self.p = (self.p + 3) & !3;
    }
    fn left(&self) -> usize {
        self.b.len().saturating_sub(self.p)
    }
}

fn read_name(r: &mut Rd) -> Result<String> {
    let n = r.u32()? as usize;
    if n > 4096 {
        return Err(PsdError::LimitExceeded("custom shape name length"));
    }
    let units: Vec<u16> = r.take(n * 2)?.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    Ok(String::from_utf16_lossy(&units).trim_end_matches('\0').trim().to_string())
}

fn read_body(body: &[u8], version: u32) -> Result<(String, [i32; 4], PathData)> {
    let mut r = Rd { b: body, p: 0 };
    let id = if version >= 2 {
        let n = usize::from(*r.take(1)?.first().unwrap_or(&0));
        String::from_utf8_lossy(r.take(n)?).to_string()
    } else {
        String::new()
    };
    let bounds = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
    let rest = body.get(r.p..).unwrap_or_default();
    // 끝의 패딩(레코드 길이 미만)은 버린다.
    let path = PathData::from_bytes(rest.get(..rest.len() / RECORD_LEN * RECORD_LEN).unwrap_or_default())?;
    Ok((id, bounds, path))
}

/// `.csh` 파일을 읽는다. 손상된 셰이프는 경고와 함께 건너뛴다.
pub fn parse(data: &[u8]) -> Result<CshFile> {
    let mut r = Rd { b: data, p: 0 };
    if r.take(4)? != b"cush" {
        return Err(PsdError::invalid("not a custom shape file (.csh)"));
    }
    let version = r.u32()?;
    if !matches!(version, 1 | 2) {
        return Err(PsdError::Unsupported(format!("custom shape file version {version}")));
    }
    let count = r.u32()? as usize;
    if count > MAX_SHAPES {
        return Err(PsdError::LimitExceeded("custom shape count"));
    }
    let mut out = CshFile { version, ..Default::default() };
    for i in 0..count {
        let entry = (|| -> Result<(String, &[u8])> {
            let name = read_name(&mut r)?;
            r.align4();
            let _unknown = r.u32()?;
            let len = r.u32()? as usize;
            let body = r.take(len)?;
            r.align4();
            Ok((name, body))
        })();
        let (name, body) = match entry {
            Ok(e) => e,
            Err(e) => {
                out.warnings.push(format!("shape {}: {e}; the rest of the file was skipped", i + 1));
                break;
            }
        };
        match read_body(body, version) {
            Ok((id, bounds, path)) if !path.records.is_empty() => {
                let name = if name.is_empty() { format!("Shape {}", i + 1) } else { name };
                out.shapes.push(CshShape { name, id, bounds, path });
            }
            Ok(_) => out.warnings.push(format!("shape \"{name}\": no path; skipped")),
            Err(e) => out.warnings.push(format!("shape \"{name}\": {e}; skipped")),
        }
    }
    // 선택적 꼬리: 프리셋 계층(폴더).
    while r.left() >= 12 {
        let Ok(sig) = r.take(4) else { break };
        if sig != b"8BIM" {
            break;
        }
        let (Ok(key), Ok(len)) = (r.take(4), r.u32()) else { break };
        let Ok(sec) = r.take((len as usize).min(r.left())) else { break };
        r.align4();
        if key == b"phry"
            && let Some(f) = abr::read_phry(sec, out.shapes.len(), &mut out.warnings)
            && f.iter().any(|f| !f.is_empty())
        {
            out.folders = f;
        }
    }
    if out.shapes.is_empty() {
        let why = out.warnings.first().map(|w| format!(" ({w})")).unwrap_or_default();
        return Err(PsdError::invalid(format!("the custom shape file holds no readable shapes{why}")));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{Descriptor, Value, VersionedDescriptor};

    fn put(o: &mut Vec<u8>, v: u32) {
        o.extend_from_slice(&v.to_be_bytes());
    }

    /// 삼각형 하나(닫힌 하위 패스, 꼭짓점 3개)의 레코드.
    fn triangle() -> Vec<u8> {
        let mut o = Vec::new();
        let rec = |sel: u16, data: [i32; 6], o: &mut Vec<u8>| {
            o.extend_from_slice(&sel.to_be_bytes());
            for v in data {
                o.extend_from_slice(&v.to_be_bytes());
            }
        };
        rec(6, [0; 6], &mut o);
        let mut len = [0i32; 6];
        len[0] = 3 << 16;
        rec(0, len, &mut o);
        let f = |v: f64| (v * f64::from(1 << 24)) as i32;
        for (y, x) in [(0.0, 0.5), (1.0, 1.0), (1.0, 0.0)] {
            rec(2, [f(y), f(x), f(y), f(x), f(y), f(x)], &mut o);
        }
        o
    }

    fn shape(o: &mut Vec<u8>, name: &str, uuid: &str) {
        let units: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        put(o, units.len() as u32);
        for u in units {
            o.extend_from_slice(&u.to_be_bytes());
        }
        while o.len() % 4 != 0 {
            o.push(0);
        }
        put(o, 1);
        let mut body = vec![uuid.len() as u8];
        body.extend_from_slice(uuid.as_bytes());
        for v in [0i32, 0, 40, 80] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        body.extend(triangle());
        body.push(0);
        put(o, body.len() as u32);
        o.extend(body);
        while o.len() % 4 != 0 {
            o.push(0);
        }
    }

    pub(crate) fn sample_file() -> Vec<u8> {
        let mut o = b"cush".to_vec();
        put(&mut o, 2);
        put(&mut o, 2);
        shape(&mut o, "Shape 9", "6fc20484-80bd-144f-b0ab-9e0a949d5d31");
        shape(&mut o, "별", "d1e50580-f46c-564c-8372-7c91382bd42d");
        let h = Descriptor::new("null").with(
            "hierarchy",
            Value::List(vec![
                Value::Descriptor(Descriptor::new("Grup").with("Nm  ", Value::Text(crate::descriptor::UnicodeString::new_nul("Etc")))),
                Value::Descriptor(Descriptor::new("preset")),
                Value::Descriptor(Descriptor::new("preset")),
                Value::Descriptor(Descriptor::new("groupEnd")),
            ]),
        );
        let data = VersionedDescriptor::new(h).to_bytes();
        o.extend_from_slice(b"8BIMphry");
        put(&mut o, data.len() as u32);
        o.extend(data);
        o
    }

    #[test]
    fn reads_names_bounds_paths_and_folders() {
        let f = parse(&sample_file()).unwrap();
        assert_eq!(f.version, 2);
        assert_eq!(f.shapes.len(), 2, "{:?}", f.warnings);
        assert_eq!(f.shapes[0].name, "Shape 9");
        assert_eq!(f.shapes[1].name, "별");
        assert_eq!(f.shapes[0].size(), (80, 40));
        let subs = f.shapes[0].path.subpaths();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].knots.len(), 3);
        assert!(subs[0].closed);
        assert_eq!(f.folders, vec![vec!["Etc".to_string()], vec!["Etc".to_string()]]);
    }

    #[test]
    fn garbage_and_truncation_fail_cleanly() {
        assert!(parse(b"nope").is_err());
        assert!(parse(b"").is_err());
        let full = sample_file();
        for cut in 0..full.len() {
            let _ = parse(&full[..cut]);
        }
    }
}
