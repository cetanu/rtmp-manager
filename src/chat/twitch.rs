use crate::chat::{ChatHandle, ChatMessagePart, IncomingChatMessage, Source};
use crate::util::now_unix_ms;
use anyhow::{Context, Result};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const TWITCH_IRC_ADDRESS: &str = "irc.chat.twitch.tv:6667";
const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const ANONYMOUS_NICK_MIN: u64 = 10_000;
const ANONYMOUS_NICK_RANGE: u64 = 90_000;

pub async fn run(chat: ChatHandle, channel: String) {
    loop {
        if let Err(error) = read_connection(&chat, &channel).await {
            tracing::warn!(
                channel,
                "Twitch IRC connection ended: {error:#}. Reconnecting in 5 seconds"
            );
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

async fn read_connection(chat: &ChatHandle, channel: &str) -> Result<()> {
    let mut stream = TcpStream::connect(TWITCH_IRC_ADDRESS)
        .await
        .context("failed to connect to Twitch IRC")?;
    let nick = format!(
        "justinfan{}",
        ANONYMOUS_NICK_MIN + now_unix_ms() % ANONYMOUS_NICK_RANGE
    );
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
            source: Source::Twitch,
            external_id: parsed
                .id
                .unwrap_or_else(|| format!("irc-{}-{fallback_message_id}", now_unix_ms())),
            author: parsed.author,
            text: parsed.text,
            parts: parsed.parts,
            avatar_url: None,
            sent_at: None,
        };
        if let Err(error) = chat.enqueue(message).await {
            tracing::warn!("Discarding invalid Twitch IRC message: {error}");
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedMessage {
    id: Option<String>,
    author: String,
    text: String,
    parts: Vec<ChatMessagePart>,
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
    let parts = tag("emotes")
        .map(|emotes| parse_emotes(text, &emotes))
        .unwrap_or_default();

    Some(ParsedMessage {
        id: tag("id"),
        author,
        text: text.to_owned(),
        parts,
    })
}

#[derive(Debug, PartialEq, Eq)]
struct EmoteRange {
    id: String,
    start: usize,
    end: usize,
}

fn parse_emotes(text: &str, emotes_tag: &str) -> Vec<ChatMessagePart> {
    let mut ranges = emotes_tag
        .split('/')
        .filter_map(|emote| {
            let (id, positions) = emote.split_once(':')?;
            if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
                return None;
            }

            Some(
                positions
                    .split(',')
                    .filter_map(|position| {
                        let (start, end) = position.split_once('-')?;
                        let start = start.parse::<usize>().ok()?;
                        let end = end.parse::<usize>().ok()?;
                        (start <= end).then(|| EmoteRange {
                            id: id.to_string(),
                            start,
                            end,
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect::<Vec<_>>();

    if ranges.is_empty() {
        return Vec::new();
    }

    ranges.sort_by_key(|range| (range.start, range.end));

    // Twitch positions are inclusive Unicode character offsets, rather than
    // UTF-8 byte offsets. Keeping byte boundaries for each character lets us
    // slice Rust strings without splitting a multi-byte character.
    let boundaries = text
        .char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    let character_count = boundaries.len().saturating_sub(1);
    let mut parts = Vec::new();
    let mut cursor = 0;

    for range in ranges {
        let end = range.end.saturating_add(1);
        if range.start < cursor || end > character_count {
            continue;
        }
        if cursor < range.start {
            parts.push(ChatMessagePart::Text(
                text[boundaries[cursor]..boundaries[range.start]].to_string(),
            ));
        }
        let alt = text[boundaries[range.start]..boundaries[end]].to_string();
        parts.push(ChatMessagePart::Emoji {
            alt,
            url: format!(
                "https://static-cdn.jtvnw.net/emoticons/v2/{}/static/light/3.0",
                range.id
            ),
        });
        cursor = end;
    }

    if cursor < character_count {
        parts.push(ChatMessagePart::Text(
            text[boundaries[cursor]..].to_string(),
        ));
    }
    parts
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
                parts: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_twitch_emotes_into_ordered_parts() {
        let message = parse_privmsg(
            "@display-name=Viewer;emotes=25:0-4/62835:6-16;id=message-123 :viewer!viewer@viewer.tmi.twitch.tv PRIVMSG #channel :Kappa bleedPurple!\r\n",
        )
        .unwrap();

        assert_eq!(
            message.parts,
            vec![
                ChatMessagePart::Emoji {
                    alt: "Kappa".into(),
                    url: "https://static-cdn.jtvnw.net/emoticons/v2/25/static/light/3.0".into(),
                },
                ChatMessagePart::Text(" ".into()),
                ChatMessagePart::Emoji {
                    alt: "bleedPurple".into(),
                    url: "https://static-cdn.jtvnw.net/emoticons/v2/62835/static/light/3.0".into(),
                },
                ChatMessagePart::Text("!".into()),
            ]
        );
    }

    #[test]
    fn parses_emotes_after_unicode_characters() {
        let message = parse_privmsg(
            "@emotes=25:2-6 :viewer!viewer@viewer.tmi.twitch.tv PRIVMSG #channel :🔥 Kappa\r\n",
        )
        .unwrap();

        assert_eq!(
            message.parts,
            vec![
                ChatMessagePart::Text("🔥 ".into()),
                ChatMessagePart::Emoji {
                    alt: "Kappa".into(),
                    url: "https://static-cdn.jtvnw.net/emoticons/v2/25/static/light/3.0".into(),
                },
            ]
        );
    }

    #[test]
    fn ignores_malformed_emote_ranges_without_losing_text() {
        assert!(parse_emotes("hello", "not-an-emote:0-4").is_empty());
        assert_eq!(
            parse_emotes("hello", "25:8-12"),
            vec![ChatMessagePart::Text("hello".into())]
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
