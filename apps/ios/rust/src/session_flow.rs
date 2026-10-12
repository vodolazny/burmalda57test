// session_flow.rs — платформонезависимая часть входа и навигации.
//
// Раньше всё это жило в android.rs рядом с JNI. Платформенные слои
// (android.rs / ios.rs) теперь только доставляют сюда данные из своих
// нативных API: токен ЕСИА после WebView-логина и событие «Назад».
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Mutex;

use crate::bridge::{apply_logging_in, apply_login_error, apply_session_to_ui, show_child_picker_for_login};
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

pub(crate) static PENDING_PARENT_LOGIN: Mutex<Option<PendingParentLogin>> = Mutex::new(None);

// ============================================================
//  Токен ЕСИА, полученный нативным слоем после WebView-логина
// ============================================================
pub(crate) fn handle_esia_token(raw_token_str: String, storage_path: String) {
    let mut token_str = raw_token_str.clone();

    if raw_token_str.contains("X1_SSO=") {
        for part in raw_token_str.split(';') {
            let trimmed = part.trim();
            if trimmed.starts_with("X1_SSO=") {
                token_str = trimmed.trim_start_matches("X1_SSO=").to_string();
                break;
            }
        }
    }

    let token_str = token_str.trim().trim_matches('"').to_string();

    // Общий рантайм приложения — не создаём новый на каждый вход.
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
                log::error!("Ошибка входа в потоке Rust: {}", msg);
                apply_login_error(&msg);
                apply_logging_in(false);
            }
        }
    });
}

// ============================================================
//  Состояние оверлеев — нужно для жеста/кнопки «Назад» (Android)
// ============================================================
pub(crate) static CAL_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static EVENT_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static CHART_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static SIM_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static CHILD_SELECT_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) static CURRENT_TAB: AtomicI32 = AtomicI32::new(0);

/// Обработка «Назад»: закрывает верхний оверлей или возвращает на вкладку 0.
/// `true` — событие поглощено, `false` — нужно штатное поведение системы.
/// На iOS кнопки «Назад» нет, функция там не вызывается.
#[allow(dead_code)]
pub(crate) fn handle_back() -> bool {
    if CHILD_SELECT_OPEN.load(Ordering::SeqCst) {
        CHILD_SELECT_OPEN.store(false, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(|| {
            if let Some(ui) = crate::APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                if !ui.get_child_select_is_login() {
                    ui.set_child_select_open(false);
                }
            }
        });
        return true;
    }
    if CAL_OPEN.load(Ordering::SeqCst) {
        CAL_OPEN.store(false, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(|| {
            if let Some(ui) = crate::APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                ui.set_cal_open(false);
            }
        });
        return true;
    }
    if EVENT_OPEN.load(Ordering::SeqCst) {
        EVENT_OPEN.store(false, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(|| {
            if let Some(ui) = crate::APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                ui.set_event_open(false);
            }
        });
        return true;
    }
    if CHART_OPEN.load(Ordering::SeqCst) {
        CHART_OPEN.store(false, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(|| {
            if let Some(ui) = crate::APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                ui.set_chart_open(false);
            }
        });
        return true;
    }
    if SIM_OPEN.load(Ordering::SeqCst) {
        SIM_OPEN.store(false, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(|| {
            if let Some(ui) = crate::APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                ui.set_sim_open(false);
            }
        });
        return true;
    }
    if CURRENT_TAB.load(Ordering::SeqCst) != 0 {
        CURRENT_TAB.store(0, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(|| {
            if let Some(ui) = crate::APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                ui.set_tab(0);
            }
        });
        return true;
    }

    false
}
