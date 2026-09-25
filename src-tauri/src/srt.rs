use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub index: usize,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleDocument {
    pub path: String,
    pub language: String,
    pub cues: Vec<Cue>,
}

fn parse_time(value: &str) -> Result<i64, String> {
    let normalized = value.trim().replace('.', ",");
    let (clock, millis) = normalized
        .rsplit_once(',')
        .ok_or_else(|| format!("Timestamp không hợp lệ: {value}"))?;
    let parts: Vec<i64> = clock
        .split(':')
        .map(|part| {
            part.parse::<i64>()
                .map_err(|_| format!("Timestamp không hợp lệ: {value}"))
        })
        .collect::<Result<_, _>>()?;
    if parts.len() != 3 {
        return Err(format!("Timestamp không hợp lệ: {value}"));
    }
    let millis = format!("{millis:0<3}")[..3]
        .parse::<i64>()
        .map_err(|_| format!("Timestamp không hợp lệ: {value}"))?;
    Ok(((parts[0] * 3600 + parts[1] * 60 + parts[2]) * 1000) + millis)
}

fn format_time(ms: i64) -> String {
    let value = ms.max(0);
    let hours = value / 3_600_000;
    let minutes = (value % 3_600_000) / 60_000;
    let seconds = (value % 60_000) / 1000;
    let millis = value % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

pub fn parse(content: &str) -> Result<Vec<Cue>, String> {
    let normalized = content
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut cues = Vec::new();
    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() < 2 {
            continue;
        }
        let (index, timing_index) = if lines[0].contains("-->") {
            (cues.len() + 1, 0)
        } else {
            (
                lines[0].trim().parse::<usize>().unwrap_or(cues.len() + 1),
                1,
            )
        };
        let timing = lines
            .get(timing_index)
            .ok_or_else(|| "Thiếu timestamp trong SRT".to_string())?;
        let (start, end) = timing
            .split_once("-->")
            .ok_or_else(|| format!("Dòng timestamp không hợp lệ: {timing}"))?;
        let start_ms = parse_time(start)?;
        let end_value = end.split_whitespace().next().unwrap_or(end);
        let end_ms = parse_time(end_value)?;
        if end_ms <= start_ms {
            return Err(format!("Cue {index} có thời gian kết thúc không hợp lệ"));
        }
        let text = lines[(timing_index + 1)..].join("\n").trim().to_string();
        if !text.is_empty() {
            cues.push(Cue {
                index,
                start_ms,
                end_ms,
                text,
            });
        }
    }
    if cues.is_empty() {
        Err("Không tìm thấy cue hợp lệ trong file SRT".to_string())
    } else {
        Ok(cues)
    }
}

pub fn serialize(cues: &[Cue]) -> String {
    cues.iter()
        .enumerate()
        .map(|(position, cue)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                position + 1,
                format_time(cue.start_ms),
                format_time(cue.end_ms),
                cue.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn read(path: &Path, language: &str) -> Result<SubtitleDocument, String> {
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(SubtitleDocument {
        path: path.to_string_lossy().into_owned(),
        language: language.to_string(),
        cues: parse(&content)?,
    })
}

pub fn write(path: &Path, cues: &[Cue]) -> Result<(), String> {
    std::fs::write(path, serialize(cues)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bom_multiline_and_dot_milliseconds() {
        let input = "\u{feff}1\r\n00:00:01,200 --> 00:00:03,400\r\nHello\r\nworld\r\n\r\n2\r\n00:00:04.000 --> 00:00:05.100\r\nBye";
        let cues = parse(input).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 1200);
        assert_eq!(cues[0].text, "Hello\nworld");
        assert_eq!(cues[1].end_ms, 5100);
    }

    #[test]
    fn round_trips_cues() {
        let cues = vec![Cue {
            index: 1,
            start_ms: 1234,
            end_ms: 5678,
            text: "Xin chào".into(),
        }];
        assert_eq!(parse(&serialize(&cues)).unwrap()[0].text, "Xin chào");
    }
}
