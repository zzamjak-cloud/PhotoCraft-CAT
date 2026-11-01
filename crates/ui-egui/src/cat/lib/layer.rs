//! 레이어 조회/선택/이름/복제/삭제/그룹/이동 (CAT lib_layer.js 대응).
//! 조회는 문서 모델을 직접 읽고, 변경은 엔진 명령(Undo 가능)으로 한다.

use photocraft_doc::{Layer, LayerContent, LayerId, SmartSource};
use serde_json::{Value, json};

use super::core::{CatResult, exec, try_exec};
use crate::PhotocraftApp;

/// 레이어 bounds (문서 픽셀, 효과 포함 아님)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Bounds {
    pub fn left(&self) -> i32 {
        self.x
    }
    pub fn top(&self) -> i32 {
        self.y
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
}

/// 레이어 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Pixel,
    Group,
    Adjustment,
    Fill,
    Type,
    Shape,
    SmartObject,
}

/// 스마트 오브젝트 원본 정보
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartInfo {
    /// 링크(외부 파일) 여부
    pub linked: bool,
    /// 링크 경로 (임베디드면 None)
    pub path: Option<String>,
    /// 원본 파일명 (임베디드 파일명 또는 링크 경로의 파일명)
    pub file_name: String,
}

/// 레이어 요약 (위→아래 평탄 목록의 한 행)
#[derive(Clone, Debug, PartialEq)]
pub struct LayerInfo {
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    /// 0..=1
    pub opacity: f32,
    /// 0..=1
    pub fill_opacity: f32,
    /// 픽셀 bounds (내용 없으면 None)
    pub bounds: Option<Bounds>,
    /// 어떤 잠금이든 걸려 있음 (투명/픽셀/위치/아트보드/전체)
    pub locked: bool,
    /// 전체 잠금
    pub locked_all: bool,
    /// 0 = 최상위
    pub depth: usize,
    pub parent: Option<LayerId>,
    pub selected: bool,
    pub active: bool,
    pub has_effects: bool,
    pub has_mask: bool,
    pub smart: Option<SmartInfo>,
}

impl LayerInfo {
    pub fn is_group(&self) -> bool {
        self.kind == LayerKind::Group
    }
    pub fn is_smart_object(&self) -> bool {
        self.kind == LayerKind::SmartObject
    }
}

fn kind_of(l: &Layer) -> LayerKind {
    match &l.content {
        LayerContent::Raster(_) => LayerKind::Pixel,
        LayerContent::Group(_) => LayerKind::Group,
        LayerContent::Adjustment(_) => LayerKind::Adjustment,
        LayerContent::Fill(_) => LayerKind::Fill,
        LayerContent::Text(_) => LayerKind::Type,
        LayerContent::Shape(_) => LayerKind::Shape,
        LayerContent::Smart(_) => LayerKind::SmartObject,
    }
}

fn smart_of(l: &Layer) -> Option<SmartInfo> {
    let LayerContent::Smart(s) = &l.content else { return None };
    Some(match &s.source {
        SmartSource::Embedded { file_name, .. } => SmartInfo { linked: false, path: None, file_name: file_name.clone() },
        SmartSource::Linked { path } => SmartInfo {
            linked: true,
            path: Some(path.clone()),
            file_name: std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        },
    })
}

fn row(l: &Layer, depth: usize, parent: Option<LayerId>, selected: &[LayerId], active: Option<LayerId>) -> LayerInfo {
    let bounds = photocraft_engine::layer_multi_cmds::layer_bounds(l).filter(|r| !r.is_empty()).map(|r| Bounds {
        x: r.x0,
        y: r.y0,
        w: r.width() as i32,
        h: r.height() as i32,
    });
    let k = &l.locks;
    LayerInfo {
        id: l.id,
        name: l.name.clone(),
        kind: kind_of(l),
        visible: l.visible,
        opacity: l.opacity,
        fill_opacity: l.fill_opacity,
        bounds,
        locked: k.all || k.pixels || k.position || k.transparency || k.artboard,
        locked_all: k.all,
        depth,
        parent,
        selected: selected.contains(&l.id),
        active: active == Some(l.id),
        has_effects: !l.effects.items.is_empty(),
        has_mask: l.mask.is_some(),
        smart: smart_of(l),
    }
}

