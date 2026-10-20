// Аватар пользователя: обычный файл avatar.jpg в приватном каталоге.
// Байты приходят из нативного выбора картинки (Android/iOS-оболочки).

use std::path::PathBuf;
use std::sync::Mutex;

use crate::ui_bridge;

const AVATAR_FILE: &str = "avatar.jpg";

// Каталог задаётся до восстановления сессии: аватар не должен зависеть
// от того, когда загрузился дневник.
static AVATAR_DIR: Mutex<Option<String>> = Mutex::new(None);

pub(crate) fn init(storage_path: &str) {
    if !storage_path.is_empty() {
        *AVATAR_DIR.lock().unwrap() = Some(storage_path.to_string());
    }
    reload();
}

pub(crate) fn set_bytes(bytes: &[u8]) {
    if let Some(path) = avatar_path() {
        if let Err(e) = std::fs::write(&path, bytes) {
            log::warn!("Не удалось сохранить аватар: {e:?}");
        }
    }
    reload();
}

pub(crate) fn delete() {
    if let Some(path) = avatar_path() {
        let _ = std::fs::remove_file(path);
    }
    ui_bridge::reload_avatar(None);
}

fn avatar_path() -> Option<PathBuf> {
    AVATAR_DIR
        .lock()
        .unwrap()
        .as_ref()
        .map(|dir| PathBuf::from(dir).join(AVATAR_FILE))
}

fn reload() {
    ui_bridge::reload_avatar(avatar_path());
}
