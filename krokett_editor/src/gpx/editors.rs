use super::*;

use egui::Color32;

use crate::constants::Colors;

impl GpxState {
    pub(crate) fn show_metadata_editor_window(&mut self, ctx: &egui::Context) {
        let Some(track_selection) = self.selected_track_index else {
            return;
        };
        if self.segment_count(track_selection).is_none() {
            self.metadata_editor_open = false;
            self.selected_track_index = None;
            return;
        }

        let mut open = self.metadata_editor_open;
        let mut track_name = self.track_name(track_selection).unwrap_or_default();
        let mut track_description = self.track_description(track_selection).unwrap_or_default();
        let source = self.source_for_file(track_selection.file_index);
        let mut open_track_profile = false;

        egui::Window::new(format!("Trace {track_name}"))
            .open(&mut open)
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                if ui.button("📊 Profil d'altitude (trace)").clicked() {
                    open_track_profile = true;
                }
                ui.separator();
                ui.label(format!("Source: {source}"));
                ui.separator();
                ui.label("Nom");
                ui.text_edit_singleline(&mut track_name);
                ui.label("Description");
                ui.text_edit_multiline(&mut track_description);
            });

        self.set_track_metadata(track_selection, track_name, track_description);

        if open_track_profile {
            if let Some(waypoints) = self.track_waypoints(track_selection) {
                if !waypoints.is_empty() {
                    let profile_title = format!(
                        "Profil d'altitude - Trace {}",
                        self.track_name(track_selection)
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or_else(|| "Sans nom".to_owned())
                    );
                    self.open_temp_altitude_profile(profile_title, waypoints);
                }
            }
        }

        self.metadata_editor_open = open;
        if !self.metadata_editor_open {
            self.selected_track_index = None;
        }
    }

    pub(crate) fn show_segment_editor_window(&mut self, ctx: &egui::Context) {
        let Some((track_selection, segment_index)) = self.selected_segment else {
            self.window_highlight_segment = None;
            return;
        };

        let Some(segment_count) = self.segment_count(track_selection) else {
            self.segment_editor_open = false;
            self.selected_segment = None;
            self.window_highlight_segment = None;
            return;
        };

        if segment_index >= segment_count {
            self.segment_editor_open = false;
            self.selected_segment = None;
            self.window_highlight_segment = None;
            return;
        }

        let mut open = self.segment_editor_open;
        let mut go_previous = false;
        let mut go_next = false;

        let track_name = self
            .track_name(track_selection)
            .unwrap_or_else(|| "Sans nom".to_owned());
        let mut segment_description = self.segment_description((track_selection, segment_index));
        let mut segment_comment = self.segment_comment((track_selection, segment_index));

        let response = egui::Window::new(format!("Segment {}", segment_index + 1))
            .id(egui::Id::new("segment_editor_window"))
            .open(&mut open)
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                if ui.button("📊 Profil d'altitude").clicked() {
                    self.altitude_profile.reset_fetch();
                    self.altitude_profile.selected_segment = Some((track_selection, segment_index));
                    self.altitude_profile.open = true;
                }

                ui.separator();
                ui.label(format!("Trace: {track_name}"));
                ui.horizontal(|ui| {
                    let prev_enabled = segment_index > 0;
                    let next_enabled = segment_index + 1 < segment_count;

                    let prev_button = egui::Button::new(egui::RichText::new("\u{e909}").size(18.0))
                        .min_size(egui::vec2(26.0, 24.0));
                    let next_button = egui::Button::new(egui::RichText::new("\u{e146}").size(18.0))
                        .min_size(egui::vec2(26.0, 24.0));

                    if ui
                        .add_enabled(prev_enabled, prev_button)
                        .on_hover_text("Précédent")
                        .clicked()
                    {
                        go_previous = true;
                    }

                    ui.label(format!(
                        "Segment: {} / {}",
                        segment_index + 1,
                        segment_count
                    ));

                    if ui
                        .add_enabled(next_enabled, next_button)
                        .on_hover_text("Suivant")
                        .clicked()
                    {
                        go_next = true;
                    }
                });
                ui.separator();
                ui.label("Couleurs");
                ui.horizontal(|ui| {
                    let button_size = egui::vec2(30.0, 20.0);
                    let selected_stroke = egui::Stroke::new(2.0, Color32::WHITE);
                    let unselected_stroke = egui::Stroke::new(2.0, Color32::TRANSPARENT);

                    let with_description_text = Colors::to_string(Colors::SEGMENT_WITH_DESCRIPTION);
                    let to_explore_text = Colors::to_string(Colors::SEGMENT_TO_EXPLORE);

                    let with_description_selected = segment_comment.trim() == with_description_text;
                    let to_explore_selected = segment_comment.trim() == to_explore_text;

                    let mut with_description_button =
                        egui::Button::new("").fill(Colors::SEGMENT_WITH_DESCRIPTION);
                    with_description_button = with_description_button.min_size(button_size).stroke(
                        if with_description_selected {
                            selected_stroke
                        } else {
                            unselected_stroke
                        },
                    );

                    let mut to_explore_button =
                        egui::Button::new("").fill(Colors::SEGMENT_TO_EXPLORE);
                    to_explore_button =
                        to_explore_button
                            .min_size(button_size)
                            .stroke(if to_explore_selected {
                                selected_stroke
                            } else {
                                unselected_stroke
                            });

                    if ui
                        .add(with_description_button)
                        .on_hover_text("segment avec description")
                        .clicked()
                    {
                        if with_description_selected {
                            segment_comment.clear();
                        } else {
                            segment_comment = with_description_text;
                        }
                    }

                    if ui
                        .add(to_explore_button)
                        .on_hover_text("segment à explorer")
                        .clicked()
                    {
                        if to_explore_selected {
                            segment_comment.clear();
                        } else {
                            segment_comment = to_explore_text;
                        }
                    }
                });
                ui.separator();

                ui.label("Description");
                ui.text_edit_multiline(&mut segment_description);
            });

        self.set_segment_description((track_selection, segment_index), segment_description);
        self.set_segment_comment((track_selection, segment_index), segment_comment);

        let window_hovered = response
            .as_ref()
            .and_then(|r| {
                ctx.pointer_hover_pos()
                    .map(|pointer| r.response.rect.contains(pointer))
            })
            .unwrap_or(false);

        self.window_highlight_segment = if window_hovered {
            Some((track_selection, segment_index))
        } else {
            None
        };

        if go_previous {
            self.selected_segment = Some((track_selection, segment_index - 1));
        } else if go_next {
            self.selected_segment = Some((track_selection, segment_index + 1));
        }

        self.segment_editor_open = open;
        if !self.segment_editor_open {
            self.selected_segment = None;
            self.window_highlight_segment = None;
        }
    }
}
