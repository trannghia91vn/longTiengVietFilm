use crate::{
    protocol::{self, Request},
    validation,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub struct Limits {
    pub session_bytes: u64,
    pub total_bytes: u64,
    pub minimum_free_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            session_bytes: 2 * 1024 * 1024 * 1024,
            total_bytes: 10 * 1024 * 1024 * 1024,
            minimum_free_bytes: 1024 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    message_seq: u64,
    operation: String,
    checksum: String,
    payload_json: String,
    image_hash: Option<String>,
    image_bytes: u64,
}
struct Session {
    _lock: File,
    directory: PathBuf,
    manifest: Value,
    records: Vec<(String, String)>,
    saved_samples: u64,
    saved_bytes: u64,
    finalized: bool,
    last_time: HashMap<u64, f64>,
    source_key: Option<String>,
    smart_images: Vec<Value>,
    observation_count: u64,
    fallback_count: u64,
    last_observation: Option<Value>,
}
pub struct Host {
    root: PathBuf,
    limits: Limits,
    sessions: HashMap<String, Session>,
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub(crate) fn no_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() => Err("Tu choi duong dan symlink.".into()),
        Ok(_) => Ok(()),
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(failure) => Err(error(failure)),
    }
}
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(error)
}
pub(crate) fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    no_symlink(path)?;
    let parent = path.parent().ok_or("Thieu parent directory.")?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(error)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(error)?;
    fs::rename(&temporary, path).map_err(error)?;
    sync_directory(parent)
}
fn global_lock(root: &Path) -> Result<File, String> {
    let path = root.join(".quota.lock");
    no_symlink(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(error)?;
    FileExt::lock_exclusive(&file).map_err(error)?;
    Ok(file)
}
fn actual_usage(root: &Path) -> Result<u64, String> {
    let mut bytes = 0u64;
    for entry in fs::read_dir(root).map_err(error)? {
        let entry = entry.map_err(error)?;
        if !entry.file_type().map_err(error)?.is_dir() {
            continue;
        }
        let frames = entry.path().join("frames");
        no_symlink(&frames)?;
        if !frames.exists() {
            continue;
        }
        for frame in fs::read_dir(frames).map_err(error)? {
            let frame = frame.map_err(error)?;
            if frame.file_type().map_err(error)?.is_file() {
                bytes = bytes
                    .checked_add(frame.metadata().map_err(error)?.len())
                    .ok_or("Quota overflow.")?;
            }
        }
    }
    Ok(bytes)
}
fn write_usage(root: &Path, bytes: u64) -> Result<(), String> {
    atomic(
        &root.join(".usage.json"),
        &serde_json::to_vec(&json!({ "imageBytes": bytes })).map_err(error)?,
    )
}

impl Session {
    fn load(root: &Path, id: &str) -> Result<Self, String> {
        let directory = root.join(id);
        no_symlink(&directory)?;
        fs::create_dir_all(&directory).map_err(error)?;
        let frames = directory.join("frames");
        no_symlink(&frames)?;
        fs::create_dir_all(&frames).map_err(error)?;
        let lock_path = directory.join(".writer.lock");
        no_symlink(&lock_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(error)?;
        FileExt::try_lock_exclusive(&lock)
            .map_err(|_| "Phien dang duoc helper khac ghi.".to_string())?;
        let mut session = Self {
            _lock: lock,
            directory,
            manifest: Value::Null,
            records: vec![],
            saved_samples: 0,
            saved_bytes: 0,
            finalized: false,
            last_time: HashMap::new(),
            source_key: None,
            smart_images: vec![],
            observation_count: 0,
            fallback_count: 0,
            last_observation: None,
        };
        let journal_path = session.directory.join("journal.ndjson");
        no_symlink(&journal_path)?;
        if journal_path.exists() {
            let journal = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&journal_path)
                .map_err(error)?;
            let mut reader = BufReader::new(&journal);
            let mut committed_length = 0u64;
            let mut line = Vec::new();
            loop {
                line.clear();
                if reader.read_until(b'\n', &mut line).map_err(error)? == 0 {
                    break;
                }
                if line.last() != Some(&b'\n') {
                    // An unterminated tail was never ACKed. Do not discard complete corrupt records.
                    journal
                        .set_len(committed_length)
                        .and_then(|_| journal.sync_all())
                        .map_err(error)?;
                    break;
                }
                let record: Record = serde_json::from_slice(&line)
                    .map_err(|_| "Journal hong; can chan doan, khong tu xoa.".to_string())?;
                if record.message_seq != session.records.len() as u64 + 1 {
                    return Err("Journal khong lien tuc.".into());
                }
                let payload: Value = serde_json::from_str(&record.payload_json).map_err(error)?;
                let image = if record.operation == "appendSample" {
                    let sequence = validation::integer(&payload, "sequence")?;
                    let path = session
                        .directory
                        .join("frames")
                        .join(format!("{sequence:08}.png"));
                    no_symlink(&path)?;
                    let image = fs::read(path).map_err(|_| "Anh da commit bi mat.".to_string())?;
                    if image.len() as u64 != record.image_bytes
                        || record.image_hash.as_deref()
                            != Some(&hex::encode(Sha256::digest(&image)))
                    {
                        return Err("Anh da commit bi hong.".into());
                    }
                    image
                } else {
                    vec![]
                };
                if protocol::checksum(
                    &record.operation,
                    id,
                    record.message_seq,
                    &record.payload_json,
                    &image,
                ) != record.checksum
                {
                    return Err("Journal checksum sai.".into());
                }
                session.apply(&record.operation, payload, image.len() as u64)?;
                session.records.push((record.operation, record.checksum));
                committed_length += line.len() as u64;
            }
            if !session.manifest.is_null() {
                session.checkpoint(true)?;
            }
        }
        sync_directory(&session.directory)?;
        sync_directory(root)?;
        Ok(session)
    }
    fn apply(&mut self, operation: &str, payload: Value, image_bytes: u64) -> Result<(), String> {
        match operation {
            "begin" | "appendEvent" | "finalize" => {
                if !self.manifest.is_null()
                    && (self.manifest["schemaVersion"] != payload["schemaVersion"]
                        || self.manifest["captureMode"] != payload["captureMode"])
                {
                    return Err("Capture schema/profile da doi.".into());
                }
                if !self.manifest.is_null() && self.manifest["source"] != payload["source"] {
                    return Err("Source identity da doi.".into());
                }
                if validation::integer(&payload, "sampleCount")? != self.saved_samples
                    || validation::integer(&payload, "imageBytes")? != self.saved_bytes
                {
                    return Err("Checkpoint count khong khop.".into());
                }
                if payload["schemaVersion"] == 2
                    && (validation::integer(&payload, "observationCount")?
                        != self.observation_count
                        || validation::integer(&payload, "fallbackCount")? != self.fallback_count)
                {
                    return Err("Checkpoint observation count sai.".into());
                }
                self.manifest = payload;
                if operation == "finalize" {
                    if self.manifest["status"] != "stopped"
                        && self.manifest["status"] != "interrupted"
                    {
                        return Err("Finalize khi capture chua dung.".into());
                    }
                    self.finalized = true;
                }
            }
            "appendSample" => {
                if self.manifest.is_null() {
                    return Err("Chua begin session.".into());
                }
                if validation::integer(&payload, "sequence")? != self.saved_samples + 1 {
                    return Err("Sample sequence khong lien tuc.".into());
                }
                if self.manifest["schemaVersion"] == 2 {
                    if payload["manual"] != true {
                        crate::smart::metadata(&payload)?;
                    }
                    self.smart_images.push(payload.clone());
                }
                let segment = validation::integer(&payload, "segment")?;
                let time = validation::number(&payload, "estimatedVideoTimeMs")?;
                let manual = payload["manual"] == true;
                if !manual
                    && self
                        .last_time
                        .get(&segment)
                        .is_some_and(|previous| time < previous - 50.)
                {
                    return Err("Clock lui trong cung segment.".into());
                }
                let key = payload["clock"]["sourceKey"]
                    .as_str()
                    .ok_or("Thieu sourceKey.")?;
                if self
                    .source_key
                    .as_ref()
                    .is_some_and(|previous| previous != key)
                {
                    return Err("Player doi video.".into());
                }
                self.source_key = Some(key.to_string());
                // A paused manual snapshot is evidence, not a continuous-playback clock anchor.
                if !manual {
                    self.last_time.insert(segment, time);
                }
                self.saved_samples += 1;
                self.saved_bytes += image_bytes;
                self.manifest["sampleCount"] = self.saved_samples.into();
                self.manifest["imageBytes"] = self.saved_bytes.into();
            }
            "appendObservations" => {
                let values = crate::smart::validate_batch(
                    &payload,
                    &self.manifest,
                    &self.smart_images,
                    self.observation_count,
                    self.last_observation.as_ref(),
                )?;
                self.observation_count += values.len() as u64;
                self.fallback_count += values
                    .iter()
                    .filter(|v| v["smart"]["role"] == "fallback")
                    .count() as u64;
                self.last_observation = values.last().cloned();
                self.manifest["observationCount"] = self.observation_count.into();
                self.manifest["fallbackCount"] = self.fallback_count.into();
            }
            _ => return Err("Operation khong hop le.".into()),
        }
        Ok(())
    }
    fn checkpoint(&self, recovering: bool) -> Result<(), String> {
        let mut manifest = self.manifest.clone();
        manifest["nativeStore"] = json!({ "protocolVersion": protocol::VERSION, "ackSequence": self.records.len(), "capturedOnly": true, "status": if self.finalized { "captured" } else if recovering { "interrupted" } else { "collecting" } });
        // A non-finalized checkpoint is deliberately not advertised as a completed capture.
        atomic(
            &self.directory.join("session.json"),
            &serde_json::to_vec_pretty(&manifest).map_err(error)?,
        )
    }
    fn status(&self) -> Value {
        json!({ "ackSequence": self.records.len(), "savedSamples": self.saved_samples, "savedBytes": self.saved_bytes, "finalized": self.finalized, "sessionPath": self.directory })
    }
}

impl Host {
    pub fn new(root: PathBuf, limits: Limits) -> Result<Self, String> {
        no_symlink(&root)?;
        fs::create_dir_all(&root).map_err(error)?;
        let _guard = global_lock(&root)?;
        write_usage(&root, actual_usage(&root)?)?;
        Ok(Self {
            root,
            limits,
            sessions: HashMap::new(),
        })
    }
    pub fn handle(&mut self, request: &Request) -> Result<Value, String> {
        if request.version != protocol::VERSION {
            return Err("Protocol version khong tuong thich.".into());
        }
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err("Request ID khong hop le.".into());
        }
        if request.operation == "hello" {
            return Ok(
                json!({ "protocolVersion": protocol::VERSION, "hostVersion": env!("CARGO_PKG_VERSION"), "capabilities": [crate::smart::CAPABILITY], "appData": self.root.parent(), "limits": { "sessionBytes": self.limits.session_bytes, "totalBytes": self.limits.total_bytes, "minimumFreeBytes": self.limits.minimum_free_bytes } }),
            );
        }
        let id = request.session_id.as_deref().ok_or("Thieu session UUID.")?;
        validation::session_id(id)?;
        if !matches!(
            request.operation.as_str(),
            "status"
                | "begin"
                | "appendSample"
                | "appendObservations"
                | "appendEvent"
                | "finalize"
                | "openFolder"
        ) {
            return Err("Operation khong ho tro.".into());
        }
        if request.operation != "begin" && !self.root.join(id).join("journal.ndjson").exists() {
            return Err("Khong tim thay session native.".into());
        }
        if !self.sessions.contains_key(id) {
            let _guard = global_lock(&self.root)?;
            let session = Session::load(&self.root, id)?;
            write_usage(&self.root, actual_usage(&self.root)?)?;
            self.sessions.insert(id.into(), session);
        }
        if request.operation == "status" || request.operation == "openFolder" {
            let result = self.sessions[id].status();
            if self.sessions[id].finalized {
                self.sessions.remove(id);
            }
            return Ok(result);
        }
        let result = self.commit(id, request);
        if result.is_err()
            || self
                .sessions
                .get(id)
                .is_some_and(|session| session.finalized)
        {
            self.sessions.remove(id);
        }
        result
    }
    fn commit(&mut self, id: &str, request: &Request) -> Result<Value, String> {
        let sequence = request.message_seq.ok_or("Thieu messageSeq.")?;
        let payload_text = request
            .payload_json
            .as_deref()
            .ok_or("Thieu payloadJson.")?;
        if payload_text.len() > protocol::MAX_MESSAGE_BYTES
            || request
                .image_base64
                .as_ref()
                .is_some_and(|image| image.len() > protocol::MAX_MESSAGE_BYTES)
        {
            return Err("Payload qua lon.".into());
        }
        let payload: Value = serde_json::from_str(payload_text).map_err(error)?;
        let image = if request.operation == "appendSample" {
            STANDARD
                .decode(request.image_base64.as_deref().ok_or("Thieu PNG.")?)
                .map_err(|_| "Base64 khong hop le.".to_string())?
        } else {
            if request.image_base64.is_some() {
                return Err("PNG khong duoc gui voi operation nay.".into());
            }
            vec![]
        };
        let checksum = protocol::checksum(&request.operation, id, sequence, payload_text, &image);
        if request.checksum.as_deref() != Some(&checksum) {
            return Err("Checksum request sai.".into());
        }
        let session = self.sessions.get_mut(id).unwrap();
        if sequence > 0 && sequence <= session.records.len() as u64 {
            if session.records[sequence as usize - 1] != (request.operation.clone(), checksum) {
                return Err("Sequence da ton tai voi noi dung khac.".into());
            }
            return Ok(session.status());
        }
        if sequence != session.records.len() as u64 + 1 || session.finalized {
            return Err("Message sequence sai hoac session da finalize.".into());
        }
        if request.operation == "begin" {
            if sequence != 1 {
                return Err("Begin chi duoc o sequence 1.".into());
            }
            validation::session(&payload, id)?;
        } else if request.operation == "appendSample" {
            validation::sample(&payload, id, &session.manifest["source"], &image)?;
        } else if request.operation != "appendObservations" {
            validation::session(&payload, id)?;
        }
        if request.operation == "appendObservations"
            && fs2::available_space(&self.root).map_err(error)? < self.limits.minimum_free_bytes
        {
            return Err("Observation dung: o dia con duoi nguong an toan.".into());
        }
        let _guard = global_lock(&self.root)?;
        if !image.is_empty() {
            let usage: Value =
                serde_json::from_slice(&fs::read(self.root.join(".usage.json")).map_err(error)?)
                    .map_err(error)?;
            let total = validation::integer(&usage, "imageBytes")?;
            let free = fs2::available_space(&self.root).map_err(error)?;
            let path = session
                .directory
                .join("frames")
                .join(format!("{:08}.png", session.saved_samples + 1));
            no_symlink(&path)?;
            let orphan_bytes = fs::metadata(path).map(|info| info.len()).unwrap_or(0);
            let reserved_total = total.saturating_sub(orphan_bytes) + image.len() as u64;
            if session.saved_bytes + image.len() as u64 > self.limits.session_bytes
                || reserved_total > self.limits.total_bytes
                || free < self.limits.minimum_free_bytes + image.len() as u64
            {
                return Err("Khong du dung luong hoac da dat quota native. Tam dung thu.".into());
            }
            // Reserve before writing. A crash can overcount, never undercount; recovery reconciles it.
            write_usage(&self.root, reserved_total)?;
        }
        session.apply(&request.operation, payload.clone(), image.len() as u64)?;
        if !image.is_empty() {
            let path = session
                .directory
                .join("frames")
                .join(format!("{:08}.png", session.saved_samples));
            atomic(&path, &image)?;
        }
        let record = Record {
            message_seq: sequence,
            operation: request.operation.clone(),
            checksum: checksum.clone(),
            payload_json: payload_text.into(),
            image_hash: if image.is_empty() {
                None
            } else {
                Some(hex::encode(Sha256::digest(&image)))
            },
            image_bytes: image.len() as u64,
        };
        let path = session.directory.join("journal.ndjson");
        no_symlink(&path)?;
        let mut journal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(error)?;
        let mut bytes = serde_json::to_vec(&record).map_err(error)?;
        bytes.push(b'\n');
        journal
            .write_all(&bytes)
            .and_then(|_| journal.sync_all())
            .map_err(error)?;
        sync_directory(&session.directory)?;
        session.records.push((request.operation.clone(), checksum));
        session.checkpoint(false)?;
        Ok(session.status())
    }
}

