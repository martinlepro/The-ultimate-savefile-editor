//! Point d'entrée Android : NativeActivity -> `android_main` -> interface SaveStudio.
#![cfg(target_os = "android")]

use winit::platform::android::activity::AndroidApp;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("SaveStudio"));
    let opts = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    if let Err(e) = savestudio::run(None, 0, opts) {
        log::error!("SaveStudio s'est arrêté : {e}");
    }
}
