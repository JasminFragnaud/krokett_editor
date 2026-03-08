use super::*;

use egui::{Color32, PointerButton, Pos2};
use walkers::Plugin;

const PIN_BODY_RADIUS: f32 = 8.0;
const PIN_BODY_OFFSET_Y: f32 = -19.0;
const PIN_CLICK_DISTANCE: f32 = 12.0;

fn pin_body_center(tip: Pos2) -> Pos2 {
    tip + egui::vec2(0.0, PIN_BODY_OFFSET_Y)
}

fn draw_pin(painter: &egui::Painter, tip: Pos2, highlighted: bool) {
    let scale = if highlighted { 1.4 } else { 1.0 };
    let body_center = tip + egui::vec2(0.0, PIN_BODY_OFFSET_Y * scale);
    let fill = if highlighted {
        Color32::from_rgb(72, 182, 255)
    } else {
        Color32::from_rgb(31, 123, 210)
    };
    let stroke = egui::Stroke::new(1.0, Color32::from_rgb(22, 86, 145));
    let body_radius = PIN_BODY_RADIUS * scale;
    let tail_half_width = 5.0 * scale;
    let tail_base_y = 3.4 * scale;

    // Tail triangle to emulate a simple map-pin shape.
    painter.add(egui::Shape::convex_polygon(
        vec![
            body_center + egui::vec2(-tail_half_width, tail_base_y),
            body_center + egui::vec2(tail_half_width, tail_base_y),
            tip,
        ],
        fill,
        stroke,
    ));

    painter.circle_filled(body_center, body_radius, fill);
    painter.circle_stroke(body_center, body_radius, stroke);
    painter.circle_filled(body_center, 3.6 * scale, Color32::WHITE);
}

pub struct GpxWaypointMarkers {
    pub waypoints: Vec<(WaypointSelection, walkers::Position, String)>,
    pub waypoint_tool_enabled: bool,
    pub window_highlight_waypoint: Option<WaypointSelection>,
    pub clicked_waypoint: ClickedWaypoint,
    pub add_waypoint_request: PendingAddWaypointRequest,
}

fn nearest_waypoint(
    pointer: Pos2,
    waypoints: &[(WaypointSelection, walkers::Position, String)],
    projector: &walkers::Projector,
) -> Option<(WaypointSelection, f32)> {
    let mut best: Option<(WaypointSelection, f32)> = None;

    for (selection, position, _) in waypoints {
        let tip = projector.project(*position).to_pos2();
        let center = pin_body_center(tip);
        let distance = pointer.distance(center);
        match best {
            Some((_, best_distance)) if distance >= best_distance => {}
            _ => best = Some((*selection, distance)),
        }
    }

    best.and_then(|(selection, distance)| {
        if distance <= PIN_CLICK_DISTANCE {
            Some((selection, distance))
        } else {
            None
        }
    })
}

impl Plugin for GpxWaypointMarkers {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &walkers::Projector,
        _map_memory: &walkers::MapMemory,
    ) {
        let hovered = response
            .hover_pos()
            .and_then(|pointer_pos| nearest_waypoint(pointer_pos, &self.waypoints, projector))
            .map(|(selection, _)| selection);

        for (selection, position, _) in &self.waypoints {
            let tip = projector.project(*position).to_pos2();
            let highlighted =
                hovered == Some(*selection) || self.window_highlight_waypoint == Some(*selection);
            draw_pin(ui.painter(), tip, highlighted);
        }

        if let Some(hovered_selection) = hovered {
            if let Some((_, position, description)) = self
                .waypoints
                .iter()
                .find(|(selection, _, _)| *selection == hovered_selection)
            {
                if !description.trim().is_empty() {
                    let tip = projector.project(*position).to_pos2();
                    let tooltip_pos =
                        pin_body_center(tip) + egui::vec2(0.0, -PIN_BODY_RADIUS - 4.0);
                    let tooltip_id = egui::Id::new(("waypoint_desc", hovered_selection));
                    let mut tooltip = egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        response.layer_id,
                        tooltip_id,
                        egui::PopupAnchor::Position(tooltip_pos),
                    )
                    .gap(2.0);
                    tooltip.popup = tooltip.popup.align(egui::RectAlign::TOP);
                    tooltip.show(|ui| {
                        ui.label(description);
                    });
                }
            }
        }

        if response.clicked_by(PointerButton::Primary) {
            let Some(pointer_pos) = response.interact_pointer_pos() else {
                return;
            };

            if let Some((selection, _)) = nearest_waypoint(pointer_pos, &self.waypoints, projector)
            {
                if let Ok(mut clicked_waypoint) = self.clicked_waypoint.lock() {
                    *clicked_waypoint = Some(selection);
                }
                return;
            }

            if self.waypoint_tool_enabled {
                let map_position = projector.unproject(pointer_pos.to_vec2());
                if let Ok(mut request) = self.add_waypoint_request.lock() {
                    *request = Some(map_position);
                }
            }
        }
    }
}
