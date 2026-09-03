#![allow(unused)]

pub mod crypto;

mod bridge;
mod cache;
mod diary;
mod events;
mod login;
mod marks;
mod net;
mod platform;
mod theme;
mod finals;
mod profile;
mod homework;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::atomic::AtomicU64;
use std::sync::{Mutex, OnceLock};
use slint::ComponentHandle;
use crate::bridge::apply_session_to_ui;
use crate::crypto::UserSession;
use crate::diary::{force_refresh, refresh_diary, refresh_recent_grades};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

// Генерируется из ui/app.slint (см. build.rs)
slint::include_modules!();

// ---------- Глобальное состояние ----------
pub(crate) static APP_WEAK: Mutex<Option<slint::Weak<AppWindow>>> = Mutex::new(None);
pub(crate) static SESSION: Mutex<Option<UserSession>> = Mutex::new(None);
pub(crate) static CURRENT_DATE: Mutex<Option<String>> = Mutex::new(None);
pub(crate) static DIARY_GEN: AtomicU64 = AtomicU64::new(0);
pub(crate) static DEMO: AtomicBool = AtomicBool::new(false);
// Идентификатор хранилища: на вебе это логическое имя (localStorage),
// реальные байты лежат в браузере.
pub(crate) static STORAGE: OnceLock<String> = OnceLock::new();

// ============================================================
//  ТОЧКА ВХОДА WASM
// ============================================================
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    let _ = STORAGE.set("web".to_string());
    wasm_bindgen_futures::spawn_local(app_main());
    Ok(())
}

