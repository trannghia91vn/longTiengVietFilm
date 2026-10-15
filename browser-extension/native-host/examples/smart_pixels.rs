// QA-only streaming decoder. Reads originals without loading/recheckpointing a Session.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, Write},
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(std::env::args().nth(1).ok_or("Missing session directory")?);
    let mut output = std::io::BufWriter::new(std::io::stdout().lock());
    for line in std::io::BufReader::new(fs::File::open(directory.join("journal.ndjson"))?).lines() {
        let record: Value = serde_json::from_str(&line?)?;
        if record["operation"] != "appendSample" {
            continue;
        }
        let metadata: Value =
            serde_json::from_str(record["payloadJson"].as_str().ok_or("Missing payload")?)?;
        let bytes = fs::read(directory.join("frames").join(format!(
            "{:08}.png",
            metadata["sequence"].as_u64().ok_or("Missing sequence")?
        )))?;
        let mut decoder = png::Decoder::new(bytes.as_slice());
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels)?;
        let channels = info.color_type.samples();
        let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
        for p in pixels[..info.buffer_size()].chunks_exact(channels) {
            if channels < 3 {
                rgba.extend_from_slice(&[p[0], p[0], p[0], if channels == 2 { p[1] } else { 255 }]);
            } else {
                rgba.extend_from_slice(&[p[0], p[1], p[2], if channels == 4 { p[3] } else { 255 }]);
            }
        }
        serde_json::to_writer(
            &mut output,
            &json!({"metadata":metadata,"imageBytes":bytes.len(),"width":info.width,"height":info.height,"rgba":STANDARD.encode(rgba)}),
        )?;
        output.write_all(b"\n")?;
    }
    output.flush()?;
    Ok(())
}
