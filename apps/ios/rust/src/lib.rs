// iOS-оболочка: точка входа, Sentry/лог, Objective-C мост.
// Всё остальное (состояние, UI, бизнес-логика) — в burmalda57-mobile.

#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
mod platform;

// DSN Sentry задаётся при сборке; без переменной крашлитика отключена
#[cfg(target_os = "ios")]
const SENTRY_DSN: Option<&str> = option_env!("SENTRY_DSN");

#[cfg(target_os = "ios")]
struct StderrLogger;

#[cfg(target_os = "ios")]
impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("[burmalda57] {} {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

// ============================================================
//  ТОЧКА ВХОДА iOS (Slint собирается как обычный бинарник, см. src/main.rs)
// ============================================================
#[cfg(target_os = "ios")]
pub fn ios_main() {
    let _guard = sentry::init((SENTRY_DSN, sentry::ClientOptions {
        release: sentry::release_name!(),
        debug: false,
        send_default_pii: false, // данные ученика — PII не отправляем
        ..Default::default()
    }));
    sentry::configure_scope(|scope| scope.set_tag("platform", "ios"));
    let default_hook = std::panic::take_hook(); // тут уже стоит хук Sentry
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info); // Sentry захватывает событие в очередь
        if let Some(client) = sentry::Hub::current().client() {
            client.flush(Some(std::time::Duration::from_secs(5))); // блокируемся, пока отправит
        }
    }));
    let _ = log::set_logger(&StderrLogger);
    log::set_max_level(log::LevelFilter::Warn);

    // Обработчик BGAppRefreshTask надо зарегистрировать ДО старта цикла событий UIKit.
    ios::bg_register();

    let platform: std::sync::Arc<dyn burmalda57_mobile::Platform> =
        std::sync::Arc::new(platform::IosPlatform);
    burmalda57_mobile::run_app(platform, ios::storage_dir()).expect("Ошибка цикла событий");
}
