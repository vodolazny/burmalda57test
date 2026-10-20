// Web-приложение: desktop-Slint-UI + общий core.
//
// Бизнес-логика (сессия, дневник, оценки, кеши, локальные данные) живёт в
// burmalda57-core; здесь остаются только браузерные адаптеры (fetch,
// WebCrypto/localStorage), вход через вставку X1_SSO и маппинг в UI.

mod app;
mod http;
mod platform;
mod secure_storage;
mod theme;
mod ui_bridge;

use std::sync::Mutex;

use wasm_bindgen::prelude::*;

// Генерируется из ui/app.slint (см. build.rs)
slint::include_modules!();

pub(crate) static APP_WEAK: Mutex<Option<slint::Weak<AppWindow>>> = Mutex::new(None);

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    wasm_bindgen_futures::spawn_local(app::app_main());
    Ok(())
}