fn walk(layers: &[Layer], depth: usize, parent: Option<LayerId>, selected: &[LayerId], active: Option<LayerId>, out: &mut Vec<LayerInfo>) {
    // 엔진은 아래→위로 저장하므로 뒤집어 패널 순서(위→아래)로 만든다
    for l in layers.iter().rev() {
        out.push(row(l, depth, parent, selected, active));
        if let Some(children) = l.children() {
            walk(children, depth + 1, Some(l.id), selected, active, out);
        }
    }
}

/// 활성 문서의 모든 레이어 (위→아래 평탄, depth/parent 포함). 문서가 없으면 오류.
pub fn all_layers(app: &PhotocraftApp) -> CatResult<Vec<LayerInfo>> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let selected = st.selected_layers();
    let mut out = Vec::new();
    walk(&st.doc.layers, 0, None, &selected, st.active_layer, &mut out);
    Ok(out)
}

/// 최상위 레이어만 (위→아래)
pub fn top_level_layers(app: &PhotocraftApp) -> CatResult<Vec<LayerInfo>> {
    Ok(all_layers(app)?.into_iter().filter(|l| l.depth == 0).collect())
}

/// 그룹의 직계 자식 (위→아래)
pub fn children_of(app: &PhotocraftApp, group: LayerId) -> CatResult<Vec<LayerInfo>> {
    Ok(all_layers(app)?.into_iter().filter(|l| l.parent == Some(group)).collect())
}

/// 선택된 레이어 (위→아래; UXP activeLayers 대응)
pub fn selected_layers(app: &PhotocraftApp) -> CatResult<Vec<LayerInfo>> {
    Ok(all_layers(app)?.into_iter().filter(|l| l.selected).collect())
}

/// 활성(키) 레이어
pub fn active_layer(app: &PhotocraftApp) -> Option<LayerInfo> {
    all_layers(app).ok()?.into_iter().find(|l| l.active)
}

/// ID 로 레이어 조회
pub fn layer_by_id(app: &PhotocraftApp, id: LayerId) -> Option<LayerInfo> {
    all_layers(app).ok()?.into_iter().find(|l| l.id == id)
}

/// 레이어 ID 로 선택 (add: 기존 선택에 추가)
pub fn select_layer(app: &mut PhotocraftApp, id: LayerId, add: bool) -> CatResult<()> {
    exec(app, "layer.select", json!({"layer": id.0, "mode": if add { "add" } else { "replace" }})).map(|_| ())
}

/// 여러 레이어 선택 (첫 레이어가 활성)
pub fn select_layers(app: &mut PhotocraftApp, ids: &[LayerId]) -> CatResult<()> {
    let Some((first, rest)) = ids.split_first() else { return Ok(()) };
    select_layer(app, *first, false)?;
    for id in rest {
        select_layer(app, *id, true)?;
    }
    Ok(())
}

/// 모든 레이어 선택 해제
pub fn select_no_layers(app: &mut PhotocraftApp) -> CatResult<()> {
    exec(app, "select.deselectLayers", json!({})).map(|_| ())
}

/// 이름 변경
pub fn rename_layer(app: &mut PhotocraftApp, id: LayerId, name: &str) -> CatResult<()> {
    exec(app, "layer.renameLayer", json!({"layer": id.0, "name": name})).map(|_| ())
}

/// 표시/숨김
pub fn set_visible(app: &mut PhotocraftApp, id: LayerId, visible: bool) -> CatResult<()> {
    exec(app, "layer.setProps", json!({"layer": id.0, "visible": visible})).map(|_| ())
}

/// 불투명도 (0..=1)
pub fn set_opacity(app: &mut PhotocraftApp, id: LayerId, opacity: f32) -> CatResult<()> {
    exec(app, "layer.setProps", json!({"layer": id.0, "opacity": opacity})).map(|_| ())
}

