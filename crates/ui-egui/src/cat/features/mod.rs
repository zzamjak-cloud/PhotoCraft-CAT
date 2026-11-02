//! CAT 기능 로직 (CAT 루트의 기능 파일 · JP_Plugin `src/features/` 대응). UI 없음, `cat::lib` 만 사용한다.
//! 각 파일의 `run(...)` 이 UI 버튼의 진입점이다. (animationMatchLayers 는 이식하지 않음)

pub mod add_guide;
pub mod app_icon_maker;
pub mod apply_grid_layout;
pub mod auto_paste_png;
pub mod clean_psd;
pub mod clear_empty_layers;
pub mod clear_hidden_effects;
pub mod curve_fit;
pub mod export_market_screenshot;
pub mod export_selected_and_link;
pub mod export_selected_layers;
pub mod export_speed_save;
pub mod folder_preset;
pub mod icon_search;
pub mod image_trace;
pub mod import_files;
pub mod nine_slice_panel;
pub mod nine_slice_resize;
pub mod outline_stroke;
pub mod patch_maker;
pub mod renamer_layers;
pub mod size_presets;
pub mod sorting_layer;
pub mod split_to_layers;
pub mod svg_to_shape_layer;
pub mod vector_ops;
