use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

pub const VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024 - 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub request_id: String,
    pub operation: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub message_seq: Option<u64>,
    #[serde(default)]
    pub payload_json: Option<String>,
    #[serde(default)]
    pub image_base64: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub version: u32,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn checksum(
    operation: &str,
    session_id: &str,
    sequence: u64,
    payload: &str,
    image: &[u8],
) -> String {
    let mut digest = Sha256::new();
    digest.update(format!("{operation}\n{session_id}\n{sequence}\n{payload}"));
    digest.update(image);
    hex::encode(digest.finalize())
}

pub fn read_frame(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0; 4];
    if reader.read(&mut length[..1])? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut length[1..])?;
    let length = u32::from_ne_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native message too large or empty",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}

pub fn write_frame(writer: &mut impl Write, response: &Response) -> io::Result<()> {
    let bytes = serde_json::to_vec(response)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Native response too large",
        ));
    }
    writer.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_oversize_truncated_and_zero_frames() {
        for length in [0, MAX_MESSAGE_BYTES as u32 + 1] {
            assert!(read_frame(&mut length.to_ne_bytes().as_slice()).is_err());
        }
        assert!(read_frame(&mut [1, 0].as_slice()).is_err());
        assert!(read_frame(&mut [5, 0, 0, 0, 1].as_slice()).is_err());
        assert!(read_frame(&mut [].as_slice()).unwrap().is_none());
    }
    #[test]
    fn roundtrip_response_length_is_utf8_bytes() {
        let response = Response {
            version: VERSION,
            request_id: "test".into(),
            ok: false,
            value: None,
            error: Some("Khong luu duoc anh".into()),
        };
        let mut bytes = vec![];
        write_frame(&mut bytes, &response).unwrap();
        let body = read_frame(&mut bytes.as_slice()).unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["ok"],
            false
        );
    }
}
