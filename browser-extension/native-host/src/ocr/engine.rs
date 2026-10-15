use crate::storage::{atomic, no_symlink};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Cursor,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use unicode_normalization::UnicodeNormalization;

pub const MODEL_ID: &str = "ocr-vie-fast";
pub const MODEL_HASH: &str = "79df64caf7bcfb2a27df5042ecb6121e196eada34da774956995747636d5bfa1";
pub const PIPELINE_VERSION: u32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub text: String,
    pub confidence: f64,
    pub variant: String,
}
#[derive(Debug)]
pub struct Engine {
    binary: PathBuf,
    models: PathBuf,
    pub fingerprint: String,
    pub version: String,
    vision: Option<PathBuf>,
    pub vision_system: Option<serde_json::Value>,
}

struct ManagedChild(Child);
impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn run(
    command: &mut Command,
    scratch: &Path,
    cancelled: impl Fn() -> bool,
    timeout: Duration,
) -> Result<String, String> {
    let stdout = scratch.join("stdout.txt");
    let stderr = scratch.join("stderr.txt");
    for path in [&stdout, &stderr] {
        no_symlink(path)?;
    }
    let mut child = ManagedChild(
        command
            .stdin(Stdio::null())
            .stdout(File::create(&stdout).map_err(|e| e.to_string())?)
            .stderr(File::create(&stderr).map_err(|e| e.to_string())?)
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let start = Instant::now();
    loop {
        if cancelled() {
            return Err("OCR da huy; checkpoint duoc giu de tiep tuc.".into());
        }
        if start.elapsed() > timeout {
            return Err("OCR engine timeout.".into());
        }
        if fs::metadata(&stdout).map(|m| m.len()).unwrap_or(0) > 8 * 1024 * 1024
            || fs::metadata(&stderr).map(|m| m.len()).unwrap_or(0) > 2 * 1024 * 1024
        {
            return Err("OCR engine output qua lon.".into());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!(
                    "OCR engine failed: {}",
                    fs::read_to_string(&stderr)
                        .unwrap_or_default()
                        .chars()
                        .take(1000)
                        .collect::<String>()
                ));
            }
            return fs::read_to_string(&stdout).map_err(|e| e.to_string());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub fn normalize(text: &str) -> String {
    text.nfc()
        .collect::<String>()
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn parse_tsv(text: &str, variant: &str) -> Result<Candidate, String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .quoting(false)
        .from_reader(text.as_bytes());
    let mut lines: BTreeMap<(u32, u32, u32), Vec<String>> = BTreeMap::new();
    let mut score = 0.;
    let mut weight = 0.;
    for row in reader.records() {
        let row = row.map_err(|e| e.to_string())?;
        if row.len() != 12 {
            return Err("TSV OCR sai schema.".into());
        }
        if &row[0] != "5" {
            continue;
        }
        let confidence = row[10].parse::<f64>().map_err(|_| "OCR confidence sai.")?;
        if !confidence.is_finite() || !(0. ..=100.).contains(&confidence) {
            return Err("OCR confidence sai.".into());
        }
        let word = row[11].trim();
        if word.is_empty() {
            continue;
        }
        let key = (
            row[2].parse().map_err(|_| "block sai")?,
            row[3].parse().map_err(|_| "paragraph sai")?,
            row[4].parse().map_err(|_| "line sai")?,
        );
        lines.entry(key).or_default().push(word.into());
        let size = word.chars().count() as f64;
        score += confidence * size;
        weight += size;
    }
    let text = normalize(
        &lines
            .values()
            .map(|words| words.join(" "))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if text.len() > 8192 {
        return Err("OCR text qua dai.".into());
    }
    Ok(Candidate {
        text,
        confidence: if weight == 0. { 0. } else { score / weight },
        variant: variant.into(),
    })
}
impl Engine {
    pub fn resolve(data: &Path, scratch: &Path) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let installed = executable
            .parent()
            .ok_or("Thieu executable parent.")?
            .join("tesseract");
        let resources = executable
            .parent()
            .ok_or("Thieu executable parent")?
            .join("../Resources/engines");
        let adjacent = executable
            .parent()
            .ok_or("Thieu executable parent")?
            .join("engines");
        let binary = if installed.exists() {
            installed
        } else if resources.join("tesseract").is_file() {
            resources.join("tesseract")
        } else if adjacent.join("tesseract").is_file() {
            adjacent.join("tesseract")
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/engines/tesseract")
        };
        let models = data.join("models").join(MODEL_ID);
        for path in [&binary, &models, &models.join("vie.traineddata")] {
            no_symlink(path)?;
        }
        let bytes = fs::read(models.join("vie.traineddata")).map_err(|_|"Thieu model OCR vie; tai OCR Vietnamese Fast trong Model Manager hoac chay ocr:prepare va native:register.".to_string())?;
        if hex::encode(Sha256::digest(&bytes)) != MODEL_HASH {
            return Err("Checksum model OCR vie sai; tai/cai lai model.".into());
        }
        let binary_hash = hex::encode(Sha256::digest(fs::read(&binary).map_err(|_| {
            "Thieu engine tesseract; chay npm run ocr:prepare va dang ky helper lai.".to_string()
        })?));
        let version = run(
            Command::new(&binary).arg("--version"),
            scratch,
            || false,
            Duration::from_secs(5),
        )?
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
        if !version.starts_with("tesseract 5.") {
            return Err("Can Tesseract 5 LSTM.".into());
        }
        let mut identity = format!("{PIPELINE_VERSION}:{MODEL_HASH}:{binary_hash}");
        let installed_vision = executable
            .parent()
            .ok_or("Thieu parent")?
            .join("vision-ocr");
        let vision_path = if installed_vision.exists() {
            installed_vision
        } else if resources.join("vision-ocr").is_file() {
            resources.join("vision-ocr")
        } else if adjacent.join("vision-ocr").is_file() {
            adjacent.join("vision-ocr")
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/engines/vision-ocr")
        };
        no_symlink(&vision_path)?;
        let probe = if vision_path.is_file() {
            run(
                Command::new(&vision_path).arg("--probe"),
                scratch,
                || false,
                Duration::from_secs(5),
            )
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .filter(|p| p["language"].as_str().is_some_and(|l| l.starts_with("vi-")))
        } else {
            None
        };
        let vision = if probe.is_some() {
            identity.push_str(&format!(
                ":{}:{}",
                hex::encode(Sha256::digest(
                    fs::read(&vision_path).map_err(|e| e.to_string())?
                )),
                serde_json::to_string(&probe).map_err(|e| e.to_string())?
            ));
            Some(vision_path.canonicalize().map_err(|e| e.to_string())?)
        } else {
            None
        };
        let fingerprint = hex::encode(Sha256::digest(identity));
        let version = if vision.is_some() {
            format!("Apple Vision accurate + fallback {version}")
        } else {
            version
        };
        Ok(Self {
            binary: binary.canonicalize().map_err(|e| e.to_string())?,
            models: models.canonicalize().map_err(|e| e.to_string())?,
            fingerprint,
            version,
            vision,
            vision_system: probe,
        })
    }
    pub fn recognize(
        &self,
        image: &Path,
        scratch: &Path,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<Candidate>, String> {
        no_symlink(image)?;
        let image = image.canonicalize().map_err(|e| e.to_string())?;
        let mut candidates = vec![];
        if let Some(vision) = &self.vision {
            let result = run(
                Command::new(vision).arg(&image),
                scratch,
                &cancelled,
                Duration::from_secs(20),
            )?;
            candidates =
                serde_json::from_str::<Vec<Candidate>>(&result).map_err(|e| e.to_string())?;
            if candidates.len() != 3
                || candidates.iter().any(|c| {
                    !c.confidence.is_finite()
                        || !(0. ..=100.).contains(&c.confidence)
                        || c.text.len() > 8192
                })
            {
                return Err("Vision OCR output khong hop le.".into());
            }
            for candidate in &mut candidates {
                candidate.text = normalize(&candidate.text);
            }
            // No detected text is evidence of absence, not a reason to OCR background again.
            if candidates[0].text.is_empty() || candidates[0].confidence >= 80. {
                return Ok(candidates);
            }
        }
        let mask = scratch.join("outlined.png");
        let focused = scratch.join("focused.png");
        let (mask_bytes, focused_bytes) = prepare(&fs::read(&image).map_err(|e| e.to_string())?)?;
        atomic(&mask, &mask_bytes)?;
        atomic(&focused, &focused_bytes)?;
        let mask = mask.canonicalize().map_err(|e| e.to_string())?;
        let focused = focused.canonicalize().map_err(|e| e.to_string())?;
        for (input, variant) in [
            (mask.as_path(), "outlined-2x"),
            (focused.as_path(), "focused-original-2x"),
        ] {
            let tsv = run(
                Command::new(&self.binary)
                    .arg(input)
                    .arg("stdout")
                    .arg("--tessdata-dir")
                    .arg(&self.models)
                    .args([
                        "-l",
                        "vie",
                        "--oem",
                        "1",
                        "--psm",
                        "6",
                        "-c",
                        "tessedit_create_tsv=1",
                        "-c",
                        "preserve_interword_spaces=0",
                    ])
                    .env("OMP_THREAD_LIMIT", "1"),
                scratch,
                &cancelled,
                Duration::from_secs(20),
            )?;
            candidates.push(parse_tsv(&tsv, variant)?);
        }
        Ok(candidates)
    }
}

// Isolate bright glyph interiors with a nearby dark outline; preserve accents and scale 2x.
pub fn prepare(image: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(Cursor::new(image));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    if reader.info().width > 1920
        || reader.info().height > 1080
        || reader.output_buffer_size() > 1920 * 1080 * 8
    {
        return Err("PNG OCR qua lon.".into());
    }
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes).map_err(|e| e.to_string())?;
    let w = info.width as usize;
    let h = info.height as usize;
    let channels = info.color_type.samples();
    let mut gray = vec![0u8; w * h];
    for (i, pixel) in bytes[..info.buffer_size()]
        .chunks_exact(channels)
        .enumerate()
    {
        gray[i] = if channels < 3 {
            pixel[0]
        } else {
            ((pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8) as u8
        };
    }
    let mut mask = vec![255; w * h];
    for y in 0..h {
        for x in 0..w {
            if gray[y * w + x] < 130 {
                continue;
            }
            let dark = (y.saturating_sub(3)..=(y + 3).min(h - 1)).any(|dy| {
                (x.saturating_sub(3)..=(x + 3).min(w - 1)).any(|dx| gray[dy * w + dx] < 85)
            });
            if dark {
                mask[y * w + x] = 0;
            }
        }
    }
    let region = text_region(&mask, w, h);
    let (left, top, right, bottom) = region.unwrap_or((0, 0, w, h));
    let pad = 12;
    let out_w = (right - left) * 2 + pad * 2;
    let out_h = (bottom - top) * 2 + pad * 2;
    let mut output = vec![255; out_w * out_h];
    let mut focused = output.clone();
    if region.is_some() {
        for y in top..bottom {
            for x in left..right {
                for oy in 0..2 {
                    for ox in 0..2 {
                        let index = ((y - top) * 2 + pad + oy) * out_w + (x - left) * 2 + pad + ox;
                        output[index] = mask[y * w + x];
                        focused[index] = gray[y * w + x];
                    }
                }
            }
        }
    }
    Ok((
        encode_gray(out_w, out_h, &output)?,
        encode_gray(out_w, out_h, &focused)?,
    ))
}
fn encode_gray(w: usize, h: usize, pixels: &[u8]) -> Result<Vec<u8>, String> {
    let mut png_bytes = vec![];
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w as u32, h as u32);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|e| e.to_string())?
            .write_image_data(pixels)
            .map_err(|e| e.to_string())?;
    }
    Ok(png_bytes)
}
fn text_region(mask: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let mut seen = vec![false; mask.len()];
    let mut glyphs = vec![];
    for seed in 0..mask.len() {
        if seen[seed] || mask[seed] != 0 {
            continue;
        }
        let mut queue = vec![seed];
        seen[seed] = true;
        let (mut left, mut right, mut top, mut bottom) = (seed % w, seed % w, seed / w, seed / w);
        let mut count = 0;
        while let Some(i) = queue.pop() {
            let x = i % w;
            let y = i / w;
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
            count += 1;
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let j = ny * w + nx;
                    if !seen[j] && mask[j] == 0 {
                        seen[j] = true;
                        queue.push(j);
                    }
                }
            }
        }
        let gh = bottom - top + 1;
        let gw = right - left + 1;
        if (6..=60).contains(&gh) && gw <= 60 && count >= 8 {
            glyphs.push((left, top, right + 1, bottom + 1));
        }
    }
    let mut bands = vec![];
    for glyph in &glyphs {
        let cy = (glyph.1 + glyph.3) / 2;
        let aligned: Vec<_> = glyphs
            .iter()
            .filter(|g| ((g.1 + g.3) / 2).abs_diff(cy) <= 8)
            .collect();
        if aligned.len() < 2 {
            continue;
        }
        let left = aligned.iter().map(|g| g.0).min()?;
        let right = aligned.iter().map(|g| g.2).max()?;
        let top = aligned.iter().map(|g| g.1).min()?;
        let bottom = aligned.iter().map(|g| g.3).max()?;
        if ((left + right) / 2).abs_diff(w / 2) > w / 4 || bottom - top > 70 {
            continue;
        }
        bands.push((aligned.len(), left, top, right, bottom));
    }
    let best = bands.iter().max_by_key(|b| b.0)?;
    let mut region = (best.1, best.2, best.3, best.4);
    // Preserve a second centered subtitle line close to the strongest glyph band.
    for band in &bands {
        if band.0 >= 4 && band.2.abs_diff(best.2) <= 55 {
            region = (
                region.0.min(band.1),
                region.1.min(band.2),
                region.2.max(band.3),
                region.3.max(band.4),
            );
        }
    }
    Some((
        region.0.saturating_sub(8),
        region.1.saturating_sub(8),
        (region.2 + 8).min(w),
        (region.3 + 8).min(h),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires prepared native OCR engine and OCR_BENCHMARK_IMAGE"]
    fn benchmark_real_crop() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let engine = Engine {
            binary: root.join("../../src-tauri/engines/tesseract"),
            models: root.join(std::env::var("OCR_BENCHMARK_MODEL").unwrap_or("runtime".into())),
            fingerprint: "benchmark".into(),
            version: "5.5.0".into(),
            vision: if std::env::var_os("OCR_BENCHMARK_VISION").is_some() {
                Some(root.join("../../src-tauri/engines/vision-ocr"))
            } else {
                None
            },
            vision_system: None,
        };
        let scratch = tempfile::tempdir().unwrap();
        let image = std::env::var("OCR_BENCHMARK_IMAGE").expect("Set OCR_BENCHMARK_IMAGE");
        let start = Instant::now();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &engine
                    .recognize(Path::new(&image), scratch.path(), || false)
                    .unwrap()
            )
            .unwrap()
        );
        println!("elapsedMs={}", start.elapsed().as_millis());
        if let Ok(destination) = std::env::var("OCR_BENCHMARK_MASK") {
            fs::copy(scratch.path().join("outlined.png"), destination).unwrap();
        }
    }
    #[test]
    fn tsv_preserves_lines_and_unicode_without_rewriting_dialogue() {
        let tsv="level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t0\t0\t5\t5\t95\tXin\n5\t1\t1\t1\t1\t2\t0\t0\t5\t5\t85\tcha\u{0300}o\n5\t1\t1\t1\t2\t1\t0\t0\t5\t5\t90\tTakemichi\n";
        assert_eq!(
            parse_tsv(tsv, "original").unwrap().text,
            "Xin chào\nTakemichi"
        );
        assert!(parse_tsv(&tsv.replace("95", "NaN"), "original").is_err());
    }
    #[test]
    fn missing_corrupt_model_and_timeout_fail_closed() {
        let t = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        assert!(Engine::resolve(t.path(), scratch.path())
            .unwrap_err()
            .contains("Thieu model"));
        let models = t.path().join("models").join(MODEL_ID);
        fs::create_dir_all(&models).unwrap();
        fs::write(models.join("vie.traineddata"), b"bad").unwrap();
        assert!(Engine::resolve(t.path(), scratch.path())
            .unwrap_err()
            .contains("Checksum"));
        #[cfg(unix)]
        {
            assert!(run(
                Command::new("/bin/sleep").arg("2"),
                scratch.path(),
                || false,
                Duration::from_millis(20)
            )
            .unwrap_err()
            .contains("timeout"));
            assert!(run(
                Command::new("/bin/sleep").arg("2"),
                scratch.path(),
                || true,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .contains("huy"));
        }
    }
}
