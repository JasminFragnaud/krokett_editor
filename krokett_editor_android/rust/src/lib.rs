#[cfg(target_os = "android")]
use egui_winit::winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
use eframe::egui::Ui;

#[cfg(target_os = "android")]
use std::time::Duration;

// Helper to convert JObject to String via JNI
#[cfg(target_os = "android")]
unsafe fn jobject_to_string(
    env_ptr: *mut jni_sys::JNIEnv,
    obj: jni_sys::jobject,
) -> Option<String> {
    if obj.is_null() {
        return None;
    }

    let env = &**env_ptr;
    let jstring = obj as jni_sys::jstring;
    let cstr = (env.v1_1.GetStringUTFChars)(env_ptr, jstring, std::ptr::null_mut());
    if cstr.is_null() {
        return None;
    }

    let result = std::ffi::CStr::from_ptr(cstr).to_string_lossy().to_string();
    (env.v1_1.ReleaseStringUTFChars)(env_ptr, jstring, cstr);
    Some(result)
}

// Helper to convert JObject to byte array via JNI
#[cfg(target_os = "android")]
unsafe fn jobject_to_bytes(
    env_ptr: *mut jni_sys::JNIEnv,
    obj: jni_sys::jobject,
) -> Option<Vec<u8>> {
    if obj.is_null() {
        return None;
    }

    let env = &**env_ptr;
    let jarray = obj as jni_sys::jbyteArray;
    let len = (env.v1_1.GetArrayLength)(env_ptr, jarray as jni_sys::jarray) as usize;

    let buf = (env.v1_1.GetByteArrayElements)(env_ptr, jarray, std::ptr::null_mut());
    if buf.is_null() {
        return None;
    }

    let slice = std::slice::from_raw_parts(buf as *const u8, len);
    let result = slice.to_vec();
    (env.v1_1.ReleaseByteArrayElements)(env_ptr, jarray, buf, 0);
    Some(result)
}

#[cfg(target_os = "android")]
struct AndroidTextInputWorkaroundApp {
    inner: krokett_editor::MyApp,
    android_app: AndroidApp,
    last_text_state: String,
    ime_active: bool,
}

#[cfg(target_os = "android")]
impl AndroidTextInputWorkaroundApp {
    fn new(egui_ctx: eframe::egui::Context, android_app: AndroidApp) -> Self {
        let last_text_state = android_app.text_input_state().text;
        Self {
            inner: krokett_editor::MyApp::new(egui_ctx),
            android_app,
            last_text_state,
            ime_active: false,
        }
    }

    fn diff_text(prev: &str, curr: &str) -> (usize, String) {
        let prev_chars: Vec<char> = prev.chars().collect();
        let curr_chars: Vec<char> = curr.chars().collect();

        let mut prefix = 0usize;
        while prefix < prev_chars.len()
            && prefix < curr_chars.len()
            && prev_chars[prefix] == curr_chars[prefix]
        {
            prefix += 1;
        }

        let mut suffix = 0usize;
        while suffix < (prev_chars.len() - prefix)
            && suffix < (curr_chars.len() - prefix)
            && prev_chars[prev_chars.len() - 1 - suffix]
                == curr_chars[curr_chars.len() - 1 - suffix]
        {
            suffix += 1;
        }

        let deleted = prev_chars.len().saturating_sub(prefix + suffix);
        let inserted: String = curr_chars[prefix..(curr_chars.len() - suffix)]
            .iter()
            .collect();

        (deleted, inserted)
    }

    fn strip_native_text_input_events(raw_input: &mut eframe::egui::RawInput) {
        raw_input.events.retain(|event| {
            !matches!(event, eframe::egui::Event::Text(_))
                && !matches!(
                    event,
                    eframe::egui::Event::Ime(eframe::egui::ImeEvent::Commit(_))
                )
                && !matches!(
                    event,
                    eframe::egui::Event::Key {
                        key: eframe::egui::Key::Backspace,
                        ..
                    }
                )
        });
    }
}

