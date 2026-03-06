#[cfg(target_os = "android")]
use jni::objects::{JByteArray, JClass, JObject, JString};

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

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_github_khep_krokett_1editor_MainActivity_nativeOnGpxOpened(
    mut env: jni::JNIEnv,
    _class: JClass,
    name_obj: JObject,
    data_obj: JObject,
    error_obj: JObject,
) {
    let name = if name_obj.is_null() {
        None
    } else {
        env.get_string(&JString::from(name_obj))
            .ok()
            .map(|s| s.into())
    };

    let data = if data_obj.is_null() {
        None
    } else {
        env.convert_byte_array(JByteArray::from(data_obj)).ok()
    };

    let error = if error_obj.is_null() {
        None
    } else {
        env.get_string(&JString::from(error_obj))
            .ok()
            .map(|s| s.into())
    };

    krokett_editor::android_intent_io::push_open_result(name, data, error);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_github_khep_krokett_1editor_MainActivity_nativeOnGpxSaved(
    mut env: jni::JNIEnv,
    _class: JClass,
    file_name_obj: JObject,
    error_obj: JObject,
) {
    let file_name = if file_name_obj.is_null() {
        None
    } else {
        env.get_string(&JString::from(file_name_obj))
            .ok()
            .map(|s| s.into())
    };

    let error = if error_obj.is_null() {
        None
    } else {
        env.get_string(&JString::from(error_obj))
            .ok()
            .map(|s| s.into())
    };

    krokett_editor::android_intent_io::push_save_result(file_name, error);
}
