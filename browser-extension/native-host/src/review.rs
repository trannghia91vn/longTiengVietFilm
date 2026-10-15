//! Desktop review snapshots never overwrite capture or worker-owned OCR revisions.
use crate::{
    ocr::{
        self,
        cues::{Cue, Observation},
    },
    storage::{atomic, no_symlink},
    validation,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use unicode_normalization::UnicodeNormalization;

use crate::ocr::srt;

const MAX_DOCUMENT: u64 = 32 * 1024 * 1024;
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Revision không hợp lệ.".into());
    }
    Ok(())
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    no_symlink(path)?;
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > limit {
        return Err("Dữ liệu vượt giới hạn.".into());
    }
    fs::read(path).map_err(|e| e.to_string())
}
fn json_file(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&read(path, MAX_DOCUMENT)?).map_err(|e| e.to_string())
}
pub fn directory(data: &Path, id: &str) -> Result<PathBuf, String> {
    validation::session_id(id)?;
    let root = data.join("ocr-sessions");
    let dir = root.join(id);
    for p in [data, root.as_path(), dir.as_path()] {
        no_symlink(p)?;
    }
    if !dir.is_dir() {
        return Err("Không tìm thấy bản thu.".into());
    }
    Ok(dir)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub reviewed: bool,
    pub evidence_sequences: Vec<u64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub session_id: String,
    pub revision: String,
    pub base_revision: String,
    pub edited: bool,
    pub newer_ocr_available: bool,
    pub cues: Vec<Cue>,
    pub original_cues: Vec<Cue>,
    pub coverage: Value,
    pub srt: String,
    pub text: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub session_id: String,
    pub title: String,
    pub url: String,
    pub created_at: Value,
    pub capture_state: String,
    pub sample_count: u64,
    pub state: String,
    pub processed_samples: u64,
    pub total_samples: u64,
    pub cue_count: usize,
    pub reviewed_count: usize,
    pub warning_count: usize,
    pub revision: Option<String>,
    pub edited: bool,
    pub error: Option<String>,
}
fn base(dir: &Path) -> Result<Value, String> {
    let state = json_file(&dir.join("ocr.json"))?;
    let manifest = if state["committed"] == true {
        state
    } else {
        state["previousResult"].clone()
    };
    if manifest["committed"] != true {
        return Err("Chưa có kết quả OCR đã lưu.".into());
    }
    ocr::verify_commit(dir, &manifest)?;
    Ok(manifest)
}
fn output(dir: &Path, manifest: &Value, edited: bool) -> Result<PathBuf, String> {
    let revision = manifest["revision"].as_str().ok_or("Thiếu revision")?;
    hash(revision)?;
    let parent = dir.join(if edited {
        "review-revisions"
    } else {
        "ocr-revisions"
    });
    let folder = parent.join(revision);
    no_symlink(&parent)?;
    no_symlink(&folder)?;
    for (file, field) in [
        ("subtitles.vi.srt", "srtHash"),
        ("subtitles.vi.txt", "txtHash"),
        ("cues.json", "cuesHash"),
    ] {
        if sha(&read(&folder.join(file), MAX_DOCUMENT)?) != manifest[field].as_str().unwrap_or("") {
            return Err("Revision đã lưu bị thiếu/hỏng; không ghi đè.".into());
        }
    }
    Ok(folder)
}
fn selected(dir: &Path) -> Result<(Value, Value, bool), String> {
    let pointer = dir.join("review.json");
    if pointer.exists() {
        let review = json_file(&pointer)?;
        if review["sessionId"] != dir.file_name().unwrap().to_string_lossy().as_ref()
            || review["schemaVersion"] != 1
        {
            return Err("Review manifest không hợp lệ.".into());
        }
        output(dir, &review, true)?;
        let source = review["baseOcr"].clone();
        ocr::verify_commit(dir, &source)?;
        Ok((review, source, true))
    } else {
        let source = base(dir)?;
        Ok((source.clone(), source, false))
    }
}
pub fn load(data: &Path, id: &str) -> Result<Document, String> {
    let dir = directory(data, id)?;
    let (manifest, source, edited) = selected(&dir)?;
    let folder = output(&dir, &manifest, edited)?;
    let original = output(&dir, &source, false)?;
    let doc = json_file(&folder.join("cues.json"))?;
    let original_doc = json_file(&original.join("cues.json"))?;
    if doc["sessionId"] != id || original_doc["sessionId"] != id {
        return Err("Sai UUID của cue.".into());
    }
    let latest = base(&dir).ok();
    Ok(Document {
        session_id: id.into(),
        revision: manifest["revision"]
            .as_str()
            .ok_or("Thiếu revision")?
            .into(),
        base_revision: source["revision"]
            .as_str()
            .ok_or("Thiếu base revision")?
            .into(),
        edited,
        newer_ocr_available: latest.is_some_and(|v| v["revision"] != source["revision"]),
        cues: serde_json::from_value(doc["cues"].clone()).map_err(|e| e.to_string())?,
        original_cues: serde_json::from_value(original_doc["cues"].clone())
            .map_err(|e| e.to_string())?,
        coverage: original_doc["coverage"].clone(),
        srt: String::from_utf8(read(&folder.join("subtitles.vi.srt"), MAX_DOCUMENT)?)
            .map_err(|e| e.to_string())?,
        text: String::from_utf8(read(&folder.join("subtitles.vi.txt"), MAX_DOCUMENT)?)
            .map_err(|e| e.to_string())?,
    })
}
pub fn list(data: &Path) -> Result<Vec<Summary>, String> {
    let root = data.join("ocr-sessions");
    no_symlink(&root)?;
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut items = vec![];
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().to_string();
        if validation::session_id(&id).is_err() {
            continue;
        }
        let mut item = Summary {
            session_id: id.clone(),
            title: id.clone(),
            url: String::new(),
            created_at: Value::Null,
            capture_state: "unknown".into(),
            sample_count: 0,
            state: "not-started".into(),
            processed_samples: 0,
            total_samples: 0,
            cue_count: 0,
            reviewed_count: 0,
            warning_count: 0,
            revision: None,
            edited: false,
            error: None,
        };
        let result = (|| -> Result<(), String> {
            let dir = directory(data, &id)?;
            let capture = json_file(&dir.join("session.json"))?;
            item.title = capture["title"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(&id)
                .into();
            item.url = capture["url"].as_str().unwrap_or("").into();
            item.created_at = capture["createdAt"].clone();
            item.capture_state = capture["nativeStore"]["status"]
                .as_str()
                .unwrap_or("unknown")
                .into();
            item.sample_count = capture["sampleCount"].as_u64().unwrap_or(0);
            let state = ocr::status(data, &id)?;
            item.state = state["state"].as_str().unwrap_or("error").into();
            item.processed_samples = state["processedSamples"].as_u64().unwrap_or(0);
            item.total_samples = state["totalSamples"].as_u64().unwrap_or(item.sample_count);
            item.error = state["error"].as_str().map(String::from);
            if dir.join("review.json").exists()
                || state["committed"] == true
                || state["previousResult"]["committed"] == true
            {
                let doc = load(data, &id)?;
                item.cue_count = doc.cues.len();
                item.reviewed_count = doc.cues.iter().filter(|c| c.reviewed).count();
                item.warning_count = doc
                    .cues
                    .iter()
                    .filter(|c| !c.warnings.is_empty() && !c.reviewed)
                    .count();
                item.revision = Some(doc.revision);
                item.edited = doc.edited;
            }
            Ok(())
        })();
        if let Err(e) = result {
            item.state = "error".into();
            item.error = Some(e);
        }
        items.push(item);
    }
    items.sort_by(|a, b| {
        b.created_at
            .to_string()
            .cmp(&a.created_at.to_string())
            .then(a.session_id.cmp(&b.session_id))
    });
    Ok(items)
}
pub fn save(
    data: &Path,
    id: &str,
    expected: &str,
    edits: Vec<EditCue>,
) -> Result<Document, String> {
    hash(expected)?;
    if edits.len() > 100_000 {
        return Err("Quá nhiều cue.".into());
    }
    let dir = directory(data, id)?;
    let lock = dir.join(".review.lock");
    no_symlink(&lock)?;
    let guard = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock)
        .map_err(|e| e.to_string())?;
    FileExt::try_lock_exclusive(&guard).map_err(|_| "Bản sửa đang được lưu ở cửa sổ khác.")?;
    let current = load(data, id)?;
    if current.revision != expected {
        return Err(
            "REVISION_CONFLICT: Bản lưu đã thay đổi; giữ bản nháp và tải lại để đối chiếu.".into(),
        );
    }
    let (_, source, _) = selected(&dir)?;
    if source["revision"] != current.base_revision {
        return Err(
            "REVISION_CONFLICT: OCR vừa commit revision mới; giữ bản nháp để đối chiếu.".into(),
        );
    }
    let mut by_evidence = HashMap::new();
    for cue in &current.original_cues {
        for sequence in &cue.evidence_sequences {
            by_evidence.insert(*sequence, cue);
        }
    }
    let mut cues = vec![];
    for edit in edits {
        let text: String = edit
            .text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .nfc()
            .collect::<String>()
            .trim()
            .into();
        if edit.end_ms <= edit.start_ms
            || edit.end_ms > 3_600_000_000
            || text.is_empty()
            || text.len() > 8192
            || text.split('\n').any(|l| l.trim().is_empty())
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err("Cue có text/thời gian không hợp lệ.".into());
        }
        if edit.evidence_sequences.is_empty() || edit.evidence_sequences.len() > 100_000 {
            return Err("Cue thiếu bằng chứng.".into());
        }
        let evidence: BTreeSet<_> = edit.evidence_sequences.into_iter().collect();
        let originals: Vec<_> = evidence
            .iter()
            .map(|s| {
                by_evidence
                    .get(s)
                    .copied()
                    .ok_or("Bằng chứng không thuộc bản OCR gốc.")
            })
            .collect::<Result<_, _>>()?;
        let mut cue = originals[0].clone();
        if originals.iter().any(|c| c.segment != cue.segment) {
            return Err("Không gộp cue từ các segment khác nhau.".into());
        }
        cue.start_ms = edit.start_ms;
        cue.end_ms = edit.end_ms;
        cue.text = text;
        cue.reviewed = edit.reviewed;
        cue.evidence_sequences = evidence.into_iter().collect();
        cue.confidence = originals.iter().map(|c| c.confidence).fold(100., f64::min);
        cue.warnings = originals
            .iter()
            .flat_map(|c| c.warnings.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let modified = originals.iter().any(|original| {
            original.start_ms != cue.start_ms
                || original.end_ms != cue.end_ms
                || original.text != cue.text
        });
        if modified && !cue.warnings.iter().any(|w| w == "user-edited") {
            cue.warnings.push("user-edited".into());
        }
        cues.push(cue);
    }
    cues.sort_by_key(|c| (c.start_ms, c.end_ms));
    for (i, cue) in cues.iter_mut().enumerate() {
        cue.index = i + 1;
    }
    for i in 1..cues.len() {
        if cues[i].start_ms < cues[i - 1].end_ms {
            for c in &mut cues[i - 1..=i] {
                if !c.warnings.iter().any(|w| w == "overlap-needs-review") {
                    c.warnings.push("overlap-needs-review".into());
                }
            }
        }
    }
    let srt = srt::serialize(
        &cues
            .iter()
            .map(|c| srt::Cue {
                index: c.index,
                start_ms: c.start_ms as i64,
                end_ms: c.end_ms as i64,
                text: c.text.clone(),
            })
            .collect::<Vec<_>>(),
    );
    if !cues.is_empty() {
        srt::parse(&srt)?;
    }
    let text = cues
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let revision = sha(&serde_json::to_vec(
        &json!({"parent":expected,"cues":cues,"nonce":uuid::Uuid::new_v4().to_string()}),
    )
    .map_err(|e| e.to_string())?);
    let bytes=serde_json::to_vec_pretty(&json!({"schemaVersion":1,"sessionId":id,"revision":revision,"baseRevision":source["revision"],"coverage":current.coverage,"cues":cues})).map_err(|e|e.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT {
        return Err("Bản sửa vượt giới hạn.".into());
    }
    let parent = dir.join("review-revisions");
    no_symlink(&parent)?;
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let folder = parent.join(&revision);
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let manifest = json!({"schemaVersion":1,"sessionId":id,"revision":revision,"parentRevision":expected,"baseOcr":source,"createdAt":created,"srtHash":sha(srt.as_bytes()),"txtHash":sha(text.as_bytes()),"cuesHash":sha(&bytes)});
    for (file, content) in [
        ("subtitles.vi.srt", srt.as_bytes()),
        ("subtitles.vi.txt", text.as_bytes()),
        ("cues.json", bytes.as_slice()),
    ] {
        atomic(&folder.join(file), content)?;
    }
    let committed = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    atomic(&folder.join("manifest.json"), &committed)?;
    fs::File::open(&parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    // Pointer is the commit boundary; orphan revisions from a crash remain harmless.
    atomic(&dir.join("review.json"), &committed)?;
    load(data, id)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub observation: Observation,
    pub image_url: String,
}
pub fn evidence(data: &Path, id: &str, sequence: u64) -> Result<Evidence, String> {
    let dir = directory(data, id)?;
    let (_, source, _) = selected(&dir)?;
    let folder = output(&dir, &source, false)?;
    let original = json_file(&folder.join("cues.json"))?;
    let cues: Vec<Cue> =
        serde_json::from_value(original["cues"].clone()).map_err(|e| e.to_string())?;
    if !cues
        .iter()
        .any(|c| c.evidence_sequences.contains(&sequence))
    {
        return Err("Ảnh không thuộc cue OCR.".into());
    }
    let revision = source["observationRevision"]
        .as_str()
        .ok_or("Thiếu observation revision")?;
    hash(revision)?;
    let work = dir.join("ocr-work");
    let records = work.join(revision);
    no_symlink(&work)?;
    no_symlink(&records)?;
    let path = records.join("observations.ndjson");
    no_symlink(&path)?;
    let reader = BufReader::new(fs::File::open(path).map_err(|e| e.to_string())?);
    #[derive(Deserialize)]
    struct Record<'a> {
        #[serde(borrow)]
        observation: &'a serde_json::value::RawValue,
        checksum: String,
    }
    let mut found = None;
    for (i, line) in reader.split(b'\n').enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        if line.is_empty() {
            continue;
        }
        if line.len() > 128 * 1024 {
            return Err("Observation quá lớn.".into());
        }
        let record: Record<'_> = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
        if sha(record.observation.get().as_bytes()) != record.checksum {
            return Err("Observation checksum sai.".into());
        }
        let observation: Observation =
            serde_json::from_str(record.observation.get()).map_err(|e| e.to_string())?;
        if observation.sequence != i as u64 + 1 {
            return Err("Observation sequence sai.".into());
        }
        if observation.sequence == sequence {
            found = Some(observation);
            break;
        }
    }
    let observation = found.ok_or("Thiếu observation.")?;
    let frames = dir.join("frames");
    no_symlink(&frames)?;
    let image = read(&frames.join(format!("{sequence:08}.png")), 16 * 1024 * 1024)?;
    if sha(&image) != observation.image_hash {
        return Err("Ảnh bằng chứng checksum sai.".into());
    }
    Ok(Evidence {
        observation,
        image_url: format!("data:image/png;base64,{}", STANDARD.encode(image)),
    })
}

