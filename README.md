# burmalda57

Неофициальный клиент электронного дневника [obr57.ru](https://obr57.ru/gis)
для Android, iOS и web. Мобильные приложения используют общий Rust/Slint-код,
web — общее ядро и отдельный desktop-UI.

[![Android](https://github.com/vodolazny/burmalda57/actions/workflows/android.yml/badge.svg)](https://github.com/vodolazny/burmalda57/actions/workflows/android.yml)
[![iOS](https://github.com/vodolazny/burmalda57/actions/workflows/ios.yml/badge.svg)](https://github.com/vodolazny/burmalda57/actions/workflows/ios.yml)
[![Web](https://github.com/vodolazny/burmalda57/actions/workflows/web.yml/badge.svg)](https://github.com/vodolazny/burmalda57/actions/workflows/web.yml)

**Скачать:** [APK и iOS IPA](https://github.com/vodolazny/burmalda57/releases)

> ⚠️ Неофициальный проект. Не связан с Департаментом образования Орловской
> области, Госуслугами/ЕСИА или разработчиками официального дневника.
> Используется на свой страх и риск.

## Возможности

- дневник, расписание и домашние задания;
- оценки по периодам, веса и средний балл, графики успеваемости;
- итоговые оценки и события;
- заметки к урокам и скрытие предметов (в мобильных приложениях);
- офлайн-кеш и восстановление сессии без повторного входа;
- родительский режим с выбором ребёнка;
- уведомления о новых оценках (Android/iOS);
- светлая/тёмная тема, Material You на Android;
- демо-режим без входа.

## Архитектура

```
apps/android ─┐
              ├──→ crates/mobile ──→ crates/core
apps/ios ─────┘          │
                         └──────→ crates/ui

apps/web ───────────────────────→ crates/core
```

- `apps/android` — Gradle/Kotlin и тонкая Rust-оболочка: JNI, Keystore,
  WorkManager, Material You.
- `apps/ios` — Rust-бинарник и Objective-C-мост: WKWebView, Keychain,
  PHPicker, BGTask, safe area.
- `apps/web` — wasm-клиент: `fetch`, WebCrypto, localStorage и собственный
  desktop-Slint-UI. На GitHub Pages публикуется только демо-сборка.
- `crates/core` — домен, API-запросы, парсеры, криптография, кэш, локальные
  хранилища, уведомления, обновления. Без Slint, `reqwest` и `tokio`.
- `crates/ui` — общий Slint-UI для Android и iOS.
- `crates/mobile` — общее состояние Android/iOS: колбэки, сеть, `Platform`,
  адаптеры `core` → Slint.
- `vendor/` — патченный Slint Android backend.

Направление зависимостей: `apps/* → mobile → core`, `apps/* → ui`,
`web → core`. Бизнес-логика и мобильный UI существуют в одном экземпляре.

### Что где менять

| Задача | Где |
|---|---|
| Логика, API, парсинг, кэш, крипта | `crates/core/src/` |
| Экраны и компоненты мобильного UI | `crates/ui/ui/` |
| Состояние, колбэки, маппинг в Slint, сеть | `crates/mobile/src/` |
| JNI, Kotlin, Keystore, WorkManager | `apps/android/` |
| ObjC-мост, Keychain, BGTask, safe area | `apps/ios/` |
| `fetch`, WebCrypto, localStorage, web-UI | `apps/web/` |
| CI и релизы | `.github/workflows/` |

## Сборка

Собирать нужно конкретный пакет, а не весь workspace: Slint backend/renderer
включаются оболочками приложений, и `cargo build --workspace` смешает их.

| Платформа | Требования | Команда | Подробнее |
|---|---|---|---|
| Android | JDK 17, SDK 34, NDK 29.0.14206865, `cargo-ndk` | `cd apps/android && ./build.sh --debug` | [apps/android/README.md](apps/android/README.md) |
| iOS | macOS, Xcode, Rust target | `cargo build --release --target aarch64-apple-ios-sim -p burmalda57-ios --bin burmalda57-ios --features ios-app` | [apps/ios/README.md](apps/ios/README.md) |
| Web | `wasm32-unknown-unknown`, `wasm-bindgen-cli` 0.2.128 | `./apps/web/build_web.sh` (полная) или `--demo-only` | [apps/web/README.md](apps/web/README.md) |

Debug APK для Android также собирается локально из `apps/android` командой
`./build.sh --debug`; release — `./build.sh --release` с настроенным
`keystore.properties`.

## Релизы

- **Android:** тег `android-vX.Y.Z` запускает подписанную release-сборку и
  публикует `burmalda57-X.Y.Z.apk` в GitHub Releases с пометкой latest —
  именно этот релиз использует встроенная проверка обновлений. Версия тега
  должна совпадать с `versionName` и версией Rust-крейта; `versionCode`
  увеличивается вручную.
- **iOS:** тег `ios-vX.Y.Z` публикует неподписанный `.ipa` в Releases.
  Инструкция по переподписи — в [apps/ios/README.md](apps/ios/README.md).
- **Web:** каждый push в `main` собирает wasm в демо-режиме и деплоит на
  GitHub Pages (источник Pages — «GitHub Actions»). Вход отключён: публичный
  хостинг не может обращаться к API дневника из-за CORS и ограничений по IP.
  Полная версия с входом собирается локально: `./apps/web/build_web.sh`.
- Push в `main` и pull request'ы собирают debug APK в артефактах Actions.

Подробные workflow: [android.yml](.github/workflows/android.yml),
[ios.yml](.github/workflows/ios.yml), [web.yml](.github/workflows/web.yml).

Для Android-release в настройках репозитория нужны secrets:

| Secret | Значение |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | `base64 -w0 release.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | пароль хранилища |
| `ANDROID_KEY_ALIAS` | alias ключа |
| `ANDROID_KEY_PASSWORD` | пароль ключа |

Без них release-сборка завершится ошибкой: неподписанный APK не установится
поверх предыдущих версий.

Необязательный секрет `SENTRY_DSN` включает Sentry в Android- и iOS-сборках:
DSN вшивается в бинарник при компиляции. Без секрета крашлитика отключена.

## Приватность

Пароль от Госуслуг приложение не видит и не хранит — используется только
сессионная кука `X1_SSO`. Локальные данные шифруются AES-256-GCM, DEK
заворачивается ключом Android Keystore / iOS Keychain / WebCrypto. Данные
уходят только на серверы дневника; рекламы и поведенческой аналитики нет.
Если задан `SENTRY_DSN`, мобильные сборки отправляют краш-репорты без PII.
Подробнее — в README приложений.

## Лицензия

Apache-2.0, см. [LICENSE](LICENSE).
