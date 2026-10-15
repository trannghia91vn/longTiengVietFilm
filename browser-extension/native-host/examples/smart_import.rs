// QA-only importer exercises the real durable storage API on an isolated output directory.
use base64::{engine::general_purpose::STANDARD, Engine};
use long_tieng_capture_host::{
    protocol::{self, Request},
    storage::{Host, Limits},
};
use serde_json::Value;
use std::{fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let source = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let plan: Value = serde_json::from_slice(&fs::read(&args[3])?)?;
    let mut host = Host::new(output, Limits::default())?;
    let id = plan["session"]["id"].as_str().ok_or("Missing id")?;
    let mut message_seq = 0;
    let mut send = |operation: &str,
                    payload: &Value,
                    image: &[u8]|
     -> Result<(), Box<dyn std::error::Error>> {
        message_seq += 1;
        let text = serde_json::to_string(payload)?;
        host.handle(&Request {
            version: 1,
            request_id: format!("qa-{message_seq}"),
            operation: operation.into(),
            session_id: Some(id.into()),
            message_seq: Some(message_seq),
            payload_json: Some(text.clone()),
            checksum: Some(protocol::checksum(operation, id, message_seq, &text, image)),
            image_base64: if image.is_empty() {
                None
            } else {
                Some(STANDARD.encode(image))
            },
        })?;
        Ok(())
    };
    send("begin", &plan["session"], &[])?;
    for image in plan["images"].as_array().ok_or("Missing images")? {
        let bytes = fs::read(source.join("frames").join(
            format!("{:08}.png",image["sourceSequence"].as_u64().ok_or("Missing source sequence")?),
        ))?;
        send("appendSample", &image["metadata"], &bytes)?;
    }
    for batch in plan["observations"]
        .as_array()
        .ok_or("Missing observations")?
        .chunks(5)
    {
        send(
            "appendObservations",
            &serde_json::json!({"sessionId":id,"observations":batch}),
            &[],
        )?;
    }
    send("finalize", &plan["finalize"], &[])?;
    println!("{}", id);
    Ok(())
}
