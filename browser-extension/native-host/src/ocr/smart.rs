use super::{cues::Observation, engine::Candidate};
use crate::validation;
use serde_json::Value;
use std::collections::HashMap;

pub struct GroupRecognition {
    pub candidates: HashMap<u64, Vec<Candidate>>,
    pub calls: usize,
    pub uncertain: bool,
}
pub fn best(candidates: &[Candidate]) -> Option<&Candidate> {
    candidates
        .first()
        .filter(|c| {
            c.variant == "vision-accurate-vi-candidate-1"
                && c.confidence >= 80.
                && c.text.chars().any(char::is_alphanumeric)
        })
        .or_else(|| {
            candidates
                .iter()
                .filter(|c| c.text.chars().any(char::is_alphanumeric))
                .max_by(|a, b| a.confidence.total_cmp(&b.confidence))
        })
}
pub fn recognize_group<F>(
    samples: &[Value],
    sample: &Value,
    previous: &[Observation],
    mut recognize: F,
) -> Result<GroupRecognition, String>
where
    F: FnMut(&Value) -> Result<Vec<Candidate>, String>,
{
    let group = &sample["smart"]["group"];
    let members: Vec<&Value> = if group.is_null() {
        vec![sample]
    } else {
        samples
            .iter()
            .filter(|v| {
                v["smart"]["group"] == *group
                    && v["segment"] == sample["segment"]
                    && v["region"] == sample["region"]
            })
            .collect()
    };
    let selected: Vec<&Value> = members
        .iter()
        .copied()
        .filter(|v| {
            v["smart"]["stable"] == true
                && matches!(v["smart"]["role"].as_str(), Some("stable" | "periodic"))
        })
        .collect();
    let mut result = GroupRecognition {
        candidates: HashMap::new(),
        calls: 0,
        uncertain: selected.is_empty(),
    };
    for observation in previous {
        if members
            .iter()
            .any(|v| v["sequence"] == observation.sequence)
        {
            result
                .candidates
                .insert(observation.sequence, observation.candidates.clone());
        }
    }
    let initial = if selected.is_empty() {
        &members
    } else {
        &selected
    };
    for input in initial {
        let sequence = validation::integer(input, "sequence")?;
        if let std::collections::hash_map::Entry::Vacant(entry) = result.candidates.entry(sequence)
        {
            entry.insert(recognize(input)?);
            result.calls += 1;
        }
    }
    if !selected.is_empty() {
        let reference = best(&result.candidates[&validation::integer(selected[0], "sequence")?]);
        let consistent = reference.is_some_and(|r| r.confidence >= 80.)
            && selected.iter().all(|input| {
                best(&result.candidates[&input["sequence"].as_u64().unwrap()]).is_some_and(|next| {
                    next.confidence >= 80.
                        && Some(next.text.as_str()) == reference.map(|r| r.text.as_str())
                })
            });
        if consistent {
            let candidates =
                result.candidates[&validation::integer(selected[0], "sequence")?].clone();
            for member in members {
                result
                    .candidates
                    .entry(validation::integer(member, "sequence")?)
                    .or_insert_with(|| candidates.clone());
            }
        } else {
            result.uncertain = true;
            for member in members {
                let sequence = validation::integer(member, "sequence")?;
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    result.candidates.entry(sequence)
                {
                    entry.insert(recognize(member)?);
                    result.calls += 1;
                }
            }
        }
    }
    Ok(result)
}
pub fn expand(
    timeline: &[Value],
    images: &[Observation],
) -> Result<(Vec<Observation>, Vec<u64>), String> {
    let mut expanded = Vec::with_capacity(timeline.len());
    let mut groups = Vec::with_capacity(timeline.len());
    for sample in timeline {
        let sequence = validation::integer(sample, "imageSequence")?;
        let mut observation = images
            .get(sequence.checked_sub(1).ok_or("Image sequence 0.")? as usize)
            .filter(|o| o.sequence == sequence)
            .ok_or("OCR observation thieu anh.")?
            .clone();
        observation.time_ms = validation::number(sample, "estimatedVideoTimeMs")?.round() as u64;
        observation.captured_epoch_ms = validation::number(sample, "capturedEpochMs")?;
        observation.uncertainty_ms = validation::number(sample, "uncertaintyMs")?;
        observation.segment = validation::integer(sample, "segment")?;
        groups.push(validation::integer(&sample["smart"], "group")?);
        expanded.push(observation);
    }
    for image in images.iter().filter(|o| o.manual) {
        expanded.push(image.clone());
        groups.push(u64::MAX - image.sequence);
    }
    let mut combined: Vec<_> = expanded.into_iter().zip(groups).collect();
    combined.sort_by_key(|(o, _)| (o.segment, o.time_ms));
    Ok(combined.into_iter().unzip())
}
pub fn uncertain_groups(
    samples: &[Value],
    observations: &[Observation],
) -> std::collections::HashSet<u64> {
    let mut selected: HashMap<u64, Vec<usize>> = HashMap::new();
    for sample in samples {
        let group = sample["smart"]["group"].as_u64().unwrap_or(0);
        let entries = selected.entry(group).or_default();
        if sample["smart"]["stable"] == true
            && matches!(
                sample["smart"]["role"].as_str(),
                Some("stable" | "periodic")
            )
        {
            entries.push(sample["sequence"].as_u64().unwrap() as usize - 1);
        }
    }
    selected
        .into_iter()
        .filter_map(|(group, indices)| {
            let first = indices
                .first()
                .and_then(|i| best(&observations[*i].candidates));
            let valid = first.is_some_and(|c| c.confidence >= 80.)
                && indices.iter().all(|i| {
                    best(&observations[*i].candidates).is_some_and(|c| {
                        c.confidence >= 80.
                            && Some(c.text.as_str()) == first.map(|f| f.text.as_str())
                    })
                });
            (!valid).then_some(group)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn candidate(text: &str) -> Vec<Candidate> {
        vec![Candidate {
            text: text.into(),
            confidence: 95.,
            variant: "vision-accurate-vi-candidate-1".into(),
        }]
    }
    fn samples() -> Vec<Value> {
        (1..=6).map(|i|json!({"sequence":i,"segment":0,"region":{},"smart":{"group":1,"stable":i>=3,"role":if i==3{"stable"}else if i==5{"periodic"}else{"end"}}})).collect()
    }
    #[test]
    fn stable_group_only_recognizes_representatives() {
        let s = samples();
        let r = recognize_group(&s, &s[0], &[], |_| Ok(candidate("Xin chào"))).unwrap();
        assert_eq!(r.calls, 2);
        assert_eq!(r.candidates.len(), 6);
        assert!(!r.uncertain);
    }
    #[test]
    fn disagreement_reads_every_retained_image() {
        let s = samples();
        let r = recognize_group(&s, &s[0], &[], |v| {
            Ok(candidate(if v["sequence"] == 5 {
                "Xin cháo"
            } else {
                "Xin chào"
            }))
        })
        .unwrap();
        assert_eq!(r.calls, 6);
        assert!(r.uncertain);
    }
    #[test]
    fn unknown_and_empty_results_never_propagate_as_verified_silence() {
        let s = samples();
        let r = recognize_group(&s, &s[0], &[], |_| Ok(vec![])).unwrap();
        assert_eq!(r.calls, 6);
        assert!(r.uncertain);
    }
    #[test]
    fn groups_never_reuse_across_seek_or_roi() {
        let mut s = samples();
        s[4]["segment"] = 1.into();
        s[5]["smart"]["group"] = 2.into();
        let r = recognize_group(&s, &s[0], &[], |_| Ok(candidate("Xin chào"))).unwrap();
        assert_eq!(r.candidates.len(), 4);
        assert_eq!(r.calls, 1);
    }
    #[test]
    fn sparse_images_expand_to_continuous_observation_timeline() {
        let image = Observation {
            sequence: 1,
            segment: 0,
            time_ms: 0,
            captured_epoch_ms: 0.,
            uncertainty_ms: 0.,
            manual: false,
            image_hash: "hash".into(),
            text: "Xin chào".into(),
            confidence: 95.,
            candidates: candidate("Xin chào"),
        };
        let timeline:Vec<Value>=(0..30).map(|i|json!({"imageSequence":1,"segment":0,"estimatedVideoTimeMs":i*100,"capturedEpochMs":i*100,"uncertaintyMs":0,"smart":{"group":1}})).collect();
        let (expanded, groups) = expand(&timeline, &[image]).unwrap();
        let cues = super::super::cues::assemble_smart(&expanded, &groups);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms, 0);
        assert_eq!(cues[0].end_ms, 2950);
        assert_eq!(cues[0].evidence_sequences, vec![1, 1, 1]);
    }
    #[test]
    fn manual_evidence_is_preserved_without_creating_timeline_observations() {
        let image = Observation {
            sequence: 1,
            segment: 0,
            time_ms: 1200,
            captured_epoch_ms: 0.,
            uncertainty_ms: 100.,
            manual: true,
            image_hash: "hash".into(),
            text: "Xin chào".into(),
            confidence: 95.,
            candidates: candidate("Xin chào"),
        };
        let (expanded, groups) = expand(&[], &[image]).unwrap();
        assert_eq!(expanded.len(), 1);
        assert!(expanded[0].manual);
        assert_eq!(groups, vec![u64::MAX - 1]);
    }
}
