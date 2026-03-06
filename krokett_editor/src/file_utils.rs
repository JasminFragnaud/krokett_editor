use std::sync::mpsc::Sender;

use anyhow::{Result, anyhow};

use crate::task_utils::execute;

pub type FileName = String;

pub struct FileContent {
    pub name: FileName,
    pub data: Vec<u8>,
}

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
