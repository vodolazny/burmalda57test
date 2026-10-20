// platform.rs — единая точка для платформенных вызовов из общего кода (lib.rs).
// Android: JNI (android.rs). iOS: Objective-C (ios.rs → ios/glue.m).

/// Открыть экран входа через Госуслуги (ЕСИА).
#[cfg(target_os = "android")]
pub(crate) fn launch_login() -> Result<(), String> {
    crate::android::launch_login_activity().map_err(|e| format!("{:?}", e))
}

#[cfg(target_os = "ios")]
pub(crate) fn launch_login() -> Result<(), String> {
    crate::ios::launch_login();
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn launch_login() -> Result<(), String> {
    Err("вход не поддерживается на этой платформе".to_string())
}

/// Короткое всплывающее сообщение (Toast на Android, плашка на iOS).
#[cfg(target_os = "android")]
pub(crate) fn show_toast(text: &str) {
    crate::android::show_toast(text);
}

#[cfg(target_os = "ios")]
pub(crate) fn show_toast(text: &str) {
    crate::ios::show_toast(text);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn show_toast(text: &str) {
    log::info!("toast: {}", text);
}
