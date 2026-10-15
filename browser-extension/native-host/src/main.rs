use long_tieng_capture_host::{
    app_data_dir, ocr,
    protocol::{self, Request, Response},
    storage::{Host, Limits},
};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
};

fn run() -> Result<(), String> {
    let argument = std::env::args().nth(1).ok_or("Thieu Chrome origin.")?;
    if argument == "--app-data-dir" {
        println!("{}", app_data_dir()?.display());
        return Ok(());
    }
    if argument == "--ocr-worker" {
        let id = std::env::args().nth(2).ok_or("Thieu session UUID.")?;
        let job = std::env::args().nth(3).ok_or("Thieu OCR job UUID.")?;
        let result = ocr::run(&app_data_dir()?, &id, &job)?;
        eprintln!("OCR committed: {}", result);
        return Ok(());
    }
    if argument == "--ocr-process" || argument == "--ocr-reprocess" {
        let id = std::env::args().nth(2).ok_or("Thieu session UUID.")?;
        println!(
            "{}",
            ocr::process(&app_data_dir()?, &id, argument == "--ocr-reprocess")?
        );
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let config: serde_json::Value = serde_json::from_slice(
        &fs::read(executable.parent().unwrap().join("host-config.json"))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if config["allowedOrigin"].as_str() != Some(&argument)
        || !argument.starts_with("chrome-extension://")
    {
        return Err("Chrome origin khong duoc phep.".into());
    }
    let data = app_data_dir()?;
    fs::create_dir_all(data.join("diagnostics")).map_err(|error| error.to_string())?;
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(data.join("diagnostics/capture-host.log"))
        .map_err(|error| error.to_string())?;
    let mut host = Host::new(data.join("ocr-sessions"), Limits::default())?;
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    while let Some(bytes) = protocol::read_frame(&mut stdin).map_err(|error| error.to_string())? {
        let parsed = serde_json::from_slice::<Request>(&bytes);
        let request_id = parsed
            .as_ref()
            .map(|request| request.request_id.clone())
            .unwrap_or_default();
        let finalize_session = parsed
            .as_ref()
            .ok()
            .filter(|r| r.operation == "finalize")
            .and_then(|r| r.session_id.clone());
        let result = match parsed {
            Ok(request) => (if matches!(
                request.operation.as_str(),
                "ocrStart" | "ocrStatus" | "ocrCancel"
            ) {
                if request.version != protocol::VERSION
                    || request.request_id.is_empty()
                    || request.request_id.len() > 128
                {
                    Err("OCR protocol/request ID sai.".into())
                } else if let Some(id) = request.session_id.as_deref() {
                    match request.operation.as_str() {
                        "ocrStart" => ocr::start(&data, id),
                        "ocrStatus" => ocr::status(&data, id),
                        _ => ocr::cancel(&data, id),
                    }
                } else {
                    Err("Thieu session UUID.".into())
                }
            } else {
                host.handle(&request)
            })
            .and_then(|value| {
                if request.operation == "openFolder" {
                    let directory = value["sessionPath"]
                        .as_str()
                        .ok_or("Thieu session directory.")?;
                    let status = std::process::Command::new("/usr/bin/open")
                        .arg(PathBuf::from(directory))
                        .status()
                        .map_err(|error| error.to_string())?;
                    if !status.success() {
                        return Err("Khong mo duoc thu muc.".into());
                    }
                }
                Ok(value)
            }),
            Err(_) => Err("Native request JSON khong hop le.".into()),
        };
        if let Err(failure) = &result {
            let _ = writeln!(log, "{}", failure);
            let _ = writeln!(io::stderr(), "{}", failure);
        }
        let response = match result {
            Ok(value) => Response {
                version: protocol::VERSION,
                request_id,
                ok: true,
                value: Some(value),
                error: None,
            },
            Err(error) => Response {
                version: protocol::VERSION,
                request_id,
                ok: false,
                value: None,
                error: Some(error),
            },
        };
        protocol::write_frame(&mut stdout, &response).map_err(|error| error.to_string())?;
        if response.ok && config["autoOcr"] == true {
            if let Some(id) = finalize_session {
                if let Err(error) = ocr::start(&data, &id) {
                    let _ = writeln!(log, "OCR start: {error}");
                }
            }
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
