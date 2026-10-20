// Криптографические примитивы, общие для мобильных и веб-приложений.
//
// Здесь только чистая криптография: шифрование AES-256-GCM с ключом,
// выведенным HKDF из DEK, и реверс-функция apikey. Хранение ключей
// (Keystore/Keychain/WebCrypto) и файлов/localStorage — в адаптерах.

use aes::cipher::{BlockEncryptMut, KeyInit as AesKeyInit};
use aes::Aes128;
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;

pub const NONCE_LEN: usize = 12;

// ============================================================
//  Имена зашифрованных файлов и контексты HKDF.
//  МЕНЯТЬ НЕЛЬЗЯ: по ним читаются данные, созданные прошлыми версиями.
// ============================================================
pub const FILE_SESSION: &str = ".session";
pub const FILE_MARKS_CACHE: &str = ".marks_cache";
pub const FILE_PERIODS: &str = ".periods";
pub const FILE_GRADES_CACHE: &str = ".grades_cache";
pub const FILE_FINALS_CACHE: &str = ".finals_cache";
pub const FILE_GRADES_NOTIFY: &str = ".grades_notify";
pub const FILE_EVENTS: &str = ".events";
pub const FILE_HOMEWORK: &str = ".homework";
pub const FILE_NOTES: &str = ".notes";
pub const FILE_SUBJECTS: &str = ".subjects_filter";

pub const CONTEXT_SESSION: &[u8] = b"burmalda57_session_context_v1";
pub const CONTEXT_MARKS_CACHE: &[u8] = b"burmalda57_marks_cache_context_v1";
pub const CONTEXT_PERIODS: &[u8] = b"burmalda57_periods_context_v1";
pub const CONTEXT_GRADES: &[u8] = b"burmalda57_grades_context_v1";
pub const CONTEXT_FINALS: &[u8] = b"burmalda57_finals_context_v1";
pub const CONTEXT_NOTIFY: &[u8] = b"burmalda57_grades_notify_context_v1";
pub const CONTEXT_EVENTS: &[u8] = b"burmalda57_events_context_v1";
pub const CONTEXT_HOMEWORK: &[u8] = b"burmalda57_homework_context_v1";
pub const CONTEXT_NOTES: &[u8] = b"burmalda57_notes_context_v1";
pub const CONTEXT_SUBJECTS: &[u8] = b"burmalda57_subjects_filter_context_v1";

// ============================================================
//  Реверс-инжиниринговый apikey: AES-128-ECB + Base64.
// ============================================================
pub fn ahh_encrypt(text: &str) -> String {
    let key_bytes: [u8; 16] = [31, 23, 19, 50, 40, 23, 19, 10, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut encrypted_bytes = text.as_bytes().to_vec();

    let block_size = 16;
    let padding_len = block_size - (encrypted_bytes.len() % block_size);
    encrypted_bytes.resize(encrypted_bytes.len() + padding_len, padding_len as u8);

    let mut cipher = Aes128::new(&key_bytes.into());
    for chunk in encrypted_bytes.chunks_exact_mut(16) {
        let block = aes::cipher::generic_array::GenericArray::from_mut_slice(chunk);
        cipher.encrypt_block_mut(block);
    }

    STANDARD.encode(encrypted_bytes)
}

// ============================================================
//  Envelope: DEK -> (HKDF) ключ контекста -> AES-256-GCM.
//  Формат blob: [12 байт nonce][ciphertext+tag].
// ============================================================
fn derive_key(dek: &[u8; 32], context: &[u8]) -> Result<Key<Aes256Gcm>, String> {
    let hk = Hkdf::<Sha256>::new(None, dek);
    let mut okm = [0u8; 32];
    hk.expand(context, &mut okm)
        .map_err(|_| "Ошибка расширения ключа в HKDF".to_string())?;
    Ok(*Key::<Aes256Gcm>::from_slice(&okm))
}

pub fn encrypt_blob(dek: &[u8; 32], context: &[u8], plaintext: &str) -> Result<Vec<u8>, String> {
    let key = derive_key(dek, context)?;
    let cipher = Aes256Gcm::new(&key);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Ошибка шифрования: {:?}", e))?;

    let mut blob = nonce_bytes.to_vec();
    blob.extend(encrypted);
    Ok(blob)
}

pub fn decrypt_blob(dek: &[u8; 32], context: &[u8], blob: &[u8]) -> Option<String> {
    if blob.len() < NONCE_LEN + 1 + 16 {
        return None;
    }
    let key = derive_key(dek, context).ok()?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Nonce::from_slice(&blob[..NONCE_LEN]);
    let plain = cipher.decrypt(nonce, &blob[NONCE_LEN..]).ok()?;
    String::from_utf8(plain).ok()
}

/// Случайные байты для нового DEK (32) или обёртки KEK.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    rand::thread_rng().fill_bytes(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ahh_encrypt_matches_legacy() {
        // Значение зафиксировано по прежней реализации (AES-128-ECB + PKCS#7).
        assert_eq!(ahh_encrypt("hello"), "wba93X6DFZC/T6LbQ0SINA==");
    }

    #[test]
    fn envelope_roundtrip_and_context_isolation() {
        let dek = [7u8; 32];
        let blob = encrypt_blob(&dek, CONTEXT_SESSION, "секрет").unwrap();
        assert_eq!(
            decrypt_blob(&dek, CONTEXT_SESSION, &blob).as_deref(),
            Some("секрет")
        );
        assert_eq!(decrypt_blob(&dek, CONTEXT_MARKS_CACHE, &blob), None);
        assert_eq!(decrypt_blob(&[8u8; 32], CONTEXT_SESSION, &blob), None);
    }

    #[test]
    fn envelope_rejects_short_blob() {
        assert_eq!(decrypt_blob(&[1u8; 32], CONTEXT_NOTES, &[0u8; 10]), None);
    }
}
