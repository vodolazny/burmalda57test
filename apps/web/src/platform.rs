// Платформенные хелперы веб-версии (замена android.rs без JNI/Kotlin).
//
// Вход через ЕСИА: в браузере нельзя открыть чужую страницу и прочитать её
// cookie, поэтому пользователь сам вставляет значение cookie X1_SSO
// (логин/пароль приложению по-прежнему не нужны). Далее — тот же флоу,
// что был в Kotlin-WebView: токен отдаётся в login_with_token().
use std::sync::atomic::{AtomicBool, AtomicI32};

use web_sys::window;

use crate::bridge::{apply_login_error, apply_logging_in, apply_session_to_ui, show_child_picker_for_login};
use crate::cache;
use crate::diary::{refresh_diary, refresh_recent_grades};
use crate::login::{login_and_save, LoginOutcome};
use crate::SESSION;

#[derive(Clone, Debug)]
pub(crate) struct PendingParentLogin {
    pub sid: String,
    pub storage_path: String,
    pub children: Vec<crate::crypto::ChildInfo>,
    pub parent_name: String,
}

pub(crate) static PENDING_PARENT_LOGIN: std::sync::Mutex<Option<PendingParentLogin>> = std::sync::Mutex::new(None);

pub(crate) static CAL_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static EVENT_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static CHART_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static SIM_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static CHILD_SELECT_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static CURRENT_TAB: AtomicI32 = AtomicI32::new(0);

// «Toast» на вебе — просто сообщение в консоль.
pub(crate) fn show_toast(text: &str) {
    log::info!("{}", text);
}

// Показ диалога вставки токена. Через window.prompt: минимум интеграций,
// поле ввода появляется на любой странице без изменения .slint-верстки.
pub(crate) fn launch_login_flow() {
    let win = match window() {
        Some(w) => w,
        None => return,
    };
    match win.prompt_with_message("Вход через Госуслуги: откройте obr57.ru/gis, войдите и вставьте сюда значение cookie X1_SSO") {
        Ok(Some(token)) if !token.trim().is_empty() => {
            login_with_token(&token);
        }
        _ => {
            apply_login_error("Токен не введён. Скопируйте значение cookie X1_SSO из браузера.");
        }
    }
}

// Обработка вставленного токена (бывший JNI-колбэк sendTokenToRust).
pub(crate) fn login_with_token(raw_token: &str) {
    let mut token_str = raw_token.to_string();

    if raw_token.contains("X1_SSO=") {
        for part in raw_token.split(';') {
            let trimmed = part.trim();
            if trimmed.starts_with("X1_SSO=") {
                token_str = trimmed.trim_start_matches("X1_SSO=").to_string();
                break;
            }
        }
    }
    let token_str = token_str.trim().trim_matches('"').to_string();

    let storage_path = crate::STORAGE.get().cloned().unwrap_or_else(|| "web".to_string());

    crate::net::runtime().spawn(async move {
        apply_logging_in(true);
        apply_login_error("");

        match login_and_save(&token_str, &storage_path).await {
            Ok(LoginOutcome::Success(session)) => {
                *PENDING_PARENT_LOGIN.lock().unwrap() = None;
                *SESSION.lock().unwrap() = Some(session.clone());
                cache::init(&storage_path);
                apply_session_to_ui(&session);
                refresh_diary(0);
                refresh_recent_grades();
                crate::marks::init_marks();
                crate::finals::init_finals();
                apply_logging_in(false);
            }
            Ok(LoginOutcome::NeedChildSelection { sid, storage_path, children, parent_name }) => {
                *PENDING_PARENT_LOGIN.lock().unwrap() = Some(PendingParentLogin {
                    sid,
                    storage_path,
                    children: children.clone(),
                    parent_name,
                });
                show_child_picker_for_login(&children);
            }
            Err(e) => {
                let msg = e.user_message();
                log::error!("Ошибка входа: {}", msg);
                apply_login_error(&msg);
                apply_logging_in(false);
            }
        }
    });
}
