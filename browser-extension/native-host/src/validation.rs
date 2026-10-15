use serde_json::Value;
use std::io::Cursor;

pub fn session_id(id: &str) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "Session UUID khong hop le.")?;
    if parsed.to_string() != id {
        return Err("Session UUID phai o dang canonical.".into());
    }
    Ok(())
}
pub fn number(value: &Value, field: &str) -> Result<f64, String> {
    value[field]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.)
        .ok_or_else(|| format!("{field} khong hop le."))
}
pub fn integer(value: &Value, field: &str) -> Result<u64, String> {
    value[field]
        .as_u64()
        .ok_or_else(|| format!("{field} khong hop le."))
}
pub fn region(value: &Value) -> Result<(), String> {
    let x = number(value, "x")?;
    let y = number(value, "y")?;
    let width = number(value, "width")?;
    let height = number(value, "height")?;
    if width < 0.01 || height < 0.01 || x + width > 1.000001 || y + height > 1.000001 {
        return Err("ROI ngoai anh.".into());
    }
    Ok(())
}
pub fn session(value: &Value, id: &str) -> Result<(), String> {
    if value["id"] != id
        || !matches!(value["schemaVersion"].as_u64(), Some(1 | 2))
        || !value.is_object()
    {
        return Err("Session metadata khong hop le.".into());
    }
    number(value, "createdAt")?;
    integer(value, "sampleCount")?;
    integer(value, "imageBytes")?;
    if value["schemaVersion"] == 2 {
        if value["captureMode"] != "smart" {
            return Err("Schema 2 chi ho tro smart capture.".into());
        }
        integer(value, "observationCount")?;
        integer(value, "fallbackCount")?;
    }
    region(&value["region"])?;
    for field in ["title", "url"] {
        let text = value[field].as_str().ok_or("Thieu title/url.")?;
        if text.len() > 8192 {
            return Err("Title/url qua dai.".into());
        }
    }
    if !value["url"].as_str().unwrap().starts_with("https://")
        && !value["url"].as_str().unwrap().starts_with("http://")
    {
        return Err("URL nguon khong hop le.".into());
    }
    for field in ["id", "documentId", "origin"] {
        if value["source"][field]
            .as_str()
            .filter(|text| !text.is_empty() && text.len() <= 8192)
            .is_none()
        {
            return Err("Thieu source identity.".into());
        }
    }
    integer(&value["source"], "frameId")?;
    if !matches!(
        value["status"].as_str(),
        Some("recording" | "paused" | "stopped" | "interrupted")
    ) {
        return Err("Capture status khong hop le.".into());
    }
    if !value["endedAt"].is_null() {
        number(value, "endedAt")?;
    }
    let events = value["events"].as_array().ok_or("Thieu events.")?;
    if events.len() > 10_000 {
        return Err("Qua nhieu event.".into());
    }
    for event in events {
        number(event, "epochMs")?;
        if !event["mediaTimeMs"].is_null() {
            number(event, "mediaTimeMs")?;
        }
        for field in ["kind", "detail"] {
            if event[field]
                .as_str()
                .filter(|text| text.len() <= 16_384)
                .is_none()
            {
                return Err("Event khong hop le.".into());
            }
        }
    }
    Ok(())
}
pub fn sample(value: &Value, id: &str, source: &Value, image: &[u8]) -> Result<(), String> {
    if image.is_empty() || image.len() > 12 * 1024 * 1024 {
        return Err("Sample/payload khong hop le.".into());
    }
    sample_metadata(value, id, source)?;
    validate_png(value, image)
}
pub fn sample_metadata(value: &Value, id: &str, source: &Value) -> Result<(), String> {
    if value["sessionId"] != id {
        return Err("Sample sai session.".into());
    }
    for field in ["sequence", "segment"] {
        integer(value, field)?;
    }
    for field in [
        "capturedEpochMs",
        "estimatedVideoTimeMs",
        "uncertaintyMs",
        "roundTripMs",
    ] {
        number(value, field)?;
    }
    if number(value, "roundTripMs")? > 500. {
        return Err("Clock RTT qua cham.".into());
    }
    region(&value["region"])?;
    let clock = &value["clock"];
    for field in ["mediaTime", "epochMs", "rate", "width", "height"] {
        number(clock, field)?;
    }
    if number(clock, "rate")? <= 0. {
        return Err("Playback rate khong hop le.".into());
    }
    if !clock["duration"].is_null() && number(clock, "duration")? <= 0. {
        return Err("Duration khong hop le.".into());
    }
    for field in ["frameMediaTime", "frameEpochMs"] {
        if !clock[field].is_null() {
            number(clock, field)?;
        }
    }
    for field in ["paused", "seeking", "waiting", "ended"] {
        if !clock[field].is_boolean() {
            return Err("Clock state khong hop le.".into());
        }
    }
    if clock["seeking"] == true || clock["waiting"] == true || integer(clock, "readyState")? < 2 {
        return Err("Clock dang tua/tai dem.".into());
    }
    let video_id = clock["videoId"].as_str().ok_or("Thieu video ID.")?;
    if !source["id"]
        .as_str()
        .unwrap_or("")
        .ends_with(&format!(":{video_id}"))
    {
        return Err("Clock sai video.".into());
    }
    if clock["sourceKey"].as_str().is_none()
        || clock["layout"].as_str().is_none()
        || !value["manual"].is_boolean()
    {
        return Err("Clock/source khong hop le.".into());
    }
    let width = integer(&value["captureSize"], "width")?;
    let height = integer(&value["captureSize"], "height")?;
    if width == 0 || height == 0 || width > 1920 || height > 1080 {
        return Err("Capture qua lon.".into());
    }
    Ok(())
}
fn validate_png(value: &Value, image: &[u8]) -> Result<(), String> {
    let width = integer(&value["captureSize"], "width")?;
    let height = integer(&value["captureSize"], "height")?;
    let region = &value["region"];
    let x = (number(region, "x")? * width as f64).floor() as u64;
    let y = (number(region, "y")? * height as f64).floor() as u64;
    let crop_width = ((number(region, "width")? * width as f64).round() as u64)
        .max(1)
        .min(width - x);
    let crop_height = ((number(region, "height")? * height as f64).round() as u64)
        .max(1)
        .min(height - y);
    let mut reader = png::Decoder::new(Cursor::new(image))
        .read_info()
        .map_err(|_| "PNG header khong hop le.")?;
    if reader.info().width as u64 != crop_width
        || reader.info().height as u64 != crop_height
        || reader.output_buffer_size() > 1920 * 1080 * 8
    {
        return Err("PNG khong khop ROI.".into());
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut pixels).map_err(|_| "PNG bi hong.")?;
    Ok(())
}
