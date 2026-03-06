use crate::{MyApp, toggle_switch};
use egui::{Align, Align2, ComboBox, Image, Layout, Response, RichText, Ui, Vec2, Window};
use walkers::{MapMemory, sources::Attribution};

pub fn top_menu(app: &mut MyApp, ui: &mut Ui) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button("Fichier", |ui| {
                if ui.button("Charger GPX…").clicked() {
                    app.request_load_gpx_from_disk();
                    ui.close();
                }

                if ui.button("Sauver GPX…").clicked() {
                    app.save_gpx_to_disk();
                    ui.close();
                }

                if app.gpx_state.tracks_count() > 0 && ui.button("Supprimer GPXs").clicked() {
                    if app.gpx_state.tracks_count() > 0 {
                        app.clear_gpx_confirm_open = true;
                    }
                    ui.close();
                }

                let mut auto_fit = app.gpx_state.auto_fit_enabled();
                if ui
                    .checkbox(&mut auto_fit, "Auto-zoom chargement GPX")
                    .changed()
                {
                    app.gpx_state.set_auto_fit_enabled(auto_fit);
                }

                let mut show_tree = app.gpx_state.tree_window_visible();
                if ui
                    .checkbox(&mut show_tree, "Afficher l'arbre GPX")
                    .changed()
                {
                    app.gpx_state.set_tree_window_visible(show_tree);
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Sombre");
                let mut dark_mode = app.dark_mode();
                if ui.add(toggle_switch::toggle(&mut dark_mode)).changed() {
                    app.set_dark_mode(ui.ctx(), dark_mode);
                }
                ui.label("Clair");
            });
        });
    });
}

pub fn map_selector(app: &mut MyApp, ui: &Ui, attributions: Vec<Attribution>) {
    Window::new("Sélecteur de carte")
        .collapsible(true)
        .resizable(true)
        .min_size(Vec2 { x: 200., y: 50. })
        .title_bar(false)
        .anchor(Align2::LEFT_TOP, [10., 44.])
        .show(ui.ctx(), |ui| {
            ComboBox::from_id_salt("Source Carte")
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
            ui.label("Déposez des fichiers .gpx sur la carte pour afficher les traces");
            ui.label(format!("Segments GPX : {}", app.gpx_state.tracks_count()));

            if app.gpx_state.cut_tool_enabled() {
                ui.label(
                    "Édition de segment activé:\nClic gauche : créer un nouveau segment,\nClic droit sur un séparateur : supprimer le segment",
                );
            }

            if let Some(status) = app.gpx_state.status() {
                ui.label(status);
            }
        });
}

pub fn clear_gpx_confirmation_modal(app: &mut MyApp, ctx: &egui::Context) {
    if !app.clear_gpx_confirm_open {
        return;
    }

    let mut confirm = false;

    let modal_response =
        egui::Modal::new(egui::Id::new("clear_gpx_confirmation")).show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.heading("Supprimer tous les GPX ?");
            ui.add_space(4.0);
            ui.label("Cette action supprimera toutes les GPX chargées de la carte.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Ok").clicked() {
                    confirm = true;
                    ui.close();
                }
                if ui.button("Annuler").clicked() {
                    ui.close();
                }
            });
        });

    if confirm {
        app.gpx_state.clear();
        app.clear_gpx_confirm_open = false;
    } else if modal_response.should_close() {
        app.clear_gpx_confirm_open = false;
    }
}

pub fn large_material_button(ui: &mut Ui, text: &str) -> Response {
    ui.button(RichText::new(text).size(24.0))
}

pub fn cut_tool_controls(app: &mut MyApp, ui: &Ui) {
    Window::new("Découpe segments")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .anchor(Align2::RIGHT_TOP, [-10., 44.])
        .show(ui.ctx(), |ui| {
            let mut cut_tool = app.gpx_state.cut_tool_enabled();
            if ui.checkbox(&mut cut_tool, "Édition de segment").changed() {
                app.gpx_state.set_cut_tool_enabled(cut_tool);
            }
        });
}

/// Simple GUI to zoom in and out.
pub fn zoom(ui: &Ui, map_memory: &mut MapMemory) {
    Window::new("Carte")
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
