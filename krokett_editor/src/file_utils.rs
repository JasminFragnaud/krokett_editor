use std::sync::mpsc::Sender;

#[cfg(target_os = "android")]
use std::path::PathBuf;

use anyhow::{Result, anyhow};

use crate::task_utils::execute;

pub type FileName = String;

pub struct FileContent {
    pub name: FileName,
    pub data: Vec<u8>,
}

#[cfg(not(target_os = "android"))]
pub fn load_file(sender: Sender<FileContent>) {
    let task = rfd::AsyncFileDialog::new().pick_file();
    execute(async move {
        let Some(file) = task.await else { return };
        let content = FileContent {
            name: file.file_name(),
            data: file.read().await,
        };
        if let Err(e) = sender.send(content) {
            log::error!("Erreur lors de l'envoi du contenu du fichier sur le canal : {e}");
        }
    });
}

#[cfg(target_os = "android")]
pub fn load_file_from_path(path: PathBuf, sender: Sender<FileContent>) {
    execute(async move {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| path.display().to_string());

        match std::fs::read(&path) {
            Ok(data) => {
                let content = FileContent {
                    name: file_name,
                    data,
                };

                if let Err(e) = sender.send(content) {
                    log::error!("Erreur lors de l'envoi du contenu du fichier sur le canal : {e}");
                }
            }
            Err(e) => {
                log::error!("Erreur lors du chargement du fichier GPX {} : {e}", path.display());
            }
        }
    });
}

#[cfg(not(target_os = "android"))]
pub fn save_as(content: FileContent, sender: Sender<Result<FileName>>) {
    let task = rfd::AsyncFileDialog::new()
        .set_file_name(content.name)
        .save_file();
    execute(async move {
        let Some(file) = task.await else { return };
        let file_name = match file.write(&content.data).await {
            Ok(()) => Ok(file.file_name()),
            Err(e) => Err(anyhow!("Erreur lors de la sauvegarde du fichier : {e}")),
        };
        if let Err(e) = sender.send(file_name) {
            log::error!("Erreur lors de l'envoi du nom du fichier sur le canal : {e}");
        }
    });
}

#[cfg(target_os = "android")]
pub fn save_as_to_path(path: PathBuf, content: FileContent, sender: Sender<Result<FileName>>) {
    execute(async move {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| content.name.clone());

        let save_result = match std::fs::write(&path, &content.data) {
            Ok(()) => Ok(file_name),
            Err(e) => Err(anyhow!(
                "Erreur lors de la sauvegarde du fichier {} : {e}",
                path.display()
            )),
        };

        if let Err(e) = sender.send(save_result) {
            log::error!("Erreur lors de l'envoi du nom du fichier sur le canal : {e}");
        }
    });
}
