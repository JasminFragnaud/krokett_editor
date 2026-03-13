#[cfg(target_os = "android")]
use jni::objects::{JByteArray, JClass, JObject, JString};

#[cfg(target_os = "android")]
use egui_winit::winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
use std::time::Duration;

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
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);

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

        if !ctx.wants_keyboard_input() {
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

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_github_khep_krokett_1editor_MainActivity_nativeOnLocationUpdated(
    mut env: jni::JNIEnv,
    _class: JClass,
    latitude: jni_sys::jdouble,
    longitude: jni_sys::jdouble,
    error_obj: JObject,
) {
    let error = if error_obj.is_null() {
        None
    } else {
        env.get_string(&JString::from(error_obj))
            .ok()
            .map(|s| s.into())
    };

    let latitude = if latitude.is_nan() {
        None
    } else {
        Some(latitude as f64)
    };

    let longitude = if longitude.is_nan() {
        None
    } else {
        Some(longitude as f64)
    };

    krokett_editor::geolocation::push_android_location_result(latitude, longitude, error);
}
