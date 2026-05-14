use egui::{Align2, Color32, Frame, Stroke, Ui, Vec2, Window};
use walkers::{MapMemory, Position};

pub fn scale_bar(ui: &Ui, map_memory: &MapMemory, my_position: Position) {
    const EARTH_RADIUS_METERS: f64 = 6_378_137.0;
    const TILE_SIZE_PX: f64 = 256.0;
    const MAX_BAR_WIDTH_PX: f32 = 130.0;

    let center = map_memory.detached().unwrap_or(my_position);
    let latitude_deg = center.y().clamp(-85.051_128_78, 85.051_128_78);
    let latitude_rad = latitude_deg.to_radians();
    let zoom = map_memory.zoom();

    let meters_per_pixel = latitude_rad.cos() * (2.0 * std::f64::consts::PI * EARTH_RADIUS_METERS)
        / (TILE_SIZE_PX * 2.0_f64.powf(zoom));

    if !meters_per_pixel.is_finite() || meters_per_pixel <= 0.0 {
        return;
    }

    let target_meters = meters_per_pixel * MAX_BAR_WIDTH_PX as f64;
    let bar_meters = nice_scale_distance(target_meters);
    let bar_width = (bar_meters / meters_per_pixel) as f32;

    Window::new("Échelle carte")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .frame(Frame::NONE)
        .anchor(Align2::RIGHT_BOTTOM, [-10.0, -10.0])
        .show(ui.ctx(), |ui| {
            let text = format_scale_label(bar_meters);
            let text_font = egui::TextStyle::Body.resolve(ui.style());
            let dark_mode = ui.style().visuals.dark_mode;
            let fg = if dark_mode {
                Color32::WHITE
            } else {
                Color32::BLACK
            };
            let halo = if dark_mode {
                Color32::from_black_alpha(170)
            } else {
                Color32::from_white_alpha(190)
            };
            let bg = if dark_mode {
                Color32::from_black_alpha(145)
            } else {
                Color32::from_white_alpha(205)
            };

            let galley = ui
                .painter()
                .layout_no_wrap(text.clone(), text_font.clone(), fg);

            let content_width = bar_width.max(galley.size().x);
            let total_size = Vec2::new(content_width + 10.0, galley.size().y + 20.0);
            let (rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());

            let painter = ui.painter_at(rect);
            let baseline_y = rect.bottom() - 4.0;
            let line_start_x = rect.left() + 3.0;
            let line_end_x = line_start_x + bar_width;

            let stroke = Stroke::new(2.0, fg);
            let text_bg = egui::Rect::from_min_size(
                egui::pos2(line_start_x - 3.0, rect.top()),
                Vec2::new(galley.size().x + 6.0, galley.size().y + 1.0),
            );
            let bar_bg = egui::Rect::from_min_max(
                egui::pos2(line_start_x - 3.0, baseline_y - 8.0),
                egui::pos2(line_end_x + 3.0, baseline_y + 2.0),
            );

            painter.rect_filled(text_bg, 2.0, bg);
            painter.rect_filled(bar_bg, 2.0, bg);
            painter.rect_stroke(
                text_bg,
                2.0,
                Stroke::new(1.0, Color32::from_black_alpha(70)),
                egui::StrokeKind::Outside,
            );
            painter.rect_stroke(
                bar_bg,
                2.0,
                Stroke::new(1.0, Color32::from_black_alpha(70)),
                egui::StrokeKind::Outside,
            );

            let halo_stroke = Stroke::new(4.0, halo);
            painter.line_segment(
                [
                    egui::pos2(line_start_x, baseline_y),
                    egui::pos2(line_end_x, baseline_y),
                ],
                halo_stroke,
            );
            let tick_height = 7.0;
            painter.line_segment(
                [
                    egui::pos2(line_start_x, baseline_y - tick_height),
                    egui::pos2(line_start_x, baseline_y),
                ],
                halo_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(line_end_x, baseline_y - tick_height),
                    egui::pos2(line_end_x, baseline_y),
                ],
                halo_stroke,
            );

            painter.line_segment(
                [
                    egui::pos2(line_start_x, baseline_y),
                    egui::pos2(line_end_x, baseline_y),
                ],
                stroke,
            );

            painter.line_segment(
                [
                    egui::pos2(line_start_x, baseline_y - tick_height),
                    egui::pos2(line_start_x, baseline_y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(line_end_x, baseline_y - tick_height),
                    egui::pos2(line_end_x, baseline_y),
                ],
                stroke,
            );

            painter.text(
                egui::pos2(line_start_x + 1.0, rect.top() + 1.0),
                Align2::LEFT_TOP,
                text.clone(),
                text_font.clone(),
                halo,
            );
            painter.text(
                egui::pos2(line_start_x + 0.6, rect.top()),
                Align2::LEFT_TOP,
                text.clone(),
                text_font.clone(),
                fg,
            );
            painter.text(
                egui::pos2(line_start_x, rect.top()),
                Align2::LEFT_TOP,
                text,
                text_font,
                fg,
            );
        });
}

fn nice_scale_distance(max_distance_meters: f64) -> f64 {
    if max_distance_meters <= 0.0 {
        return 1.0;
    }

    let exponent = max_distance_meters.log10().floor();
    let base = 10.0_f64.powf(exponent);

    for factor in [5.0, 2.0, 1.0] {
        let candidate = factor * base;
        if candidate <= max_distance_meters {
            return candidate;
        }
    }

    1.0
}

fn format_scale_label(distance_meters: f64) -> String {
    if distance_meters >= 1_000.0 {
        let km = distance_meters / 1_000.0;
        if (km.fract()).abs() < f64::EPSILON {
            format!("{:.0} km", km)
        } else {
            format!("{km:.1} km")
        }
    } else {
        format!("{distance_meters:.0} m")
    }
}
