//! Import a complete custom-shape set before changing the persistent library.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::{CommandSpec, Group, Session, ShapePreset, always, bad, fit};
use crate::Result;

const CMD: &str = "shape.presets.importCsh";

pub(super) fn spec() -> CommandSpec {
    CommandSpec {
        id: CMD,
        label: "Import Shapes",
        menu: &[],
        shortcut: None,
        params: r#"{"path":str (.csh) | "data":str (base64),"group":str?} → {group,groups,count,names,warnings}; appends uniquely named groups (one per folder of the file), never replaces existing presets"#,
        enabled: always,
        run: import,
        journal: true,
    }
}

/// Parse and convert the complete set before applying any library changes.
/// Group and shape names are resolved against the live library on apply, not the worker's
/// snapshot, so edits made while the job runs do not introduce duplicate addressable names.
/// Conversion fits each path into a 100-by-100 reference box while preserving its aspect ratio.
fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let group = p
        .get("group")
        .map(|v| v.as_str().map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned).ok_or_else(|| bad(CMD, "`group` must be a non-empty string")))
        .transpose()?;
    let params = p.clone();
    crate::jobs::run(
        s,
        "Import Shapes",
        false,
        move |ctx| {
            ctx.check()?;
            let (bytes, stem) = crate::preset_import_cmds::file_bytes_limited(&params, CMD, ".csh file", photocraft_psd::csh::MAX_FILE_BYTES as u64)?;
            // 포크 파서: v1/v2, 손상된 셰이프는 경고로 건너뛰고, `phry` 폴더마다 그룹 하나.
            let file = photocraft_psd::csh::parse(&bytes).map_err(|e| bad(CMD, format!("not a readable custom shape file (.csh): {e}")))?;
            let mut warnings = file.warnings.clone();
            let base = group.unwrap_or_else(|| if stem.trim().is_empty() { "Imported Shapes".into() } else { stem.trim().to_owned() });
            let mut by_folder: Vec<(String, Vec<ShapePreset>)> = Vec::new();
            for (i, shape) in file.shapes.into_iter().enumerate() {
                ctx.check()?;
                let (width, height) = shape.size();
                let path = photocraft_io::vector_map::path_from_records(&shape.path, width, height);
                if path.is_empty() {
                    warnings.push(format!("shape \"{}\": empty path; skipped", shape.name));
                    continue;
                }
                let path = fit(&path, [0.0, 0.0, 100.0, 100.0], true);
                // Photoshop stores some preset names as `$$$/…/Key=Shown`. The Shapes panel
                // renders the stored name, so keep the text after `=`.
                let shown = photocraft_doc::pattern::display_name(&shape.name);
                let name = if shown.trim().is_empty() { "Shape".to_owned() } else { shown.to_owned() };
                // Keep opaque IDs unchanged. An anonymous preset remains usable by name;
                // repeated IDs are checked against geometry when a filter resolves them.
                let id = (!shape.id.is_empty()).then_some(shape.id);
                // 폴더가 있으면 폴더 경로만 그룹 이름으로 쓴다 (파일명 접두사는 목록만 길게 만든다).
                let folder = file.folders.get(i).filter(|f| !f.is_empty()).map_or_else(|| base.clone(), |f| f.join(" / "));
                let item = ShapePreset { name, path, id };
                match by_folder.iter_mut().find(|(k, _)| *k == folder) {
                    Some((_, items)) => items.push(item),
                    None => by_folder.push((folder, vec![item])),
                }
            }
            if by_folder.is_empty() {
                return Err(bad(CMD, "the CSH set contains no shapes"));
            }
            Ok((by_folder, warnings))
        },
        |s, (by_folder, warnings)| {
            let mut taken: HashSet<String> = s.presets.shapes.iter().flat_map(|g| g.items.iter().map(|i| i.name.clone())).collect();
            taken.extend(s.edit_state.custom_shapes.iter().map(|c| c.name.clone()));
            let mut suffixes: HashMap<String, usize> = HashMap::new();
            let (mut groups, mut names) = (Vec::new(), Vec::new());
            for (group, mut items) in by_folder {
                let group = super::super::gradients::unique_group(&s.presets.shapes, &group);
                for item in &mut items {
                    let base = item.name.clone();
                    let suffix = suffixes.entry(base.clone()).or_insert(2);
                    while !taken.insert(item.name.clone()) {
                        item.name = format!("{base} {suffix}");
                        *suffix += 1;
                    }
                    names.push(item.name.clone());
                }
                s.presets.shapes.push(Group::new(&group, items));
                groups.push(group);
            }
            let result = json!({"group": groups.first(), "groups": groups, "count": names.len(), "names": names, "warnings": warnings});
            s.presets_changed();
            Ok(result)
        },
    )
}
