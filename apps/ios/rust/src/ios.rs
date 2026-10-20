// ios.rs — тонкая обёртка над rust/ios/glue.m (Objective-C),
// который собирается в build.rs через крейт `cc`.
//
// Что где:
//   вход через Госуслуги  — WKWebView (непостоянное хранилище куков)
//   шифрование            — ключ-обёртка (KEK) лежит в Keychain
//   аватар                — PHPickerViewController
//   уведомления           — UNUserNotificationCenter (локальные)
//   фоновый опрос         — BGAppRefreshTask (iOS сама решает, когда запускать)
//   Toast                 — UILabel поверх окна
//
// Все функции glue.m безопасны для вызова из любого потока: внутри они сами
// переходят на главную очередь, где это нужно.
use std::ffi::{c_char, c_int, CStr, CString};

use burmalda57_ui::{AppWindow, Theme};
use slint::ComponentHandle;

extern "C" {
    fn bw_app_support_dir(buf: *mut c_char, cap: usize) -> usize;
    fn bw_launch_login();
    fn bw_show_toast(text: *const c_char);
    fn bw_open_url(url: *const c_char);
    fn bw_pick_avatar();
    fn bw_is_dark() -> c_int;
    fn bw_safe_area(top: *mut f32, bottom: *mut f32);
    fn bw_keychain_get(account: *const c_char, out: *mut u8, cap: usize, out_len: *mut usize) -> c_int;
    fn bw_keychain_set(account: *const c_char, data: *const u8, len: usize) -> c_int;
    fn bw_notify_request_permission();
    fn bw_notify_show(id: c_int, title: *const c_char, text: *const c_char);
    fn bw_bg_register();
    fn bw_bg_schedule(minutes: c_int);
    fn bw_bg_cancel();
}

fn cstr(s: &str) -> CString {
    // Внутренний NUL — единственное, на чём CString::new может упасть.
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

// ---------------------------------------------------------------------------
//  Хранилище
// ---------------------------------------------------------------------------

/// Приватный каталог приложения (Library/Application Support), создаётся при необходимости.
pub(crate) fn storage_dir() -> String {
    let mut buf = vec![0u8; 1024];
    let n = unsafe { bw_app_support_dir(buf.as_mut_ptr() as *mut c_char, buf.len()) };
    buf.truncate(n);
    String::from_utf8(buf).unwrap_or_default()
}

// ---------------------------------------------------------------------------
//  UI-хелперы
// ---------------------------------------------------------------------------

pub(crate) fn launch_login() {
    unsafe { bw_launch_login() }
}

pub(crate) fn show_toast(text: &str) {
    let c = cstr(text);
    unsafe { bw_show_toast(c.as_ptr()) }
}

pub(crate) fn open_url(url: &str) {
    let c = cstr(url);
    unsafe { bw_open_url(c.as_ptr()) }
}

pub(crate) fn pick_avatar() {
    unsafe { bw_pick_avatar() }
}

pub(crate) fn is_dark() -> bool {
    unsafe { bw_is_dark() != 0 }
}

/// Безопасные области экрана (вырез / Dynamic Island / полоска «домой») в логических пикселях.
pub(crate) fn safe_area() -> (f32, f32) {
    let (mut top, mut bottom) = (0.0f32, 0.0f32);
    unsafe { bw_safe_area(&mut top, &mut bottom) };
    (top, bottom)
}

/// UI рисуется на весь экран (включая зону выреза), а в app.slint отступы
/// top-inset/bottom-inset по умолчанию подогнаны под Android. Окно появляется
/// не сразу, поэтому читаем отступы несколько раз с нарастающей задержкой.
pub(crate) fn sync_safe_area(weak: slint::Weak<AppWindow>) {
    for delay_ms in [250u64, 800, 2000] {
        let w = weak.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(delay_ms), move || {
            if let Some(ui) = w.upgrade() {
                let (top, bottom) = safe_area();
                if top > 0.0 {
                    ui.set_top_inset(top);
                }
                ui.set_bottom_inset(bottom);
                ui.global::<Theme>()
                    .set_nav_bottom_extra((bottom - 20.0).max(0.0));
            }
        });
    }
}

// ---------------------------------------------------------------------------
//  Keychain (хранит ключ-обёртку KEK; см. platform.rs)
// ---------------------------------------------------------------------------

/// `Ok(None)` — записи нет. `Err` — временный сбой (например, устройство ещё
/// не разблокировано после перезагрузки): вызывающий НЕ должен считать ключ потерянным.
pub(crate) fn keychain_get(account: &str) -> Result<Option<Vec<u8>>, String> {
    let acc = cstr(account);
    let mut buf = vec![0u8; 256];
    let mut len: usize = 0;
    let rc = unsafe { bw_keychain_get(acc.as_ptr(), buf.as_mut_ptr(), buf.len(), &mut len) };
    match rc {
        0 => {
            buf.truncate(len);
            Ok(Some(buf))
        }
        1 => Ok(None),
        _ => Err("Keychain: не удалось прочитать запись".to_string()),
    }
}

pub(crate) fn keychain_set(account: &str, data: &[u8]) -> Result<(), String> {
    let acc = cstr(account);
    let rc = unsafe { bw_keychain_set(acc.as_ptr(), data.as_ptr(), data.len()) };
    if rc == 0 {
        Ok(())
    } else {
        Err("Keychain: не удалось записать ключ".to_string())
    }
}

// ---------------------------------------------------------------------------
//  Уведомления и фоновый опрос
// ---------------------------------------------------------------------------

pub(crate) fn notify_request_permission() {
    unsafe { bw_notify_request_permission() }
}

pub(crate) fn notify_show(id: i32, title: &str, text: &str) {
    let (t, b) = (cstr(title), cstr(text));
    unsafe { bw_notify_show(id as c_int, t.as_ptr(), b.as_ptr()) }
}

/// Регистрация обработчика BGAppRefreshTask. iOS требует сделать это ДО завершения
/// запуска приложения, поэтому вызывается из ios_main() перед стартом цикла событий.
pub(crate) fn bg_register() {
    unsafe { bw_bg_register() }
}

pub(crate) fn bg_schedule(minutes: i32) {
    unsafe { bw_bg_schedule(minutes as c_int) }
}

pub(crate) fn bg_cancel() {
    unsafe { bw_bg_cancel() }
}

// ===========================================================================
//  Колбэки: Objective-C → Rust
//  panic = "abort" в release, поэтому здесь никаких unwrap()/expect().
// ===========================================================================

/// WKWebView получил куку X1_SSO — запускаем тот же вход, что и на Android.
#[no_mangle]
pub extern "C" fn bw_on_esia_token(token: *const c_char) {
    if token.is_null() {
        return;
    }
    let raw = unsafe { CStr::from_ptr(token) }.to_string_lossy().into_owned();
    burmalda57_mobile::handle_esia_token(&raw);
}

/// PHPicker выбрал картинку, glue.m уже ужал её до 512 px и закодировал в JPEG.
#[no_mangle]
pub extern "C" fn bw_on_avatar_bytes(data: *const u8, len: usize) {
    if data.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    burmalda57_mobile::set_avatar_bytes(bytes);
}

/// BGAppRefreshTask: один прогон опроса оценок. 0 — успех, 1 — повторить позже.
#[no_mangle]
pub extern "C" fn bw_poll_blocking() -> c_int {
    let platform: std::sync::Arc<dyn burmalda57_mobile::Platform> =
        std::sync::Arc::new(crate::platform::IosPlatform);
    burmalda57_mobile::poll_blocking(platform, storage_dir()) as c_int
}
