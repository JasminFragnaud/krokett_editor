use egui::{Align2, Color32, FontId};
use walkers::{MapMemory, Plugin, Position, Projector};

const ICON_VISUAL_OFFSET_Y: f32 = -1.0;

pub(crate) struct PositionIndicator {
    pub(crate) position: Position,
}

impl Plugin for PositionIndicator {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        _response: &egui::Response,
        projector: &Projector,
        _map_memory: &MapMemory,
    ) {
        let center = projector.project(self.position).to_pos2();

        // Google-Maps-like marker: soft accuracy halo + static blue dot with icon.
        let base_blue = Color32::from_rgb(33, 150, 243);

        let halo_color = Color32::from_rgba_unmultiplied(33, 150, 243, 48);
        ui.painter().circle_filled(center, 20.0, halo_color);
        ui.painter().circle_filled(center, 8.0, base_blue);
        ui.painter().text(
            center + egui::vec2(0.0, ICON_VISUAL_OFFSET_Y),
            Align2::CENTER_CENTER,
            egui_material_icons::icons::ICON_MY_LOCATION,
            FontId::proportional(25.0),
            Color32::BLACK,
        );
    }
}