#[cfg(target_os = "android")]
impl eframe::App for AndroidTextInputWorkaroundApp {
    fn ui(&mut self, ctx: &mut Ui, frame: &mut eframe::Frame) {
        self.inner.ui(ctx, frame);

        // Keep a small repaint cadence only while IME text editing is active.
        if self.ime_active {
            ctx.request_repaint_after(Duration::from_millis(8));
        }
    }

    fn raw_input_hook(
        &mut self,
        ctx: &eframe::egui::Context,
        raw_input: &mut eframe::egui::RawInput,
    ) {
        self.inner.raw_input_hook(ctx, raw_input);

        #[allow(deprecated)]
        let wants_input = ctx.wants_keyboard_input();
        if !wants_input {
            self.ime_active = false;
            return;
        }

        if !self.ime_active {
            self.last_text_state = self.android_app.text_input_state().text;
            self.ime_active = true;
            return;
        }

        // Deterministic pipeline: remove native text/backspace events while editing,
        // then inject exactly what changed according to IME state.
        Self::strip_native_text_input_events(raw_input);

        let state = self.android_app.text_input_state();
        let (deleted, inserted) = Self::diff_text(&self.last_text_state, &state.text);
        let mut deleted = deleted;
        let mut inserted = inserted;

        // Some devices report duplicated IME deltas (e.g. "11" for one key or
        // two deletes for one backspace). Compress these specific patterns.
        if deleted == 0 {
            let mut chars = inserted.chars();
            if let Some(first) = chars.next() {
                if chars.clone().next().is_some() && chars.all(|c| c == first) {
                    inserted = first.to_string();
                }
            }
        }

        if inserted.is_empty() && deleted > 1 {
            deleted = 1;
        }
        if state.text != self.last_text_state {
            for _ in 0..deleted {
                raw_input.events.push(eframe::egui::Event::Key {
                    key: eframe::egui::Key::Backspace,
                    physical_key: Some(eframe::egui::Key::Backspace),
                    pressed: true,
                    repeat: false,
                    modifiers: eframe::egui::Modifiers::default(),
                });
                raw_input.events.push(eframe::egui::Event::Key {
                    key: eframe::egui::Key::Backspace,
                    physical_key: Some(eframe::egui::Key::Backspace),
                    pressed: false,
                    repeat: false,
                    modifiers: eframe::egui::Modifiers::default(),
                });
            }

            if !inserted.is_empty() {
                raw_input.events.push(eframe::egui::Event::Text(inserted));
            }

            if deleted > 0 || !state.text.is_empty() {
                // Ask for another frame immediately so the newly injected input is painted fast.
                ctx.request_repaint();
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
    env: *mut jni_sys::JNIEnv,
    _class: jni_sys::jclass,
    name_obj: jni_sys::jobject,
    data_obj: jni_sys::jobject,
    error_obj: jni_sys::jobject,
) {
    unsafe {
        let name = jobject_to_string(env, name_obj);
        let data = jobject_to_bytes(env, data_obj);
        let error = jobject_to_string(env, error_obj);
        krokett_editor::android_intent_io::push_open_result(name, data, error);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_github_khep_krokett_1editor_MainActivity_nativeOnGpxSaved(
    env: *mut jni_sys::JNIEnv,
    _class: jni_sys::jclass,
    file_name_obj: jni_sys::jobject,
    error_obj: jni_sys::jobject,
) {
    unsafe {
        let file_name = jobject_to_string(env, file_name_obj);
        let error = jobject_to_string(env, error_obj);
        krokett_editor::android_intent_io::push_save_result(file_name, error);
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_github_khep_krokett_1editor_MainActivity_nativeOnLocationUpdated(
    env: *mut jni_sys::JNIEnv,
    _class: jni_sys::jclass,
    latitude: jni_sys::jdouble,
    longitude: jni_sys::jdouble,
    error_obj: jni_sys::jobject,
) {
    unsafe {
        let error = jobject_to_string(env, error_obj);
        let latitude_opt = if latitude.is_nan() {
            None
        } else {
            Some(latitude as f64)
        };
        let longitude_opt = if longitude.is_nan() {
            None
        } else {
            Some(longitude as f64)
        };
        krokett_editor::geolocation::push_android_location_result(
            latitude_opt,
            longitude_opt,
            error,
        );
    }
}
