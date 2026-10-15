//! Full-timeline validation and bounded storage, shared by command and render preflight.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const PROJECT_QUOTA: u64 = 2 * 1024 * 1024 * 1024;
pub const LIBRARY_QUOTA: u64 = 10 * 1024 * 1024 * 1024;
const FREE_RESERVE: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub allow_gaps: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cue_voices: BTreeMap<usize, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Gap {
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: String,
    pub cue_index: Option<usize>,
    pub message: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub issues: Vec<Issue>,
    pub warnings: Vec<String>,
    pub gaps: Vec<Gap>,
    pub duration_ms: u64,
    pub known_duration_ms: u64,
    pub cue_count: usize,
    pub reviewed_count: usize,
    pub wav_bytes: u64,
    pub estimated_growth_bytes: u64,
    pub project_bytes: u64,
    pub library_bytes: u64,
    pub free_bytes: u64,
    pub project_quota_bytes: u64,
    pub library_quota_bytes: u64,
    pub partial: bool,
}
impl Preflight {
    fn issue(&mut self, code: &str, cue: Option<usize>, message: impl Into<String>) {
        self.issues.push(Issue {
            code: code.into(),
            cue_index: cue,
            message: message.into(),
        });
    }
}
pub fn voice_for<'a>(settings: &'a TrialSettings, cue: &Cue) -> &'a str {
    settings
        .full
        .as_ref()
        .and_then(|o| o.cue_voices.get(&cue.index))
        .map(String::as_str)
        .unwrap_or(&settings.voice)
}

