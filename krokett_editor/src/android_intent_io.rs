#![cfg(target_os = "android")]

use std::sync::{Mutex, OnceLock};

use jni::{
    JavaVM,
    objects::{JClass, JObject, JValue},
};

use crate::file_utils::{FileContent, FileName};

fn open_results() -> &'static Mutex<Vec<Result<FileContent, String>>> {
    static OPEN_RESULTS: OnceLock<Mutex<Vec<Result<FileContent, String>>>> = OnceLock::new();
    OPEN_RESULTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn save_results() -> &'static Mutex<Vec<Result<FileName, String>>> {
    static SAVE_RESULTS: OnceLock<Mutex<Vec<Result<FileName, String>>>> = OnceLock::new();
    SAVE_RESULTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn main_activity_class<'a>(env: &mut jni::JNIEnv<'a>) -> Result<JClass<'a>, String> {
    let ctx = ndk_context::android_context();
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    env.get_object_class(activity)
        .map_err(|e| format!("Classe MainActivity introuvable : {e}"))
}

pub fn request_open_gpx() -> Result<(), String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|e| format!("Impossible de recuperer la VM Android : {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("Impossible d'attacher le thread JNI : {e}"))?;

    let class = main_activity_class(&mut env)?;

    env.call_static_method(class, "requestOpenGpx", "()V", &[])
        .map_err(|e| format!("Echec de requestOpenGpx : {e}"))?;
    Ok(())
}

pub fn request_save_gpx(file_name: String, data: Vec<u8>) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|e| format!("Impossible de recuperer la VM Android : {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("Impossible d'attacher le thread JNI : {e}"))?;

    let class = main_activity_class(&mut env)?;

    let j_file_name = env
        .new_string(file_name)
        .map_err(|e| format!("Impossible de convertir le nom de fichier : {e}"))?;
    let j_data = env
        .byte_array_from_slice(&data)
        .map_err(|e| format!("Impossible de convertir les donnees GPX : {e}"))?;

    env.call_static_method(
        class,
        "requestSaveGpx",
        "(Ljava/lang/String;[B)V",
        &[
            JValue::Object(&JObject::from(j_file_name)),
            JValue::Object(&JObject::from(j_data)),
        ],
    )
    .map_err(|e| format!("Echec de requestSaveGpx : {e}"))?;

    Ok(())
}

pub fn push_open_result(name: Option<String>, data: Option<Vec<u8>>, error: Option<String>) {
    let result = match error {
        Some(e) => Err(e),
        None => {
            let file_name = name.unwrap_or_else(|| "fichier.gpx".to_owned());
            let content = FileContent {
                name: file_name,
                data: data.unwrap_or_default(),
            };
            Ok(content)
        }
    };

    if let Ok(mut queue) = open_results().lock() {
        queue.push(result);
    }
}

pub fn push_save_result(file_name: Option<String>, error: Option<String>) {
    let result = match error {
        Some(e) => Err(e),
        None => Ok(file_name.unwrap_or_else(|| "fichier.gpx".to_owned())),
    };

    if let Ok(mut queue) = save_results().lock() {
        queue.push(result);
    }
}

pub fn drain_open_results() -> Vec<Result<FileContent, String>> {
    let Ok(mut queue) = open_results().lock() else {
        return Vec::new();
    };
    queue.drain(..).collect()
}

pub fn drain_save_results() -> Vec<Result<FileName, String>> {
    let Ok(mut queue) = save_results().lock() else {
        return Vec::new();
    };
    queue.drain(..).collect()
}
