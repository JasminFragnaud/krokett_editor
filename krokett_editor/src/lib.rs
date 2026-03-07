#[cfg(target_os = "android")]
pub mod android_intent_io;
mod constants;
mod file_utils;
mod gpx;
mod places;
mod style;
mod task_utils;
mod tiles;
mod toggle_switch;
mod windows;

use std::{
    collections::BTreeMap,
    sync::mpsc::{Receiver, Sender},
};

use crate::{
    file_utils::{FileContent, FileName},
    windows::{clear_gpx_confirmation_modal, cut_tool_controls, map_selector, zoom},
};
use anyhow::Result;
use egui::{CentralPanel, Context, Frame, Theme, TopBottomPanel, Visuals};
use tiles::{Provider, TilesKind, providers};
use walkers::{Map, MapMemory};

pub struct MyApp {
    providers: BTreeMap<Provider, Vec<TilesKind>>,
    selected_provider: Provider,
    map_memory: MapMemory,
    gpx_state: gpx::GpxState,
    dark_mode: bool,
    clear_gpx_confirm_open: bool,
    load_gpx_channel: (Sender<FileContent>, Receiver<FileContent>),
    save_gpx_channel: (Sender<Result<FileName>>, Receiver<Result<FileName>>),
}

impl MyApp {
    fn light_visuals_with_black_text() -> Visuals {
        let mut visuals = Visuals::light();
        visuals.override_text_color = Some(egui::Color32::BLACK);
        visuals
    }

    pub fn new(egui_ctx: Context) -> Self {
        let dark_mode = egui_ctx
            .system_theme()
            .map(|theme| matches!(theme, Theme::Dark))
            .unwrap_or_else(|| egui_ctx.style().visuals.dark_mode);
        if dark_mode {
            egui_ctx.set_style(style::amoled_friendly());
        } else {
            egui_ctx.set_visuals(Self::light_visuals_with_black_text());
        }
        egui_material_icons::initialize(&egui_ctx);

        Self {
            providers: providers(egui_ctx.to_owned()),
            selected_provider: Provider::IgnRandonnee25k,
            map_memory: MapMemory::default(),
            gpx_state: gpx::GpxState::new(),
            dark_mode,
            load_gpx_channel: (std::sync::mpsc::channel()),
            save_gpx_channel: (std::sync::mpsc::channel()),
            clear_gpx_confirm_open: false,
        }
    }

    pub(crate) fn dark_mode(&self) -> bool {
        self.dark_mode
    }

    pub(crate) fn set_dark_mode(&mut self, ctx: &egui::Context, dark_mode: bool) {
        self.dark_mode = dark_mode;
        if dark_mode {
            ctx.set_style(style::amoled_friendly());
        } else {
            ctx.set_visuals(Self::light_visuals_with_black_text());
        }
    }

    pub(crate) fn load_gpx_from_disk(&mut self, ctx: &egui::Context) {
        while let Ok(file_content) = self.load_gpx_channel.1.try_recv() {
            self.gpx_state.load_gpx_file(
                &file_content.name,
                &file_content.data,
                ctx,
                &mut self.map_memory,
            );
        }
    }

    pub(crate) fn save_gpx_to_disk(&mut self) {
        let data = match self.gpx_state.export_gpx_bytes() {
            Ok(data) => data,
            Err(error) => {
                self.gpx_state.set_status_message(format!(
                    "Erreur dans la sauvegarde du fichier GPX : {error}"
                ));
                log::error!(
                    "Erreur lors de la préparation de la sauvegarde du fichier GPX : {error}"
                );
                return;
            }
        };

        let content = FileContent {
            name: self.gpx_state.export_file_name(),
            data,
        };

        #[cfg(not(target_os = "android"))]
        {
            file_utils::save_as(content, self.save_gpx_channel.0.clone());
        }

        #[cfg(target_os = "android")]
        {
            if let Err(error) = android_intent_io::request_save_gpx(content.name, content.data) {
                self.gpx_state.set_status_message(format!(
                    "Erreur lors de la demande de sauvegarde Android : {error}"
                ));
                log::error!("Erreur lors de la demande de sauvegarde Android : {error}");
            }
        }
    }