// Replayed segments can fill an earlier gap. Never treat a large missing interval as silence coverage.
fn gaps(source: &Source, end: u64) -> Result<Vec<Gap>, String> {
    let ranges = source.coverage["ranges"]
        .as_array()
        .ok_or("Coverage thiếu ranges.")?;
    let mut intervals = vec![];
    for r in ranges {
        let start = r["startMs"]
            .as_u64()
            .ok_or("Coverage start không hợp lệ.")?;
        let stop = r["endMs"].as_u64().ok_or("Coverage end không hợp lệ.")?;
        if stop < start || stop > MAX_END_MS {
            return Err("Coverage vượt giới hạn hoặc đảo thời gian.".into());
        }
        if start <= end {
            intervals.push((start, stop.min(end)));
        }
    }
    intervals.sort_unstable();
    let mut cursor = 0u64;
    let mut missing = vec![];
    for (start, stop) in intervals {
        if start > cursor.saturating_add(250) {
            missing.push(Gap {
                start_ms: cursor,
                end_ms: start,
            });
        }
        cursor = cursor.max(stop);
    }
    if end > cursor.saturating_add(250) {
        missing.push(Gap {
            start_ms: cursor,
            end_ms: end,
        });
    }
    Ok(missing)
}
pub fn validate(source: &Source, settings: &TrialSettings) -> Preflight {
    let end = settings.duration_ms;
    let known = source.coverage["durationMs"].as_u64().unwrap_or(0);
    let mut p = Preflight {
        issues: vec![],
        warnings: vec![],
        gaps: vec![],
        duration_ms: end,
        known_duration_ms: known,
        cue_count: source.cues.len(),
        reviewed_count: source.cues.iter().filter(|c| c.reviewed).count(),
        wav_bytes: end.saturating_mul(RATE as u64 * 2) / 1000 + 44,
        estimated_growth_bytes: 0,
        project_bytes: 0,
        library_bytes: 0,
        free_bytes: 0,
        project_quota_bytes: PROJECT_QUOTA,
        library_quota_bytes: LIBRARY_QUOTA,
        partial: false,
    };
    let Some(options) = settings.full.as_ref() else {
        p.issue("mode", None, "Thiếu cấu hình toàn tập.");
        return p;
    };
    if settings.start_ms != 0 || end == 0 || end > MAX_END_MS {
        p.issue(
            "duration",
            None,
            "WAV toàn timeline phải bắt đầu ở 0 và dài tối đa 2 giờ.",
        );
    }
    if known > 0 && known != end {
        p.issue(
            "duration",
            None,
            "Thời lượng phải bằng video đã ghi, không cắt hoặc kéo dài tập.",
        );
    }
    if known == 0 {
        p.partial = true;
        p.warnings.push(
            "Chưa biết thời lượng video; mốc cuối do người dùng nhập, không xác nhận cả tập."
                .into(),
        );
        if !options.allow_gaps {
            p.issue(
                "unknown-duration",
                None,
                "Chưa biết thời lượng video; chỉ xuất bản Một phần khi chủ động xác nhận.",
            );
        }
    }
    if !voice_valid(&settings.voice) {
        p.issue("voice", None, "Giọng mặc định không hợp lệ.");
    }
    let ids: BTreeSet<_> = source.cues.iter().map(|c| c.index).collect();
    for (index, voice) in &options.cue_voices {
        if !ids.contains(index) || !voice_valid(voice) {
            p.issue(
                "voice",
                Some(*index),
                "Giọng riêng hoặc ID câu không hợp lệ.",
            );
        }
    }
    if source.cues.is_empty() || source.cues.len() > 10000 {
        p.issue("count", None, "Cần 1–10.000 câu để tạo WAV toàn timeline.");
    }
    let mut cues: Vec<_> = source.cues.iter().collect();
    cues.sort_by_key(|c| (c.start_ms, c.end_ms));
    let mut previous_end = 0;
    let mut seen = BTreeSet::new();
    let mut cache_estimate = 0u64;
    for c in cues {
        if !seen.insert(c.index) {
            p.issue("duplicate", Some(c.index), "ID câu trùng.");
        }
        if !c.reviewed {
            p.issue(
                "unreviewed",
                Some(c.index),
                format!("Câu {} chưa kiểm tra và lưu.", c.index),
            );
        }
        if c.start_ms >= c.end_ms || c.end_ms > end {
            p.issue(
                "timing",
                Some(c.index),
                format!("Câu {} sai thời gian hoặc vượt cuối video.", c.index),
            );
        }
        if c.start_ms < previous_end {
            p.issue(
                "overlap",
                Some(c.index),
                format!("Câu {} chồng lấn; không dồn hoặc ghi đè lời.", c.index),
            );
        }
        previous_end = previous_end.max(c.end_ms);
        let spoken = pipeline::speech_text(&c.text);
        if spoken.trim().is_empty() || spoken.len() > 8192 {
            p.issue(
                "text",
                Some(c.index),
                format!("Câu {} không có lời đọc hoặc quá dài.", c.index),
            );
        }
        let slot = c.end_ms.saturating_sub(c.start_ms) * RATE as u64 * 2 / 1000;
        let speech = spoken.chars().count() as u64 * RATE as u64 * 2 / 18;
        cache_estimate = cache_estimate.saturating_add(slot.max(speech).max(100_000) * 3);
    }
    match gaps(source, end) {
        Ok(g) => p.gaps = g,
        Err(e) => p.issue("coverage", None, e),
    }
    if !p.gaps.is_empty() {
        p.partial = true;
        p.warnings.push(format!(
            "{} khoảng chưa thu; WAV giữ silence, không khôi phục thoại bị thiếu.",
            p.gaps.len()
        ));
        if !options.allow_gaps {
            p.issue(
                "coverage-gap",
                None,
                "Chưa thu đủ timeline; thu bổ sung hoặc xác nhận xuất Một phần.",
            );
        }
    }
    p.estimated_growth_bytes = p
        .wav_bytes
        .saturating_mul(2)
        .saturating_add(cache_estimate)
        .saturating_add(1_048_576);
    p
}

