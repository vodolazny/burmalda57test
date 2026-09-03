# Сборка веб-версии (wasm)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot

# 1. Сборка wasm-библиотеки
Push-Location "$root\rust"
cargo build --release --target wasm32-unknown-unknown
Pop-Location

# 2. Генерация JS-обвязки (wasm-bindgen CLI)
#    Ставится один раз: cargo install wasm-bindgen-cli
wasm-bindgen `
    "$root\rust\target\wasm32-unknown-unknown\release\burmalda57.wasm" `
    --out-dir "$root\web\pkg" `
    --target web

Write-Host ""
Write-Host "Готово. Для локального запуска (нужен http-сервер, file:// не работает):"
Write-Host "  cd $root\web"
Write-Host "  python -m http.server 8080"
Write-Host "и открой http://localhost:8080"
