// Платформенные хелперы веба: ввод cookie X1_SSO, внешние ссылки, «toast».
//
// В браузере нельзя открыть чужую страницу и прочитать её cookie, поэтому
// пользователь сам вставляет значение X1_SSO через window.prompt. Дальше
// работает общий login-флоу core.

use web_sys::window;

// «Toast» на вебе — сообщение в консоль.
pub(crate) fn show_toast(text: &str) {
    log::info!("{}", text);
}

pub(crate) fn launch_login_flow() {
    let Some(win) = window() else {
        return;
    };
    match win.prompt_with_message(
        "Вход через Госуслуги: откройте obr57.ru/gis, войдите и вставьте сюда значение cookie X1_SSO",
    ) {
        Ok(Some(token)) if !token.trim().is_empty() => {
            crate::app::handle_token(&token);
        }
        _ => {
            crate::ui_bridge::apply_login_error(
                "Токен не введён. Скопируйте значение cookie X1_SSO из браузера.",
            );
        }
    }
}

pub(crate) fn open_url(url: &str) {
    if let Some(win) = window() {
        if let Err(e) = win.open_with_url_and_target(url, "_blank") {
            log::warn!("open_url({url}) failed: {e:?}");
        }
    }
}

pub(crate) fn pick_avatar() {
    log::info!("pick_avatar: на веб-версии не поддерживается");
}
