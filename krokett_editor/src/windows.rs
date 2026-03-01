use crate::MyApp;
use egui::{Align, Align2, ComboBox, Image, Layout, Response, RichText, Ui, Window};
use walkers::{sources::Attribution, MapMemory};

pub fn top_menu(app: &mut MyApp, ui: &mut Ui, ctx: &egui::Context) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Load GPX…").clicked() {
                    app.load_gpx_from_disk(ctx);
                    ui.close();
                }

                if ui.button("Save GPX…").clicked() {
                    app.save_gpx_to_disk();
                    ui.close();
                }

                if app.gpx_tracks_count() > 0 && ui.button("Remove GPXs").clicked() {
                    app.request_clear_gpx_tracks();
                    ui.close();
                }

                let mut auto_fit = app.gpx_auto_fit_enabled();
                if ui.checkbox(&mut auto_fit, "Auto-fit GPX on load").changed() {
                    app.set_gpx_auto_fit_enabled(auto_fit);
                }

                let mut show_tree = app.gpx_tree_window_visible();
                if ui.checkbox(&mut show_tree, "Show GPX tree").changed() {
                    app.set_gpx_tree_window_visible(show_tree);
                }
            });
        });
    });
}

pub fn map_selector(app: &mut MyApp, ui: &Ui, attributions: Vec<Attribution>) {
    Window::new("Map Selector")
        .collapsible(true)
        .resizable(false)
        .title_bar(false)
        .anchor(Align2::LEFT_TOP, [10., 44.])
        .show(ui.ctx(), |ui| {
            ComboBox::from_id_salt("Tile Provider")
                .selected_text(format!("{:?}", app.selected_provider))
                .show_ui(ui, |ui| {
                    for p in app.providers.keys() {
                        ui.selectable_value(&mut app.selected_provider, *p, format!("{p:?}"));
                    }
                });

            for attribution in attributions {
                ui.horizontal(|ui| {
                    if let Some(logo) = attribution.logo_light {
                        ui.add(Image::new(logo).max_height(30.0).max_width(80.0));
                    }
                    ui.hyperlink_to(attribution.text, attribution.url);
                });
            }

            ui.separator();
            ui.label("Drop .gpx files on the map to display tracks");
            ui.label(format!("GPX segments: {}", app.gpx_tracks_count()));

            if app.gpx_cut_tool_enabled() {
                ui.label(
                    "Cut tool enabled — Left click: cut, Right click separator: merge adjacent",
                );
            }

            if let Some(status) = app.gpx_status() {
                ui.label(status);
            }
        });
}

pub fn clear_gpx_confirmation_modal(app: &mut MyApp, ctx: &egui::Context) {
    if !app.clear_gpx_confirm_open() {
        return;
    }

    let mut confirm = false;

    let modal_response =
        egui::Modal::new(egui::Id::new("clear_gpx_confirmation")).show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.heading("Clear all GPX overlays?");
            ui.add_space(4.0);
            ui.label("This action removes all loaded GPX tracks from the map.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Ok").clicked() {
                    confirm = true;
                    ui.close();
                }
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
            });
        });

    if confirm {
        app.confirm_clear_gpx_tracks();
    } else if modal_response.should_close() {
        app.cancel_clear_gpx_tracks();
    }
}

pub fn large_material_button(ui: &mut Ui, text: &str) -> Response {
    ui.button(RichText::new(text).size(24.0))
}

pub fn cut_tool_controls(app: &mut MyApp, ui: &Ui) {
    Window::new("Cut Tool")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .anchor(Align2::RIGHT_TOP, [-10., 44.])
        .show(ui.ctx(), |ui| {
            let mut cut_tool = app.gpx_cut_tool_enabled();
            if ui.checkbox(&mut cut_tool, "Segment edit").changed() {
                app.set_gpx_cut_tool_enabled(cut_tool);
            }
        });
}

/// Simple GUI to zoom in and out.
pub fn zoom(ui: &Ui, map_memory: &mut MapMemory) {
    Window::new("Map")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .anchor(Align2::LEFT_BOTTOM, [10., -10.])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                if large_material_button(ui, "\u{e145}").clicked() {
                    let _ = map_memory.zoom_in();
                }

                if large_material_button(ui, "\u{e15b}").clicked() {
                    let _ = map_memory.zoom_out();
                }

                if map_memory.detached().is_some()
                    && large_material_button(ui, "\u{e55c}").clicked()
                {
                    map_memory.follow_my_position();
                }
            });
        });
}