/// 같은 문서 안 복제 → 새 레이어 ID
pub fn duplicate_layer(app: &mut PhotocraftApp, id: LayerId, name: Option<&str>) -> CatResult<LayerId> {
    let mut p = json!({"layer": id.0});
    if let Some(n) = name {
        p["name"] = json!(n);
    }
    layer_id_of(&exec(app, "layer.duplicate", p)?).ok_or_else(|| "레이어를 복제하지 못했습니다.".into())
}

/// 레이어 삭제
pub fn delete_layer(app: &mut PhotocraftApp, id: LayerId) -> CatResult<()> {
    exec(app, "layer.delete", json!({"layer": id.0})).map(|_| ())
}

/// 이름이 같은 첫 번째 최상위 레이어 삭제 → 삭제 여부 (UXP deleteLayerByName 대응)
pub fn delete_layer_by_name(app: &mut PhotocraftApp, name: &str) -> CatResult<bool> {
    let Some(l) = top_level_layers(app)?.into_iter().find(|l| l.name == name) else { return Ok(false) };
    delete_layer(app, l.id)?;
    Ok(true)
}

/// 새 빈 픽셀 레이어 → ID
pub fn new_layer(app: &mut PhotocraftApp, name: &str) -> CatResult<LayerId> {
    layer_id_of(&exec(app, "layer.new.layer", json!({"name": name}))?).ok_or_else(|| "새 레이어를 만들지 못했습니다.".into())
}

/// 선택 레이어 그룹화 → 그룹 ID (UXP makeGroupFromSelectLayers 대응)
pub fn group_selected_layers(app: &mut PhotocraftApp, name: &str) -> CatResult<LayerId> {
    let r = exec(app, "layer.groupLayers", json!({"name": name}))?;
    layer_id_of(&r).or_else(|| r.get("group").and_then(Value::as_u64).map(LayerId)).ok_or_else(|| "그룹을 만들지 못했습니다.".into())
}

/// 레이어들을 대상 기준 위/아래/안으로 이동 (position: "above" | "below" | "into")
pub fn move_layers_to(app: &mut PhotocraftApp, ids: &[LayerId], target: LayerId, position: &str) -> CatResult<()> {
    let ids: Vec<u64> = ids.iter().map(|l| l.0).collect();
    exec(app, "layer.moveTo", json!({"layers": ids, "target": target.0, "position": position})).map(|_| ())
}

/// 레이어 픽셀 이동 (UXP moveLayerOffset 대응)
pub fn move_layer_offset(app: &mut PhotocraftApp, id: LayerId, dx: i32, dy: i32) -> CatResult<()> {
    exec(app, "layer.translate", json!({"layer": id.0, "dx": dx, "dy": dy})).map(|_| ())
}

/// 래스터화
pub fn rasterize_layer(app: &mut PhotocraftApp, id: LayerId) -> CatResult<()> {
    exec(app, "layer.rasterize.layer", json!({"layer": id.0})).map(|_| ())
}

/// 배경 레이어를 일반 레이어로 (배경이 없으면 그대로)
pub fn background_to_layer(app: &mut PhotocraftApp) -> CatResult<()> {
    exec(app, "layer.new.layerFromBackground", json!({"ifPresent": true})).map(|_| ())
}

/// 스마트 오브젝트로 변환
pub fn convert_to_smart_object(app: &mut PhotocraftApp, id: LayerId) -> CatResult<()> {
    exec(app, "layer.smartObjects.convertToSmartObject", json!({"layer": id.0})).map(|_| ())
}

/// 레이어 스타일 전체 제거 (없으면 무시)
pub fn clear_layer_style(app: &mut PhotocraftApp, id: LayerId) -> bool {
    try_exec(app, "layer.layerStyle.clear", json!({"layer": id.0})).is_some()
}

/// 범용 명령 실행 (UXP actionCommands 대응). 기능 파일은 가능하면 전용 함수를 쓴다.
pub fn action_command(app: &mut PhotocraftApp, command: &str, params: Value) -> CatResult<Value> {
    exec(app, command, params)
}

/// 명령 결과에서 레이어 ID 읽기 (`layer`)
pub fn layer_id_of(r: &Value) -> Option<LayerId> {
    r.get("layer").and_then(Value::as_u64).map(LayerId)
}
