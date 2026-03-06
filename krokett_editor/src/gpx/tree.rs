use super::*;

use egui::Color32;
use egui_ltreeview::{NodeBuilder, TreeView};

impl GpxState {
    pub(crate) fn tree_window_visible(&self) -> bool {
        self.tree_window_visible
    }

    pub(crate) fn set_tree_window_visible(&mut self, visible: bool) {
        self.tree_window_visible = visible;
        if !visible {
            self.tree_hover_track = None;
            self.tree_hover_segment = None;
        }
    }

    pub(crate) fn show_tree_window(&mut self, ctx: &egui::Context) {
        self.tree_hover_track = None;
        self.tree_hover_segment = None;

        if !self.tree_window_visible {
            return;
        }

        let mut open = self.tree_window_visible;
        let default_pos = ctx.available_rect().left_top() + egui::vec2(10., 200.0);
        egui::Window::new("GPXs")
            .open(&mut open)
            .resizable(true)
            .vscroll(true)
            .default_pos(default_pos)
            .show(ctx, |ui| {
                if self.gpx_documents.is_empty() {
                    ui.label("Pas de GPX chargé");
                    return;
                }

                ui.horizontal(|ui| {
                    let button_size = egui::vec2(30.0, 20.0);
                    let selected_stroke = egui::Stroke::new(2.0, Color32::WHITE);
                    let unselected_stroke = egui::Stroke::new(2.0, Color32::TRANSPARENT);

                    let with_description_button = egui::Button::new("")
                        .fill(crate::constants::Colors::SEGMENT_WITH_DESCRIPTION)
                        .min_size(button_size)
                        .stroke(if self.filter_with_description_color {
                            selected_stroke
                        } else {
                            unselected_stroke
                        });
                    if ui
                        .add(with_description_button)
                        .on_hover_text("Filtre segment avec description")
                        .clicked()
                    {
                        self.filter_with_description_color = !self.filter_with_description_color;
                    }

                    let to_explore_button = egui::Button::new("")
                        .fill(crate::constants::Colors::SEGMENT_TO_EXPLORE)
                        .min_size(button_size)
                        .stroke(if self.filter_to_explore_color {
                            selected_stroke
                        } else {
                            unselected_stroke
                        });
                    if ui
                        .add(to_explore_button)
                        .on_hover_text("Filtre segment à explorer")
                        .clicked()
                    {
                        self.filter_to_explore_color = !self.filter_to_explore_color;
                    }

                    let no_button = egui::Button::new("NO")
                        .min_size(egui::vec2(40.0, 20.0))
                        .stroke(if self.filter_no_color_or_description {
                            selected_stroke
                        } else {
                            unselected_stroke
                        });
                    if ui
                        .add(no_button)
                        .on_hover_text("Filtre segments non a explorer et sans description")
                        .clicked()
                    {
                        self.filter_no_color_or_description = !self.filter_no_color_or_description;
                    }
                });
                ui.separator();

                let mut hover_track = None;
                let mut hover_segment = None;
                let mut click_track = None;
                let mut click_segment = None;

                let mut file_visibility_updates: Vec<(Vec<TrackSelection>, bool)> = Vec::new();
                let mut track_visibility_updates: Vec<(TrackSelection, bool)> = Vec::new();
                let mut segment_visibility_updates: Vec<(SegmentSelection, bool)> = Vec::new();

                let tree_id = ui.make_persistent_id("gpx_tree_view");
                let (_response, _actions) = TreeView::new(tree_id).show(ui, |builder| {
                    for file_index in 0..self.gpx_documents.len() {
                        let track_selections = self.file_track_selections(file_index);
                        if track_selections.is_empty() {
                            continue;
                        }

                        let mut file_visible = true;
                        for &track_selection in &track_selections {
                            if !self.is_track_visible(track_selection) {
                                file_visible = false;
                                break;
                            }
                            if let Some(segment_count) = self.segment_count(track_selection) {
                                if (0..segment_count).any(|segment_index| {
                                    let selection = (track_selection, segment_index);
                                    let description = self.segment_description(selection);
                                    let comment = self.segment_comment(selection);
                                    let matches_filter =
                                        self.segment_matches_active_filters(&description, &comment);
                                    !(self.is_segment_visible(selection) && matches_filter)
                                }) {
                                    file_visible = false;
                                    break;
                                }
                            }
                        }

                        let source = self.source_for_file(file_index);
                        let file_label = format!("Fichier : {source}");
                        let file_is_open = builder.node(
                            NodeBuilder::dir(GpxTreeNodeId::File(file_index)).label_ui(|ui| {
                                let row = ui.horizontal(|ui| {
                                    let checkbox_response = ui.checkbox(&mut file_visible, "");
                                    if checkbox_response.changed() {
                                        file_visibility_updates
                                            .push((track_selections.clone(), file_visible));
                                    }
                                    let label_response = ui.label(&file_label);
                                    checkbox_response.hovered() || label_response.hovered()
                                });

                                let mut full_line_rect = row.response.rect;
                                full_line_rect.set_left(ui.min_rect().left());
                                full_line_rect.set_right(ui.max_rect().right());
                                let row_hovered = row
                                    .response
                                    .ctx
                                    .pointer_hover_pos()
                                    .map(|pointer| full_line_rect.contains(pointer))
                                    .unwrap_or(false);

                                if row.inner || row_hovered {
                                    hover_track = track_selections.first().copied();
                                }
                            }),
                        );

                        if file_is_open {
                            for (in_file_index, &track_selection) in
                                track_selections.iter().enumerate()
                            {
                                let mut track_visible = self.is_track_visible(track_selection)
                                    && (0..self.segment_count(track_selection).unwrap_or(0)).all(
                                        |segment_index| {
                                            let selection = (track_selection, segment_index);
                                            let description = self.segment_description(selection);
                                            let comment = self.segment_comment(selection);
                                            let matches_filter = self
                                                .segment_matches_active_filters(
                                                    &description,
                                                    &comment,
                                                );
                                            self.is_segment_visible(selection) && matches_filter
                                        },
                                    );

                                let default_prefix = match track_selection.kind {
                                    GpxTrackKind::Track => "Track",
                                    GpxTrackKind::Route => "Route",
                                };
                                let track_title = self
                                    .track_name(track_selection)
                                    .filter(|name| !name.trim().is_empty())
                                    .unwrap_or_else(|| {
                                        format!("{default_prefix} {}", in_file_index + 1)
                                    });

                                let track_is_open = builder.node(
                                    NodeBuilder::dir(GpxTreeNodeId::Track(track_selection))
                                        .label_ui(|ui| {
                                            let row = ui.horizontal(|ui| {
                                                let checkbox_response =
                                                    ui.checkbox(&mut track_visible, "");
                                                if checkbox_response.changed() {
                                                    track_visibility_updates
                                                        .push((track_selection, track_visible));
                                                }
                                                let label_response = ui.label(&track_title);
                                                (
                                                    checkbox_response.hovered()
                                                        || label_response.hovered(),
                                                    label_response.clicked(),
                                                    checkbox_response.rect,
                                                )
                                            });
                                            let mut full_line_rect = row.response.rect;
                                            full_line_rect.set_left(ui.min_rect().left());
                                            full_line_rect.set_right(ui.max_rect().right());
                                            let row_hovered = row
                                                .response
                                                .ctx
                                                .pointer_hover_pos()
                                                .map(|pointer| full_line_rect.contains(pointer))
                                                .unwrap_or(false);
                                            let row_clicked = row.response.ctx.input(|input| {
                                                input.pointer.primary_clicked()
                                                    && input.pointer.interact_pos().is_some_and(
                                                        |pointer| {
                                                            full_line_rect.contains(pointer)
                                                                && !row.inner.2.contains(pointer)
                                                        },
                                                    )
                                            });

                                            if row.inner.0 || row_hovered {
                                                hover_track = Some(track_selection);
                                            }
                                            if row.inner.1 || row_clicked {
                                                click_track = Some(track_selection);
                                            }
                                        }),
                                );

                                if track_is_open {
                                    let segment_count =
                                        self.segment_count(track_selection).unwrap_or(0);
                                    for segment_index in 0..segment_count {
                                        let selection = (track_selection, segment_index);
                                        let description = self.segment_description(selection);
                                        let comment = self.segment_comment(selection);
                                        let matches_filter = self
                                            .segment_matches_active_filters(&description, &comment);

                                        let mut segment_visible =
                                            self.is_segment_visible(selection) && matches_filter;
                                        let segment_label =
                                            format!("{}: {}", segment_index + 1, description);

                                        builder.node(
                                            NodeBuilder::leaf(GpxTreeNodeId::Segment(
                                                track_selection,
                                                segment_index,
                                            ))
                                            .label_ui(|ui| {
                                                let row = ui.horizontal(|ui| {
                                                    let checkbox_response =
                                                        ui.checkbox(&mut segment_visible, "");
                                                    if checkbox_response.changed() {
                                                        segment_visibility_updates.push((
                                                            (track_selection, segment_index),
                                                            segment_visible,
                                                        ));
                                                    }
                                                    let label_response = ui.label(&segment_label);
                                                    (
                                                        checkbox_response.hovered()
                                                            || label_response.hovered(),
                                                        label_response.clicked(),
                                                        checkbox_response.rect,
                                                    )
                                                });
                                                let mut full_line_rect = row.response.rect;
                                                full_line_rect.set_left(ui.min_rect().left());
                                                full_line_rect.set_right(ui.max_rect().right());
                                                let row_hovered = row
                                                    .response
                                                    .ctx
                                                    .pointer_hover_pos()
                                                    .map(|pointer| full_line_rect.contains(pointer))
                                                    .unwrap_or(false);
                                                let row_clicked = row.response.ctx.input(|input| {
                                                    input.pointer.primary_clicked()
                                                        && input.pointer.interact_pos().is_some_and(
                                                            |pointer| {
                                                                full_line_rect.contains(pointer)
                                                                    && !row
                                                                        .inner
                                                                        .2
                                                                        .contains(pointer)
                                                            },
                                                        )
                                                });

                                                if row.inner.0 || row_hovered {
                                                    hover_segment =
                                                        Some((track_selection, segment_index));
                                                }
                                                if row.inner.1 || row_clicked {
                                                    click_segment =
                                                        Some((track_selection, segment_index));
                                                }
                                            }),
                                        );
                                    }
                                }

                                builder.close_dir();
                            }
                        }

                        builder.close_dir();
                    }
                });

                for (track_selections, visible) in file_visibility_updates {
                    for track_selection in track_selections {
                        self.set_track_visible(track_selection, visible);
                    }
                }

                for (track_selection, visible) in track_visibility_updates {
                    self.set_track_visible(track_selection, visible);
                }

                for (segment_selection, visible) in segment_visibility_updates {
                    self.set_segment_visible(segment_selection, visible);
                }

                if let Some(track_selection) = click_track {
                    self.selected_track_index = Some(track_selection);
                    self.metadata_editor_open = true;
                }

                if let Some(segment_selection) = click_segment {
                    self.selected_segment = Some(segment_selection);
                    self.segment_editor_open = true;
                }

                self.tree_hover_track = hover_track;
                self.tree_hover_segment = hover_segment;
            });

        self.tree_window_visible = open;
        if !self.tree_window_visible {
            self.tree_hover_track = None;
            self.tree_hover_segment = None;
        }
    }
}
