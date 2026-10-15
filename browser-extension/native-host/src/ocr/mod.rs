pub mod cues;
pub mod engine;
pub mod smart;

use crate::{
    storage::{atomic, finalized_capture, no_symlink},
    validation,
};
use cues::{assemble, Observation};
use engine::{Engine, MODEL_HASH, MODEL_ID};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[path = "../../../../src-tauri/src/srt.rs"]
#[allow(dead_code)]
pub(crate) mod srt;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn directory(data: &Path, id: &str) -> Result<PathBuf, String> {
    validation::session_id(id)?;
    let root = data.join("ocr-sessions");
    let dir = root.join(id);
    for p in [
        &root,
        &dir,
        &dir.join("journal.ndjson"),
        &dir.join("ocr.json"),
    ] {
        no_symlink(p)?;
    }
    if !dir.join("journal.ndjson").is_file() {
        return Err("Khong tim thay phien native.".into());
    }
    Ok(dir)
}
fn lock(dir: &Path, wait: bool) -> Result<File, String> {
    let p = dir.join(".ocr.lock");
    no_symlink(&p)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(p)
        .map_err(|e| e.to_string())?;
    for _ in 0..if wait { 100 } else { 1 } {
        if FileExt::try_lock_exclusive(&file).is_ok() {
            return Ok(file);
        }
        if wait {
            thread::sleep(Duration::from_millis(20));
        }
    }
    Err("OCR phien nay dang chay.".into())
}
fn read_json(p: &Path) -> Result<Value, String> {
    no_symlink(p)?;
    if fs::metadata(p).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
        return Err("OCR manifest qua lon.".into());
    }
    serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn save(dir: &Path, value: &Value) -> Result<(), String> {
    atomic(
        &dir.join("ocr.json"),
        &serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
}
pub fn status(data: &Path, id: &str) -> Result<Value, String> {
    let dir = directory(data, id)?;
    if !dir.join("ocr.json").exists() {
        return Ok(json!({"state":"not-started","sessionId":id,"committed":false}));
    }
    let mut value = read_json(&dir.join("ocr.json"))?;
    if value["sessionId"] != id {
        return Err("OCR manifest sai UUID.".into());
    }
    let starting = value["state"] == "queued"
        && now().saturating_sub(value["updatedAt"].as_u64().unwrap_or(0)) < 5000;
    if !starting
        && matches!(value["state"].as_str(), Some("processing" | "queued"))
        && lock(&dir, false).is_ok()
    {
        value["state"] = "interrupted".into();
        value["error"] = "Worker da dung; bam OCR de tiep tuc tu checkpoint.".into();
    }
    if value["committed"] == true {
        verify_commit(&dir, &value)?;
        let revision = value["revision"].as_str().ok_or("Thieu revision")?;
        let text = fs::read_to_string(
            dir.join("ocr-revisions")
                .join(revision)
                .join("subtitles.vi.srt"),
        )
        .map_err(|e| e.to_string())?;
        value["preview"] = text.chars().take(4000).collect::<String>().into();
    }
    Ok(value)
}
pub fn cancel(data: &Path, id: &str) -> Result<Value, String> {
    let dir = directory(data, id)?;
    let value = read_json(&dir.join("ocr.json"))?;
    let job = value["jobId"].as_str().ok_or("Thieu OCR job ID.")?;
    validation::session_id(job)?;
    atomic(
        &dir.join("ocr-cancel.json"),
        &serde_json::to_vec(&json!({"jobId":job})).map_err(|e| e.to_string())?,
    )?;
    let mut value = status(data, id)?;
    value["cancelRequested"] = true.into();
    Ok(value)
}
pub fn start(data: &Path, id: &str) -> Result<Value, String> {
    let dir = directory(data, id)?;
    let guard = match lock(&dir, false) {
        Ok(v) => v,
        Err(_) => return status(data, id),
    };
    let capture = read_json(&dir.join("session.json"))?;
    if capture["nativeStore"]["status"] != "captured" {
        return Err("Can finalize capture truoc OCR.".into());
    }
    if dir.join("ocr.json").exists() {
        let current = read_json(&dir.join("ocr.json"))?;
        if current["state"] == "queued"
            && now().saturating_sub(current["updatedAt"].as_u64().unwrap_or(0)) < 5000
        {
            return Ok(current);
        }
        if current["committed"] == true {
            verify_commit(&dir, &current)?;
            return Ok(current);
        }
    }
    let job = uuid::Uuid::new_v4().to_string();
    let initial = json!({"schemaVersion":1,"sessionId":id,"jobId":job,"state":"queued","committed":false,"processedSamples":0,"totalSamples":capture["sampleCount"],"updatedAt":now(),"error":null});
    save(&dir, &initial)?;
    let log_path = data.join("diagnostics/ocr-worker.log");
    no_symlink(&log_path)?;
    fs::create_dir_all(data.join("diagnostics")).map_err(|e| e.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .args(["--ocr-worker", id, &job])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .map_err(|e| e.to_string())?,
        );
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let spawned = command.spawn().map_err(|e| e.to_string());
    if let Err(error) = spawned {
        let mut failed = initial.clone();
        failed["state"] = "error".into();
        failed["error"] = error.clone().into();
        save(&dir, &failed)?;
        return Err(error);
    }
    drop(guard);
    Ok(initial)
}
pub fn process(data: &Path, id: &str, reprocess: bool) -> Result<Value, String> {
    let dir = directory(data, id)?;
    let guard = lock(&dir, false)?;
    let mut previous = Value::Null;
    if dir.join("ocr.json").exists() {
        let mut current = read_json(&dir.join("ocr.json"))?;
        if current["committed"] == true {
            verify_commit(&dir, &current)?;
            if !reprocess {
                return Ok(current);
            }
            for (file, field) in [
                ("subtitles.vi.srt", "srtHash"),
                ("subtitles.vi.txt", "txtHash"),
                ("cues.json", "cuesHash"),
            ] {
                if sha(&fs::read(dir.join(file)).map_err(|e| e.to_string())?)
                    != current[field].as_str().unwrap_or("")
                {
                    return Err("Output da sua ngoai OCR; khong ghi de edit.".into());
                }
            }
            if let Some(object) = current.as_object_mut() {
                object.remove("previousResult");
            }
            previous = current;
        }
    }
    let job = uuid::Uuid::new_v4().to_string();
    save(
        &dir,
        &json!({"schemaVersion":1,"sessionId":id,"jobId":job,"state":"queued","committed":false,"updatedAt":now(),"previousResult":previous}),
    )?;
    let result = run_locked(data, id, &job, &dir);
    drop(guard);
    result
}
fn cancelled(dir: &Path, job: &str) -> bool {
    read_json(&dir.join("ocr-cancel.json")).is_ok_and(|v| v["jobId"] == job)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservationRecord {
    observation: Observation,
    checksum: String,
}
#[derive(Deserialize)]
struct RawObservationRecord<'a> {
    #[serde(borrow)]
    observation: &'a serde_json::value::RawValue,
    checksum: String,
}
fn recover_observations(path: &Path) -> Result<Vec<Observation>, String> {
    no_symlink(path)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let journal = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(&journal);
    let mut records = vec![];
    let mut length = 0u64;
    loop {
        let mut row = vec![];
        if reader
            .read_until(b'\n', &mut row)
            .map_err(|e| e.to_string())?
            == 0
        {
            break;
        }
        if row.last() != Some(&b'\n') {
            journal
                .set_len(length)
                .and_then(|_| journal.sync_all())
                .map_err(|e| e.to_string())?;
            break;
        }
        if row.len() > 128 * 1024 {
            return Err("OCR journal record qua lon.".into());
        }
        let raw: RawObservationRecord<'_> = serde_json::from_slice(&row)
            .map_err(|_| "OCR journal hong; khong tu xoa.".to_string())?;
        if sha(raw.observation.get().as_bytes()) != raw.checksum {
            return Err("OCR journal checksum sai.".into());
        }
        let observation: Observation =
            serde_json::from_str(raw.observation.get()).map_err(|e| e.to_string())?;
        if observation.sequence != records.len() as u64 + 1 {
            return Err("OCR journal checksum/sequence sai.".into());
        }
        length += row.len() as u64;
        records.push(observation);
    }
    Ok(records)
}
fn append(journal: &mut File, observation: &Observation) -> Result<(), String> {
    let record = ObservationRecord {
        checksum: sha(&serde_json::to_vec(observation).map_err(|e| e.to_string())?),
        observation: observation.clone(),
    };
    let mut bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    journal
        .write_all(&bytes)
        .and_then(|_| journal.sync_all())
        .map_err(|e| e.to_string())
}
fn coverage(samples: &[Value]) -> Value {
    let mut ranges = vec![];
    let mut gaps = vec![];
    let mut active: Option<(u64, u64, u64)> = None;
    let mut duration = 0u64;
    for sample in samples.iter().filter(|s| s["manual"] != true) {
        let time = sample["estimatedVideoTimeMs"]
            .as_f64()
            .unwrap_or(0.)
            .round() as u64;
        let segment = sample["segment"].as_u64().unwrap_or(0);
        duration = duration
            .max((sample["clock"]["duration"].as_f64().unwrap_or(0.) * 1000.).round() as u64);
        if let Some((start, end, old_segment)) = active {
            if old_segment != segment || time.saturating_sub(end) > 750 || time + 50 < end {
                ranges.push(json!({"startMs":start,"endMs":end,"segment":old_segment}));
                gaps.push(json!({"afterMs":end,"beforeMs":time,"reason":if old_segment!=segment{"segment-boundary"}else{"sample-gap"}}));
                active = Some((time, time, segment));
            } else {
                active = Some((start, time.max(end), segment));
            }
        } else {
            active = Some((time, time, segment));
        }
    }
    if let Some((start, end, segment)) = active {
        ranges.push(json!({"startMs":start,"endMs":end,"segment":segment}));
    }
    json!({"ranges":ranges,"gaps":gaps,"durationMs":duration,"wholeEpisodeVerified":false,"note":"Ranges describe sampled images, not proof of subtitle recall or reviewed coverage."})
}
pub(crate) fn verify_commit(dir: &Path, manifest: &Value) -> Result<(), String> {
    let revision = manifest["revision"].as_str().ok_or("Thieu OCR revision.")?;
    if revision.len() != 64 || !revision.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("OCR revision sai.".into());
    }
    let root = dir.join("ocr-revisions");
    let output = root.join(revision);
    no_symlink(&root)?;
    no_symlink(&output)?;
    for (file, field) in [
        ("subtitles.vi.srt", "srtHash"),
        ("subtitles.vi.txt", "txtHash"),
        ("cues.json", "cuesHash"),
    ] {
        let path = output.join(file);
        no_symlink(&path)?;
        if sha(&fs::read(path).map_err(|e| e.to_string())?)
            != manifest[field].as_str().unwrap_or("")
        {
            return Err("OCR output da commit bi mat/hong; khong ghi de am tham.".into());
        }
    }
    Ok(())
}
fn write_revision_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    no_symlink(path)?;
    if path.exists() {
        if fs::read(path).map_err(|e| e.to_string())? != bytes {
            return Err("OCR revision immutable conflict; khong ghi de bang chung cu.".into());
        }
        Ok(())
    } else {
        atomic(path, bytes)
    }
}
pub fn run(data: &Path, id: &str, job: &str) -> Result<Value, String> {
    validation::session_id(job)?;
    let dir = directory(data, id)?;
    let _guard = lock(&dir, true)?;
    run_locked(data, id, job, &dir)
}
fn run_locked(data: &Path, id: &str, job: &str, dir: &Path) -> Result<Value, String> {
    let mut state = read_json(&dir.join("ocr.json"))?;
    if state["jobId"] != job {
        return Err("OCR job da duoc thay the.".into());
    }
    let result = run_inner(data, id, job, dir, &mut state);
    if let Err(error) = &result {
        state["state"] = if cancelled(dir, job) {
            "cancelled"
        } else {
            "error"
        }
        .into();
        state["error"] = error.clone().into();
        state["updatedAt"] = now().into();
        let _ = save(dir, &state);
    }
    result
}
fn run_inner(
    data: &Path,
    id: &str,
    job: &str,
    dir: &Path,
    state: &mut Value,
) -> Result<Value, String> {
    let (capture, samples, capture_hash) = finalized_capture(&data.join("ocr-sessions"), id)?;
    if cancelled(dir, job) {
        return Err("OCR da huy.".into());
    }
    let work_root = dir.join("ocr-work");
    no_symlink(&work_root)?;
    fs::create_dir_all(&work_root).map_err(|e| e.to_string())?;
    let scratch = work_root.join(format!("scratch-{job}"));
    no_symlink(&scratch)?;
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let engine = Engine::resolve(data, &scratch)?;
    let smart_capture = capture["schemaVersion"] == 2;
    let timeline = if smart_capture {
        crate::smart::observations(dir)?
    } else {
        vec![]
    };
    let observation_revision = sha(format!(
        "{}:{capture_hash}{}",
        engine.fingerprint,
        if smart_capture { ":smart-1" } else { "" }
    )
    .as_bytes());
    let assembly_version = if smart_capture { 3 } else { 2 };
    let revision = sha(format!("assembly-{assembly_version}:{observation_revision}").as_bytes());
    let work = work_root.join(&observation_revision);
    no_symlink(&work)?;
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let path = work.join("observations.ndjson");
    let mut observations = recover_observations(&path)?;
    if observations.len() > samples.len() {
        return Err("Checkpoint OCR vuot capture.".into());
    }
    for observation in &observations {
        let source = &samples[observation.sequence as usize - 1];
        if observation.segment != validation::integer(source, "segment")?
            || observation.time_ms
                != validation::number(source, "estimatedVideoTimeMs")?.round() as u64
        {
            return Err("Checkpoint OCR sai source timeline.".into());
        }
        let path = dir
            .join("frames")
            .join(format!("{:08}.png", observation.sequence));
        no_symlink(&path)?;
        if sha(&fs::read(path).map_err(|e| e.to_string())?) != observation.image_hash {
            return Err("Checkpoint OCR sai image hash.".into());
        }
    }
    let previous = state["previousResult"].clone();
    *state = json!({"schemaVersion":1,"sessionId":id,"jobId":job,"state":"processing","committed":false,"revision":revision,"observationRevision":observation_revision,"assemblyVersion":assembly_version,"previousResult":previous,"sourceJournalHash":capture_hash,"engine":engine.version,"modelId":MODEL_ID,"modelHash":MODEL_HASH,"processedSamples":observations.len(),"totalSamples":samples.len(),"startedAt":now(),"updatedAt":now(),"error":null});
    save(dir, state)?;
    state["visionSystem"] =
        serde_json::to_value(&engine.vision_system).map_err(|e| e.to_string())?;
    state["workerPid"] = std::process::id().into();
    save(dir, state)?;
    no_symlink(&path)?;
    let mut journal = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let resumed = observations.len();
    let mut smart_candidates = std::collections::HashMap::new();
    let mut recognition_calls = 0;
    let recognition_cache = work.join("recognition");
    if smart_capture {
        no_symlink(&recognition_cache)?;
        fs::create_dir_all(&recognition_cache).map_err(|e| e.to_string())?;
    }
    for sample in samples.iter().skip(resumed) {
        if cancelled(dir, job) {
            return Err("OCR da huy; checkpoint giu lai.".into());
        }
        if fs2::available_space(dir).map_err(|e| e.to_string())? < 1024 * 1024 * 1024 {
            return Err("OCR dung: o dia con duoi 1 GiB.".into());
        }
        let sequence = validation::integer(sample, "sequence")?;
        let time = validation::number(sample, "estimatedVideoTimeMs")?.round() as u64;
        if time > 3_600_000_000 {
            return Err("Timestamp OCR vuot gioi han 1000 gio.".into());
        }
        let image = dir.join("frames").join(format!("{sequence:08}.png"));
        no_symlink(&image)?;
        let candidates = if smart_capture {
            if !smart_candidates.contains_key(&sequence) {
                let mut new_calls = 0;
                let recognized =
                    smart::recognize_group(&samples, sample, &observations, |input| {
                        let input_sequence = validation::integer(input, "sequence")?;
                        let path = dir.join("frames").join(format!("{input_sequence:08}.png"));
                        no_symlink(&path)?;
                        if cancelled(dir, job) {
                            return Err("OCR da huy; checkpoint giu lai.".into());
                        }
                        let cache = recognition_cache.join(format!("{input_sequence:08}.json"));
                        no_symlink(&cache)?;
                        let image_hash = sha(&fs::read(&path).map_err(|e| e.to_string())?);
                        if cache.exists() {
                            if fs::metadata(&cache).map_err(|e| e.to_string())?.len() > 1024 * 1024
                            {
                                return Err("Recognition cache qua lon.".into());
                            }
                            let record: Value = serde_json::from_slice(
                                &fs::read(&cache).map_err(|e| e.to_string())?,
                            )
                            .map_err(|e| e.to_string())?;
                            let text = record["payloadJson"]
                                .as_str()
                                .ok_or("Recognition cache thieu payload.")?;
                            if record["checksum"] != sha(text.as_bytes()) {
                                return Err("Recognition cache checksum sai.".into());
                            }
                            let value: Value =
                                serde_json::from_str(text).map_err(|e| e.to_string())?;
                            if value["imageHash"] != image_hash {
                                return Err("Recognition cache sai anh.".into());
                            }
                            let candidates: Vec<engine::Candidate> =
                                serde_json::from_value(value["candidates"].clone())
                                    .map_err(|e| e.to_string())?;
                            if candidates.len() > 64
                                || candidates.iter().any(|c| {
                                    !c.confidence.is_finite()
                                        || !(0.0..=100.0).contains(&c.confidence)
                                        || c.text.len() > 8192
                                        || c.variant.len() > 256
                                })
                            {
                                return Err("Recognition cache candidates khong hop le.".into());
                            }
                            return Ok(candidates);
                        }
                        let candidates =
                            engine.recognize(&path, &scratch, || cancelled(dir, job))?;
                        new_calls += 1;
                        let text = serde_json::to_string(
                            &json!({"imageHash":image_hash,"candidates":candidates}),
                        )
                        .map_err(|e| e.to_string())?;
                        atomic(
                            &cache,
                            &serde_json::to_vec(
                                &json!({"payloadJson":text,"checksum":sha(text.as_bytes())}),
                            )
                            .map_err(|e| e.to_string())?,
                        )?;
                        Ok(candidates)
                    })?;
                recognition_calls += new_calls;
                smart_candidates.extend(recognized.candidates);
            }
            smart_candidates
                .remove(&sequence)
                .ok_or("Thieu smart recognition.")?
        } else {
            recognition_calls += 1;
            engine.recognize(&image, &scratch, || cancelled(dir, job))?
        };
        let primary = candidates.first().filter(|c| {
            c.variant == "vision-accurate-vi-candidate-1"
                && c.confidence >= 80.
                && c.text.chars().any(char::is_alphanumeric)
        });
        let best = primary.or_else(|| {
            candidates
                .iter()
                .filter(|c| c.text.chars().any(char::is_alphanumeric))
                .reduce(|best, next| {
                    if next.confidence > best.confidence {
                        next
                    } else {
                        best
                    }
                })
        });
        let observation = Observation {
            sequence,
            segment: validation::integer(sample, "segment")?,
            time_ms: time,
            captured_epoch_ms: validation::number(sample, "capturedEpochMs")?,
            uncertainty_ms: validation::number(sample, "uncertaintyMs")?,
            manual: sample["manual"] == true,
            image_hash: sha(&fs::read(&image).map_err(|e| e.to_string())?),
            text: best
                .filter(|c| c.confidence >= 35.)
                .map(|c| c.text.clone())
                .unwrap_or_default(),
            confidence: best.map(|c| c.confidence).unwrap_or(0.),
            candidates,
        };
        append(&mut journal, &observation)?;
        observations.push(observation);
        if observations.len() % 10 == 0 || observations.len() == samples.len() {
            state["processedSamples"] = observations.len().into();
            state["updatedAt"] = now().into();
            if smart_capture {
                state["recognitionCallsThisRun"] = recognition_calls.into();
                state["observationCount"] = timeline.len().into();
            }
            save(dir, state)?;
        }
    }
    if cancelled(dir, job) {
        return Err("OCR da huy truoc commit.".into());
    }
    let mut cues = if smart_capture {
        let (expanded, groups) = smart::expand(&timeline, &observations)?;
        cues::assemble_smart(&expanded, &groups)
    } else {
        assemble(&observations)
    };
    if smart_capture {
        let uncertain_groups = smart::uncertain_groups(&samples, &observations);
        for cue in &mut cues {
            cue.warnings.push("smart-visual-reuse-needs-review".into());
            if cue.evidence_sequences.iter().any(|sequence| {
                uncertain_groups.contains(
                    &samples[*sequence as usize - 1]["smart"]["group"]
                        .as_u64()
                        .unwrap_or(0),
                )
            }) {
                cue.warnings.push("smart-group-uncertain".into());
            }
        }
        state["recognitionCallsThisRun"] = recognition_calls.into();
    }
    let export: Vec<srt::Cue> = cues
        .iter()
        .map(|c| srt::Cue {
            index: c.index,
            start_ms: c.start_ms as i64,
            end_ms: c.end_ms as i64,
            text: c.text.clone(),
        })
        .collect();
    let srt_text = srt::serialize(&export);
    if !export.is_empty() && srt::parse(&srt_text)?.len() != export.len() {
        return Err("SRT roundtrip sai.".into());
    }
    let text = export
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let coverage = coverage(if smart_capture { &timeline } else { &samples });
    let cue_bytes=serde_json::to_vec_pretty(&json!({"schemaVersion":1,"sessionId":id,"revision":revision,"coverage":coverage,"cues":cues,"events":capture["events"],"source":capture["source"],"title":capture["title"],"url":capture["url"],"reviewed":false})).map_err(|e|e.to_string())?;
    let root = dir.join("ocr-revisions");
    no_symlink(&root)?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let output = root.join(&revision);
    no_symlink(&output)?;
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    for (file, bytes) in [
        ("subtitles.vi.srt", srt_text.as_bytes()),
        ("subtitles.vi.txt", text.as_bytes()),
        ("cues.json", cue_bytes.as_slice()),
    ] {
        write_revision_file(&output.join(file), bytes)?;
        atomic(&dir.join(file), bytes)?;
    }
    File::open(&root)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    state["state"] = if cues.is_empty() {
        "no-text"
    } else {
        "partial"
    }
    .into();
    state["committed"] = true.into();
    state["completedAt"] = now().into();
    state["updatedAt"] = now().into();
    state["cueCount"] = cues.len().into();
    state["reviewCount"] = cues
        .iter()
        .filter(|c| !c.warnings.is_empty())
        .count()
        .into();
    state["srtHash"] = sha(srt_text.as_bytes()).into();
    state["txtHash"] = sha(text.as_bytes()).into();
    state["cuesHash"] = sha(&cue_bytes).into();
    state["wholeEpisodeVerified"] = false.into();
    state["reviewed"] = false.into();
    state["outputDirectory"] = output.to_string_lossy().into_owned().into();
    state["srtPath"] = dir
        .join("subtitles.vi.srt")
        .to_string_lossy()
        .into_owned()
        .into();
    save(dir, state)?;
    verify_commit(dir, state)?;
    // Only generated per-job scratch is disposable; original crops/checkpoints/revisions remain.
    let _ = fs::remove_dir_all(&scratch);
    Ok(state.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_revision_is_immutable_and_missing_orphan_files_can_resume() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("output.srt");
        write_revision_file(&path, b"original").unwrap();
        write_revision_file(&path, b"original").unwrap();
        assert!(write_revision_file(&path, b"different").is_err());
        assert_eq!(fs::read(path).unwrap(), b"original");
        write_revision_file(&t.path().join("missing.txt"), b"resume").unwrap();
    }
    #[test]
    fn checkpoint_recovers_partial_tail_and_rejects_complete_corruption() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("obs.ndjson");
        fs::write(&p, b"{partial").unwrap();
        assert!(recover_observations(&p).unwrap().is_empty());
        assert_eq!(fs::metadata(&p).unwrap().len(), 0);
        fs::write(&p, b"bad complete row\n").unwrap();
        assert!(recover_observations(&p).is_err());
    }
    #[test]
    fn path_traversal_and_symlink_jobs_are_rejected() {
        let t = tempfile::tempdir().unwrap();
        assert!(directory(t.path(), "../escape").is_err());
        #[cfg(unix)]
        {
            let root = t.path().join("ocr-sessions");
            fs::create_dir(&root).unwrap();
            std::os::unix::fs::symlink(t.path(), root.join("9c4212da-a38b-4452-bb9c-ebfb5d55c66c"))
                .unwrap();
            assert!(directory(t.path(), "9c4212da-a38b-4452-bb9c-ebfb5d55c66c").is_err());
        }
    }
    #[test]
    fn coverage_never_claims_whole_episode_or_bridges_skips() {
        let s = |time, segment| json!({"estimatedVideoTimeMs":time,"segment":segment,"manual":false,"clock":{"duration":20}});
        let c = coverage(&[s(100, 0), s(200, 0), s(5000, 0), s(100, 2)]);
        assert_eq!(c["wholeEpisodeVerified"], false);
        assert_eq!(c["ranges"].as_array().unwrap().len(), 3);
        assert_eq!(c["gaps"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn observation_checksum_and_writer_lock_prevent_silent_recovery_loss() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("observations.ndjson");
        let observation = Observation {
            sequence: 1,
            segment: 0,
            time_ms: 100,
            captured_epoch_ms: 100.,
            uncertainty_ms: 25.,
            manual: false,
            image_hash: "abc".into(),
            text: "Xin chào".into(),
            confidence: 95.,
            candidates: vec![],
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        append(&mut file, &observation).unwrap();
        assert_eq!(recover_observations(&path).unwrap()[0].text, "Xin chào");
        let bytes = fs::read_to_string(&path)
            .unwrap()
            .replace("Xin chào", "Xin chao");
        fs::write(&path, bytes).unwrap();
        assert!(recover_observations(&path).is_err());
        let guard = lock(t.path(), false).unwrap();
        assert!(lock(t.path(), false).is_err());
        drop(guard);
        assert!(lock(t.path(), false).is_ok());
    }
    #[test]
    fn commit_verifies_revision_files_and_rejects_arbitrary_output_paths() {
        let t = tempfile::tempdir().unwrap();
        let revision = "a".repeat(64);
        let dir = t.path().join("ocr-revisions").join(&revision);
        fs::create_dir_all(&dir).unwrap();
        for file in ["subtitles.vi.srt", "subtitles.vi.txt", "cues.json"] {
            atomic(&dir.join(file), b"fixture").unwrap();
        }
        let mut m = json!({"revision":revision,"srtHash":sha(b"fixture"),"txtHash":sha(b"fixture"),"cuesHash":sha(b"fixture")});
        assert!(verify_commit(t.path(), &m).is_ok());
        fs::write(dir.join("subtitles.vi.srt"), b"corrupt").unwrap();
        assert!(verify_commit(t.path(), &m).is_err());
        m["revision"] = "../../escape".into();
        assert!(verify_commit(t.path(), &m).is_err());
    }
}
