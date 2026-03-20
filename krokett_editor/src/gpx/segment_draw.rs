use super::*;

use egui::{Color32, PointerButton};
use geo_types::Point;
use walkers::Plugin;

/// Approximate spacing in km between interpolated waypoints.
pub(super) const DRAW_LINE_SPACING_KM: f64 = 0.2;

/// Maximum number of waypoints sent to the elevation API.
const MAX_WAYPOINTS: usize = 200;

/// Haversine distance in km between two lat/lon coordinates.
pub(super) fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    EARTH_RADIUS_KM * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

/// Interpolate positions from a list of anchor points at approximately `spacing_km` intervals.
/// Always includes the first anchor and each subsequent anchor endpoint.
pub(super) fn build_full_positions(
    anchors: &[walkers::Position],
    spacing_km: f64,
) -> Vec<walkers::Position> {
    if anchors.is_empty() {
        return Vec::new();
    }
    let mut result = vec![anchors[0]];
    for w in anchors.windows(2) {
        let dist = haversine_km(w[0].y(), w[0].x(), w[1].y(), w[1].x());
        let n = ((dist / spacing_km).ceil() as usize).max(1);
        for i in 1..=n {
            let t = i as f64 / n as f64;
            result.push(walkers::lat_lon(
                w[0].y() + t * (w[1].y() - w[0].y()),
                w[0].x() + t * (w[1].x() - w[0].x()),
            ));
        }
    }
    result
}

/// Plugin that handles drawing a temporary segment on the map.
/// Left click adds an anchor point. Visual feedback shows the segment being drawn.
pub struct SegmentDrawPlugin {
    pub drawing_points: Vec<walkers::Position>,
    pub draw_action: PendingDrawSegmentAction,
}

impl Plugin for SegmentDrawPlugin {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &walkers::Projector,
        _map_memory: &walkers::MapMemory,
    ) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);

        let draw_color = Color32::from_rgb(220, 40, 40);
        let painter = ui.painter();

        // Draw lines between existing anchor points
        for w in self.drawing_points.windows(2) {
            let from = projector.project(w[0]).to_pos2();
            let to = projector.project(w[1]).to_pos2();
            painter.add(egui::Shape::line(
                vec![from, to],
                egui::Stroke::new(3.0, draw_color),
            ));
        }

        // Draw anchor dots
        for pos in &self.drawing_points {
            let p = projector.project(*pos).to_pos2();
            painter.circle_filled(p, 5.0, draw_color);
            painter.circle_stroke(p, 5.0, egui::Stroke::new(1.5, Color32::WHITE));
        }

        // Preview line from last anchor to cursor position
        if let (Some(last), Some(cursor)) = (self.drawing_points.last(), response.hover_pos()) {
            let from = projector.project(*last).to_pos2();
            painter.add(egui::Shape::line(
                vec![from, cursor],
                egui::Stroke::new(2.0, draw_color.gamma_multiply(0.5)),
            ));
        }

        // Left click: add anchor point
        if response.clicked_by(PointerButton::Primary) {
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                let map_pos = projector.unproject(pointer_pos.to_vec2());
                if let Ok(mut action) = self.draw_action.lock() {
                    *action = Some(DrawSegmentAction::AddPoint(map_pos));
                }
            }
        }

        // Right click: undo last anchor point
        if response.clicked_by(PointerButton::Secondary) {
            if let Ok(mut action) = self.draw_action.lock() {
                *action = Some(DrawSegmentAction::UndoLast);
            }
        }
    }
}

impl GpxState {
    pub(crate) fn segment_draw_tool_enabled(&self) -> bool {
        self.segment_draw_tool_enabled
    }

    pub(crate) fn set_segment_draw_tool_enabled(&mut self, enabled: bool) {
        self.segment_draw_tool_enabled = enabled;
        if enabled {
            self.cut_tool_enabled = false;
            self.waypoint_tool_enabled = false;
        }
        if !enabled {
            self.drawing_segment_points.clear();
            self.temp_altitude_profile.close();
        }
    }

    pub(crate) fn drawing_segment_points(&self) -> &[walkers::Position] {
        &self.drawing_segment_points
    }

    pub(crate) fn clear_drawing_segment(&mut self) {
        self.drawing_segment_points.clear();
        self.temp_altitude_profile.close();
    }

    /// Build intermediate waypoints between anchor points and start elevation fetch.
    pub(crate) fn finalize_temp_segment(&mut self) {
        if self.drawing_segment_points.len() < 2 {
            return;
        }

        // Compute total distance to decide spacing (cap at MAX_WAYPOINTS)
        let total_dist: f64 = self
            .drawing_segment_points
            .windows(2)
            .map(|w| haversine_km(w[0].y(), w[0].x(), w[1].y(), w[1].x()))
            .sum();

        let spacing = if total_dist / DRAW_LINE_SPACING_KM > MAX_WAYPOINTS as f64 {
            total_dist / MAX_WAYPOINTS as f64
        } else {
            DRAW_LINE_SPACING_KM
        };

        let positions = build_full_positions(&self.drawing_segment_points, spacing);

        let waypoints: Vec<gpx::Waypoint> = positions
            .into_iter()
            .map(|pos| gpx::Waypoint::new(Point::new(pos.x(), pos.y())))
            .collect();

        self.open_temp_altitude_profile("Profil d'altitude - Segment temporaire", waypoints);
    }

    pub(crate) fn consume_draw_segment_action(&mut self, action: PendingDrawSegmentAction) {
        if let Some(action) = action.lock().ok().and_then(|mut lock| lock.take()) {
            match action {
                DrawSegmentAction::AddPoint(pos) => {
                    self.drawing_segment_points.push(pos);
                }
                DrawSegmentAction::UndoLast => {
                    let _ = self.drawing_segment_points.pop();
                    if self.drawing_segment_points.len() < 2 {
                        self.temp_altitude_profile.close();
                    }
                }
            }
        }
    }
}
