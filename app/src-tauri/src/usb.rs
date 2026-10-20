// Live USB token detection. SCardGetStatusChange blocks until a reader or card
// changes (PnP notification covers newly attached readers), so detection is
// instant and costs no CPU, and it never connects to the card.
use pcsc::{Context, Error, PNP_NOTIFICATION, ReaderState, Scope, State};
use serde::Serialize;
use std::{ffi::CString, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reader {
    pub name: String,
    pub present: bool,
}

#[derive(Default)]
pub struct UsbState(pub Mutex<Vec<Reader>>);

fn watch(app: &AppHandle) -> Result<(), Error> {
    let ctx = Context::establish(Scope::User)?;
    let pnp: CString = PNP_NOTIFICATION().into();
    let mut states = vec![ReaderState::new(pnp.clone(), State::UNAWARE)];
    let mut last: Option<Vec<Reader>> = None;
    loop {
        let names = match ctx.list_readers_owned() {
            Ok(names) => names,
            Err(Error::NoReadersAvailable) => Vec::new(),
            Err(e) => return Err(e),
        };
        states.retain(|s| s.name() == pnp.as_c_str() || names.iter().any(|n| n.as_c_str() == s.name()));
        for name in names {
            if !states.iter().any(|s| s.name() == name.as_c_str()) {
                states.push(ReaderState::new(name, State::UNAWARE));
            }
        }
        match ctx.get_status_change(Duration::from_secs(5), &mut states) {
            Ok(()) | Err(Error::Timeout) => {}
            Err(Error::ServiceStopped | Error::NoService) => return Err(Error::NoService),
            Err(e) => return Err(e),
        }
        for s in &mut states {
            s.sync_current_state();
        }
        let snapshot: Vec<Reader> = states
            .iter()
            .filter(|s| s.name() != pnp.as_c_str())
            .map(|s| Reader {
                name: s.name().to_string_lossy().into_owned(),
                present: s.event_state().contains(State::PRESENT),
            })
            .collect();
        if last.as_ref() != Some(&snapshot) {
            if let Ok(mut current) = app.state::<UsbState>().0.lock() {
                *current = snapshot.clone();
            }
            let _ = app.emit("token-status", &snapshot);
            last = Some(snapshot);
        }
    }
}

/// Runs for the lifetime of the app; restarts if the smart-card service restarts.
pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("usb-token-watch".into())
        .spawn(move || loop {
            let _ = watch(&app);
            std::thread::sleep(Duration::from_secs(2));
        })
        .ok();
}

#[tauri::command]
pub fn token_status(state: tauri::State<'_, UsbState>) -> Vec<Reader> {
    state.0.lock().map(|r| r.clone()).unwrap_or_default()
}
