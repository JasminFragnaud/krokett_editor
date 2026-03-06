#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(
    app: egui_winit::winit::platform::android::activity::AndroidApp,
) -> Result<(), Box<dyn std::error::Error>> {
    use eframe::{NativeOptions, Renderer};

    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("krokett_editor")
            .with_max_level(log::LevelFilter::Info),
    );
    let mut options = NativeOptions::default();
    options.renderer = Renderer::Wgpu;
    options.android_app = Some(app);
    eframe::run_native(
        "krokett_editor",
        options,
        Box::new(|cc| Ok(Box::new(krokett_editor::MyApp::new(cc.egui_ctx.clone())))),
    )?;

    Ok(())
}

#[cfg(target_os = "android")]
#[no_mangle]
pub unsafe extern "system" fn Java_com_github_khep_krokett_1editor_MainActivity_setAppInBackground(
    _env: *mut jni_sys::JNIEnv,
    _class: jni_sys::jclass,
    is_background: jni_sys::jboolean,
) {
    log::info!("App moved to background: {is_background}");
}
