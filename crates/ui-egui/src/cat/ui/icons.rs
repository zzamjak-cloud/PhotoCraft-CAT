//! CAT 아이콘 (CAT/JP_Plugin public/icons 의 PNG 를 내장). 컨텍스트마다 한 번 디코드해 텍스처로 보관한다.

use egui::{Context, Id, TextureHandle, TextureOptions};
use photocraft_codecs::{ChannelLayout, SampleType};

/// (이름, PNG 바이트)
const ICONS: &[(&str, &[u8])] = &[
    ("addnum", include_bytes!("icons/addnum.png")),
    ("appicon", include_bytes!("icons/appicon.png")),
    ("array", include_bytes!("icons/array.png")),
    ("clear_empty_layers", include_bytes!("icons/clear_empty_layers.png")),
    ("clear_hiddenfx", include_bytes!("icons/clear_hiddenfx.png")),
    ("clearguide", include_bytes!("icons/clearguide.png")),
    ("copy", include_bytes!("icons/copy.png")),
    ("expandstroke", include_bytes!("icons/expandstroke.png")),
    ("export_all_lays_import_png", include_bytes!("icons/export_all_lays_import_png.png")),
    ("export_all_lays_import_psd", include_bytes!("icons/export_all_lays_import_psd.png")),
    ("export_gif", include_bytes!("icons/export_gif.png")),
    ("export_jpg", include_bytes!("icons/export_jpg.png")),
    ("export_market_screenshop", include_bytes!("icons/export_market_screenshop.png")),
    ("export_png", include_bytes!("icons/export_png.png")),
    ("guide_bottom", include_bytes!("icons/guide_bottom.png")),
    ("guide_left", include_bytes!("icons/guide_left.png")),
    ("guide_right", include_bytes!("icons/guide_right.png")),
    ("guide_top", include_bytes!("icons/guide_top.png")),
    ("guidemake", include_bytes!("icons/guidemake.png")),
    ("imagetracing", include_bytes!("icons/imagetracing.png")),
    ("import_searchicon", include_bytes!("icons/import_searchicon.png")),
    ("import_svg", include_bytes!("icons/import_svg.png")),
    ("link_1", include_bytes!("icons/link_1.png")),
    ("outlinestroke", include_bytes!("icons/outlinestroke.png")),
    ("paste", include_bytes!("icons/paste.png")),
    ("patch", include_bytes!("icons/patch.png")),
    ("psd_cleaner", include_bytes!("icons/psd_cleaner.png")),
    ("remove_text", include_bytes!("icons/remove_text.png")),
    ("rename_number", include_bytes!("icons/rename_number.png")),
    ("rename_prefix", include_bytes!("icons/rename_prefix.png")),
    ("rename_reversenumber", include_bytes!("icons/rename_reversenumber.png")),
    ("rename_sufix", include_bytes!("icons/rename_sufix.png")),
    ("renametext", include_bytes!("icons/renametext.png")),
    ("replacetext", include_bytes!("icons/replacetext.png")),
    ("sort", include_bytes!("icons/sort.png")),
    ("sort_pick", include_bytes!("icons/sort_pick.png")),
    ("split_to_layers", include_bytes!("icons/split_to_layers.png")),
];

/// 아이콘 이름 목록 (테스트용)
pub fn names() -> impl Iterator<Item = &'static str> {
    ICONS.iter().map(|(n, _)| *n)
}

/// PNG → egui 이미지 (실패하면 None)
fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = photocraft_codecs::decode(bytes).ok()?.convert(ChannelLayout::Rgba, SampleType::U8);
    let (w, h) = (img.width() as usize, img.height() as usize);
    (img.data().len() == w * h * 4).then(|| egui::ColorImage::from_rgba_unmultiplied([w, h], img.data()))
}

/// 아이콘 텍스처 (처음 쓸 때 업로드해 컨텍스트에 보관). 없는 이름·디코드 실패면 None.
pub fn texture(ctx: &Context, name: &str) -> Option<TextureHandle> {
    let id = Id::unique(("cat-icon", name));
    if let Some(tex) = ctx.data(|d| d.get_temp::<Option<TextureHandle>>(id)) {
        return tex;
    }
    let tex = ICONS
        .iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, bytes)| decode(bytes))
        .map(|img| ctx.load_texture(format!("cat-icon-{name}"), img, TextureOptions::LINEAR));
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}
