// crypto.rs — хранение сессии и кэша в браузере (wasm).
//
// Схема (envelope encryption, аналог Android Keystore на Web Crypto):
//   * DEK  — случайные 32 байта, ими AES-256-GCM шифруются сами данные.
//   * KEK  — НЕИЗВЛЕКАЕМЫЙ (extractable=false) ключ AES-256-GCM из
//            window.crypto.subtle (usages: wrapKey/unwrapKey). Он живёт
//            только внутри браузера, экспортировать его невозможно.
//   * KEK хранится в IndexedDB, «завёрнутый» KEK'ом DEK — в localStorage
//            (base64(nonce || ct)). Сам DEK raw никогда не попадает на диск.
//   * HKDF-SHA256 разделяет ключи по контексту (.session / .marks_cache …).
//
// Данные лежат в localStorage зашифрованно; без KEK из IndexedDB и
// соответствующего браузера/профиля они нечитаемы. API синхронный: KEK и DEK
// инициализируются один раз при старте (init_keystore().await), дальше
// шифрование выполняется чистым Rust (крейт aes-gcm) синхронно.

use aes::cipher::{BlockEncryptMut, KeyInit as AesKeyInit};
use aes::Aes128;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::window;

use rand::{Rng, SeedableRng};

pub const CONTEXT_SESSION: &[u8] = b"burmalda57_session_context_v1";
pub const CONTEXT_MARKS_CACHE: &[u8] = b"burmalda57_marks_cache_context_v1";

const NONCE_LEN: usize = 12;

