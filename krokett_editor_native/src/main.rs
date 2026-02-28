#[cfg(not(target_arch = "wasm32"))]
use krokett_editor::MyApp;

#[cfg(not(target_arch = "wasm32"))]
const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), eframe::Error> {
    env_logger::init();

    let app_icon = eframe::icon_data::from_png_bytes(APP_ICON_PNG)
        .expect("assets/icon.png must be a valid PNG");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_icon(app_icon),
        ..Default::default()
    };

    eframe::run_native(
        "krokett_editor",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc.egui_ctx.clone())))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    println!("This demo is not meant to be compiled for WASM.");
}
