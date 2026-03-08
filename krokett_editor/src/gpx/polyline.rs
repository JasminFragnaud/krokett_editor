use super::*;

use egui::{Color32, PointerButton, Pos2};
use itertools::Itertools as _;
use walkers::Plugin;

use crate::constants::Colors;

pub struct GpxPolyline {
    pub positions: Vec<walkers::Position>,
    pub description: String,
    pub comment: String,
    pub track_selection: TrackSelection,
    pub segment_index: usize,
    pub has_previous_separator: bool,
    pub has_next_separator: bool,
    pub window_highlighted: bool,
    pub cut_tool_enabled: bool,
    pub clicked_track: ClickedTrack,
    pub clicked_segment: Arc<Mutex<Option<SegmentSelection>>>,
    pub cut_request: Arc<Mutex<Option<CutRequest>>>,
    pub remove_request: Arc<Mutex<Option<MergeRequest>>>,
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.length_sq();
    if ab_len_sq <= f32::EPSILON {
        return ap.length();
    }
    let t = (ap.dot(ab) / ab_len_sq).clamp(0.0, 1.0);
    let projection = a + ab * t;
    (p - projection).length()
}

fn pointer_hits_polyline(
    pointer: Pos2,
    positions: &[walkers::Position],
    projector: &walkers::Projector,
) -> bool {
    const CLICK_DISTANCE: f32 = 8.0;
    for (from, to) in positions.iter().tuple_windows() {
        let from_projected = projector.project(*from).to_pos2();
        let to_projected = projector.project(*to).to_pos2();
        if distance_to_segment(pointer, from_projected, to_projected) <= CLICK_DISTANCE {
            return true;
        }
    }
    false
}

fn nearest_segment_split_index(
    pointer: Pos2,
    positions: &[walkers::Position],
    projector: &walkers::Projector,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, (from, to)) in positions.iter().tuple_windows().enumerate() {
        let from_projected = projector.project(*from).to_pos2();
        let to_projected = projector.project(*to).to_pos2();
        let distance = distance_to_segment(pointer, from_projected, to_projected);
        match best {
            Some((_, best_distance)) if distance >= best_distance => {}
            _ => best = Some((index, distance)),
        }
    }
    best.and_then(|(index, distance)| {
        if distance <= 8.0 {
            Some(index + 1)
        } else {
            None
        }
    })
}

fn merge_left_index_from_separator_click(
    pointer: Pos2,
    positions: &[walkers::Position],
    projector: &walkers::Projector,
    segment_index: usize,
    has_previous_separator: bool,
    has_next_separator: bool,
) -> Option<usize> {
    const CLICK_DISTANCE: f32 = 8.0;
    let mut closest: Option<(usize, f32)> = None;

    if has_previous_separator {
        if let Some(first) = positions.first() {
            let first_projected = projector.project(*first).to_pos2();
            let distance = pointer.distance(first_projected);
            if distance <= CLICK_DISTANCE {
                closest = Some((segment_index - 1, distance));
            }
        }
    }

    if has_next_separator {
        if let Some(last) = positions.last() {
            let last_projected = projector.project(*last).to_pos2();
            let distance = pointer.distance(last_projected);
            if distance <= CLICK_DISTANCE {
                match closest {
                    Some((_, best_distance)) if distance >= best_distance => {}
                    _ => closest = Some((segment_index, distance)),
                }
            }
        }
    }

    closest.map(|(left_index, _)| left_index)
}

