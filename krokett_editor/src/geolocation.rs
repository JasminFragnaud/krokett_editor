use std::sync::{Arc, Mutex};

#[cfg(target_os = "android")]
use std::time::{Duration, Instant};

use walkers::{Position, lon_lat};

const IP_GEOLOCATION_URL: &str = "https://ipapi.co/json/";

#[cfg(target_os = "android")]
const ANDROID_PRECISE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[cfg(target_os = "android")]
fn android_results() -> &'static Mutex<Vec<Result<Position, String>>> {
    use std::sync::OnceLock;

    static RESULTS: OnceLock<Mutex<Vec<Result<Position, String>>>> = OnceLock::new();
    RESULTS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(target_os = "android")]
pub fn push_android_location_result(
    latitude: Option<f64>,
    longitude: Option<f64>,
    error: Option<String>,
) {
    let result = match error {
        Some(error) => Err(error),
        None => match (latitude, longitude) {
            (Some(lat), Some(lon)) => Ok(lon_lat(lon, lat)),
            _ => Err("Coordonnees GPS invalides".to_owned()),
        },
    };

    if let Ok(mut queue) = android_results().lock() {
        queue.push(result);
    }
}

#[cfg(target_os = "android")]
fn drain_android_results() -> Vec<Result<Position, String>> {
    let Ok(mut queue) = android_results().lock() else {
        return Vec::new();
    };
    queue.drain(..).collect()
}

#[derive(Default)]
struct SharedState {
    position: Option<Position>,
}

#[derive(Default)]
pub(crate) struct GeolocationState {
    precise_tracking_started: bool,
    fallback_request_started: bool,
    shared: Arc<Mutex<SharedState>>,
    #[cfg(target_os = "android")]
    last_android_precise_request_at: Option<Instant>,
}

#[derive(serde::Deserialize)]
struct IpApiResponse {
    latitude: f64,
    longitude: f64,
}

impl GeolocationState {
    pub(crate) fn update(&mut self) {
        #[cfg(target_os = "android")]
        {
            self.update_android_precise_position();
            if self.position().is_none() {
                self.start_fallback_lookup();
            }
            return;
        }

        #[cfg(not(target_os = "android"))]
        {
            if self.position().is_some() {
                return;
            }

            if !self.precise_tracking_started {
                self.precise_tracking_started = true;
                if start_precise_location_request(self.shared.clone()) {
                    return;
                }
            }

            self.start_fallback_lookup();
        }
    }

    pub(crate) fn position(&self) -> Option<Position> {
        self.shared.lock().ok().and_then(|state| state.position)
    }

    pub(crate) fn has_position(&self) -> bool {
        self.position().is_some()
    }

    #[cfg(target_os = "android")]
    fn store_position(&self, position: Position) {
        if let Ok(mut state) = self.shared.lock() {
            state.position = Some(position);
        }
    }

    #[cfg(target_os = "android")]
    fn update_android_precise_position(&mut self) {
        for result in drain_android_results() {
            match result {
                Ok(position) => self.store_position(position),
                Err(error) => {
                    log::warn!("Android geolocation failed: {error}");
                    self.start_fallback_lookup();
                }
            }
        }

        let should_request = self
            .last_android_precise_request_at
            .map(|last| last.elapsed() >= ANDROID_PRECISE_REFRESH_INTERVAL)
            .unwrap_or(true);

        if !should_request {
            return;
        }

        self.last_android_precise_request_at = Some(Instant::now());
        self.precise_tracking_started = true;

        if !start_precise_location_request(self.shared.clone()) {
            self.start_fallback_lookup();
        }
    }

    fn start_fallback_lookup(&mut self) {
        if self.fallback_request_started {
            return;
        }

        self.fallback_request_started = true;
        request_ip_location(self.shared.clone());
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn start_precise_location_request(_shared: Arc<Mutex<SharedState>>) -> bool {
    false
}

#[cfg(target_os = "android")]
fn start_precise_location_request(_shared: Arc<Mutex<SharedState>>) -> bool {
    use jni::{JavaVM, objects::JObject};

    let ctx = ndk_context::android_context();
    let vm = match unsafe { JavaVM::from_raw(ctx.vm().cast()) } {
        Ok(vm) => vm,
        Err(error) => {
            log::warn!("Impossible de recuperer la VM Android pour la geolocalisation: {error}");
            return false;
        }
    };

    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(error) => {
            log::warn!("Impossible d'attacher le thread JNI pour la geolocalisation: {error}");
            return false;
        }
    };

    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let class = match env.get_object_class(&activity) {
        Ok(class) => class,
        Err(error) => {
            log::warn!("Classe MainActivity introuvable pour la geolocalisation: {error}");
            return false;
        }
    };

    if let Err(error) = env.call_static_method(class, "requestDeviceLocation", "()V", &[]) {
        log::warn!("Echec de requestDeviceLocation: {error}");
        return false;
    }

    true
}

#[cfg(target_arch = "wasm32")]
fn start_precise_location_request(shared: Arc<Mutex<SharedState>>) -> bool {
    use wasm_bindgen::{JsCast, closure::Closure};

    let Some(window) = web_sys::window() else {
        return false;
    };

    let Ok(geolocation) = window.navigator().geolocation() else {
        return false;
    };

    let success_shared = shared.clone();
    let success = Closure::wrap(Box::new(move |position: web_sys::Position| {
        let coords = position.coords();
        if let Ok(mut state) = success_shared.lock() {
            state.position = Some(lon_lat(coords.longitude(), coords.latitude()));
        }
    }) as Box<dyn FnMut(web_sys::Position)>);

    let failure_shared = shared.clone();
    let failure = Closure::wrap(Box::new(move |error: web_sys::PositionError| {
        log::warn!(
            "Browser geolocation failed (code {}): {}",
            error.code(),
            error.message()
        );
        request_ip_location(failure_shared.clone());
    }) as Box<dyn FnMut(web_sys::PositionError)>);

    let success_fn: &js_sys::Function = success.as_ref().unchecked_ref();
    let failure_fn: &js_sys::Function = failure.as_ref().unchecked_ref();

    let call_result = geolocation.watch_position_with_error_callback(success_fn, Some(failure_fn));
    if call_result.is_err() {
        return false;
    }

    success.forget();
    failure.forget();
    true
}

fn request_ip_location(shared: Arc<Mutex<SharedState>>) {
    let request = ehttp::Request::get(IP_GEOLOCATION_URL);
    ehttp::fetch(request, move |result| {
        let maybe_position = match result {
            Ok(response) if response.ok => response
                .text()
                .and_then(|text| serde_json::from_str::<IpApiResponse>(text).ok())
                .map(|payload| lon_lat(payload.longitude, payload.latitude)),
            Ok(response) => {
                log::warn!(
                    "Geolocation lookup failed with status {} {}",
                    response.status,
                    response.status_text
                );
                None
            }
            Err(error) => {
                log::warn!("Geolocation lookup request failed: {error}");
                None
            }
        };

        if let Ok(mut state) = shared.lock() {
            state.position = maybe_position;
        }
    });
}