    pub(crate) fn request_load_gpx_from_disk(&mut self) {
        #[cfg(not(target_os = "android"))]
        {
            file_utils::load_file(self.load_gpx_channel.0.clone());
        }

        #[cfg(target_os = "android")]
        {
            if let Err(error) = android_intent_io::request_open_gpx() {
                self.gpx_state.set_status_message(format!(
                    "Erreur lors de la demande d'ouverture Android : {error}"
                ));
                log::error!("Erreur lors de la demande d'ouverture Android : {error}");
            }
        }
    }

    #[cfg(target_os = "android")]
    fn handle_android_intent_results(&mut self, ctx: &egui::Context) {
        for open_result in android_intent_io::drain_open_results() {
            match open_result {
                Ok(file_content) => {
                    self.gpx_state.load_gpx_file(
                        &file_content.name,
                        &file_content.data,
                        ctx,
                        &mut self.map_memory,
                    );
                }
                Err(error) => {
                    self.gpx_state.set_status_message(format!(
                        "Erreur lors de l'ouverture du fichier GPX : {error}"
                    ));
                    log::error!("Erreur lors de l'ouverture du fichier GPX : {error}");
                }
            }
        }

        for save_result in android_intent_io::drain_save_results() {
            match save_result {
                Ok(file_name) => {
                    self.gpx_state
                        .set_status_message(format!("GPX sauvegarde : {file_name}"));
                    log::info!("Fichier GPX sauvegarde avec succes : {file_name}");
                }
                Err(error) => {
                    self.gpx_state.set_status_message(format!(
                        "Erreur lors de la sauvegarde du fichier GPX : {error}"
                    ));
                    log::error!("Erreur lors de la sauvegarde du fichier GPX : {error}");
                }
            }
        }
    }

    pub(crate) fn handle_save_gpx_result(&mut self) {
        while let Ok(save_result) = self.save_gpx_channel.1.try_recv() {
            match save_result {
                Ok(file_name) => {
                    self.gpx_state
                        .set_status_message(format!("GPX sauvegardé : {file_name}"));
                    log::info!("Fichier GPX sauvegardé avec succès : {file_name}");
                }
                Err(error) => {
                    self.gpx_state.set_status_message(format!(
                        "Erreur lors de la sauvegarde du fichier GPX : {error}"
                    ));
                    log::error!("Erreur lors de la sauvegarde du fichier GPX : {error}");
                }
            }
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "android")]
        self.handle_android_intent_results(ctx);

        self.load_gpx_from_disk(ctx);
        self.handle_save_gpx_result();

        self.gpx_state
            .handle_dropped_files(ctx, &mut self.map_memory);

        TopBottomPanel::top("main_menu").show(ctx, |ui| {
            windows::top_menu(self, ui);
        });

        self.gpx_state.show_tree_window(ctx);

        CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            self.gpx_state
                .apply_pending_fit(ui.available_size(), &mut self.map_memory);

            let my_position = places::amancy();

            let tiles = self.providers.get_mut(&self.selected_provider).unwrap();
            let attributions: Vec<_> = tiles
                .iter()
                .map(|tile| tile.as_ref().attribution())
                .collect();

            let mut map = Map::new(None, &mut self.map_memory, my_position).zoom_with_ctrl(false);

            let (map_with_plugins, clicked_track, clicked_segment, cut_request, remove_request) =
                self.gpx_state.add_plugins(map);
            map = map_with_plugins;

            for (n, tiles) in tiles.iter_mut().enumerate() {
                let transparency = if n == 0 { 1.0 } else { 0.25 };
                map = map.with_layer(tiles.as_mut(), transparency);
            }

            ui.add(map);
            self.gpx_state.consume_track_click(clicked_track);
            self.gpx_state.consume_segment_click(clicked_segment);
            self.gpx_state.consume_cut_request(cut_request);
            self.gpx_state.consume_remove_request(remove_request);

            {
                cut_tool_controls(self, ui);
                zoom(ui, &mut self.map_memory);
                map_selector(self, ui, attributions);
            }
        });

        self.gpx_state.show_metadata_editor_window(ctx);
        self.gpx_state.show_segment_editor_window(ctx);
        clear_gpx_confirmation_modal(self, ctx);
        self.gpx_state.show_toast(ctx);
    }
}