pub(crate) fn finalized_capture(
    root: &Path,
    id: &str,
) -> Result<(Value, Vec<Value>, String), String> {
    validation::session_id(id)?;
    no_symlink(root)?;
    if !root.join(id).join("journal.ndjson").exists() {
        return Err("Khong tim thay capture native.".into());
    }
    let session = Session::load(root, id)?;
    if !session.finalized {
        return Err("Capture chua finalize; khong OCR khi dang ghi anh.".into());
    }
    let bytes = fs::read(session.directory.join("journal.ndjson")).map_err(error)?;
    let samples = BufReader::new(bytes.as_slice())
        .lines()
        .map(|line| {
            let record: Record = serde_json::from_str(&line.map_err(error)?).map_err(error)?;
            if record.operation == "appendSample" {
                Ok(Some(
                    serde_json::from_str(&record.payload_json).map_err(error)?,
                ))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<Option<Value>>, String>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok((
        session.manifest.clone(),
        samples,
        hex::encode(Sha256::digest(bytes)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "9c4212da-a38b-4452-bb9c-ebfb5d55c66c";
    fn limits() -> Limits {
        Limits {
            minimum_free_bytes: 0,
            ..Limits::default()
        }
    }
    fn metadata() -> Value {
        json!({ "schemaVersion": 1, "id": ID, "title": "fixture", "url": "http://localhost/movie", "createdAt": 1, "endedAt": null, "status": "recording", "source": { "id": "doc:video", "documentId": "doc", "frameId": 1, "origin": "http://localhost" }, "region": { "x": 0., "y": 0., "width": 1., "height": 1. }, "sampleCount": 0, "imageBytes": 0, "events": [] })
    }
    fn image() -> Vec<u8> {
        let mut bytes = vec![];
        {
            let encoder = png::Encoder::new(&mut bytes, 2, 2);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[1, 2, 3, 4]).unwrap();
        }
        bytes
    }
    fn sample() -> Value {
        json!({ "sessionId": ID, "sequence": 1, "segment": 0, "capturedEpochMs": 1, "estimatedVideoTimeMs": 1, "uncertaintyMs": 100, "roundTripMs": 1, "captureSize": { "width": 2, "height": 2 }, "region": { "x": 0., "y": 0., "width": 1., "height": 1. }, "manual": false, "clock": { "videoId": "video", "sourceKey": "movie", "mediaTime": 0.001, "duration": 20., "rate": 1., "paused": false, "seeking": false, "waiting": false, "ended": false, "readyState": 4, "epochMs": 1., "frameMediaTime": null, "frameEpochMs": null, "layout": "same", "width": 2, "height": 2 } })
    }
    fn request(operation: &str, sequence: u64, payload: Value, bytes: &[u8]) -> Request {
        let text = serde_json::to_string(&payload).unwrap();
        Request {
            version: 1,
            request_id: "test".into(),
            operation: operation.into(),
            session_id: Some(ID.into()),
            message_seq: Some(sequence),
            checksum: Some(protocol::checksum(operation, ID, sequence, &text, bytes)),
            payload_json: Some(text),
            image_base64: if bytes.is_empty() {
                None
            } else {
                Some(STANDARD.encode(bytes))
            },
        }
    }
    #[test]
    fn durable_replay_conflict_and_finalize() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sessions");
        let mut host = Host::new(root.clone(), limits()).unwrap();
        let begin = request("begin", 1, metadata(), &[]);
        host.handle(&begin).unwrap();
        let bytes = image();
        let frame = request("appendSample", 2, sample(), &bytes);
        assert_eq!(host.handle(&frame).unwrap()["savedSamples"], 1);
        drop(host);
        let mut host = Host::new(root.clone(), limits()).unwrap();
        assert_eq!(host.handle(&frame).unwrap()["ackSequence"], 2);
        let mut changed = sample();
        changed["estimatedVideoTimeMs"] = 99.into();
        assert!(host
            .handle(&request("appendSample", 2, changed, &bytes))
            .unwrap_err()
            .contains("noi dung khac"));
        let mut end = metadata();
        end["status"] = "stopped".into();
        end["endedAt"] = 5.into();
        end["sampleCount"] = 1.into();
        end["imageBytes"] = (bytes.len() as u64).into();
        let finish = request("finalize", 3, end, &[]);
        assert_eq!(host.handle(&finish).unwrap()["finalized"], true);
        assert_eq!(host.handle(&finish).unwrap()["ackSequence"], 3);
        assert!(host
            .handle(&request("appendSample", 4, sample(), &bytes))
            .is_err());
        assert_eq!(
            fs::read_dir(root.join(ID).join("frames")).unwrap().count(),
            1
        );
    }
    #[test]
    fn recovers_unacked_tail_and_orphan_png_but_preserves_complete_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sessions");
        let mut host = Host::new(root.clone(), limits()).unwrap();
        host.handle(&request("begin", 1, metadata(), &[])).unwrap();
        drop(host);
        let directory = root.join(ID);
        OpenOptions::new()
            .append(true)
            .open(directory.join("journal.ndjson"))
            .unwrap()
            .write_all(b"{\"incomplete")
            .unwrap();
        fs::write(directory.join("frames/00000001.png"), image()).unwrap();
        fs::write(directory.join("session.json"), b"broken checkpoint").unwrap();
        let mut host = Host::new(root.clone(), limits()).unwrap();
        assert_eq!(
            host.handle(&request("appendSample", 2, sample(), &image()))
                .unwrap()["savedSamples"],
            1
        );
        drop(host);
        OpenOptions::new()
            .append(true)
            .open(directory.join("journal.ndjson"))
            .unwrap()
            .write_all(b"bad complete record\n")
            .unwrap();
        let mut host = Host::new(root, limits()).unwrap();
        assert!(host.handle(&request("begin", 1, metadata(), &[])).is_err());
    }
    #[test]
    fn rejects_bad_clock_png_checksum_path_and_version() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = Host::new(temp.path().join("sessions"), limits()).unwrap();
        let mut begin = request("begin", 1, metadata(), &[]);
        begin.version = 2;
        assert!(host.handle(&begin).is_err());
        begin.version = 1;
        begin.session_id = Some("../escape".into());
        assert!(host.handle(&begin).is_err());
        begin.session_id = Some(ID.into());
        begin.checksum = Some("bad".into());
        assert!(host.handle(&begin).is_err());
        host.handle(&request("begin", 1, metadata(), &[])).unwrap();
        let mut bad = sample();
        bad["estimatedVideoTimeMs"] = (-1).into();
        assert!(host
            .handle(&request("appendSample", 2, bad, &image()))
            .is_err());
        assert!(host
            .handle(&request("appendSample", 2, sample(), b"not png"))
            .is_err());
        let mut bad = sample();
        bad["clock"]["videoId"] = "other".into();
        assert!(host
            .handle(&request("appendSample", 2, bad, &image()))
            .is_err());
    }
    #[test]
    fn locks_and_quota_do_not_ack_or_lose_committed_data() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sessions");
        let mut first = Host::new(
            root.clone(),
            Limits {
                session_bytes: 1,
                ..limits()
            },
        )
        .unwrap();
        let begin = request("begin", 1, metadata(), &[]);
        first.handle(&begin).unwrap();
        let mut second = Host::new(root.clone(), limits()).unwrap();
        assert!(second.handle(&begin).is_err());
        assert!(first
            .handle(&request("appendSample", 2, sample(), &image()))
            .is_err());
        assert_eq!(first.handle(&begin).unwrap()["ackSequence"], 1);
        drop(first);
        assert_eq!(
            second
                .handle(&request("appendSample", 2, sample(), &image()))
                .unwrap()["savedSamples"],
            1
        );
    }
    #[test]
    fn missing_or_modified_committed_png_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sessions");
        let mut host = Host::new(root.clone(), limits()).unwrap();
        host.handle(&request("begin", 1, metadata(), &[])).unwrap();
        host.handle(&request("appendSample", 2, sample(), &image()))
            .unwrap();
        drop(host);
        fs::write(root.join(ID).join("frames/00000001.png"), b"corrupt").unwrap();
        let mut host = Host::new(root, limits()).unwrap();
        assert!(host.handle(&request("begin", 1, metadata(), &[])).is_err());
    }
    #[test]
    fn commit_survives_checkpoint_failure_after_journal_fsync() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sessions");
        let mut host = Host::new(root.clone(), limits()).unwrap();
        host.handle(&request("begin", 1, metadata(), &[])).unwrap();
        let checkpoint = root.join(ID).join("session.json");
        fs::remove_file(&checkpoint).unwrap();
        fs::create_dir(&checkpoint).unwrap();
        let frame = request("appendSample", 2, sample(), &image());
        assert!(host.handle(&frame).is_err());
        fs::remove_dir(checkpoint).unwrap();
        drop(host);
        let mut recovered = Host::new(root, limits()).unwrap();
        assert_eq!(recovered.handle(&frame).unwrap()["savedSamples"], 1);
        assert_eq!(recovered.handle(&frame).unwrap()["ackSequence"], 2);
    }
    #[test]
    fn manual_snapshot_does_not_reject_first_playback_estimate() {
        let temp = tempfile::tempdir().unwrap();
        let mut host = Host::new(temp.path().join("sessions"), limits()).unwrap();
        host.handle(&request("begin", 1, metadata(), &[])).unwrap();
        let mut snapshot = sample();
        snapshot["manual"] = true.into();
        snapshot["estimatedVideoTimeMs"] = 525333.into();
        host.handle(&request("appendSample", 2, snapshot, &image()))
            .unwrap();
        let mut first = sample();
        first["sequence"] = 2.into();
        first["estimatedVideoTimeMs"] = 525279.into();
        host.handle(&request("appendSample", 3, first, &image()))
            .unwrap();
        let mut bad = sample();
        bad["sequence"] = 3.into();
        bad["estimatedVideoTimeMs"] = 525100.into();
        assert!(host
            .handle(&request("appendSample", 4, bad, &image()))
            .is_err());
    }
    #[test]
    fn total_quota_and_free_space_fail_before_ack() {
        for limits in [
            Limits {
                total_bytes: 1,
                ..limits()
            },
            Limits {
                minimum_free_bytes: u64::MAX / 2,
                ..limits()
            },
        ] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("sessions");
            let mut host = Host::new(root.clone(), limits).unwrap();
            host.handle(&request("begin", 1, metadata(), &[])).unwrap();
            assert!(host
                .handle(&request("appendSample", 2, sample(), &image()))
                .is_err());
            assert_eq!(
                fs::read_dir(root.join(ID).join("frames")).unwrap().count(),
                0
            );
            assert_eq!(
                host.handle(&request("begin", 1, metadata(), &[])).unwrap()["ackSequence"],
                1
            );
        }
    }
    #[test]
    fn smart_observations_durable_replay_validation_and_finalize() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sessions");
        let mut host = Host::new(root.clone(), limits()).unwrap();
        let mut begin = metadata();
        begin["schemaVersion"] = 2.into();
        begin["captureMode"] = "smart".into();
        begin["observationCount"] = 0.into();
        begin["fallbackCount"] = 0.into();
        host.handle(&request("begin", 1, begin.clone(), &[]))
            .unwrap();
        let mut frame = sample();
        frame["smart"] = json!({"group":1,"stable":true,"save":true,"role":"stable"});
        let bytes = image();
        host.handle(&request("appendSample", 2, frame.clone(), &bytes))
            .unwrap();
        let mut observation = frame.clone();
        observation.as_object_mut().unwrap().remove("sequence");
        observation["observationSeq"] = 1.into();
        observation["imageSequence"] = 1.into();
        let payload = json!({"sessionId":ID,"observations":[observation]});
        for (field, bad) in [
            ("imageSequence", json!(2)),
            ("observationSeq", json!(2)),
            ("segment", json!(1)),
            ("estimatedVideoTimeMs", json!(-1)),
            ("region", json!({"x":0.,"y":0.,"width":0.5,"height":1.})),
        ] {
            let mut invalid = payload.clone();
            invalid["observations"][0][field] = bad;
            assert!(host
                .handle(&request("appendObservations", 3, invalid, &[]))
                .is_err());
        }
        let committed = request("appendObservations", 3, payload.clone(), &[]);
        // Journal commit survives a failed manifest checkpoint and a lost response.
        let checkpoint = root.join(ID).join("session.json");
        fs::remove_file(&checkpoint).unwrap();
        fs::create_dir(&checkpoint).unwrap();
        assert!(host.handle(&committed).is_err());
        fs::remove_dir(&checkpoint).unwrap();
        drop(host);
        let journal = root.join(ID).join("journal.ndjson");
        OpenOptions::new()
            .append(true)
            .open(&journal)
            .unwrap()
            .write_all(b"{partial")
            .unwrap();
        let mut host = Host::new(root.clone(), limits()).unwrap();
        assert_eq!(host.handle(&committed).unwrap()["ackSequence"], 3);
        let mut conflict = payload;
        conflict["observations"][0]["estimatedVideoTimeMs"] = 2.into();
        assert!(host
            .handle(&request("appendObservations", 3, conflict, &[]))
            .is_err());
        begin["status"] = "stopped".into();
        begin["endedAt"] = 3.into();
        begin["sampleCount"] = 1.into();
        begin["imageBytes"] = (bytes.len() as u64).into();
        begin["observationCount"] = 1.into();
        assert!(
            host.handle(&request("finalize", 4, begin, &[])).unwrap()["finalized"]
                .as_bool()
                .unwrap()
        );
        drop(host);
        let (manifest, images, _) = finalized_capture(&root, ID).unwrap();
        assert_eq!(manifest["observationCount"], 1);
        assert_eq!(images.len(), 1);
        assert_eq!(crate::smart::observations(&root.join(ID)).unwrap().len(), 1);
    }
}
