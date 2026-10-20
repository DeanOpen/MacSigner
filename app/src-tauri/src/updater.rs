// Automatic updates from GitHub Releases. Release builds check latest.json
// (plugins.updater in tauri.conf.json) shortly after start and then every few
// hours; a newer, signature-verified build is downloaded, installed and the app
// restarts into it. The window gets an "app-update" event first so the user
// sees why. MACSIGNER_NO_UPDATE=1 turns this off (e.g. managed installs).
use crate::bridge::BrowserSigning;
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

const FIRST_CHECK: Duration = Duration::from_secs(15);
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Time between the notice in the window and the restart.
const NOTICE: Duration = Duration::from_secs(5);

#[derive(Serialize, Clone)]
struct Notice {
    version: String,
}

async fn update_once(app: &AppHandle, browser: &BrowserSigning) -> tauri_plugin_updater::Result<()> {
    let Some(update) = app.updater()?.check().await? else {
        return Ok(());
    };
    let bytes = update.download(|_, _| {}, || {}).await?;
    // Never restart under a signing request the user is answering.
    while browser.busy() {
        std::thread::sleep(Duration::from_secs(5));
    }
    let _ = app.emit("app-update", Notice { version: update.version.clone() });
    std::thread::sleep(NOTICE);
    // On Windows the installer takes over here: it exits the app, installs
    // passively and starts the new version itself.
    update.install(bytes)?;
    app.restart();
}

pub fn spawn(app: AppHandle, browser: Arc<BrowserSigning>) {
    if cfg!(debug_assertions) || std::env::var_os("MACSIGNER_NO_UPDATE").is_some_and(|v| v != "0") {
        return;
    }
    std::thread::Builder::new()
        .name("updater".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            loop {
                if let Err(e) = tauri::async_runtime::block_on(update_once(&app, &browser)) {
                    eprintln!("update check failed: {e}");
                }
                std::thread::sleep(INTERVAL);
            }
        })
        .ok();
}
