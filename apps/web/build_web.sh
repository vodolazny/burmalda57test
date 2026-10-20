#!/usr/bin/env bash
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "==> 1. Сборка wasm-библиотеки (cargo build)..."
cd "$SCRIPT_DIR"
cargo build --release --target wasm32-unknown-unknown

echo "==> 2. Генерация JS-обвязки (wasm-bindgen)..."
wasm-bindgen \
    "$SCRIPT_DIR/target/wasm32-unknown-unknown/release/burmalda57.wasm" \
    --out-dir "$SCRIPT_DIR/web/pkg" \
    --target web

if command -v wasm-opt &> /dev/null; then
    echo "==> 3. Оптимизация размера WASM (wasm-opt -Oz)..."
    wasm-opt --all-features -Oz --strip-debug -o "$SCRIPT_DIR/web/pkg/burmalda57_bg.wasm" "$SCRIPT_DIR/web/pkg/burmalda57_bg.wasm"
else
    echo "==> Подсказка: wasm-opt не найден, пропускаем post-оптимизацию"
fi

echo ""
echo "Готово! web/pkg/ собран и готов к запуску:"
ls -lh "$SCRIPT_DIR/web/pkg"
echo ""
echo "Для запуска: cd web && python -m http.server 8080"
