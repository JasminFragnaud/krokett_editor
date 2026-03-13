#[cfg(target_os = "android")]
use jni::objects::{JByteArray, JClass, JObject, JString};

#[cfg(target_os = "android")]
use egui_winit::winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
struct AndroidTextInputWorkaroundApp {
    inner: krokett_editor::MyApp,
    android_app: AndroidApp,
    last_text_state: String,
}

#[cfg(target_os = "android")]
impl AndroidTextInputWorkaroundApp {
    fn new(egui_ctx: eframe::egui::Context, android_app: AndroidApp) -> Self {
        let last_text_state = android_app.text_input_state().text;
        Self {
            inner: krokett_editor::MyApp::new(egui_ctx),
            android_app,
            last_text_state,
        }
    }
}

#[cfg(target_os = "android")]
impl eframe::App for AndroidTextInputWorkaroundApp {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &eframe::egui::Context, raw_input: &mut eframe::egui::RawInput) {
        self.inner.raw_input_hook(ctx, raw_input);

        // Work around missing text payloads in Android keyboard events for this stack.
        let state = self.android_app.text_input_state();
        if state.text != self.last_text_state {
            if let Some(inserted_text) = state.text.strip_prefix(&self.last_text_state) {
                if !inserted_text.is_empty() {
                    raw_input
                        .events
                        .push(eframe::egui::Event::Text(inserted_text.to_owned()));
                }
            }

            self.last_text_state = state.text;
        }
    }
}

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
    options.android_app = Some(app.clone());
    eframe::run_native(
        "krokett_editor",
        options,
        Box::new(move |cc| {
            Ok(Box::new(AndroidTextInputWorkaroundApp::new(
                cc.egui_ctx.clone(),
                app.clone(),
            )))
        }),
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
