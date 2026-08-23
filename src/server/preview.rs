use anyhow::{Result, bail};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Child;
use tokio::sync::Mutex;

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

struct StagedStream {
    session_id: u64,
    stream_key: String,
    preview_process: Child,
    preview_failed: bool,
    published: bool,
}

/// Manages the local HLS preview generation, lifecycle, and preview file serving.
pub struct HlsPreviewManager {
    preview_dir: PathBuf,
    staged_stream: Mutex<Option<StagedStream>>,
    next_session_id: AtomicU64,
}

impl HlsPreviewManager {
    pub fn new() -> Result<Self> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let preview_dir =
            std::env::temp_dir().join(format!("rtmp-manager-hls-{}-{unique}", std::process::id()));
        create_preview_dir(&preview_dir)?;
        Ok(Self {
            preview_dir,
            staged_stream: Mutex::new(None),
            next_session_id: AtomicU64::new(1),
        })
    }

    /// Spawns an FFmpeg HLS preview process for a newly published ingest stream key.
    pub async fn stage_stream(&self, listen_port: u16, stream_key: String) -> Result<u64> {
        self.end_current_stream().await;
        create_preview_dir(&self.preview_dir)?;

        let source_url = format!("rtmp://127.0.0.1:{listen_port}/live/{stream_key}");
        let playlist = self.preview_dir.join("index.m3u8");
        let segments = self.preview_dir.join("segment_%06d.ts");
        let playlist = playlist.to_string_lossy().into_owned();
        let segments = segments.to_string_lossy().into_owned();

        let preview_process = tokio::process::Command::new("ffmpeg")
            .args([
                "-loglevel",
                "warning",
                "-i",
                &source_url,
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-tune",
                "zerolatency",
                "-g",
                "60",
                "-keyint_min",
                "60",
                "-sc_threshold",
                "0",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-f",
                "hls",
                "-hls_time",
                "2",
                "-hls_list_size",
                "6",
                "-hls_flags",
                "delete_segments+append_list+omit_endlist+independent_segments",
                "-hls_segment_filename",
                &segments,
                &playlist,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        let session_id = self.next_session_id.fetch_add(1, Ordering::Relaxed);
        *self.staged_stream.lock().await = Some(StagedStream {
            session_id,
            stream_key,
            preview_process,
            preview_failed: false,
            published: false,
        });

        tracing::info!("Stream staged with local HLS preview");
        Ok(session_id)
    }

    /// Marks the staged stream as published, returning its session ID and stream key.
    pub async fn begin_publishing(&self) -> Result<(u64, String)> {
        let mut staged = self.staged_stream.lock().await;
        let stream = staged
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No stream is currently staged"))?;
        if stream.published {
            bail!("The staged stream is already published");
        }
        stream.published = true;
        Ok((stream.session_id, stream.stream_key.clone()))
    }

    /// Checks if a session is still active and published.
    pub async fn is_session_published(&self, session_id: u64) -> bool {
        let staged = self.staged_stream.lock().await;
        staged
            .as_ref()
            .is_some_and(|stream| stream.session_id == session_id && stream.published)
    }

    /// Stops external publishing on the staged stream while keeping preview active.
    pub async fn stop_publishing(&self) -> Result<String> {
        let mut staged = self.staged_stream.lock().await;
        let stream = staged
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No stream is currently staged"))?;
        stream.published = false;
        Ok(stream.stream_key.clone())
    }

    /// Ends the currently staged preview and cleans up preview files.
    pub async fn end_current_stream(&self) -> Option<String> {
        let stream = self.staged_stream.lock().await.take();
        let stream_key = if let Some(mut stream) = stream {
            let _ = stream.preview_process.kill().await;
            Some(stream.stream_key)
        } else {
            None
        };

        if let Err(error) = tokio::fs::remove_dir_all(&self.preview_dir).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "Failed to remove HLS preview files");
        }

        stream_key
    }

    /// Checks if the stream key matches current stream, and if so, ends it.
    pub async fn end_if_current(&self, stream_key: &str) -> bool {
        let is_current = self
            .staged_stream
            .lock()
            .await
            .as_ref()
            .is_some_and(|stream| stream.stream_key == stream_key);
        if is_current {
            self.end_current_stream().await;
            true
        } else {
            false
        }
    }

    /// Inspects the current stream and preview status.
    pub async fn status(&self) -> StreamStatus {
        let mut staged = self.staged_stream.lock().await;
        if let Some(stream) = staged.as_mut() {
            match stream.preview_process.try_wait() {
                Ok(Some(status)) if !stream.preview_failed => {
                    tracing::error!(%status, "HLS preview process stopped unexpectedly");
                    stream.preview_failed = true;
                }
                Err(error) if !stream.preview_failed => {
                    tracing::error!(%error, "Failed to inspect HLS preview process");
                    stream.preview_failed = true;
                }
                _ => {}
            }
        }
        let state = match staged.as_ref() {
            None => StreamState::Offline,
            Some(stream) if stream.published => StreamState::Live,
            Some(stream) if stream.preview_failed => StreamState::PreviewFailed,
            Some(_) if self.preview_dir.join("index.m3u8").is_file() => StreamState::PreviewReady,
            Some(_) => StreamState::Preparing,
        };

        StreamStatus { state }
    }

    /// Resolves an allowlisted preview file path (playlist or segment).
    pub fn preview_file(&self, name: &str) -> Option<PathBuf> {
        valid_preview_file_name(name).then(|| self.preview_dir.join(name))
    }
}

impl Drop for HlsPreviewManager {
    fn drop(&mut self) {
        if let Some(stream) = self.staged_stream.get_mut().as_mut() {
            let _ = stream.preview_process.start_kill();
        }
        let _ = std::fs::remove_dir_all(&self.preview_dir);
    }
}

fn create_preview_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn valid_preview_file_name(name: &str) -> bool {
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
