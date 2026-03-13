use super::*;

use geo_types::Point;
use time::OffsetDateTime;

const CURRENT_LOCATION_DUPLICATE_TOLERANCE_DEG: f64 = 1e-5;

fn ensure_marker_document(documents: &mut Vec<gpx::Gpx>) -> usize {
    if documents.is_empty() {
        documents.push(gpx::Gpx {
            version: gpx::GpxVersion::Gpx11,
            creator: Some("krokett_editor".to_owned()),
            ..Default::default()
        });
    }
    0
}

impl GpxState {
    pub(crate) fn add_waypoint_at_position(&mut self, position: walkers::Position) {
        let file_index = ensure_marker_document(&mut self.gpx_documents);
        let Some(waypoints) = self
            .gpx_documents
            .get_mut(file_index)
            .map(|document| &mut document.waypoints)
        else {
            return;
        };

        let mut waypoint = gpx::Waypoint::new(Point::new(position.x(), position.y()));
        waypoint.time = Some(OffsetDateTime::now_utc().into());
        waypoints.push(waypoint);

        let waypoint_index = waypoints.len() - 1;

        self.selected_waypoint = Some((file_index, waypoint_index));
        self.waypoint_editor_open = true;
        self.status = Some("Waypoint ajouté".to_owned());
        self.toasts.success("Waypoint ajouté");
    }

    pub(crate) fn add_waypoint_at_current_position(&mut self, position: walkers::Position) {
        if let Some(selection) =
            self.find_waypoint_near_position(position, CURRENT_LOCATION_DUPLICATE_TOLERANCE_DEG)
        {
            self.selected_waypoint = Some(selection);
            self.waypoint_editor_open = true;
            self.status = Some("Waypoint déjà présent à cette position".to_owned());
            self.toasts.info("Waypoint déjà présent à cette position");
            return;
        }

        self.add_waypoint_at_position(position);
    }

    fn find_waypoint_near_position(
        &self,
        position: walkers::Position,
        tolerance_deg: f64,
    ) -> Option<WaypointSelection> {
        self.gpx_documents
            .iter()
            .enumerate()
            .find_map(|(file_index, document)| {
                document
                    .waypoints
                    .iter()
                    .enumerate()
                    .find(|(_, waypoint)| {
                        let point = waypoint.point();
                        (point.x() - position.x()).abs() <= tolerance_deg
                            && (point.y() - position.y()).abs() <= tolerance_deg
                    })
                    .map(|(waypoint_index, _)| (file_index, waypoint_index))
            })
    }

    pub(crate) fn consume_waypoint_click(&mut self, clicked_waypoint: ClickedWaypoint) {
        if let Some(waypoint_selection) = clicked_waypoint
            .lock()
            .ok()
            .and_then(|mut lock| lock.take())
        {
            self.selected_waypoint = Some(waypoint_selection);
            self.waypoint_editor_open = true;
        }
    }

    pub(crate) fn consume_add_waypoint_request(
        &mut self,
        add_waypoint_request: PendingAddWaypointRequest,
    ) {
        if !self.waypoint_tool_enabled {
            return;
        }

        let Some(position) = add_waypoint_request
            .lock()
            .ok()
            .and_then(|mut lock| lock.take())
        else {
            return;
        };

        self.add_waypoint_at_position(position);
    }

