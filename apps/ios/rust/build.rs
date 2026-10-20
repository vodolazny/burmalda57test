fn main() {
    slint_build::compile("ui/app.slint").unwrap();

    // iOS: нативная часть (WKWebView, Keychain, PHPicker, уведомления, BGTask) написана
    // на Objective-C — см. ios/glue.m. Компилируем её и линкуем нужные фреймворки.
    println!("cargo:rerun-if-changed=ios/glue.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        cc::Build::new()
            .file("ios/glue.m")
            .flag("-fobjc-arc")
            .flag("-Wno-deprecated-declarations")
            .compile("burmalda_ios_glue");

        for framework in [
            "Foundation",
            "UIKit",
            "WebKit",
            "Security",
            "UserNotifications",
            "BackgroundTasks",
            "PhotosUI",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
