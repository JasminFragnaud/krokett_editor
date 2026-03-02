use std::collections::BTreeMap;

use eframe::epaint::Shadow;
use egui::{
    Color32, CornerRadius, FontFamily, FontId, Stroke, Style, TextStyle, Visuals,
    style::{HandleShape, NumericColorSpace, Selection, WidgetVisuals, Widgets},
};

pub fn amoled_friendly() -> Style {
    let widgets = Widgets {
        noninteractive: WidgetVisuals {
            weak_bg_fill: Color32::from_gray(10),
            bg_fill: Color32::from_gray(10),
            bg_stroke: Stroke::new(1.0, Color32::from_gray(20)), // separators, indentation lines
            fg_stroke: Stroke::new(1.0, Color32::from_gray(150)), // normal text color
            corner_radius: CornerRadius::same(2),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            weak_bg_fill: Color32::from_gray(0), // button background
            bg_fill: Color32::from_gray(30),     // checkbox background
            bg_stroke: Default::default(),       // button border
            fg_stroke: Stroke::new(1.0, Color32::from_gray(180)), // button text
            corner_radius: CornerRadius::same(0),
            expansion: 1.0,
        },
        hovered: WidgetVisuals {
            weak_bg_fill: Color32::from_gray(30),
            bg_fill: Color32::from_gray(70),
            bg_stroke: Stroke::default(), /* new(1.0, Color32::from_gray(150)), // e.g. hover over window edge or button */
            fg_stroke: Stroke::new(1.5, Color32::from_gray(240)),
            corner_radius: CornerRadius::same(0),
            expansion: 1.0,
        },
        active: WidgetVisuals {
            weak_bg_fill: Color32::from_gray(55),
            bg_fill: Color32::from_gray(45),
            //bg_stroke: Default::default(), // button border
            bg_stroke: Stroke::new(1.0, Color32::from_gray(50)), // button border
            fg_stroke: Stroke::new(1.0, Color32::from_gray(140)), // normal text color
            corner_radius: CornerRadius::same(0),
            expansion: 1.0,
        },
        open: WidgetVisuals {
            weak_bg_fill: Color32::from_gray(27),
            bg_fill: Color32::from_gray(27),
            bg_stroke: Stroke::new(1.0, Color32::from_gray(30)),
            fg_stroke: Stroke::new(1.0, Color32::from_gray(210)),
            corner_radius: CornerRadius::same(2),
            expansion: 0.0,
        },
    };

    Style {
        text_styles: text_styles(),
        visuals: Visuals {
            dark_mode: true,
            override_text_color: None,
            widgets,
            selection: Selection::default(),
            hyperlink_color: Color32::from_gray(200),
            faint_bg_color: Color32::from_gray(35),
            extreme_bg_color: Color32::from_gray(10), // e.g. TextEdit background
            code_bg_color: Color32::from_gray(64),
            warn_fg_color: Color32::from_rgb(255, 143, 0), // orange
            error_fg_color: Color32::from_rgb(255, 0, 0),  // red

            window_corner_radius: CornerRadius::same(6),
            window_shadow: Shadow::NONE,
            window_fill: Color32::from_gray(0),
            window_stroke: Stroke::new(1.0, Color32::from_gray(40)),

            panel_fill: Color32::from_gray(0),

            popup_shadow: Shadow::NONE,
            resize_corner_size: 12.0,
            clip_rect_margin: 3.0, /* should be at least half the size of the widest frame stroke + max WidgetVisuals::expansion */
            button_frame: true,
            collapsing_header_frame: false,
            menu_corner_radius: CornerRadius::ZERO,
            indent_has_left_vline: false,
            striped: false,
            slider_trailing_fill: true,
            text_cursor: Default::default(),
            interact_cursor: Default::default(),
            image_loading_spinners: Default::default(),
            handle_shape: HandleShape::Circle,
            window_highlight_topmost: false,
            numeric_color_space: NumericColorSpace::GammaByte,
            ..Default::default()
        },
        ..Style::default()
    }
}

fn text_styles() -> BTreeMap<TextStyle, FontId> {
    use FontFamily::{Monospace, Proportional};

    [
        (TextStyle::Small, FontId::new(12.0, Proportional)),
        (TextStyle::Body, FontId::new(12., Proportional)),
        (TextStyle::Button, FontId::new(20.0, Proportional)),
        (TextStyle::Heading, FontId::new(20.0, Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, Monospace)),
    ]
    .into()
}