impl Plugin for GpxPolyline {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &walkers::Projector,
        _map_memory: &walkers::MapMemory,
    ) {
        let hover_pos = response.hover_pos();
        let hovered = hover_pos
            .map(|pointer_pos| pointer_hits_polyline(pointer_pos, &self.positions, projector))
            .unwrap_or(false);

        let separator_hovered = hover_pos
            .and_then(|pointer_pos| {
                merge_left_index_from_separator_click(
                    pointer_pos,
                    &self.positions,
                    projector,
                    self.segment_index,
                    self.has_previous_separator,
                    self.has_next_separator,
                )
            })
            .is_some();

        if self.cut_tool_enabled {
            if separator_hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::NoDrop);
            } else if self.cut_tool_enabled && hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
            }
        }

        if hovered && !separator_hovered && !self.description.trim().is_empty() {
            let tooltip_id = egui::Id::new((
                "gpx_segment_hover_desc",
                self.track_selection,
                self.segment_index,
            ));
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                response.layer_id,
                tooltip_id,
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                ui.label(&self.description);
            });
        }

        let parsed_color = Colors::from_string(self.comment.trim());

        let stroke = if hovered || self.window_highlighted {
            // segment hover
            egui::Stroke::new(5.0, Colors::SEGMENT_HOOVER)
        } else if let Some(parsed_color) = parsed_color {
            egui::Stroke::new(4.0, parsed_color)
        } else if !self.description.trim().is_empty() {
            // segment with description
            egui::Stroke::new(4.0, Colors::SEGMENT_WITH_DESCRIPTION)
        } else {
            // segment not hover no description
            egui::Stroke::new(4.0, Colors::SEGMENT_DEFAULT)
        };

        for (from, to) in self.positions.iter().tuple_windows() {
            let from_projected = projector.project(*from).to_pos2();
            let to_projected = projector.project(*to).to_pos2();
            ui.painter().add(egui::Shape::line(
                vec![from_projected, to_projected],
                stroke,
            ));
        }

        if let Some(first) = self.positions.first() {
            let p = projector.project(*first).to_pos2();
            ui.painter()
                .circle_filled(p, 3.5, Color32::BLACK.gamma_multiply(0.9));
            if self.cut_tool_enabled && self.has_previous_separator {
                ui.painter().circle(
                    p,
                    6.,
                    Color32::from_rgb(255, 224, 96),
                    egui::Stroke::new(1.5, Color32::from_rgb(45, 45, 45)),
                );
            }
        }
        if let Some(last) = self.positions.last() {
            let p = projector.project(*last).to_pos2();
            ui.painter()
                .circle_filled(p, 3.5, Color32::BLACK.gamma_multiply(0.9));
            if self.cut_tool_enabled && self.has_next_separator {
                ui.painter().circle(
                    p,
                    6.,
                    Color32::from_rgb(255, 224, 96),
                    egui::Stroke::new(1.5, Color32::from_rgb(45, 45, 45)),
                );
            }
        }

        if response.clicked_by(PointerButton::Secondary) {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                if !self.cut_tool_enabled
                    && pointer_hits_polyline(pointer_pos, &self.positions, projector)
                {
                    if let Ok(mut clicked) = self.clicked_track.lock() {
                        *clicked = Some(self.track_selection);
                    }
                }
            }
        }

        if response.clicked_by(PointerButton::Primary) {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                if self.cut_tool_enabled {
                    if let Some(left_index) = merge_left_index_from_separator_click(
                        pointer_pos,
                        &self.positions,
                        projector,
                        self.segment_index,
                        self.has_previous_separator,
                        self.has_next_separator,
                    ) {
                        if let Ok(mut remove) = self.remove_request.lock() {
                            *remove = Some((self.track_selection, left_index));
                        }
                    } else if pointer_hits_polyline(pointer_pos, &self.positions, projector) {
                        if let Some(split_idx) =
                            nearest_segment_split_index(pointer_pos, &self.positions, projector)
                        {
                            if let Ok(mut cut) = self.cut_request.lock() {
                                *cut = Some((self.track_selection, self.segment_index, split_idx));
                            }
                        }
                    }
                } else if pointer_hits_polyline(pointer_pos, &self.positions, projector) {
                    if let Ok(mut clicked) = self.clicked_segment.lock() {
                        *clicked = Some((self.track_selection, self.segment_index));
                    }
                }
            }
        }
    }
}
