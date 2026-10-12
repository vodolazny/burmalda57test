// JNI-мост (только Android): запуск экрана входа, Toast, приём токена и «Назад».
// Платформонезависимая логика живёт в session_flow.rs, здесь — только JNI.
use jni::objects::{JClass, JObject, JString, JValue};
use jni::JNIEnv;

// ============================================================
//  JNI: Kotlin отдаёт токен после WebView-логина
// ============================================================
#[no_mangle]
pub extern "C" fn Java_ru_burmalda_journal_EsiaAuthActivity_sendTokenToRust(
    mut env: JNIEnv,
    _class: JClass,
    token: JString,
    storage_path: JString,
) {
    // Не паникуем в JNI-колбэке: с panic = "abort" .expect() уронил бы весь процесс.
    let raw_token_str: String = match env.get_string(&token) {
        Ok(s) => s.into(),
        Err(e) => {
            log::error!("sendTokenToRust: не удалось прочитать token: {:?}", e);
            return;
        }
    };
    let storage_path: String = match env.get_string(&storage_path) {
        Ok(s) => s.into(),
        Err(e) => {
            log::error!("sendTokenToRust: не удалось прочитать storage_path: {:?}", e);
            return;
        }
    };

    crate::session_flow::handle_esia_token(raw_token_str, storage_path);
}

// Запуск Kotlin-экрана входа через Intent
pub(crate) fn launch_login_activity() -> Result<(), jni::errors::Error> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };

    let res = (|| -> Result<(), jni::errors::Error> {
        let intent = env.new_object("android/content/Intent", "()V", &[])?;
        let pkg: JObject = env.new_string("ru.burmalda.journal")?.into();
        let cls: JObject = env.new_string("ru.burmalda.journal.EsiaAuthActivity")?.into();

        env.call_method(
            &intent,
            "setClassName",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&pkg), JValue::Object(&cls)],
        )?;

        // FLAG_ACTIVITY_NEW_TASK — обязателен при старте из не-Activity контекста
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(0x10000000)],
        )?;

        env.call_method(
            &activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )?;

        Ok(())
    })();

    if let Ok(true) = env.exception_check() {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }

    res
}

// Показ Toast через Kotlin Notifier.showToast
pub(crate) fn show_toast(text: &str) {
    #[cfg(target_os = "android")]
    {
        let ctx = ndk_context::android_context();
        let vm = match unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) } {
            Ok(v) => v,
            Err(_) => return,
        };
        let mut env = match vm.attach_current_thread() {
            Ok(e) => e,
            Err(_) => return,
        };
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };

        let _ = (|| -> Result<(), jni::errors::Error> {
            let class_loader = env
                .call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
                .l()?;
            let class_name = env.new_string("ru.burmalda.journal.Notifier")?;
            let cls_obj = env
                .call_method(
                    &class_loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&class_name)],
                )?
                .l()?;
            let cls: jni::objects::JClass = cls_obj.into();
            let j_text = env.new_string(text)?;
            env.call_static_method(
                &cls,
                "showToast",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[JValue::Object(&activity), JValue::Object(&j_text)],
            )?;
            Ok(())
        })();

        if let Ok(true) = env.exception_check() {
            let _ = env.exception_clear();
        }
    }
}

// ============================================================
//  JNI: Обработка системного жеста / кнопки «Назад»
// ============================================================
#[no_mangle]
pub extern "C" fn Java_ru_burmalda_journal_MainActivity_nativeOnBackPressed(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jboolean {
    if crate::session_flow::handle_back() { 1 } else { 0 }
}
