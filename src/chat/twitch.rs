use crate::chat::{EnqueueOutcome, IncomingChatMessage};
use crate::server::state::ProxyState;
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const TWITCH_IRC_ADDRESS: &str = "irc.chat.twitch.tv:6667";
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub async fn run(state: Arc<ProxyState>, channel: String) {
    loop {
        if let Err(error) = read_connection(&state, &channel).await {
            tracing::warn!(
                channel,
                "Twitch IRC connection ended: {error:#}. Reconnecting in 5 seconds"
            );
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn read_connection(state: &Arc<ProxyState>, channel: &str) -> Result<()> {
    let mut stream = TcpStream::connect(TWITCH_IRC_ADDRESS)
        .await
        .context("failed to connect to Twitch IRC")?;
    let nick = format!("justinfan{}", 10_000 + now_unix_ms() % 90_000);
    let handshake = format!(
        "CAP REQ :twitch.tv/tags twitch.tv/commands\r\n\
         PASS SCHMOOPIIE\r\n\
         NICK {nick}\r\n\
         JOIN #{channel}\r\n"
    );
    stream
        .write_all(handshake.as_bytes())
        .await
        .context("failed to send Twitch IRC handshake")?;
    tracing::info!(channel, "Connected to Twitch chat over anonymous IRC");

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut fallback_message_id = 0_u64;

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .await
            .context("failed to read from Twitch IRC")?
            == 0
        {
            anyhow::bail!("Twitch closed the IRC connection");
        }

        if line.starts_with("PING ") {
            let pong = line.replacen("PING", "PONG", 1);
            writer
                .write_all(pong.as_bytes())
                .await
                .context("failed to answer Twitch IRC PING")?;
            continue;
        }
        if line.trim_end() == ":tmi.twitch.tv RECONNECT" {
            anyhow::bail!("Twitch requested reconnection");
        }

        let Some(parsed) = parse_privmsg(&line) else {
            continue;
        };
        fallback_message_id = fallback_message_id.wrapping_add(1);
        let message = IncomingChatMessage {
            source: "twitch".into(),
            external_id: parsed
                .id
                .unwrap_or_else(|| format!("irc-{}-{fallback_message_id}", now_unix_ms())),
            author: parsed.author,
            text: parsed.text,
            avatar_url: None,
            sent_at: None,
        };
        match state.chat_inbox.lock().await.enqueue(message) {
            Ok(EnqueueOutcome::Accepted | EnqueueOutcome::Dropped) => {
                state.notify_chat_changed();
            }
            Ok(EnqueueOutcome::Duplicate) => {}
            Err(error) => tracing::warn!("Discarding invalid Twitch IRC message: {error}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedMessage {
    id: Option<String>,
    author: String,
    text: String,
}

fn parse_privmsg(line: &str) -> Option<ParsedMessage> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (tags, message) = if let Some(tagged) = line.strip_prefix('@') {
        let (tags, message) = tagged.split_once(' ')?;
        (Some(tags), message)
    } else {
        (None, line)
    };
    let message = message.strip_prefix(':')?;
    let (prefix, message) = message.split_once(" PRIVMSG ")?;
    let (_, text) = message.split_once(" :")?;
    if text.trim().is_empty() {
        return None;
    }

    let tag = |name: &str| {
        tags.and_then(|tags| {
            tags.split(';').find_map(|tag| {
                let (key, value) = tag.split_once('=')?;
                (key == name && !value.is_empty()).then(|| decode_tag(value))
            })
        })
    };
    let author = tag("display-name")
        .or_else(|| prefix.split('!').next().map(str::to_owned))
        .filter(|author| !author.trim().is_empty())?;

    Some(ParsedMessage {
        id: tag("id"),
        author,
        text: text.to_owned(),
    })
}

fn decode_tag(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match characters.next() {
            Some('s') => decoded.push(' '),
            Some(':') => decoded.push(';'),
            Some('\\') => decoded.push('\\'),
            Some('r') => decoded.push('\r'),
            Some('n') => decoded.push('\n'),
            Some(other) => decoded.push(other),
            None => decoded.push('\\'),
        }
    }
    decoded
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tagged_twitch_privmsg() {
        let message = parse_privmsg(
            "@display-name=Some\\sViewer;id=message-123 :someviewer!someviewer@someviewer.tmi.twitch.tv PRIVMSG #channel :hello chat!\r\n",
        )
        .unwrap();

        assert_eq!(
            message,
            ParsedMessage {
                id: Some("message-123".into()),
                author: "Some Viewer".into(),
                text: "hello chat!".into(),
            }
        );
    }

    #[test]
    fn parses_untagged_privmsg_like_the_reference_implementation() {
        let message =
            parse_privmsg(":viewer!viewer@viewer.tmi.twitch.tv PRIVMSG #channel :hello").unwrap();

        assert_eq!(message.id, None);
        assert_eq!(message.author, "viewer");
        assert_eq!(message.text, "hello");
    }

    #[test]
    fn ignores_non_chat_irc_messages() {
        assert!(parse_privmsg("PING :tmi.twitch.tv\r\n").is_none());
        assert!(parse_privmsg(":tmi.twitch.tv 001 justinfan12345 :Welcome\r\n").is_none());
    }
}
