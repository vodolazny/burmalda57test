// Порты, которые реализуют платформы: шифрованное хранилище и часы.
//
// SecureStore синхронный: и мобильные (Keystore/Keychain + файлы), и веб
// (WebCrypto + localStorage) успевают подготовить ключ до старта приложения,
// поэтому дальше чтение/запись не требуют async.

/// Хранилище расшифрованных строк по имени файла и HKDF-контексту.
/// Платформа сама решает, где лежат байты (файл или localStorage) и как
/// защищён DEK.
pub trait SecureStore: Send + Sync {
    fn load(&self, storage_path: &str, filename: &str, context: &[u8]) -> Option<String>;
    fn save(
        &self,
        storage_path: &str,
        filename: &str,
        context: &[u8],
        data: &str,
    ) -> Result<(), String>;
    fn delete(&self, storage_path: &str, filename: &str);
}

/// Формат кеша дней на диске. Мобильные приложения пишут `{days,stamps}`,
/// веб исторически писал простую карту «дата -> ответ»; читаем оба.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheFormat {
    Stamps,
    LegacyMap,
}
