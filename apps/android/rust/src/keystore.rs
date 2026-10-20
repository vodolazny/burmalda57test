// KEK Android: заворачивание/разворачивание DEK через Android Keystore.
// Ключ и его alias живут в Kotlin (ru.burmalda.journal.KeystoreCrypto).

use jni::objects::{JByteArray, JValue};

use crate::android;

fn call(method: &str, input: &[u8]) -> Result<Vec<u8>, jni::errors::Error> {
    android::with_class(
        "ru.burmalda.journal.KeystoreCrypto",
        |env, cls, _context| {
            let arg = env.byte_array_from_slice(input)?;
            let res: JByteArray = env
                .call_static_method(cls, method, "([B)[B", &[JValue::Object(&arg)])?
                .l()?
                .into();
            Ok(env.convert_byte_array(res)?)
        },
    )
}

pub(crate) fn wrap(plain: &[u8]) -> Result<Vec<u8>, jni::errors::Error> {
    call("wrap", plain)
}

/// Пустой ответ — сентинал «ключ потерян», а не ошибка транспорта.
pub(crate) fn unwrap(blob: &[u8]) -> Result<Vec<u8>, jni::errors::Error> {
    call("unwrap", blob)
}
