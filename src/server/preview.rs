use std::fmt::Display;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamState {
    Offline,
    Preparing,
    PreviewReady,
    PreviewFailed,
    Live,
}

impl Display for StreamState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                StreamState::Offline => "Offline",
                StreamState::Preparing => "Preparing...",
                StreamState::PreviewFailed => "Preview unavailable",
                _ => "",
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct StreamStatus {
    pub state: StreamState,
}

pub fn create_preview_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn valid_preview_file_name(name: &str) -> bool {
    name == "index.m3u8"
        || name
            .strip_prefix("segment_")
            .and_then(|name| name.strip_suffix(".ts"))
            .is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_files_are_strictly_allowlisted() {
        assert!(valid_preview_file_name("index.m3u8"));
        assert!(valid_preview_file_name("segment_000123.ts"));
        assert!(!valid_preview_file_name("../config.sqlite3"));
        assert!(!valid_preview_file_name("segment_.ts"));
        assert!(!valid_preview_file_name("segment_1.m3u8"));
    }
}
