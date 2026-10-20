// Release builds on Windows have no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Token discovery re-runs this executable to probe each vendor PKCS#11
    // module in a child process; answer those requests without starting the UI.
    if matches!(args.first().map(String::as_str), Some("probe-token" | "probe-cert")) {
        if let Err(e) = macsigner::cli_esign::run(&args) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let initial = args
        .into_iter()
        .find(|a| a.to_ascii_lowercase().ends_with(".pdf"));
    macsigner_app_lib::run(initial);
}
