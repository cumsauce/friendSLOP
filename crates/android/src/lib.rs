//! friendSLOP on Android (#354). The same app the desktop binary builds
//! (`crates/game/src/main.rs`), started by NativeActivity through
//! `#[bevy_main]`. Assets ship inside the APK and load through Bevy's
//! Android asset reader; data files take the browser's asset-server path
//! (see `friendslop-data`). Build with `scripts/build-android.sh`.
//!
//! On every other platform this crate is empty.

#[cfg(target_os = "android")]
#[path = "../../game/src/main.rs"]
#[allow(dead_code)]
mod game;

#[cfg(target_os = "android")]
#[bevy::prelude::bevy_main]
fn main() {
    use_app_storage();
    game::main();
}

/// Settings and saves go under the app's private storage: Android gives a
/// native app no `HOME`, so `config_path` would find nowhere to write.
#[cfg(target_os = "android")]
fn use_app_storage() {
    let Some(app) = bevy::android::ANDROID_APP.get() else {
        return;
    };
    if std::env::var_os("XDG_CONFIG_HOME").is_none()
        && let Some(dir) = app.internal_data_path()
    {
        // SAFETY: called first thing in `main`, before any other thread starts.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", dir) };
    }
}
