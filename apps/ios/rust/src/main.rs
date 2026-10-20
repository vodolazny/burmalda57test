// Точка входа iOS-приложения.
//
// Slint под iOS собирается как обычный бинарник (а не .so, как на Android) —
// см. https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/ios/
// Вся логика живёт в библиотеке (src/lib.rs), здесь только запуск.
fn main() {
    burmalda57::ios_main();
}