fn tree_bytes(root: &Path) -> Result<u64, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0usize;
    let mut bytes = 0u64;
    while let Some(path) = pending.pop() {
        no_link(&path)?;
        if !path.exists() {
            continue;
        }
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            count += 1;
            if count > 500_000 {
                return Err("Thư viện audio quá nhiều file; cần quản lý dung lượng trước.".into());
            }
            if kind.is_symlink() {
                return Err("Thư viện audio chứa symlink; từ chối đo/ghi dữ liệu.".into());
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                let file = entry.path();
                let bounded = file.components().any(|c| {
                    c.as_os_str()
                        .to_str()
                        .is_some_and(|s| s == "cache" || s.starts_with("job-"))
                });
                if bounded
                    && file.extension().is_some_and(|e| e == "wav")
                    && entry.metadata().map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024
                {
                    return Err(
                        "Audio cue/workspace vượt 16 MiB; dừng engine, cần chia/sửa câu.".into(),
                    );
                }
                bytes = bytes
                    .checked_add(entry.metadata().map_err(|e| e.to_string())?.len())
                    .ok_or("Dung lượng audio overflow")?;
            } else {
                return Err("Thư viện audio chứa file đặc biệt.".into());
            }
        }
    }
    Ok(bytes)
}
fn budget(project: u64, library: u64, free: u64, growth: u64) -> Result<(), String> {
    if project.saturating_add(growth) > PROJECT_QUOTA {
        return Err(
            "Vượt 2 GiB/dự án audio; xuất/backup dữ liệu trước. App không tự xóa cache.".into(),
        );
    }
    if library.saturating_add(growth) > LIBRARY_QUOTA {
        return Err(
            "Vượt 10 GiB thư viện audio; xuất/backup dữ liệu trước. App không tự xóa dữ liệu."
                .into(),
        );
    }
    if free < growth.saturating_add(FREE_RESERVE) {
        return Err("Không đủ dung lượng; cần giữ ít nhất 1 GiB trống ngoài WAV/cache.".into());
    }
    Ok(())
}
pub fn check_budget(data: &Path, dir: &Path, growth: u64) -> Result<(), String> {
    budget(
        tree_bytes(dir)?,
        tree_bytes(&data.join("audio-projects"))?,
        fs2::available_space(dir).map_err(|e| e.to_string())?,
        growth,
    )
}
pub struct BudgetWatcher {
    task: tauri::async_runtime::JoinHandle<()>,
    error: Arc<std::sync::Mutex<Option<String>>>,
}
impl BudgetWatcher {
    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }
}
impl Drop for BudgetWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub fn watch_budget(data: PathBuf, dir: PathBuf, cancel: Arc<AtomicBool>) -> BudgetWatcher {
    let error = Arc::new(std::sync::Mutex::new(None));
    let output = error.clone();
    let task = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let d = data.clone();
            let p = dir.clone();
            let result =
                tauri::async_runtime::spawn_blocking(move || check_budget(&d, &p, 0)).await;
            let failure = match result {
                Ok(Err(e)) => Some(e),
                Err(e) => Some(e.to_string()),
                _ => None,
            };
            if let Some(e) = failure {
                if let Ok(mut out) = output.lock() {
                    *out = Some(e);
                }
                cancel.store(true, Ordering::Relaxed);
                break;
            }
        }
    });
    BudgetWatcher { task, error }
}
pub fn inspect(data: &Path, id: &str, settings: &TrialSettings) -> Result<Preflight, String> {
    let project = read_project(data, id)?;
    let dir = directory(data, id)?;
    let mut p = validate(&source(&dir, &project)?, settings);
    p.project_bytes = tree_bytes(&dir)?;
    p.library_bytes = tree_bytes(&data.join("audio-projects"))?;
    p.free_bytes = fs2::available_space(&dir).map_err(|e| e.to_string())?;
    if let Err(e) = budget(
        p.project_bytes,
        p.library_bytes,
        p.free_bytes,
        p.estimated_growth_bytes,
    ) {
        p.issue("storage", None, e);
    }
    Ok(p)
}
pub fn subtitle_bytes(source: &Source) -> Vec<u8> {
    crate::srt::serialize(
        &source
            .cues
            .iter()
            .map(|c| crate::srt::Cue {
                index: c.index,
                start_ms: c.start_ms as i64,
                end_ms: c.end_ms as i64,
                text: c.text.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .into_bytes()
}

pub fn export_bundle(data: &Path, id: &str, key: &str, parent: &Path) -> Result<String, String> {
    let p = load(data, id)?;
    let a = p.last_full.ok_or("Chưa có WAV toàn timeline đã lưu.")?;
    if a.key != key {
        return Err("WAV đã thay đổi; tải lại bản lưu.".into());
    }
    no_link(parent)?;
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    if !parent.is_dir() || parent.starts_with(data.canonicalize().map_err(|e| e.to_string())?) {
        return Err("Không xuất vào dữ liệu nội bộ app.".into());
    }
    let source_dir = directory(data, id)?.join("previews").join(key);
    let target = parent.join(format!("narration-{}", key));
    no_link(&target)?;
    let files = [
        ("preview.wav", "narration.vi.wav"),
        ("report.json", "narration.vi.json"),
        ("subtitles.vi.srt", "subtitles.vi.srt"),
    ];
    let _guard = lock(&parent, &format!(".narration-{key}.lock"))?;
    if target.exists() {
        for (from, to) in files {
            if file_hash(&source_dir.join(from))? != file_hash(&target.join(to))? {
                return Err("Bộ xuất cùng ID đã bị sửa; chọn thư mục khác, không ghi đè.".into());
            }
        }
        return Ok(target.to_string_lossy().into_owned());
    }
    if fs2::available_space(&parent).map_err(|e| e.to_string())?
        < a.duration_ms * RATE as u64 * 2 / 1000 + FREE_RESERVE
    {
        return Err("Không đủ dung lượng nơi xuất WAV.".into());
    }
    let temporary = parent.join(format!(".narration-{}.partial", Uuid::new_v4()));
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    for (from, to) in files {
        let to = temporary.join(to);
        fs::copy(source_dir.join(from), &to).map_err(|e| e.to_string())?;
        File::open(&to)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        if file_hash(&to)? != file_hash(&source_dir.join(from))? {
            return Err("Checksum bộ xuất không khớp.".into());
        }
    }
    File::open(&temporary)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, &target).map_err(|e| e.to_string())?;
    File::open(&parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::super::tests::{input, settings};
    use super::*;
    fn params() -> TrialSettings {
        TrialSettings {
            start_ms: 0,
            duration_ms: 120000,
            full: Some(Options::default()),
            ..settings()
        }
    }
    #[test]
    fn full_validation_keeps_every_cue_and_end_duration() {
        let mut s = input();
        let p = params();
        assert!(validate(&s, &p).issues.is_empty());
        s.cues[0].reviewed = false;
        s.cues[1].end_ms = 121000;
        let result = validate(&s, &p);
        assert_eq!(result.cue_count, 2);
        assert!(result.issues.iter().any(|i| i.code == "unreviewed"));
        assert!(result.issues.iter().any(|i| i.code == "timing"));
    }
    #[test]
    fn coverage_union_replays_and_explicit_partial_are_distinct() {
        let mut s = input();
        s.coverage["ranges"] =
            serde_json::json!([{"startMs":0,"endMs":60000},{"startMs":90000,"endMs":120000}]);
        let mut p = params();
        let result = validate(&s, &p);
        assert_eq!(
            result.gaps,
            vec![Gap {
                start_ms: 60000,
                end_ms: 90000
            }]
        );
        assert!(!result.issues.is_empty());
        p.full.as_mut().unwrap().allow_gaps = true;
        assert!(validate(&s, &p).issues.is_empty());
        assert!(validate(&s, &p).partial);
        s.coverage["ranges"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"startMs":55000,"endMs":95000}));
        assert!(!validate(&s, &p).partial);
        s.coverage["durationMs"] = serde_json::json!(0);
        p.full.as_mut().unwrap().allow_gaps = false;
        assert!(validate(&s, &p)
            .issues
            .iter()
            .any(|i| i.code == "unknown-duration"));
    }
    #[test]
    fn quota_disk_and_invalid_voice_fail_closed() {
        assert!(budget(PROJECT_QUOTA, 0, 20 * FREE_RESERVE, 1).is_err());
        assert!(budget(0, LIBRARY_QUOTA, 20 * FREE_RESERVE, 1).is_err());
        assert!(budget(0, 0, FREE_RESERVE, 1).is_err());
        let mut p = params();
        p.full
            .as_mut()
            .unwrap()
            .cue_voices
            .insert(999, "../../evil".into());
        assert!(validate(&input(), &p)
            .issues
            .iter()
            .any(|i| i.code == "voice"));
    }
    #[test]
    fn malformed_coverage_limits_and_overlaps_never_become_partial_success() {
        let mut s = input();
        let mut p = params();
        p.full.as_mut().unwrap().allow_gaps = true;
        s.coverage["ranges"] = serde_json::json!([{"startMs":2000,"endMs":1000}]);
        assert!(validate(&s, &p).issues.iter().any(|i| i.code == "coverage"));
        s = input();
        s.cues[1].start_ms = 32000;
        assert!(validate(&s, &p).issues.iter().any(|i| i.code == "overlap"));
        p.duration_ms = MAX_END_MS + 1;
        assert!(validate(&s, &p).issues.iter().any(|i| i.code == "duration"));
        p = params();
        p.full
            .as_mut()
            .unwrap()
            .cue_voices
            .insert(1, "thai_son".into());
        assert_eq!(voice_for(&p, &s.cues[0]), "thai_son");
        assert_eq!(voice_for(&p, &s.cues[1]), "thuy_dung");
    }
    #[tokio::test]
    async fn resource_watcher_stops_oversize_engine_output_without_deleting_data() {
        let root = std::env::temp_dir().join(format!("long-tieng-budget-test-{}", Uuid::new_v4()));
        let dir = root.join("audio-projects").join("test");
        let output = dir.join("job-test/output");
        fs::create_dir_all(&output).unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let watcher = watch_budget(root.clone(), dir, flag.clone());
        File::create(output.join("cue.wav"))
            .unwrap()
            .set_len(17 * 1024 * 1024)
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        assert!(flag.load(Ordering::Relaxed));
        assert!(watcher.error().unwrap().contains("16 MiB"));
        assert!(output.join("cue.wav").exists());
        drop(watcher);
        fs::remove_dir_all(root).unwrap();
    }
}