async fn app_main() {
    // Разблокировка шифрованного хранилища: KEK из IndexedDB, DEK из
    // localStorage (или первичное создание). Всё остальное — синхронно.
    if let Err(e) = crypto::init_keystore().await {
        log::error!("Не удалось инициализировать хранилище сессии: {e}");
    }

    let ui = AppWindow::new().expect("Не удалось создать окно");
    *APP_WEAK.lock().unwrap() = Some(ui.as_weak());

    // --- Коллбэки UI ---
    ui.on_login_requested(|| {
        crate::bridge::apply_login_error(""); // гасим прошлую ошибку при повторе
        crate::platform::launch_login_flow();
    });
    ui.on_prev_day(|| refresh_diary(-1));
    ui.on_next_day(|| refresh_diary(1));
    ui.on_refresh_day(|| force_refresh());
    ui.on_grade_select(|i| crate::marks::select_period(i));
    ui.on_grade_mode_select(|m| finals::select_mode(m));
    ui.on_grade_open_chart(|s| crate::marks::open_chart(s.to_string()));
    ui.on_pick_date(move |y, m, d| {
        let date = format!("{:04}-{:02}-{:02}", y, m, d);
        *CURRENT_DATE.lock().unwrap() = Some(date);
        refresh_diary(0); // delta 0 → грузит выбранный день
    });
    ui.on_open_url(|u| profile::open_url(u.as_str()));
    ui.on_pick_avatar(|| profile::pick_avatar());

    ui.on_open_child_select(|| {
        if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
            ui.set_child_select_is_login(false);
            ui.set_child_select_open(true);
        }
    });
    ui.on_close_child_select(|| {
        if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
            ui.set_child_select_open(false);
        }
    });
    ui.on_select_child(|guid| {
        let guid_str = guid.to_string();

        // 1. Если был выбор при входе (Pending login)
        if let Some(pending) = platform::PENDING_PARENT_LOGIN.lock().unwrap().take() {
            crate::bridge::apply_logging_in(true);
            crate::net::runtime().spawn(async move {
                match crate::login::complete_child_login(
                    &pending.sid,
                    &pending.storage_path,
                    &guid_str,
                    pending.parent_name,
                    pending.children,
                ).await {
                    Ok(session) => {
                        *SESSION.lock().unwrap() = Some(session.clone());
                        cache::init(&pending.storage_path);
                        slint::invoke_from_event_loop(move || {
                            if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                                ui.set_child_select_open(false);
                            }
                            apply_session_to_ui(&session);
                            refresh_diary(0);
                            refresh_recent_grades();
                            crate::marks::init_marks();
                            crate::finals::init_finals();
                            crate::bridge::apply_logging_in(false);
                        }).ok();
                    }
                    Err(e) => {
                        let msg = e.user_message();
                        crate::bridge::apply_login_error(&msg);
                        crate::bridge::apply_logging_in(false);
                    }
                }
            });
            return;
        }

        // 2. Демо-режим: переключение ученика
        if crate::DEMO.load(Ordering::SeqCst) {
            let mut session_guard = SESSION.lock().unwrap();
            if let Some(session) = session_guard.as_mut() {
                if let Some(child) = session.children.iter().find(|c| c.guid == guid_str).cloned() {
                    session.user_guid = child.guid.clone();
                    session.full_name = child.full_name.clone();
                    session.school_name = child.school_name.clone();
                    session.school_class = child.school_class.clone();
                    let updated_session = session.clone();
                    drop(session_guard);

                    finals::reset();
                    crate::marks::reset();
                    crate::cache::reset();

                    crate::marks::init_marks();
                    crate::finals::init_finals();

                    slint::invoke_from_event_loop(move || {
                        if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                            ui.set_child_select_open(false);
                        }
                        apply_session_to_ui(&updated_session);
                        refresh_diary(0);
                        refresh_recent_grades();
                    }).ok();
                    platform::show_toast(&format!("Выбран ученик: {}", child.full_name));
                }
            }
            return;
        }

        // 3. Переключение ребёнка в профиле
        let current_session = SESSION.lock().unwrap().clone();
        if let Some(mut session) = current_session {
            if session.user_guid == guid_str {
                if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                    ui.set_child_select_open(false);
                }
                return;
            }

            if let Some(child) = session.children.iter().find(|c| c.guid == guid_str).cloned() {
                let storage_path = STORAGE.get().cloned().unwrap_or_else(|| "web".to_string());
                crate::net::runtime().spawn(async move {
                    match crate::login::init_session(&session.sid, &guid_str).await {
                        Ok(new_key) => {
                            session.user_guid = child.guid.clone();
                            session.apikey = new_key;
                            session.full_name = child.full_name.clone();
                            session.school_name = child.school_name.clone();
                            session.school_class = child.school_class.clone();

                            let _ = crypto::save_session(&storage_path, &session);
                            for f in [
                                ".marks_cache",
                                ".grades_cache",
                                ".periods",
                                ".finals_cache",
                            ] {
                                crypto::delete_encrypted_file(&storage_path, f);
                            }

                            *SESSION.lock().unwrap() = Some(session.clone());

                            finals::reset();
                            crate::marks::reset();
                            crate::cache::reset();

                            let session_clone = session.clone();
                            let child_name = child.full_name.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
                                    ui.set_child_select_open(false);
                                }
                                apply_session_to_ui(&session_clone);
                                refresh_diary(0);
                                refresh_recent_grades();
                                crate::marks::init_marks();
                                crate::finals::init_finals();
                            }).ok();

                            platform::show_toast(&format!("Выбран ученик: {}", child_name));
                        }
                        Err(e) => {
                            log::error!("Не удалось переключить ученика: {:?}", e.user_message());
                            platform::show_toast("Не удалось переключить ученика. Проверьте сеть.");
                        }
                    }
                });
            }
        }
    });

    ui.on_add_event(|name, start, end| {
        crate::diary::add_event(name.as_str(), start.as_str(), end.as_str());
    });
    ui.on_delete_event(|id| {
        crate::diary::delete_event(id.as_str());
    });
    ui.on_demo_requested(|| crate::enter_demo_mode());
    ui.on_tab_changed(|t| platform::CURRENT_TAB.store(t, Ordering::SeqCst));
    ui.on_cal_open_changed(|o| platform::CAL_OPEN.store(o, Ordering::SeqCst));
    ui.on_event_open_changed(|o| platform::EVENT_OPEN.store(o, Ordering::SeqCst));
    ui.on_child_select_open_changed(|o| platform::CHILD_SELECT_OPEN.store(o, Ordering::SeqCst));
    ui.on_logout(|| {
        crate::DEMO.store(false, Ordering::SeqCst);
        *SESSION.lock().unwrap() = None;
        finals::reset();
        crate::marks::reset();
        crate::cache::reset();
        profile::delete_avatar();
        // Удаляем сессию и серверные кеши: «Выйти» должно переживать
        // перезапуск, а следующий пользователь не должен видеть чужие оценки.
        // Локальные события/ДЗ не трогаем.
        if let Some(path) = STORAGE.get() {
            for f in [
                ".session",
                ".marks_cache",
                ".grades_cache",
                ".periods",
                ".finals_cache",
            ] {
                crypto::delete_encrypted_file(path, f);
            }
        }
        if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
            ui.set_logged_in(false);
            ui.set_full_name("—".into());
            ui.set_school_name("—".into());
            ui.set_school_class("—".into());
            ui.set_is_parent(false);
            ui.set_parent_name("".into());
            ui.set_current_child_guid("".into());
            ui.set_children_list(slint::ModelRc::new(slint::VecModel::<ChildItem>::default()));
            ui.set_lessons(slint::ModelRc::new(slint::VecModel::<Lesson>::default()));
            ui.set_recent_grades(slint::ModelRc::new(slint::VecModel::<RecentGrade>::default()));
            ui.set_grade_subjects(slint::ModelRc::new(slint::VecModel::<SubjectGrades>::default()));
            ui.set_grade_finals(slint::ModelRc::new(slint::VecModel::<SubjectFinals>::default()));
            ui.set_grade_periods(slint::ModelRc::new(
                slint::VecModel::<slint::SharedString>::default(),
            ));
            ui.set_has_avatar(false);
        }
    });
    ui.on_toggle_homework(|key, done| crate::diary::toggle_homework(key.as_str(), done));
    ui.global::<Validate>().on_invalid(|s| !crate::events::is_valid_time(s.as_str()));

    theme::apply_system_theme(&ui);

    // --- Восстановление сессии ---
    events::init("web");
    homework::init("web");
    if let Some(session) = crypto::load_session("web") {
        *SESSION.lock().unwrap() = Some(session.clone());
        cache::init("web"); // подхватываем кеш дней
        crate::marks::init_marks(); // периоды + оценки текущей четверти
        finals::init_finals();
        apply_session_to_ui(&session);
        refresh_diary(0); // грузим сегодняшний день
        refresh_recent_grades(); // лента недавних оценок
    } else {
        log::info!("Локальная сессия не найдена или не удалось дешифровать при старте.");
    }

    ui.run().expect("Ошибка цикла событий");
}

pub(crate) fn enter_demo_mode() {
    DEMO.store(true, Ordering::SeqCst);

    let session = UserSession {
        sid: "demo".into(),
        user_guid: "demo_student".into(),
        apikey: "demo".into(),
        full_name: "Иван Петров".into(),
        school_name: "МБОУ СОШ №1 г. Орёл".into(),
        school_class: "9 «А»".into(),
        is_parent: false,
        parent_name: String::new(),
        children: Vec::new(),
    };
    *SESSION.lock().unwrap() = Some(session.clone());

    // Эти функции увидят DEMO и положат заглушки вместо похода в сеть
    crate::marks::init_marks();
    crate::finals::init_finals();

    let s = session.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = APP_WEAK.lock().unwrap().as_ref().and_then(|w| w.upgrade()) {
            ui.set_logged_in(true);
        }
        apply_session_to_ui(&s);
        refresh_diary(0);
        refresh_recent_grades();
    });
}