    pub(crate) fn show_waypoint_editor_window(&mut self, ctx: &egui::Context) {
        let Some((file_index, waypoint_index)) = self.selected_waypoint else {
            self.window_highlight_waypoint = None;
            self.waypoint_delete_confirm_open = false;
            return;
        };

        let Some(waypoints) = self
            .gpx_documents
            .get(file_index)
            .map(|document| document.waypoints.as_slice())
        else {
            self.waypoint_editor_open = false;
            self.selected_waypoint = None;
            self.window_highlight_waypoint = None;
            self.waypoint_delete_confirm_open = false;
            return;
        };

        if waypoint_index >= waypoints.len() {
            self.waypoint_editor_open = false;
            self.selected_waypoint = None;
            self.window_highlight_waypoint = None;
            self.waypoint_delete_confirm_open = false;
            return;
        }

        let waypoint_count = waypoints.len();
        let mut description = waypoints[waypoint_index]
            .description
            .clone()
            .unwrap_or_default();
        let mut time_text = waypoints[waypoint_index]
            .time
            .as_ref()
            .and_then(|time| time.format().ok())
            .unwrap_or_default();

        let source = self.source_for_file(file_index);

        let mut open = self.waypoint_editor_open;
        let mut go_previous = false;
        let mut go_next = false;
        let mut ask_delete = false;
        let mut confirm_delete = false;

        let response = egui::Window::new(format!("Waypoint {}", waypoint_index + 1))
            .id(egui::Id::new("waypoint_editor_window"))
            .open(&mut open)
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Source: {source}"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let delete_button =
                            egui::Button::new(egui::RichText::new("\u{e872}").size(18.0))
                                .min_size(egui::vec2(28.0, 28.0));
                        if ui.add(delete_button).on_hover_text("Supprimer").clicked() {
                            ask_delete = true;
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let prev_enabled = waypoint_index > 0;
                    let next_enabled = waypoint_index + 1 < waypoint_count;

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
                        "Waypoint: {} / {}",
                        waypoint_index + 1,
                        waypoint_count
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
                ui.label("Heure");
                ui.add_enabled(false, egui::TextEdit::singleline(&mut time_text));
                ui.label("Description");
                ui.text_edit_multiline(&mut description);
            });

        let window_opened = response
            .as_ref()
            .map(|r| r.response.is_pointer_button_down_on() || r.response.hovered() || open)
            .unwrap_or(open);

        self.window_highlight_waypoint = if window_opened {
            Some((file_index, waypoint_index))
        } else {
            None
        };

        if ask_delete {
            self.waypoint_delete_confirm_open = true;
        }

        if self.waypoint_delete_confirm_open {
            let modal_response = egui::Modal::new(egui::Id::new("delete_waypoint_confirmation"))
                .show(ctx, |ui| {
                    ui.set_min_width(320.0);
                    ui.heading("Supprimer ce waypoint ?");
                    ui.add_space(4.0);
                    ui.label("Cette action supprimera le waypoint sélectionné.");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Supprimer").clicked() {
                            confirm_delete = true;
                            ui.close();
                        }
                        if ui.button("Annuler").clicked() {
                            ui.close();
                        }
                    });
                });

            if confirm_delete {
                if let Some(waypoints) = self
                    .gpx_documents
                    .get_mut(file_index)
                    .map(|document| &mut document.waypoints)
                {
                    if waypoint_index < waypoints.len() {
                        waypoints.remove(waypoint_index);
                        self.status = Some("Waypoint supprimé".to_owned());
                        self.toasts.success("Waypoint supprimé");

                        if waypoints.is_empty() {
                            self.selected_waypoint = None;
                            self.waypoint_editor_open = false;
                        } else {
                            let next_index = waypoint_index.min(waypoints.len() - 1);
                            self.selected_waypoint = Some((file_index, next_index));
                        }
                    }
                }
                self.waypoint_delete_confirm_open = false;
            } else if modal_response.should_close() {
                self.waypoint_delete_confirm_open = false;
            }
        }

        if confirm_delete {
            return;
        }

        if let Some(waypoints) = self
            .gpx_documents
            .get_mut(file_index)
            .map(|document| &mut document.waypoints)
        {
            if let Some(waypoint) = waypoints.get_mut(waypoint_index) {
                waypoint.description = if description.trim().is_empty() {
                    None
                } else {
                    Some(description)
                };

                if waypoint.time.is_none() {
                    waypoint.time = Some(OffsetDateTime::now_utc().into());
                }
            }
        }

        if go_previous {
            self.selected_waypoint = Some((file_index, waypoint_index - 1));
        } else if go_next {
            self.selected_waypoint = Some((file_index, waypoint_index + 1));
        }

        self.waypoint_editor_open = open;
        if !self.waypoint_editor_open {
            self.selected_waypoint = None;
            self.window_highlight_waypoint = None;
            self.waypoint_delete_confirm_open = false;
        }
    }
}
