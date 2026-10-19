//! Android integration: app directory, clipboard, splash theme and system bars.

use crate::state::theme::Theme;
use dioxus::mobile::wry::prelude::dispatch;
use jni::{
    objects::{JObject, JString, JValue},
    JavaVM,
};

pub fn settings_dir() -> Result<std::path::PathBuf, String> {
    let read = || -> jni::errors::Result<std::path::PathBuf> {
        let context = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(context.vm().cast())? };
        let mut env = vm.attach_current_thread()?;
        let context = unsafe { JObject::from_raw(context.context().cast()) };
        let directory = env
            .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])?
            .l()?;
        let path = env
            .call_method(&directory, "getAbsolutePath", "()Ljava/lang/String;", &[])?
            .l()?;
        let path = JString::from(path);
        let path: String = env.get_string(&path)?.into();
        Ok(std::path::PathBuf::from(path).join("settings"))
    };
    read().map_err(|error| format!("Could not locate app settings: {error}"))
}

pub fn copy_text(text: &str) -> jni::errors::Result<()> {
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast())? };
    let mut env = vm.attach_current_thread()?;
    let context = unsafe { JObject::from_raw(context.context().cast()) };
    let service_name = env.new_string("clipboard")?;
    let manager = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        )?
        .l()?;
    let label = env.new_string("Tantrix invite")?;
    let text = env.new_string(text)?;
    let clip_data = env
        .call_static_method(
            "android/content/ClipData",
            "newPlainText",
            "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
            &[JValue::Object(&label), JValue::Object(&text)],
        )?
        .l()?;
    env.call_method(
        &manager,
        "setPrimaryClip",
        "(Landroid/content/ClipData;)V",
        &[JValue::Object(&clip_data)],
    )?;
    Ok(())
}

/// Read the effective app configuration, which includes Android's saved override.
/// UiModeManager.getNightMode() would read the phone's global setting instead.
pub fn current_theme() -> Theme {
    let read = || -> jni::errors::Result<Theme> {
        let context = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(context.vm().cast())? };
        let mut env = vm.attach_current_thread()?;
        let context = unsafe { JObject::from_raw(context.context().cast()) };
        let resources = env
            .call_method(
                &context,
                "getResources",
                "()Landroid/content/res/Resources;",
                &[],
            )?
            .l()?;
        let configuration = env
            .call_method(
                &resources,
                "getConfiguration",
                "()Landroid/content/res/Configuration;",
                &[],
            )?
            .l()?;
        let ui_mode = env.get_field(&configuration, "uiMode", "I")?.i()?;
        Ok(Theme::from_android_ui_mode(ui_mode))
    };
    read().unwrap_or_else(|error| {
        log::warn!("Could not read Android app theme: {error}");
        Theme::Dark
    })
}

/// Persist only this app's mode; Android applies it before the next launch.
pub fn set_theme(theme: Theme) {
    use jni::objects::JValue;

    dispatch(move |env, activity, _webview| {
        let result = (|| -> jni::errors::Result<()> {
            let service = env.new_string("uimode")?;
            let manager = env
                .call_method(
                    activity,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&service)],
                )?
                .l()?;
            // UiModeManager.MODE_NIGHT_NO / MODE_NIGHT_YES.
            let mode = match theme {
                Theme::Light => 1,
                Theme::Dark => 2,
            };
            env.call_method(
                &manager,
                "setApplicationNightMode",
                "(I)V",
                &[JValue::Int(mode)],
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            log::warn!("Could not save Android app theme: {error}");
        }
    });
}

pub fn apply_system_bar_theme(theme: Theme) {
    use jni::objects::JValue;

    dispatch(move |env, activity, webview| {
        let result = (|| -> jni::errors::Result<()> {
            let window = env
                .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            let insets = env
                .call_method(
                    &window,
                    "getInsetsController",
                    "()Landroid/view/WindowInsetsController;",
                    &[],
                )?
                .l()?;

            const LIGHT_STATUS_BARS: i32 = 0x08;
            const LIGHT_NAVIGATION_BARS: i32 = 0x10;
            let mask = LIGHT_STATUS_BARS | LIGHT_NAVIGATION_BARS;
            let light = theme == Theme::Light;
            let appearance = if light { mask } else { 0 };
            if !insets.is_null() {
                env.call_method(
                    &insets,
                    "setSystemBarsAppearance",
                    "(II)V",
                    &[JValue::Int(appearance), JValue::Int(mask)],
                )?;
            }

            let color = if light {
                0xFFFFFFFFu32 as i32
            } else {
                0xFF171717u32 as i32
            };
            env.call_method(webview, "setBackgroundColor", "(I)V", &[JValue::Int(color)])?;
            env.call_method(&window, "setStatusBarColor", "(I)V", &[JValue::Int(color)])?;
            env.call_method(
                &window,
                "setNavigationBarColor",
                "(I)V",
                &[JValue::Int(color)],
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            log::warn!("Could not update Android system bar theme: {error}");
        }
    });
}

/// Returns false after opening permission settings; the downloaded APK stays ready.
pub async fn install_update(path: std::path::PathBuf) -> Result<bool, String> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    dispatch(move |env, activity, _webview| {
        let result = (|| -> jni::errors::Result<String> {
            let path = env.new_string(path.to_string_lossy())?;
            let result = env
                .call_method(
                    activity,
                    "installUpdate",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&path)],
                )?
                .l()?;
            let result = JString::from(result);
            let message = env.get_string(&result)?.into();
            Ok(message)
        })()
        .map_err(|error| error.to_string())
        .and_then(|message| match message.as_str() {
            "" => Ok(true),
            "permission" => Ok(false),
            _ => Err(message),
        });
        let _ = sender.send(result);
    });
    receiver
        .await
        .map_err(|_| "The Android installer is unavailable.".to_string())?
}
