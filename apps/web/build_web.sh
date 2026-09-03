#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> Сборка wasm-библиотеки..."
cd "$SCRIPT_DIR"
cargo build --release --target wasm32-unknown-unknown

echo "==> Генерация JS-обвязки (wasm-bindgen)..."
wasm-bindgen \
    "$SCRIPT_DIR/target/wasm32-unknown-unknown/release/burmalda57.wasm" \
    --out-dir "$SCRIPT_DIR/web/pkg" \
    --target web

if command -v wasm-opt &> /dev/null; then
    echo "==> Оптимизация размера WASM (wasm-opt -Oz)..."
    wasm-opt --all-features -Oz --strip-debug -o "$SCRIPT_DIR/web/pkg/burmalda57_bg.wasm" "$SCRIPT_DIR/web/pkg/burmalda57_bg.wasm"
else
    echo "==> Подсказка: wasm-opt не найден, пропускаем post-оптимизацию"
fi

echo ""
echo "Готово. Для локального запуска (нужен http-сервер, file:// не работает):"
echo "  cd web && python -m http.server 8080"
echo "и открой http://localhost:8080"
