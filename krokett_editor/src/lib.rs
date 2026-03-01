mod gpx;
mod places;
mod style;
mod tiles;
mod windows;

use std::collections::BTreeMap;

use egui::{CentralPanel, Context, Frame, TopBottomPanel};
use tiles::{providers, Provider, TilesKind};
use walkers::{Map, MapMemory};

pub struct MyApp {
    providers: BTreeMap<Provider, Vec<TilesKind>>,
    selected_provider: Provider,
    map_memory: MapMemory,
    gpx: gpx::GpxState,
    clear_gpx_confirm_open: bool,
}

impl MyApp {
    pub fn new(egui_ctx: Context) -> Self {
        egui_ctx.set_style(style::amoled_friendly());
        egui_material_icons::initialize(&egui_ctx);

        Self {
            providers: providers(egui_ctx.to_owned()),
            selected_provider: Provider::IgnRandonnee25k,
            map_memory: MapMemory::default(),
            gpx: gpx::GpxState::new(),
            clear_gpx_confirm_open: false,
        }
    }

    pub(crate) fn gpx_tracks_count(&self) -> usize {
        self.gpx.tracks_count()
    }

    pub(crate) fn gpx_status(&self) -> Option<&str> {
        self.gpx.status()
    }

    pub(crate) fn clear_gpx_tracks(&mut self) {
        self.gpx.clear();
    }

    pub(crate) fn request_clear_gpx_tracks(&mut self) {
        if self.gpx_tracks_count() > 0 {
            self.clear_gpx_confirm_open = true;
        }
    }

    pub(crate) fn confirm_clear_gpx_tracks(&mut self) {
        self.clear_gpx_tracks();
        self.clear_gpx_confirm_open = false;
    }

    pub(crate) fn cancel_clear_gpx_tracks(&mut self) {
        self.clear_gpx_confirm_open = false;
    }

    pub(crate) fn clear_gpx_confirm_open(&self) -> bool {
        self.clear_gpx_confirm_open
    }

    pub(crate) fn gpx_auto_fit_enabled(&self) -> bool {
        self.gpx.auto_fit_enabled()
    }

    pub(crate) fn set_gpx_auto_fit_enabled(&mut self, enabled: bool) {
        self.gpx.set_auto_fit_enabled(enabled);
    }

    pub(crate) fn load_gpx_from_disk(&mut self, ctx: &egui::Context) {
        self.gpx.load_from_disk_dialog(ctx, &mut self.map_memory);
    }

    pub(crate) fn save_gpx_to_disk(&mut self) {
        self.gpx.save_to_disk_dialog();
    }

    pub(crate) fn gpx_cut_tool_enabled(&self) -> bool {
        self.gpx.cut_tool_enabled()
    }

    pub(crate) fn set_gpx_cut_tool_enabled(&mut self, enabled: bool) {
        self.gpx.set_cut_tool_enabled(enabled);
    }

    pub(crate) fn gpx_tree_window_visible(&self) -> bool {
        self.gpx.tree_window_visible()
    }

    pub(crate) fn set_gpx_tree_window_visible(&mut self, visible: bool) {
        self.gpx.set_tree_window_visible(visible);
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.gpx.handle_dropped_files(ctx, &mut self.map_memory);

        TopBottomPanel::top("main_menu").show(ctx, |ui| {
            windows::top_menu(self, ui, ctx);
        });

        self.gpx.show_tree_window(ctx);

        CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            self.gpx
                .apply_pending_fit(ui.available_size(), &mut self.map_memory);

            let my_position = places::amancy();

            let tiles = self.providers.get_mut(&self.selected_provider).unwrap();
            let attributions: Vec<_> = tiles
                .iter()
                .map(|tile| tile.as_ref().attribution())
                .collect();

            let mut map = Map::new(None, &mut self.map_memory, my_position).zoom_with_ctrl(false);

            let (map_with_plugins, clicked_track, clicked_segment, cut_request, remove_request) =
                self.gpx.add_plugins(map);
            map = map_with_plugins;

            for (n, tiles) in tiles.iter_mut().enumerate() {
                let transparency = if n == 0 { 1.0 } else { 0.25 };
                map = map.with_layer(tiles.as_mut(), transparency);
            }

            ui.add(map);
            self.gpx.consume_track_click(clicked_track);
            self.gpx.consume_segment_click(clicked_segment);
            self.gpx.consume_cut_request(cut_request);
            self.gpx.consume_remove_request(remove_request);

            {
                use windows::*;

                cut_tool_controls(self, ui);
                zoom(ui, &mut self.map_memory);
                map_selector(self, ui, attributions);
            }
        });

        self.gpx.show_metadata_editor_window(ctx);
        self.gpx.show_segment_editor_window(ctx);
        windows::clear_gpx_confirmation_modal(self, ctx);
        self.gpx.show_toast(ctx);
    }
}