// Префиксы ключей localStorage / имя базы IndexedDB
const LS_WRAPPED_DEK: &str = "b57.wrapped-dek";
const LS_FILE_PREFIX: &str = "b57.f.";
const IDB_NAME: &str = "burmalda57-keystore";
const IDB_STORE: &str = "keys";
const IDB_KEK: &str = "kek";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChildInfo {
    pub guid: String,
    pub full_name: String,
    pub school_name: String,
    pub school_class: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserSession {
    pub sid: String,          // кука X1_SSO (нужна для дальнейших запросов)
    pub user_guid: String,    // PARTICIPANT.SYS_GUID — гуид активного ученика
    pub apikey: String,       // Сессионный apikey
    pub full_name: String,    // ФИО: SURNAME NAME SECONDNAME
    pub school_name: String,  // SCHOOL.NAME — школа
    pub school_class: String, // GRADE.NAME — класс (напр. "8Г")
    #[serde(default)]
    pub is_parent: bool,      // true если аккаунт родителя
    #[serde(default)]
    pub parent_name: String,  // ФИО родителя
    #[serde(default)]
    pub children: Vec<ChildInfo>, // Список детей (для родителя)
}

// =========================================================================
//  Реверс-инжиниринговый метод для генерации apikey
// =========================================================================
pub fn ahh_encrypt(text: &str) -> String {
    let key_bytes: [u8; 16] = [31, 23, 19, 50, 40, 23, 19, 10, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut encrypted_bytes = text.as_bytes().to_vec();

    let block_size = 16;
    let padding_len = block_size - (encrypted_bytes.len() % block_size);
    encrypted_bytes.resize(encrypted_bytes.len() + padding_len, padding_len as u8);

    // Шифрование AES-128-ECB
    let mut cipher = Aes128::new(&key_bytes.into());
    for chunk in encrypted_bytes.chunks_exact_mut(16) {
        let block = aes::cipher::generic_array::GenericArray::from_mut_slice(chunk);
        cipher.encrypt_block_mut(block);
    }

    STANDARD.encode(encrypted_bytes)
}

// =========================================================================
//  Веб-хелперы
// =========================================================================
// JsValue не реализует StdError — приводим к строке для `Box<dyn Error>`.
fn js_err(e: JsValue) -> Box<dyn std::error::Error> {
    format!("{e:?}").into()
}

fn subtle() -> Result<web_sys::SubtleCrypto, Box<dyn std::error::Error>> {
    Ok(window()
        .ok_or("нет window")?
        .crypto()
        .map_err(js_err)?
        .subtle())
}

fn ls() -> Result<web_sys::Storage, Box<dyn std::error::Error>> {
    window()
        .ok_or("нет window")?
        .local_storage()
        .map_err(js_err)?
        .ok_or("localStorage недоступен")
        .map_err(Into::into)
}

fn aes_gcm_algo() -> Result<js_sys::Object, Box<dyn std::error::Error>> {
    Ok(js_sys::JSON::parse(r#"{"name":"AES-GCM"}"#)
        .map_err(js_err)?
        .dyn_into()
        .map_err(js_err)?)
}

// =========================================================================
//  ГПСЧ
// =========================================================================
fn fill_random(buf: &mut [u8]) {
    if let Some(crypto) = window().and_then(|w| w.crypto().ok()) {
        let _ = crypto.get_random_values_with_u8_array(buf);
        return;
    }
    // Фолбэк (в браузере не должен случаться)
    let mut rng = rand::rngs::StdRng::from_entropy();
    rng.fill(buf);
}

// =========================================================================
//  IndexedDB: хранение неэкспортируемого KEK
// =========================================================================
// Ожидание завершения IdbRequest (success/error → Promise).
fn idb_request(req: &web_sys::IdbRequest) -> JsFuture {
    let req = req.clone();
    JsFuture::from(js_sys::Promise::new(&mut move |resolve, reject| {
        let onsuccess = Closure::<dyn Fn()>::new({
            let resolve = resolve.clone();
            let req = req.clone();
            move || {
                let _ = resolve.call1(&JsValue::NULL, &req);
            }
        });
        let onerror = Closure::<dyn Fn()>::new({
            let req = req.clone();
            move || {
                let _ = reject.call1(&JsValue::NULL, &req);
            }
        });
        req.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
        req.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        // Одноразовые обработчики при инициализации: связать время жизни с
        // запросом нельзя, отпускаем (сработают максимум один раз).
        onsuccess.forget();
        onerror.forget();
    }))
}

async fn open_keystore_db() -> Result<web_sys::IdbDatabase, JsValue> {
    let win = window().ok_or_else(|| JsValue::from_str("нет window"))?;
    let factory = win
        .indexed_db()?
        .ok_or_else(|| JsValue::from_str("IndexedDB недоступна"))?;

    // Открываем базу без смены версии и проверяем наличие object store.
    let req = factory.open(IDB_NAME)?;
    let opened = idb_request(&req).await?;
    let db: web_sys::IdbDatabase = opened.dyn_into()?;
    let has_store = db
        .transaction_with_str_and_mode(IDB_STORE, web_sys::IdbTransactionMode::Readonly)
        .is_ok();
    if has_store {
        return Ok(db);
    }
    let version = db.version() as u32;
    db.close();

    // Store нет — открываем с новой версией и создаём его в onupgradeneeded.
    let req = factory.open_with_u32(IDB_NAME, version + 1)?;
    let store_name = IDB_STORE;
    let onupgradeneeded = Closure::<dyn Fn(web_sys::Event)>::new(move |event: web_sys::Event| {
        if let Some(req) = event.target().and_then(|t| t.dyn_into::<web_sys::IdbOpenDbRequest>().ok()) {
            if let Ok(db) = req.result().and_then(|r| r.dyn_into::<web_sys::IdbDatabase>()) {
                let _ = db.create_object_store(store_name);
            }
        }
    });
    req.set_onupgradeneeded(Some(onupgradeneeded.as_ref().unchecked_ref()));
    let opened = idb_request(&req).await?;
    onupgradeneeded.forget();
    let db: web_sys::IdbDatabase = opened.dyn_into()?;
    Ok(db)
}

async fn idb_get_kek() -> Result<Option<web_sys::CryptoKey>, JsValue> {
    let db = open_keystore_db().await?;
    let result = (async {
        let tx = db.transaction_with_str_and_mode(IDB_STORE, web_sys::IdbTransactionMode::Readonly)?;
        let store = tx.object_store(IDB_STORE)?;
        let req = store.get(&JsValue::from_str(IDB_KEK))?;
        let val = idb_request(&req).await?;
        if val.is_undefined() || val.is_null() {
            Ok(None)
        } else {
            Ok(Some(val.dyn_into::<web_sys::CryptoKey>()?))
        }
    })
    .await;
    db.close();
    result
}

async fn idb_put_kek(key: &web_sys::CryptoKey) -> Result<(), JsValue> {
    let db = open_keystore_db().await?;
    let result = (async {
        let tx = db.transaction_with_str_and_mode(IDB_STORE, web_sys::IdbTransactionMode::Readwrite)?;
        let store = tx.object_store(IDB_STORE)?;
        // put(value, key)
        let req = store.put_with_key(&key.into(), &JsValue::from_str(IDB_KEK))?;
        idb_request(&req).await?;
        Ok(())
    })
    .await;
    db.close();
    result
}

// =========================================================================
//  KEK/DEK: инициализация при старте
// =========================================================================
static DEK: std::sync::Mutex<Option<[u8; 32]>> = std::sync::Mutex::new(None);

/// Инициализация хранилища ключей. Вызывается один раз при старте приложения.
///  * KEK нет в IndexedDB → создаём KEK и DEK, сохраняем оба;
///  * KEK есть → разворачиваем DEK из localStorage в память.
pub async fn init_keystore() -> Result<(), Box<dyn std::error::Error>> {
    let subtle = subtle()?;

    if let Some(kek) = idb_get_kek().await.map_err(js_err)? {
        // Разворачиваем DEK: unwrapKey → extractable-ключ → exportKey → байты
        let wrapped_b64 = ls()?.get_item(LS_WRAPPED_DEK).map_err(js_err)?;
        let wrapped = wrapped_b64.ok_or("KEK есть, а завёрнутого DEK нет")?;
        let wrapped = STANDARD.decode(wrapped.as_bytes())?;

        let usages = JsValue::from_str("encrypt");
        let dek_key = JsFuture::from(subtle.unwrap_key_with_u8_array_and_str_and_str(
            "raw",
            &wrapped,
            &kek,
            "AES-GCM",
            "AES-GCM",
            true, // extractable: только чтобы прочитать байты в память wasm
            &usages,
        ).map_err(js_err)?)
        .await
        .map_err(js_err)?;
        let dek_key: web_sys::CryptoKey = dek_key.dyn_into().map_err(js_err)?;

        let exported = JsFuture::from(subtle.export_key("raw", &dek_key).map_err(js_err)?)
            .await
            .map_err(js_err)?;
        let bytes = js_sys::Uint8Array::new(&exported).to_vec();
        if bytes.len() != 32 {
            return Err("DEK неверной длины".into());
        }
        let mut dek = [0u8; 32];
        dek.copy_from_slice(&bytes);
        *DEK.lock().unwrap() = Some(dek);
        log::info!("Хранилище сессии разблокировано (DEK восстановлен)");
        return Ok(());
    }

    // Первый запуск: KEK (неизвлекаемый) + DEK
    let algo = aes_gcm_algo()?;
    let usages = {
        let arr = js_sys::Array::new();
        arr.push(&JsValue::from_str("wrapKey"));
        arr.push(&JsValue::from_str("unwrapKey"));
        arr
    };
    let kek_val = JsFuture::from(
        subtle
            .generate_key_with_object(&algo, false, &usages)
            .map_err(js_err)?,
    )
    .await
    .map_err(js_err)?;
    let kek: web_sys::CryptoKey = kek_val.dyn_into().map_err(js_err)?;
    idb_put_kek(&kek).await.map_err(js_err)?;

    let mut dek = [0u8; 32];
    fill_random(&mut dek);

    // Заворачиваем DEK KEK'ом: import → wrapKey
    let dek_key_val = JsFuture::from(
        subtle
            .import_key_with_str(
                "raw",
                &js_sys::Uint8Array::from(dek.as_slice()),
                "AES-GCM",
                true,
                &JsValue::from_str("encrypt"),
            )
            .map_err(js_err)?,
    )
    .await
    .map_err(js_err)?;
    let dek_key: web_sys::CryptoKey = dek_key_val.dyn_into().map_err(js_err)?;

    let wrapped_val = JsFuture::from(
        subtle
            .wrap_key_with_str("raw", &dek_key, &kek, "AES-GCM")
            .map_err(js_err)?,
    )
    .await
    .map_err(js_err)?;
    let wrapped = js_sys::Uint8Array::new(&wrapped_val).to_vec();

    ls()?
        .set_item(LS_WRAPPED_DEK, &STANDARD.encode(&wrapped))
        .map_err(js_err)?;
    *DEK.lock().unwrap() = Some(dek);
    log::info!("Создано новое хранилище сессии (KEK + DEK)");
    Ok(())
}

fn current_dek() -> Option<[u8; 32]> {
    *DEK.lock().unwrap()
}

fn get_or_create_dek(storage_path: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    current_dek().ok_or_else(|| {
        format!(
            "хранилище ключей не инициализировано (init_keystore не отработал) — «{}»",
            storage_path
        )
        .into()
    })
}

// Путь ЧТЕНИЯ: никогда не создаёт ключ.
fn get_existing_dek(_storage_path: &str) -> Option<[u8; 32]> {
    current_dek()
}

// =========================================================================
//  Вывод рабочего ключа: IKM = секретный DEK, info = контекст.
// =========================================================================
fn derive_key_from_dek(
    dek: &[u8; 32],
    context_info: &[u8],
) -> Result<Key<Aes256Gcm>, Box<dyn std::error::Error>> {
    let hk = Hkdf::<Sha256>::new(None, dek);
    let mut okm = [0u8; 32];
    hk.expand(context_info, &mut okm)
        .map_err(|_| "Ошибка расширения ключа в HKDF")?;
    Ok(*Key::<Aes256Gcm>::from_slice(&okm))
}

// =========================================================================
//  Сохранение / загрузка зашифрованных данных (localStorage)
// =========================================================================
// Формат записи: base64([12 байт nonce][ciphertext+tag]).
pub fn save_encrypted_file(
    storage_path: &str,
    filename: &str,
    context_info: &[u8],
    data: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let dek = get_or_create_dek(storage_path)?;
    let key = derive_key_from_dek(&dek, context_info)?;
    let cipher = Aes256Gcm::new(&key);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    fill_random(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted_data = cipher
        .encrypt(nonce, data.as_bytes())
        .map_err(|e| format!("Ошибка шифрования: {:?}", e))?;

    let mut blob = nonce_bytes.to_vec();
    blob.extend(encrypted_data);

    ls()?
        .set_item(&format!("{}{}", LS_FILE_PREFIX, filename), &STANDARD.encode(&blob))
        .map_err(js_err)?;
    Ok(())
}

pub fn load_decrypted_file(
    storage_path: &str,
    filename: &str,
    context_info: &[u8],
) -> Option<String> {
    let b64 = ls().ok()?.get_item(&format!("{}{}", LS_FILE_PREFIX, filename)).ok()??;
    let buffer = STANDARD.decode(b64.as_bytes()).ok()?;

    // минимум: nonce(12) + хотя бы 1 байт данных + tag(16)
    if buffer.len() < NONCE_LEN + 1 + 16 {
        return None;
    }

    let dek = get_existing_dek(storage_path)?;
    let key = derive_key_from_dek(&dek, context_info).ok()?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Nonce::from_slice(&buffer[0..NONCE_LEN]);

    let decrypted_bytes = cipher.decrypt(nonce, &buffer[NONCE_LEN..]).ok()?;
    String::from_utf8(decrypted_bytes).ok()
}

// =========================================================================
//  Высокоуровневый API
// =========================================================================
pub fn save_session(
    storage_path: &str,
    session: &UserSession,
) -> Result<(), Box<dyn std::error::Error>> {
    let json_str = serde_json::to_string(session)?;
    save_encrypted_file(storage_path, ".session", CONTEXT_SESSION, &json_str)
}

pub fn load_session(storage_path: &str) -> Option<UserSession> {
    let json_str = load_decrypted_file(storage_path, ".session", CONTEXT_SESSION)?;
    serde_json::from_str(&json_str).ok()
}

pub fn save_marks_cache(
    storage_path: &str,
    marks_json: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    save_encrypted_file(storage_path, ".marks_cache", CONTEXT_MARKS_CACHE, marks_json)
}

pub fn load_marks_cache(storage_path: &str) -> Option<String> {
    load_decrypted_file(storage_path, ".marks_cache", CONTEXT_MARKS_CACHE)
}

// Удалить зашифрованную запись (например .session и кеши при выходе).
pub fn delete_encrypted_file(storage_path: &str, filename: &str) {
    let _ = storage_path;
    if let Ok(storage) = ls() {
        let _ = storage.remove_item(&format!("{}{}", LS_FILE_PREFIX, filename));
    }
}

// Полный сброс локальных данных (logout): сессия, кеши и завёрнутый DEK.
// KEK остаётся — он бесполезен без DEK.
pub fn wipe_all() {
    if let Ok(storage) = ls() {
        let keys: Vec<String> = (0..storage.length().unwrap_or(0))
            .filter_map(|i| storage.key(i).ok().flatten())
            .collect();
        for k in keys {
            if k.starts_with(LS_FILE_PREFIX) || k == LS_WRAPPED_DEK {
                let _ = storage.remove_item(&k);
            }
        }
    }
    *DEK.lock().unwrap() = None;
}
