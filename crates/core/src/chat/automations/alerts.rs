//! Validated script actions and the shared media library.
use anyhow::{Result, anyhow, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;

pub const MAX_MEDIA_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone)]
pub struct AlertQueue {
    db: toasty::Db,
    sender: broadcast::Sender<Alert>,
    state: Arc<Mutex<QueueState>>,
}

#[derive(Default)]
struct QueueState {
    playing: Option<Alert>,
    waiting: VecDeque<Alert>,
}

fn names(alert: &Alert) -> impl Iterator<Item = &str> {
    [&alert.image, &alert.video, &alert.sound]
        .into_iter()
        .map(String::as_str)
        .filter(|name| !name.is_empty())
}

impl AlertQueue {
    pub fn new(db: toasty::Db) -> Self {
        Self {
            db,
            sender: broadcast::channel(128).0,
            state: Arc::new(Mutex::new(QueueState::default())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Alert> {
        self.sender.subscribe()
    }

    pub async fn enqueue(&self, alerts: Vec<Alert>) -> Result<usize> {
        let listeners = self.sender.receiver_count();
        if listeners == 0 || alerts.is_empty() {
            return Ok(0);
        }
        let restricted: HashSet<String> = list(&self.db)
            .await?
            .into_iter()
            .filter(|media| !media.allow_repeats)
            .map(|media| media.name)
            .collect();
        let mut state = self.state.lock().unwrap();
        let mut accepted = false;
        for alert in alerts {
            let duplicate = names(&alert).any(|name| {
                restricted.contains(name)
                    && state
                        .playing
                        .iter()
                        .chain(state.waiting.iter())
                        .any(|queued| names(queued).any(|existing| existing == name))
            });
            if duplicate || state.waiting.len() >= 64 {
                continue;
            }
            accepted = true;
            if state.playing.is_none() {
                let _ = self.sender.send(alert.clone());
                state.playing = Some(alert);
                let queue = self.clone();
                tokio::spawn(async move {
                    queue.run().await;
                });
            } else {
                state.waiting.push_back(alert);
            }
        }
        Ok(if accepted { listeners } else { 0 })
    }

    async fn run(self) {
        loop {
            let duration = {
                let state = self.state.lock().unwrap();
                match &state.playing {
                    Some(alert) => alert.duration_ms,
                    None => return,
                }
            };
            tokio::time::sleep(Duration::from_millis(duration)).await;
            let mut state = self.state.lock().unwrap();
            state.playing = None;
            while let Some(next) = state.waiting.pop_front() {
                if self.sender.send(next.clone()).is_ok() {
                    state.playing = Some(next);
                    break;
                }
            }
            if state.playing.is_none() {
                return;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alert {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub video: String,
    #[serde(default)]
    pub sound: String,
    #[serde(default)]
    pub position: Option<AlertPosition>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertPosition {
    pub x: u8,
    pub y: u8,
    pub width: u8,
    pub height: u8,
}

impl Alert {
    pub fn validate(&self) -> Result<()> {
        if let Some(position) = &self.position {
            ensure!(
                position.width > 0
                    && position.height > 0
                    && u16::from(position.x) + u16::from(position.width) <= 100
                    && u16::from(position.y) + u16::from(position.height) <= 100,
                "Alert position must fit within the canvas (percentages 0–100)"
            );
        }
        ensure!(self.text.len() <= 4096, "Alert text exceeds 4 KiB");
        ensure!(
            (250..=60_000).contains(&self.duration_ms),
            "Alert duration must be 250–60,000 ms"
        );
        ensure!(
            !self.text.trim().is_empty()
                || !self.image.is_empty()
                || !self.video.is_empty()
                || !self.sound.is_empty(),
            "Alert must contain text, an image, a video or a sound"
        );
        for name in [&self.image, &self.video, &self.sound] {
            if !name.is_empty() {
                validate_name(name)?;
            }
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 120
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
        "Media names must be 1–120 ASCII letters, numbers, dots, dashes or underscores"
    );
    Ok(())
}

#[derive(Debug, toasty::Model)]
#[table = "automation_media"]
pub struct Media {
    #[key]
    pub name: String,
    pub content_type: String,
    pub data: String,
    pub allow_repeats: bool,
}

pub async fn list(db: &toasty::Db) -> Result<Vec<Media>> {
    // Avoid loading media bodies to list the library.
    Ok(Media::all()
        .select((
            Media::fields().name(),
            Media::fields().content_type(),
            Media::fields().allow_repeats(),
        ))
        .exec(&mut db.clone())
        .await?
        .into_iter()
        .map(|(name, content_type, allow_repeats)| Media {
            name,
            content_type,
            data: String::new(),
            allow_repeats,
        })
        .collect())
}

pub async fn get(db: &toasty::Db, name: &str) -> Result<Media> {
    validate_name(name)?;
    Media::filter(Media::fields().name().eq(name))
        .first()
        .exec(&mut db.clone())
        .await?
        .ok_or_else(|| anyhow!("Media '{name}' does not exist"))
}

pub async fn validate_media(db: &toasty::Db, alerts: &[Alert]) -> Result<()> {
    let library = list(db).await?;
    for alert in alerts {
        for (name, prefix) in [
            (&alert.image, "image/"),
            (&alert.video, "video/"),
            (&alert.sound, "audio/"),
        ] {
            if name.is_empty() {
                continue;
            }
            ensure!(
                library
                    .iter()
                    .any(|m| m.name == *name && m.content_type.starts_with(prefix)),
                "Missing or incorrect media type: {name}"
            );
        }
    }
    Ok(())
}

pub async fn upload(db: &toasty::Db, name: String, bytes: &[u8]) -> Result<()> {
    validate_name(&name)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_MEDIA_BYTES,
        "Media must be 1 byte–10 MiB"
    );
    let content_type = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        "audio/wav"
    } else if bytes.starts_with(b"OggS") {
        "audio/ogg"
    } else if bytes.len() >= 12
        && bytes.get(4..8) == Some(b"ftyp")
        && name.to_ascii_lowercase().ends_with(".mp4")
    {
        "video/mp4"
    } else if bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
    {
        "audio/mpeg"
    } else {
        return Err(anyhow!(
            "Supported media: PNG, GIF, JPEG, WebP, MP4, MP3, WAV and Ogg"
        ));
    };
    let library = list(db).await?;
    ensure!(
        !library.iter().any(|media| media.name == name),
        "A media file with this name already exists"
    );
    ensure!(
        library.len() < 100,
        "The media library supports at most 100 files"
    );
    toasty::create!(Media {
        name,
        content_type: content_type.to_owned(),
        data: STANDARD.encode(bytes),
        allow_repeats: true
    })
    .exec(&mut db.clone())
    .await?;
    Ok(())
}

pub async fn set_allow_repeats(db: &toasty::Db, name: &str, allow: bool) -> Result<()> {
    let mut media = get(db, name).await?;
    media
        .update()
        .allow_repeats(allow)
        .exec(&mut db.clone())
        .await?;
    Ok(())
}

pub async fn delete(db: &toasty::Db, name: &str) -> Result<()> {
    validate_name(name)?;
    Media::filter(Media::fields().name().eq(name))
        .delete()
        .exec(&mut db.clone())
        .await?;
    Ok(())
}
