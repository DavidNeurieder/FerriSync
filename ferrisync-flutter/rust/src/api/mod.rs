pub use ferrisync_core::api::*;

/// Route the `log` crate to logcat when this library is loaded on Android.
/// Runs at dlopen time via .init_array — no explicit call needed.
#[used]
#[link_section = ".init_array"]
static ANDROID_LOGGER_INIT: extern "C" fn() = {
    extern "C" fn init() {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("ferrisync"),
        );
    }
    init
};
