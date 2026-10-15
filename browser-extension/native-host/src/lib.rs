pub mod ocr;
pub mod protocol;
pub mod review;
pub mod smart;
pub mod storage;
pub mod validation;

use std::path::PathBuf;

pub const HOST_NAME: &str = "com.basushi.longtieng.capture";
pub const APP_IDENTIFIER: &str = "com.basushi.longtieng";

pub fn app_data_dir() -> Result<PathBuf, String> {
    dirs::data_dir()
        .map(|root| root.join(APP_IDENTIFIER))
        .ok_or_else(|| "Khong resolve duoc app data.".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn identifier_matches_tauri() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../../src-tauri/tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], super::APP_IDENTIFIER);
    }
}
