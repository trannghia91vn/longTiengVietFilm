use super::engine::{normalize, Candidate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub sequence: u64,
    pub segment: u64,
    pub time_ms: u64,
    pub captured_epoch_ms: f64,
    pub uncertainty_ms: f64,
    pub manual: bool,
    pub image_hash: String,
    pub text: String,
    pub confidence: f64,
    pub candidates: Vec<Candidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub confidence: f64,
    pub segment: u64,
    pub evidence_sequences: Vec<u64>,
    pub start_range_ms: [u64; 2],
    pub end_range_ms: [u64; 2],
    pub warnings: Vec<String>,
    pub reviewed: bool,
}
fn key(text: &str) -> String {
    normalize(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn warn(warnings: &mut Vec<String>, warning: &str) {
    if !warnings.iter().any(|w| w == warning) {
        warnings.push(warning.into());
    }
}

pub fn assemble(observations: &[Observation]) -> Vec<Cue> {
    assemble_inner(observations, None)
}
pub fn assemble_smart(observations: &[Observation], groups: &[u64]) -> Vec<Cue> {
    assert_eq!(observations.len(), groups.len());
    assemble_inner(observations, Some(groups))
}
fn assemble_inner(observations: &[Observation], groups: Option<&[u64]>) -> Vec<Cue> {
    let selected: Vec<Observation> = observations
        .iter()
        .map(|o| {
            let mut next = o.clone();
            if let Some(primary) = o.candidates.first().filter(|c| {
                c.variant == "vision-accurate-vi-candidate-1"
                    && c.confidence >= 60.
                    && !c.text.is_empty()
            }) {
                next.text = primary.text.clone();
                next.confidence = primary.confidence;
            }
            next
        })
        .collect();
    let observations = selected.as_slice();
    let mut stable = observations.to_vec();
    // Exact neighboring consensus only; do not fuzzy-merge different Vietnamese dialogue.
    for (i, current) in observations.iter().enumerate() {
        if groups.is_some() {
            continue;
        }
        if current.text.is_empty() || current.manual || current.confidence >= 90. {
            continue;
        }
        let mut votes: BTreeMap<String, (usize, &Observation)> = BTreeMap::new();
        for neighbor in observations
            .iter()
            .take((i + 3).min(observations.len()))
            .skip(i.saturating_sub(2))
        {
            if neighbor.segment != current.segment
                || neighbor.manual
                || neighbor.text.is_empty()
                || neighbor.time_ms.abs_diff(current.time_ms) > 500
            {
                continue;
            }
            let entry = votes.entry(key(&neighbor.text)).or_insert((0, neighbor));
            entry.0 += 1;
            if neighbor.confidence > entry.1.confidence {
                entry.1 = neighbor;
            }
        }
        if let Some((_, (count, best))) = votes.iter().max_by_key(|(_, v)| v.0) {
            if *count >= 3 && best.confidence > current.confidence + 5. && best.confidence >= 60. {
                stable[i].text = best.text.clone();
            }
        }
    }
    let mut cues = vec![];
    let mut start = 0;
    while start < stable.len() {
        if stable[start].text.is_empty() {
            start += 1;
            continue;
        }
        let first = &stable[start];
        let mut end = start + 1;
        while end < stable.len()
            && groups.is_none_or(|g| {
                g[start] == g[end]
                    || stable[end - 1].confidence >= 80. && stable[end].confidence >= 80.
            })
            && !first.manual
            && !stable[end].manual
            && stable[end].segment == first.segment
            && key(&stable[end].text) == key(&first.text)
            && stable[end].time_ms >= stable[end - 1].time_ms.saturating_sub(50)
            && stable[end].time_ms.saturating_sub(stable[end - 1].time_ms) <= 750
        {
            end += 1;
        }
        let group = &stable[start..end];
        let last = group.last().unwrap();
        let best = group
            .iter()
            .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
            .unwrap();
        let before = start.checked_sub(1).map(|i| &stable[i]).filter(|p| {
            p.segment == first.segment
                && !p.manual
                && !first.manual
                && first.time_ms >= p.time_ms
                && first.time_ms - p.time_ms <= 750
        });
        let after = stable.get(end).filter(|p| {
            p.segment == last.segment
                && !p.manual
                && !last.manual
                && p.time_ms >= last.time_ms
                && p.time_ms - last.time_ms <= 750
        });
        let lower_start = before.map(|p| p.time_ms).unwrap_or(first.time_ms);
        let upper_end = after
            .map(|p| p.time_ms)
            .unwrap_or(last.time_ms.saturating_add(100));
        let uncertainty = group
            .iter()
            .map(|p| p.uncertainty_ms.ceil() as u64)
            .max()
            .unwrap_or(0);
        let mut warnings = vec![];
        if group.len() < 2 {
            warn(&mut warnings, "single-observation");
        }
        if best.confidence < 85. {
            warn(&mut warnings, "low-confidence");
        }
        if before.is_none() {
            warn(&mut warnings, "start-unbounded");
        }
        if after.is_none() {
            warn(&mut warnings, "end-unbounded");
        }
        if first.manual {
            warn(&mut warnings, "manual-snapshot-only");
        }
        if groups.is_some_and(|g| (start + 1..end).any(|i| g[i] != g[i - 1])) {
            warn(&mut warnings, "smart-transition-needs-review");
        }
        if group
            .iter()
            .zip(&observations[start..end])
            .any(|(p, original)| p.text != original.text)
        {
            warn(&mut warnings, "temporal-consensus-correction");
        }
        if group.iter().any(|p| {
            p.candidates.len() > 1
                && p.candidates
                    .iter()
                    .filter(|c| !c.text.is_empty())
                    .map(|c| key(&c.text))
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    > 1
        }) {
            warn(&mut warnings, "variant-disagreement");
        }
        let start_ms = (lower_start + first.time_ms) / 2;
        let end_ms = ((last.time_ms + upper_end) / 2).max(start_ms + 1);
        if end_ms - start_ms < 300 {
            warn(&mut warnings, "short-cue");
        }
        if best.text.chars().any(|c| {
            c.is_alphabetic()
                && !c.is_ascii()
                && !('Ạ'..='ỹ').contains(&c)
                && !"ăâđêôơưĂÂĐÊÔƠƯàáãèéìíòóõùúýÀÁÃÈÉÌÍÒÓÕÙÚÝ".contains(c)
        }) {
            warn(&mut warnings, "unexpected-script-needs-review");
        }
        cues.push(Cue {
            index: 0,
            start_ms,
            end_ms,
            text: best.text.clone(),
            confidence: best.confidence,
            segment: first.segment,
            evidence_sequences: vec![first.sequence, best.sequence, last.sequence],
            start_range_ms: [
                lower_start.saturating_sub(uncertainty),
                first.time_ms + uncertainty,
            ],
            end_range_ms: [
                last.time_ms.saturating_sub(uncertainty),
                upper_end + uncertainty,
            ],
            warnings,
            reviewed: false,
        });
        start = end;
    }
    cues.sort_by_key(|c| c.start_ms);
    let mut merged: Vec<Cue> = vec![];
    for cue in cues {
        if let Some(previous) = merged.iter_mut().rev().find(|p| {
            groups.is_none()
                && p.end_ms > cue.start_ms
                && p.start_ms < cue.end_ms
                && p.segment != cue.segment
                && key(&p.text) == key(&cue.text)
                && p.start_ms.abs_diff(cue.start_ms) <= 500
                && p.end_ms.abs_diff(cue.end_ms) <= 500
        }) {
            previous.start_ms = previous.start_ms.min(cue.start_ms);
            previous.end_ms = previous.end_ms.max(cue.end_ms);
            previous.evidence_sequences.extend(cue.evidence_sequences);
            warn(&mut previous.warnings, "overlapping-recapture");
        } else {
            merged.push(cue);
        }
    }
    for i in 0..merged.len() {
        merged[i].index = i + 1;
        for j in (i + 1)..merged.len() {
            if merged[j].start_ms >= merged[i].end_ms {
                break;
            }
            warn(&mut merged[i].warnings, "overlap-needs-review");
            warn(&mut merged[j].warnings, "overlap-needs-review");
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    fn o(sequence: u64, time_ms: u64, text: &str) -> Observation {
        Observation {
            sequence,
            segment: 0,
            time_ms,
            captured_epoch_ms: time_ms as f64,
            uncertainty_ms: 20.,
            manual: false,
            image_hash: "abc".into(),
            text: text.into(),
            confidence: 95.,
            candidates: vec![],
        }
    }
    #[test]
    fn confirms_text_without_delaying_first_evidence_and_preserves_repeated_lines() {
        let cues = assemble(&[
            o(1, 0, ""),
            o(2, 100, "Xin chào"),
            o(3, 200, "Xin chào"),
            o(4, 300, ""),
            o(5, 400, "Xin chào"),
            o(6, 500, "Xin chào"),
            o(7, 600, ""),
        ]);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 50);
        assert_eq!(cues[0].end_ms, 250);
        assert_eq!(cues[1].start_ms, 350);
    }
    #[test]
    fn near_identical_dialogue_is_not_fuzzy_merged() {
        let cues = assemble(&[
            o(1, 100, "Tôi đi."),
            o(2, 200, "Tôi đi."),
            o(3, 300, "Tôi đi!"),
            o(4, 400, "Tôi đi!"),
        ]);
        assert_eq!(cues.len(), 2);
    }
    #[test]
    fn short_cue_and_final_unbounded_end_are_not_dropped_or_invented() {
        let cues = assemble(&[
            o(1, 100, ""),
            o(2, 200, "Ê!"),
            o(3, 300, ""),
            o(4, 500, "Chào"),
        ]);
        assert_eq!(cues.len(), 2);
        assert!(cues[0].warnings.contains(&"single-observation".into()));
        assert!(cues[1].warnings.contains(&"end-unbounded".into()));
        assert_eq!(cues[1].end_ms, 550);
    }
    #[test]
    fn gaps_and_seek_segments_do_not_bridge_a_cue() {
        let mut seek = o(4, 100, "Tạm biệt");
        seek.segment = 2;
        let cues = assemble(&[
            o(1, 100, "Xin chào"),
            o(2, 200, "Xin chào"),
            o(3, 5000, "Xin chào"),
            seek,
        ]);
        assert_eq!(cues.len(), 3);
        assert!(cues
            .iter()
            .any(|c| c.warnings.contains(&"overlap-needs-review".into())));
    }
    #[test]
    fn nfc_and_multiline_text_are_preserved() {
        let cues = assemble(&[
            o(1, 100, "cha\u{0300}o\nTakemichi"),
            o(2, 200, "chào\nTakemichi"),
        ]);
        assert_eq!(cues.len(), 1);
    }
}
