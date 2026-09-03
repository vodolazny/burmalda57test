// Профиль (веб): открытие внешних ссылок.
//
// Аватар на вебе не выбирается (нет интеграции с файловым пикером) —
// показывается заглушка; остальной функционал профиля не зависит от этого.

use web_sys::window;

/// Открыть URL в новой вкладке браузера.
pub(crate) fn open_url(url: &str) {
    if let Some(win) = window() {
        if let Err(e) = win.open_with_url_and_target(url, "_blank") {
            log::warn!("open_url({url}) failed: {e:?}");
        }
    }
}

/// Выбор аватара: на вебе пока не реализован.
pub(crate) fn pick_avatar() {
    log::info!("pick_avatar: на веб-версии не поддерживается");
}

/// Пере-синхронизировать аватар с UI (заглушка).
#[allow(dead_code)]
pub(crate) fn reset() {}

/// Явно удалить локальный аватар (заглушка).
#[allow(dead_code)]
pub(crate) fn delete_avatar() {}

/// Совместимость с прежним вызовом при старте.
pub(crate) fn init_profile(_storage_path: String) {}
