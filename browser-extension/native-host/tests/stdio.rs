use base64::{engine::general_purpose::STANDARD, Engine};
use long_tieng_capture_host::protocol;
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

const ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const ID: &str = "8a2dc9eb-dc15-46f8-957a-62ad6eab31ed";
fn command(home: &Path) -> Command {
    let mut command = Command::new(home.join("capture-host"));
    command
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"));
    command
}
fn start(home: &Path) -> Child {
    command(home)
        .arg(ORIGIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}
fn send(child: &mut Child, request: Value) -> Value {
    let bytes = serde_json::to_vec(&request).unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(&(bytes.len() as u32).to_ne_bytes())
        .unwrap();
    stdin.write_all(&bytes).unwrap();
    stdin.flush().unwrap();
    let response = protocol::read_frame(child.stdout.as_mut().unwrap())
        .unwrap()
        .unwrap();
    serde_json::from_slice(&response).unwrap()
}
fn packet(operation: &str, sequence: u64, payload: &Value, image: &[u8]) -> Value {
    let payload = serde_json::to_string(payload).unwrap();
    let mut request = json!({ "version": 1, "requestId": format!("{operation}-{sequence}"), "operation": operation, "sessionId": ID, "messageSeq": sequence, "payloadJson": payload, "checksum": protocol::checksum(operation, ID, sequence, &payload, image) });
    if !image.is_empty() {
        request["imageBase64"] = STANDARD.encode(image).into();
    }
    request
}
#[test]
fn real_binary_origin_framing_commit_restart_and_lost_ack() {
    for schema in [1, 2] {
        exercise_capture(schema);
    }
}
fn exercise_capture(schema: u64) {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    fs::copy(
        env!("CARGO_BIN_EXE_long-tieng-capture-host"),
        home.join("capture-host"),
    )
    .unwrap();
    fs::write(
        home.join("host-config.json"),
        serde_json::to_vec(&json!({ "allowedOrigin": ORIGIN })).unwrap(),
    )
    .unwrap();
    assert!(!command(home)
        .arg("chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/")
        .output()
        .unwrap()
        .status
        .success());
    let data = command(home).arg("--app-data-dir").output().unwrap();
    let data = std::path::PathBuf::from(String::from_utf8(data.stdout).unwrap().trim());
    assert!(data.starts_with(home));
    let mut child = start(home);
    let hello = send(
        &mut child,
        json!({ "version": 1, "requestId": "hello", "operation": "hello" }),
    );
    assert_eq!(hello["value"]["appData"], data.to_string_lossy().as_ref());
    assert!(hello["value"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "smart-observations-v1"));
    assert_eq!(
        send(
            &mut child,
            json!({ "version": 2, "requestId": "wrong", "operation": "hello" })
        )["ok"],
        false
    );
    let mut session = json!({ "schemaVersion": 1, "id": ID, "title": "stdio fixture", "url": "https://example.test/movie", "createdAt": 1, "endedAt": null, "status": "recording", "source": { "id": "doc:video", "documentId": "doc", "frameId": 1, "origin": "https://example.test" }, "region": { "x": 0., "y": 0., "width": 1., "height": 1. }, "sampleCount": 0, "imageBytes": 0, "events": [] });
    if schema == 2 {
        session["schemaVersion"] = 2.into();
        session["captureMode"] = "smart".into();
        session["observationCount"] = 0.into();
        session["fallbackCount"] = 0.into();
    }
    assert_eq!(
        send(&mut child, packet("begin", 1, &session, &[]))["ok"],
        true
    );
    let mut image = vec![];
    {
        let encoder = png::Encoder::new(&mut image, 2, 2);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[1, 2, 3, 4]).unwrap();
    }
    let mut frame = json!({ "sessionId": ID, "sequence": 1, "segment": 0, "capturedEpochMs": 1, "estimatedVideoTimeMs": 1, "uncertaintyMs": 100, "roundTripMs": 1, "captureSize": { "width": 2, "height": 2 }, "region": session["region"], "manual": false, "clock": { "videoId": "video", "sourceKey": "movie", "mediaTime": 0.001, "duration": 20., "rate": 1., "paused": false, "seeking": false, "waiting": false, "ended": false, "readyState": 4, "epochMs": 1., "frameMediaTime": null, "frameEpochMs": null, "layout": "same", "width": 2, "height": 2 } });
    if schema == 2 {
        frame["smart"] = json!({"group":1,"stable":false,"save":true,"role":"fallback"});
    }
    let request = packet("appendSample", 2, &frame, &image);
    let bytes = serde_json::to_vec(&request).unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&(bytes.len() as u32).to_ne_bytes())
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(&bytes).unwrap();
    child.stdin.as_mut().unwrap().flush().unwrap();
    let checkpoint = data.join("ocr-sessions").join(ID).join("session.json");
    for _ in 0..200 {
        if fs::read(&checkpoint)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|value| value["sampleCount"] == 1)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&checkpoint).unwrap()).unwrap()["sampleCount"],
        1
    );
    child.kill().unwrap();
    child.wait().unwrap(); // The client intentionally never read the sample ACK.
    let mut child = start(home);
    assert_eq!(send(&mut child, request)["value"]["savedSamples"], 1);
    let mut finalize_sequence = 3;
    if schema == 2 {
        let mut observation = frame.clone();
        observation.as_object_mut().unwrap().remove("sequence");
        observation["observationSeq"] = 1.into();
        observation["imageSequence"] = 2.into();
        let bad = json!({"sessionId":ID,"observations":[observation]});
        assert_eq!(
            send(&mut child, packet("appendObservations", 3, &bad, &[]))["ok"],
            false
        );
        observation["imageSequence"] = 1.into();
        let batch = json!({"sessionId":ID,"observations":[observation]});
        let request = packet("appendObservations", 3, &batch, &[]);
        assert_eq!(send(&mut child, request.clone())["ok"], true);
        child.kill().unwrap();
        child.wait().unwrap();
        child = start(home);
        assert_eq!(send(&mut child, request)["ok"], true);
        session["observationCount"] = 1.into();
        session["fallbackCount"] = 1.into();
        finalize_sequence = 4;
    }
    session["sampleCount"] = 1.into();
    session["imageBytes"] = (image.len() as u64).into();
    session["status"] = "stopped".into();
    session["endedAt"] = 5.into();
    assert_eq!(
        send(
            &mut child,
            packet("finalize", finalize_sequence, &session, &[])
        )["value"]["finalized"],
        true
    );
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
    assert_eq!(
        fs::read_dir(checkpoint.parent().unwrap().join("frames"))
            .unwrap()
            .count(),
        1
    );
}
