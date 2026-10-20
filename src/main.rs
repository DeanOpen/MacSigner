use macsigner::{
    cli_esign, config,
    ctsign::signing::SigningState,
    selftest::{self, Outcome},
    services::{CtsignService, EsignService},
};

const HELP: &str = "MacSigner — ESIGN + CTSIGN trong một ứng dụng Rust.
Cách dùng:
  macsigner                      Mở giao diện desktop
  macsigner serve [--esign] [--ctsign]
                                 Chạy dịch vụ không giao diện. Không có cờ: chạy
                                 các dịch vụ đang bật trong cấu hình
  macsigner selftest             Tự kiểm tra cả hai mô-đun (mã thoát 1 nếu lỗi)
  macsigner config               In đường dẫn và nội dung cấu hình
  macsigner esign ...            Công cụ dòng lệnh ESIGN (xem `macsigner esign --help`)
  macsigner ctsign --headless    Chỉ chạy API CTSIGN (CTSIGN_PORT ghi đè cổng)

Biến môi trường: MACSIGNER_CONFIG đặt đường dẫn tệp cấu hình.";

fn serve(args: &[String]) -> Result<(), String> {
    let path = config::default_path();
    let settings = config::Settings::load(&path)?;
    let (esign_on, ctsign_on) = if args.is_empty() {
        (settings.esign.enabled, settings.ctsign.enabled)
    } else {
        let mut flags = (false, false);
        for arg in args {
            match arg.as_str() {
                "--esign" => flags.0 = true,
                "--ctsign" => flags.1 = true,
                other => return Err(format!("Tùy chọn không hợp lệ: {other}")),
            }
        }
        flags
    };
    if !esign_on && !ctsign_on {
        return Err(format!(
            "Không có dịch vụ nào được bật. Dùng --esign/--ctsign hoặc bật trong {}",
            path.display()
        ));
    }
    let mut esign = EsignService::default();
    let mut ctsign = CtsignService::new(SigningState::default()).map_err(|e| e.to_string())?;
    if esign_on {
        let address = esign.start(&settings.esign)?;
        eprintln!("ESIGN WebSocket: ws://{address}/plugin/sign");
    }
    if ctsign_on {
        let address = ctsign.start(&settings.ctsign)?;
        eprintln!("CTSIGN API: http://{address} (chưa nạp khóa, ký hash bị tắt)");
    }
    eprintln!("Nhấn Ctrl+C để dừng.");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(tokio::signal::ctrl_c())
        .map_err(|e| e.to_string())?;
    esign.stop();
    ctsign.stop();
    Ok(())
}

fn run_selftest() -> Result<(), String> {
    let settings = config::Settings::load(&config::default_path())?;
    let mut checks = selftest::esign(&settings.esign, None);
    checks.extend(selftest::ctsign(&SigningState::default(), None));
    let mut failed = 0;
    for check in &checks {
        let mark = match check.outcome {
            Outcome::Pass => "PASS",
            Outcome::Fail => {
                failed += 1;
                "FAIL"
            }
            Outcome::Skip => "SKIP",
        };
        println!(
            "[{mark}] {:<6} {} — {}",
            check.module, check.name, check.detail
        );
    }
    if failed > 0 {
        Err(format!("{failed} kiểm tra thất bại"))
    } else {
        Ok(())
    }
}

fn ctsign_headless() -> Result<(), String> {
    let mut settings = config::Settings::load(&config::default_path())?.ctsign;
    if let Ok(port) = std::env::var("CTSIGN_PORT") {
        settings.port = port
            .parse()
            .ok()
            .filter(|p| *p != 0)
            .ok_or("CTSIGN_PORT phải là số cổng từ 1 đến 65535")?;
    }
    let mut service = CtsignService::new(SigningState::default()).map_err(|e| e.to_string())?;
    let address = service.start(&settings)?;
    eprintln!("CTSign development API: http://{address} (signing disabled)");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(tokio::signal::ctrl_c())
        .map_err(|e| e.to_string())?;
    service.stop();
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            #[cfg(all(windows, feature = "desktop"))]
            hide_own_console();
            #[cfg(feature = "desktop")]
            return macsigner::desktop::run(None).map_err(|e| format!("Desktop: {e}"));
            #[cfg(not(feature = "desktop"))]
            Err(format!("Bản build không có giao diện.\n{HELP}"))
        }
        // `macsigner file.pdf` (also used by the OS "Open with") opens the viewer.
        #[cfg(feature = "desktop")]
        Some(file) if args.len() == 1 && file.to_ascii_lowercase().ends_with(".pdf") => {
            #[cfg(windows)]
            hide_own_console();
            macsigner::desktop::run(Some(file.into())).map_err(|e| format!("Desktop: {e}"))
        }
        Some("--help" | "-h" | "help") => {
            println!("{HELP}");
            Ok(())
        }
        Some("serve") => serve(&args[1..]),
        Some("selftest") => run_selftest(),
        Some("config") if args.len() == 1 => {
            let path = config::default_path();
            let settings = config::Settings::load(&path)?;
            println!("{}", path.display());
            println!(
                "{}",
                serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        Some("esign") => cli_esign::run(&args[1..]).map_err(|e| e.to_string()),
        // Spawned by token discovery (see esign::discovery::probe).
        Some("probe-token" | "probe-cert") => cli_esign::run(&args).map_err(|e| e.to_string()),
        Some("ctsign") if args.get(1).map(String::as_str) == Some("--headless") => {
            ctsign_headless()
        }
        _ => Err(format!("Tham số không hợp lệ\n{HELP}")),
    }
}

// The binary keeps the console subsystem so CLI commands print normally. When the
// GUI is started by double-click, Windows creates a console used by this process
// alone; detach from it. A console shared with a shell (count > 1) is kept.
#[cfg(all(windows, feature = "desktop"))]
fn hide_own_console() {
    unsafe extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
        fn FreeConsole() -> i32;
    }
    let mut ids = [0u32; 2];
    // SAFETY: the buffer length passed matches the array; both calls take no
    // other pointers and only affect this process's console attachment.
    unsafe {
        if GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) == 1 {
            FreeConsole();
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("macsigner: {e}");
        std::process::exit(1);
    }
}
