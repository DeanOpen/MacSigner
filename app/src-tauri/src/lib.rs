// Tauri shell around the `macsigner` signing core.
mod bridge;
mod commands;
mod stamp;
mod updater;
mod usb;

use macsigner::{
    config::{self, Settings},
    ctsign::signing::SigningState,
    services::{CtsignService, EsignService},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// One certificate found on a USB token via PKCS#11 (not logged in yet).
#[derive(Clone)]
pub struct TokenCert {
    pub module: PathBuf,
    pub slot: u64,
    pub key_id: Vec<u8>,
    pub der: Vec<u8>,
}

pub struct AppState {
    pub signing: SigningState,
    pub esign: Mutex<EsignService>,
    pub ctsign: Mutex<CtsignService>,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub tokens: Mutex<Vec<TokenCert>>,
    /// Token devices from the last PKCS#11 scan (label, maker, PIN state…).
    pub token_devices: Mutex<Vec<serde_json::Value>>,
    pub reader_status: Mutex<String>,
    pub initial_file: Mutex<Option<String>>,
}

pub fn run(initial_file: Option<String>) {
    let settings_path = config::default_path();
    let settings = Settings::load(&settings_path).unwrap_or_default();
    macsigner::activity::open(settings_path.with_file_name("activity.jsonl"));
    let signing = SigningState::default();
    let ctsign = CtsignService::new(signing.clone()).expect("tokio runtime");
    // Browser signing requests are approved by the user in this window.
    let browser = Arc::new(bridge::BrowserSigning::default());
    let state = AppState {
        signing,
        esign: Mutex::new(EsignService::with_backend(browser.clone())),
        ctsign: Mutex::new(ctsign),
        settings: Mutex::new(settings.clone()),
        settings_path,
        tokens: Mutex::new(Vec::new()),
        token_devices: Mutex::new(Vec::new()),
        reader_status: Mutex::new(String::new()),
        initial_file: Mutex::new(initial_file),
    };
    // Services the user enabled start with the app; failures surface in the UI.
    // ESIGN starts in setup, once its prompts have a window to show in.
    if settings.ctsign.enabled {
        let _ = state.ctsign.lock().unwrap().start(&settings.ctsign);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .manage(usb::UsbState::default())
        .manage(browser.clone())
        .setup(move |app| {
            usb::spawn(app.handle().clone());
            browser.attach(app.handle().clone());
            updater::spawn(app.handle().clone(), browser.clone());
            if settings.esign.enabled {
                let state = tauri::Manager::state::<AppState>(app);
                let _ = state.esign.lock().unwrap().start(&settings.esign);
            }
            // Token PIN dialogs (Windows store keys) open in front of our window.
            #[cfg(windows)]
            if let Some(hwnd) = tauri::Manager::get_webview_window(app, "main").and_then(|w| w.hwnd().ok()) {
                macsigner::certstore::set_owner_window(hwnd.0 as isize);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::take_initial_file,
            commands::list_certificates,
            commands::import_certificate,
            commands::read_file,
            commands::default_output,
            commands::sign_pdf,
            commands::verify_pdf,
            commands::get_settings,
            commands::save_settings,
            commands::services_status,
            commands::vnpt_diagnostics,
            commands::set_service,
            commands::set_api_signing,
            commands::get_preferences,
            commands::save_preferences,
            commands::run_selftest,
            commands::sample_sign,
            commands::offline_status,
            commands::activity_list,
            commands::activity_clear,
            commands::token_devices,
            bridge::esign_answer,
            bridge::esign_pending,
            usb::token_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MacSigner");
}
