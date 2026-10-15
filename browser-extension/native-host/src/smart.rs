use crate::validation;
use serde_json::Value;

pub const CAPABILITY: &str = "smart-observations-v1";
pub fn metadata(value: &Value) -> Result<(), String> {
    let smart = &value["smart"];
    if validation::integer(smart, "group")? == 0
        || !smart["stable"].is_boolean()
        || !smart["save"].is_boolean()
        || !matches!(
            smart["role"].as_str(),
            Some("start" | "change" | "stable" | "periodic" | "end" | "fallback")
        )
    {
        return Err("Smart metadata khong hop le.".into());
    }
    if smart["role"] == "fallback" && smart["stable"] == true {
        return Err("Fallback khong duoc stable.".into());
    }
    Ok(())
}
pub fn validate_batch(
    payload: &Value,
    manifest: &Value,
    images: &[Value],
    count: u64,
    previous: Option<&Value>,
) -> Result<Vec<Value>, String> {
    if manifest["schemaVersion"] != 2 || payload["sessionId"] != manifest["id"] {
        return Err("Observation sai schema/session.".into());
    }
    let values = payload["observations"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 5)
        .ok_or("Observation batch phai co 1..5 mau.")?;
    let mut last = previous;
    for (offset, observation) in values.iter().enumerate() {
        if validation::integer(observation, "observationSeq")? != count + offset as u64 + 1
            || count + offset as u64 >= 100_000
        {
            return Err("Observation sequence/limit sai.".into());
        }
        let sequence = validation::integer(observation, "imageSequence")?;
        if validation::number(observation, "estimatedVideoTimeMs")? > 3_600_000_000. {
            return Err("Observation timestamp qua lon.".into());
        }
        let image = sequence
            .checked_sub(1)
            .and_then(|n| images.get(n as usize))
            .ok_or("Observation thieu anh da commit.")?;
        let mut sample = observation.clone();
        sample["sequence"] = sequence.into();
        validation::sample_metadata(
            &sample,
            manifest["id"].as_str().ok_or("Thieu id.")?,
            &manifest["source"],
        )?;
        metadata(observation)?;
        if observation["manual"] != false
            || image["manual"] == true
            || observation["clock"]["paused"] != false
            || observation["clock"]["ended"] != false
        {
            return Err("Observation khong phai playback.".into());
        }
        for field in ["segment", "region", "captureSize"] {
            if image[field] != observation[field] {
                return Err("Observation sai segment/ROI/size.".into());
            }
        }
        if image["smart"]["group"] != observation["smart"]["group"]
            || image["clock"]["layout"] != observation["clock"]["layout"]
            || image["clock"]["sourceKey"] != observation["clock"]["sourceKey"]
            || validation::number(image, "estimatedVideoTimeMs")?
                > validation::number(observation, "estimatedVideoTimeMs")? + 50.
        {
            return Err("Observation sai group/source/image time.".into());
        }
        if observation["smart"]["stable"] == true && image["smart"]["stable"] != true {
            return Err("Stable observation thieu stable evidence.".into());
        }
        if let Some(old) = last {
            let segment = validation::integer(observation, "segment")?;
            let old_segment = validation::integer(old, "segment")?;
            let group = validation::integer(&observation["smart"], "group")?;
            let old_group = validation::integer(&old["smart"], "group")?;
            if segment < old_segment
                || group < old_group
                || (group == old_group
                    && (segment != old_segment
                        || old["region"] != observation["region"]
                        || old["captureSize"] != observation["captureSize"]))
                || (segment == old_segment
                    && validation::number(observation, "estimatedVideoTimeMs")?
                        < validation::number(old, "estimatedVideoTimeMs")? - 50.)
                || old["clock"]["sourceKey"] != observation["clock"]["sourceKey"]
            {
                return Err("Observation timeline/group khong lien tuc.".into());
            }
        }
        last = Some(observation);
    }
    Ok(values.clone())
}

pub fn observations(directory: &std::path::Path) -> Result<Vec<Value>, String> {
    use std::io::BufRead;
    let path = directory.join("journal.ndjson");
    crate::storage::no_symlink(&path)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut result = vec![];
    for line in std::io::BufReader::new(file).lines() {
        let record: Value =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if record["operation"] == "appendObservations" {
            let payload: Value = serde_json::from_str(
                record["payloadJson"]
                    .as_str()
                    .ok_or("Thieu observation payload.")?,
            )
            .map_err(|e| e.to_string())?;
            result.extend(
                payload["observations"]
                    .as_array()
                    .ok_or("Thieu observation array.")?
                    .iter()
                    .cloned(),
            );
        }
    }
    Ok(result)
}