pub fn recognize(
    data: &Path,
    id: &str,
    sequence: u64,
) -> Result<Vec<ocr::engine::Candidate>, String> {
    evidence(data, id, sequence)?;
    let dir = directory(data, id)?;
    let work = dir.join("ocr-work");
    no_symlink(&work)?;
    let scratch = work.join(format!("review-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&scratch).map_err(|e| e.to_string())?;
    let result = (|| {
        let engine = ocr::engine::Engine::resolve(data, &scratch)?;
        engine.recognize(
            &dir.join("frames").join(format!("{sequence:08}.png")),
            &scratch,
            || false,
        )
    })();
    let _ = fs::remove_dir_all(scratch);
    result
}

pub fn export(
    data: &Path,
    id: &str,
    expected: &str,
    format: &str,
    path: &Path,
) -> Result<(), String> {
    let doc = load(data, id)?;
    if doc.revision != expected {
        return Err("REVISION_CONFLICT: Hãy tải lại bản lưu trước khi xuất.".into());
    }
    let content = match format {
        "srt" => doc.srt,
        "txt" => doc.text,
        _ => return Err("Định dạng không hợp lệ.".into()),
    };
    no_symlink(path)?;
    let parent = path
        .parent()
        .ok_or("Thiếu thư mục xuất")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if parent.starts_with(data.canonicalize().map_err(|e| e.to_string())?) {
        return Err("Không xuất đè vào dữ liệu nội bộ của app.".into());
    }
    atomic(
        &parent.join(path.file_name().ok_or("Thiếu tên file")?),
        content.as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "9c4212da-a38b-4452-bb9c-ebfb5d55c66c";
    fn fixture() -> (tempfile::TempDir, PathBuf, String) {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let dir = data.join("ocr-sessions").join(ID);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("journal.ndjson"), b"capture immutable\n").unwrap();
        fs::write(dir.join("session.json"),serde_json::to_vec(&json!({"id":ID,"title":"Phim thử","createdAt":123,"sampleCount":2,"nativeStore":{"status":"captured"}})).unwrap()).unwrap();
        let revision = "a".repeat(64);
        let observation_revision = "b".repeat(64);
        let output = dir.join("ocr-revisions").join(&revision);
        fs::create_dir_all(&output).unwrap();
        let cues = vec![
            Cue {
                index: 1,
                start_ms: 1000,
                end_ms: 2000,
                text: "Xin chào".into(),
                confidence: 90.,
                segment: 0,
                evidence_sequences: vec![1],
                start_range_ms: [950, 1050],
                end_range_ms: [1950, 2050],
                warnings: vec!["short-cue".into()],
                reviewed: false,
            },
            Cue {
                index: 2,
                start_ms: 2100,
                end_ms: 3000,
                text: "Tạm biệt".into(),
                confidence: 95.,
                segment: 0,
                evidence_sequences: vec![2],
                start_range_ms: [2050, 2150],
                end_range_ms: [2950, 3050],
                warnings: vec![],
                reviewed: false,
            },
        ];
        let cue_bytes=serde_json::to_vec(&json!({"sessionId":ID,"revision":revision,"cues":cues,"coverage":{"ranges":[{"startMs":900,"endMs":3100,"segment":0}],"gaps":[],"durationMs":10000,"wholeEpisodeVerified":false}})).unwrap();
        let srt="1\n00:00:01,000 --> 00:00:02,000\nXin chào\n\n2\n00:00:02,100 --> 00:00:03,000\nTạm biệt\n";
        for (file, bytes) in [
            ("subtitles.vi.srt", srt.as_bytes()),
            ("subtitles.vi.txt", "Xin chào\n\nTạm biệt".as_bytes()),
            ("cues.json", cue_bytes.as_slice()),
        ] {
            fs::write(output.join(file), bytes).unwrap();
            fs::write(dir.join(file), bytes).unwrap();
        }
        fs::write(dir.join("ocr.json"),serde_json::to_vec(&json!({"schemaVersion":1,"sessionId":ID,"revision":revision,"observationRevision":observation_revision,"committed":true,"state":"partial","srtHash":sha(srt.as_bytes()),"txtHash":sha("Xin chào\n\nTạm biệt".as_bytes()),"cuesHash":sha(&cue_bytes)})).unwrap()).unwrap();
        let work = dir.join("ocr-work").join(observation_revision);
        fs::create_dir_all(&work).unwrap();
        fs::create_dir(dir.join("frames")).unwrap();
        let mut rows = String::new();
        for n in 1..=2 {
            let image = format!("test PNG {n}");
            fs::write(
                dir.join("frames").join(format!("{n:08}.png")),
                image.as_bytes(),
            )
            .unwrap();
            let observation = Observation {
                sequence: n,
                segment: 0,
                time_ms: n * 1000,
                captured_epoch_ms: 123.456,
                uncertainty_ms: 25.123,
                manual: false,
                image_hash: sha(image.as_bytes()),
                text: "Xin chào".into(),
                confidence: 95.,
                candidates: vec![],
            };
            let bytes = serde_json::to_vec(&observation).unwrap();
            rows.push_str(&format!(
                "{{\"observation\":{},\"checksum\":\"{}\"}}\n",
                String::from_utf8(bytes.clone()).unwrap(),
                sha(&bytes)
            ));
        }
        fs::write(work.join("observations.ndjson"), rows).unwrap();
        (temp, data, revision)
    }
    fn edits(doc: &Document) -> Vec<EditCue> {
        doc.cues
            .iter()
            .map(|c| EditCue {
                start_ms: c.start_ms,
                end_ms: c.end_ms,
                text: c.text.clone(),
                reviewed: c.reviewed,
                evidence_sequences: c.evidence_sequences.clone(),
            })
            .collect()
    }
    #[test]
    fn durable_revision_conflict_export_and_original_preservation() {
        let (temp, data, base) = fixture();
        let dir = directory(&data, ID).unwrap();
        let capture = fs::read(dir.join("journal.ndjson")).unwrap();
        let original = fs::read(dir.join("subtitles.vi.srt")).unwrap();
        let mut edit = edits(&load(&data, ID).unwrap());
        edit[0].text = "Xin chào con!".into();
        edit[0].reviewed = true;
        let saved = save(&data, ID, &base, edit.clone()).unwrap();
        assert!(saved.edited);
        assert!(saved.cues[0].reviewed);
        assert!(saved.cues[0].warnings.contains(&"user-edited".into()));
        assert!(saved.cues[1].warnings.is_empty());
        assert_eq!(saved.base_revision, base);
        assert!(save(&data, ID, &base, edit)
            .unwrap_err()
            .contains("REVISION_CONFLICT"));
        assert_eq!(load(&data, ID).unwrap().cues[0].text, "Xin chào con!");
        assert_eq!(fs::read(dir.join("subtitles.vi.srt")).unwrap(), original);
        assert_eq!(fs::read(dir.join("journal.ndjson")).unwrap(), capture);
        let target = temp.path().join("export.srt");
        export(&data, ID, &saved.revision, "srt", &target).unwrap();
        assert_eq!(
            srt::parse(&fs::read_to_string(target).unwrap()).unwrap()[0].text,
            "Xin chào con!"
        );
        assert!(export(
            &data,
            ID,
            &saved.revision,
            "srt",
            &dir.join("subtitles.vi.srt")
        )
        .is_err());
        assert!(export(&data, ID, &base, "srt", &temp.path().join("stale.srt")).is_err());
        assert_eq!(list(&data).unwrap()[0].reviewed_count, 1);
    }
    #[test]
    fn incomplete_orphan_is_ignored_and_corrupt_commit_fails_closed() {
        let (_temp, data, base) = fixture();
        let dir = directory(&data, ID).unwrap();
        let orphan = dir.join("review-revisions").join("c".repeat(64));
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("subtitles.vi.srt"), b"partial").unwrap();
        assert_eq!(load(&data, ID).unwrap().revision, base);
        let doc = load(&data, ID).unwrap();
        let saved = save(&data, ID, &base, edits(&doc)).unwrap();
        fs::write(
            dir.join("review-revisions")
                .join(&saved.revision)
                .join("cues.json"),
            b"corrupt",
        )
        .unwrap();
        assert!(load(&data, ID).is_err());
        assert!(save(&data, ID, &saved.revision, edits(&doc)).is_err());
        assert_eq!(list(&data).unwrap()[0].state, "error");
    }
    #[test]
    fn validation_lock_traversal_and_provenance() {
        let (_temp, data, base) = fixture();
        let doc = load(&data, ID).unwrap();
        let mut edit = edits(&doc);
        assert!(load(&data, "../outside").is_err());
        assert!(save(&data, ID, "../outside", edit.clone()).is_err());
        edit[0].evidence_sequences = vec![99];
        assert!(save(&data, ID, &base, edit).is_err());
        for (start, end, text) in [
            (2000, 1000, "X"),
            (0, 3600000001, "X"),
            (0, 1, ""),
            (0, 1, "a\n\nb"),
            (0, 1, "a\0b"),
        ] {
            let mut edit = edits(&doc);
            edit[0].start_ms = start;
            edit[0].end_ms = end;
            edit[0].text = text.into();
            assert!(save(&data, ID, &base, edit).is_err());
        }
        let dir = directory(&data, ID).unwrap();
        let guard = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(".review.lock"))
            .unwrap();
        FileExt::lock_exclusive(&guard).unwrap();
        assert!(save(&data, ID, &base, edits(&doc)).is_err());
        drop(guard);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("frames/00000001.png"), dir.join("review.json"))
                .unwrap();
            assert!(load(&data, ID).is_err());
        }
    }
    #[test]
    fn worker_progress_and_new_commit_do_not_replace_review() {
        let (_temp, data, base) = fixture();
        let doc = load(&data, ID).unwrap();
        let saved = save(&data, ID, &base, edits(&doc)).unwrap();
        let dir = directory(&data, ID).unwrap();
        let old = json_file(&dir.join("ocr.json")).unwrap();
        fs::write(
            dir.join("ocr.json"),
            serde_json::to_vec(
                &json!({"sessionId":ID,"committed":false,"state":"error","previousResult":old}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(load(&data, ID).unwrap().revision, saved.revision);
        let new = "d".repeat(64);
        let folder = dir.join("ocr-revisions").join(&new);
        fs::create_dir(&folder).unwrap();
        for file in ["subtitles.vi.srt", "subtitles.vi.txt", "cues.json"] {
            fs::copy(
                dir.join("ocr-revisions").join(&base).join(file),
                folder.join(file),
            )
            .unwrap();
        }
        let mut latest = old;
        latest["revision"] = new.into();
        fs::write(dir.join("ocr.json"), serde_json::to_vec(&latest).unwrap()).unwrap();
        let reloaded = load(&data, ID).unwrap();
        assert_eq!(reloaded.revision, saved.revision);
        assert!(reloaded.newer_ocr_available);
    }
    #[test]
    fn evidence_checks_raw_json_checksum_and_image_hash() {
        let (_temp, data, _) = fixture();
        assert_eq!(
            evidence(&data, ID, 1).unwrap().observation.uncertainty_ms,
            25.123
        );
        assert!(evidence(&data, ID, 99).is_err());
        fs::write(
            directory(&data, ID).unwrap().join("frames/00000001.png"),
            b"changed",
        )
        .unwrap();
        assert!(evidence(&data, ID, 1).is_err());
    }
    #[test]
    fn split_merge_delete_and_empty_revision_round_trip() {
        let (_temp, data, base) = fixture();
        let doc = load(&data, ID).unwrap();
        let mut edit = edits(&doc);
        let mut half = edit[0].clone();
        half.start_ms = 1500;
        half.text = "chào".into();
        edit[0].end_ms = 1500;
        edit[0].text = "Xin".into();
        edit.insert(1, half);
        let split = save(&data, ID, &base, edit).unwrap();
        assert_eq!(split.cues.len(), 3);
        let mut merged = edits(&split);
        merged[0].end_ms = merged[1].end_ms;
        merged[0].text = "Xin chào".into();
        merged.remove(1);
        let merged = save(&data, ID, &split.revision, merged).unwrap();
        assert_eq!(merged.cues.len(), 2);
        let empty = save(&data, ID, &merged.revision, vec![]).unwrap();
        assert!(empty.cues.is_empty());
        assert!(empty.srt.is_empty());
        assert_eq!(load(&data, ID).unwrap().revision, empty.revision);
    }
    #[test]
    #[ignore = "Requires OCR_REVIEW_TEST_SOURCE pointing to an isolated completed 2B session"]
    fn real_film_review_on_isolated_copy() {
        let source = PathBuf::from(std::env::var("OCR_REVIEW_TEST_SOURCE").unwrap());
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let id = source.file_name().unwrap().to_str().unwrap();
        let dir = data.join("ocr-sessions").join(id);
        fs::create_dir_all(&dir).unwrap();
        for file in [
            "session.json",
            "journal.ndjson",
            "ocr.json",
            "subtitles.vi.srt",
            "subtitles.vi.txt",
            "cues.json",
        ] {
            fs::copy(source.join(file), dir.join(file)).unwrap();
        }
        let state = json_file(&source.join("ocr.json")).unwrap();
        let revision = state["revision"].as_str().unwrap();
        let observations = state["observationRevision"].as_str().unwrap();
        for (parent, key, files) in [
            (
                "ocr-revisions",
                revision,
                vec!["subtitles.vi.srt", "subtitles.vi.txt", "cues.json"],
            ),
            ("ocr-work", observations, vec!["observations.ndjson"]),
        ] {
            let folder = dir.join(parent).join(key);
            fs::create_dir_all(&folder).unwrap();
            for file in files {
                fs::copy(source.join(parent).join(key).join(file), folder.join(file)).unwrap();
            }
        }
        let doc = load(&data, id).unwrap();
        assert!(doc.cues.len() > 100);
        let sequence = doc.cues[0].evidence_sequences[0];
        fs::create_dir(dir.join("frames")).unwrap();
        fs::copy(
            source.join("frames").join(format!("{sequence:08}.png")),
            dir.join("frames").join(format!("{sequence:08}.png")),
        )
        .unwrap();
        assert!(!evidence(&data, id, sequence).unwrap().image_url.is_empty());
        let mut edit = edits(&doc);
        edit[0].text.push_str(" [kiểm thử]");
        edit[0].reviewed = true;
        let saved = save(&data, id, &doc.revision, edit).unwrap();
        assert_eq!(saved.cues.len(), doc.cues.len());
        assert!(load(&data, id).unwrap().cues[0].reviewed);
        assert_eq!(saved.original_cues[0].text, doc.cues[0].text);
        println!(
            "real film: {} cues; {} coverage segments; revision {}",
            saved.cues.len(),
            saved.coverage["ranges"].as_array().unwrap().len(),
            saved.revision
        );
    }
}
