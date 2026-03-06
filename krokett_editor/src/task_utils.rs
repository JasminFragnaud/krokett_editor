#[cfg(any(target_arch = "wasm32", not(target_os = "android")))]
use std::future::Future;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub fn execute<F: Future<Output = ()> + Send + 'static>(f: F) {
    std::thread::spawn(move || futures::executor::block_on(f));
}

#[cfg(target_arch = "wasm32")]
pub fn execute<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}
