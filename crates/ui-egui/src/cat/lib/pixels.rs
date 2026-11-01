//! 픽셀 읽기/쓰기 (JP_Plugin lib_pixels.ts 대응). 인프로세스라 PNG 왕복 없이 문서 모델을 직접 다룬다.

use photocraft_color::PixelFormat;
use photocraft_doc::{Layer, LayerContent, LayerId};
use photocraft_geom::Rect;
use photocraft_raster::Surface;

use super::core::{CatResult, edit};
use crate::PhotocraftApp;

/// straight-alpha RGBA8 이미지
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// width*height*4 바이트
    pub data: Vec<u8>,
}

impl RgbaImage {
    /// 투명 이미지
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![0; width as usize * height as usize * 4] }
    }

    /// (x, y) 의 RGBA (범위 밖이면 None)
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = self.data.get(i..i + 4)?;
        Some([p[0], p[1], p[2], p[3]])
    }
}

/// 알파가 threshold 보다 큰 픽셀의 bounds [x, y, w, h] (없으면 None)
pub fn alpha_bounds(img: &RgbaImage, threshold: u8) -> Option<[u32; 4]> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..img.height {
        for x in 0..img.width {
            if img.pixel(x, y).is_some_and(|p| p[3] > threshold) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0 < x1).then(|| [x0, y0, x1 - x0, y1 - y0])
}

fn read(surface: &Surface, w: u32, h: u32) -> RgbaImage {
    let r = Rect::new(0, 0, w as i32, h as i32);
    let mut px = vec![[0u8; 4]; w as usize * h as usize];
    surface.read_rgba8_into(r, &mut px);
    RgbaImage { width: w, height: h, data: px.into_iter().flatten().collect() }
}

/// 레이어 픽셀을 문서 크기 캔버스(0,0 기준)로 읽는다. 픽셀이 없는 레이어(그룹·조정)는 오류.
pub fn get_layer_pixels(app: &PhotocraftApp, id: LayerId) -> CatResult<RgbaImage> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let layer = st.doc.layer(id).ok_or("레이어를 찾을 수 없습니다.")?;
    let surface = layer.surface().ok_or_else(|| format!("\"{}\" 레이어에는 픽셀이 없습니다.", layer.name))?;
    Ok(read(surface, st.doc.size.width, st.doc.size.height))
}

/// 보이는 레이어를 합성한 문서 픽셀 (문서 크기)
pub fn get_document_pixels(app: &PhotocraftApp) -> CatResult<RgbaImage> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let surface = photocraft_compose::flatten_to_surface(&st.doc, PixelFormat::RGBA8, None);
    Ok(read(&surface, st.doc.size.width, st.doc.size.height))
}

/// RGBA 이미지를 (x, y) 위치의 새 픽셀 레이어로 추가 (활성 레이어 위) → 새 레이어 ID.
/// 히스토리 그룹 안이면 같은 Undo 단계에 합쳐진다.
pub fn put_pixels_as_layer(app: &mut PhotocraftApp, img: &RgbaImage, name: &str, x: i32, y: i32) -> CatResult<LayerId> {
    if img.data.len() != img.width as usize * img.height as usize * 4 {
        return Err("이미지 크기와 픽셀 데이터 길이가 맞지 않습니다.".into());
    }
    let rect = Rect::new(x, y, x + img.width as i32, y + img.height as i32);
    let mut rgba = Surface::new(PixelFormat::RGBA8);
    rgba.try_write_interleaved(rect, &img.data).map_err(|e| e.to_string())?;
    let name = name.to_string();
    edit(app, "New Layer", move |doc, active| {
        let surface = if doc.pixel_format() == PixelFormat::RGBA8 { rgba } else { rgba.convert(doc.pixel_format()) };
        let mut layer = Layer::raster(name, doc.pixel_format());
        layer.content = LayerContent::Raster(surface);
        let id = doc.insert_above(*active, layer);
        *active = Some(id);
        Ok(id)
    })
}

/// 이미지 파일(PNG/JPG/WebP 등)을 열지 않고 디코드해 합성 픽셀로 읽는다 (임시 문서 불필요)
pub fn load_image_file(path: &std::path::Path) -> CatResult<RgbaImage> {
    let bytes = std::fs::read(path).map_err(|e| format!("파일을 읽을 수 없습니다 ({}): {e}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let r = photocraft_io::import(&name, &bytes).map_err(|e| format!("{name} 을(를) 읽을 수 없습니다: {e}"))?;
    let doc = &r.document;
    let surface = photocraft_compose::flatten_to_surface(doc, PixelFormat::RGBA8, None);
    Ok(read(&surface, doc.size.width, doc.size.height))
}

/// 스마트 오브젝트 원본(임베디드 바이트 또는 링크 파일)을 원본 해상도 합성 픽셀로 읽는다 → (이미지, 원본 파일명)
pub fn smart_object_source_pixels(app: &PhotocraftApp, id: LayerId) -> CatResult<(RgbaImage, String)> {
    let st = app.session.active().ok_or("열린 문서가 없습니다.")?;
    let layer = st.doc.layer(id).ok_or("레이어를 찾을 수 없습니다.")?;
    let LayerContent::Smart(so) = &layer.content else { return Err(format!("\"{}\" 레이어는 스마트 오브젝트가 아닙니다.", layer.name)) };
    let (file_name, bytes) = match &so.source {
        photocraft_doc::SmartSource::Embedded { file_name, bytes } => (file_name.clone(), bytes.to_vec()),
        photocraft_doc::SmartSource::Linked { path } => {
            let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (name, std::fs::read(path).map_err(|e| format!("링크 파일을 읽을 수 없습니다 ({path}): {e}"))?)
        }
    };
    let doc = photocraft_engine::smart_cmds::decode_source(&file_name, &bytes).map_err(|e| e.to_string())?;
    let surface = photocraft_compose::flatten_to_surface(&doc, PixelFormat::RGBA8, None);
    Ok((read(&surface, doc.size.width, doc.size.height), file_name))
}
