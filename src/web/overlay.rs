use crate::config::OverlaySettings;
use crate::server::state::AppHandle;
use crate::util::secure_token_matches;
use futures_util::StreamExt;
use serde::Deserialize;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        content::sse::{Event as SseEvent, KeepAlive, Sse},
        error::not_found,
        page, parse_query_params, route,
    },
    view::{component, view},
};

#[derive(Deserialize)]
struct OverlayQuery {
    key: String,
}

fn authorized_settings(cx: &Cx) -> Result<OverlaySettings> {
    let query: OverlayQuery = parse_query_params(cx).map_err(|_| not_found())?;
    let app: &AppHandle = app_context(cx);
    let settings = app.config.get().overlay.clone();
    if !secure_token_matches(&settings.key, &query.key) {
        return Err(not_found().into());
    }
    Ok(settings)
}

#[component]
async fn overlay_document(settings: OverlaySettings) -> Result {
    let style = format!(
        "--chat-font-size:{}px;--chat-background-opacity:{};--chat-fade-duration:{}s",
        settings.font_size_px,
        f32::from(settings.background_opacity_percent) / 100.0,
        settings.fade_duration_secs,
    );
    view! {
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1.0" />
            <title>"Live Chat Overlay"</title>
            <style>{r#"
                :root { color-scheme: dark; background: transparent; }
                * { box-sizing: border-box; }
                html, body { margin: 0; width: 100%; height: 100%; overflow: hidden; background: transparent; }
                #chat { position: absolute; inset: auto 0 0; display: flex; flex-direction: column; gap: .45rem; padding: 1rem; font: 600 var(--chat-font-size)/1.3 Inter, system-ui, sans-serif; }
                .message { display: flex; align-items: flex-start; gap: .55rem; width: fit-content; max-width: 100%; padding: .5rem .7rem; color: white; background: rgb(8 12 20 / var(--chat-background-opacity)); border-radius: .55rem; overflow-wrap: anywhere; text-shadow: 0 1px 2px #000; animation: appear .2s ease-out; }
                .avatar { width: 1.6em; height: 1.6em; flex: none; border-radius: 999px; object-fit: cover; }
                .badge { flex: none; margin-top: .1em; border-radius: .25rem; padding: .08rem .3rem; font-size: .6em; line-height: 1.4; text-transform: uppercase; background: #64748b; }
                .badge.twitch { background: #9146ff; } .badge.youtube { background: #ff0033; } .badge.kick { background: #53fc18; color: #071205; } .badge.x { background: #111; }
                .author { margin-right: .35em; color: #8be9fd; }
                [data-theme="minimal"] .message { padding: .15rem .25rem; background: transparent; }
                [data-theme="comic"] .message { border: 3px solid #111; border-radius: 1rem; color: #111; background: rgb(255 255 255 / var(--chat-background-opacity)); font-family: "Comic Sans MS", cursive; text-shadow: none; }
                [data-theme="comic"] .author { color: #7c3aed; }
                [data-theme="transparent-box"] .message { border: 1px solid rgb(255 255 255 / .35); backdrop-filter: blur(5px); }
                .message.fading { animation: fade var(--chat-fade-duration) linear forwards; }
                @keyframes appear { from { opacity: 0; transform: translateY(.5rem); } }
                @keyframes fade { 0%, 80% { opacity: 1; } 100% { opacity: 0; } }
            "#}</style>
            <script src=(super::CHAT_OVERLAY_SCRIPT) defer="defer"></script>
        </head>
        <body
            style=(style)
            data-theme=(settings.theme)
            data-show-badges=(settings.show_badges.to_string())
            data-show-avatars=(settings.show_avatars.to_string())
            data-show-emotes=(settings.show_emotes.to_string())
            data-fade-duration=(settings.fade_duration_secs.to_string())
        >
            <main id="chat" aria-live="polite"></main>
        </body>
        </html>
    }
}

#[page("/overlay/chat")]
async fn chat_overlay(cx: &Cx) -> Result {
    let settings = authorized_settings(cx)?;
    view! { overlay_document(settings: settings) }
}

#[route(GET "/overlay/chat/events")]
async fn chat_overlay_events(
    cx: &Cx,
) -> Result<Sse<impl futures_util::Stream<Item = Result<SseEvent>> + use<>>> {
    authorized_settings(cx)?;
    let app: &AppHandle = app_context(cx);
    let initial = SseEvent::new()
        .event("messages")
        .json_data(&app.chat.snapshot().await?)?;
    let changes = app.chat.subscribe_changes();
    let chat = app.chat.clone();
    let updates = futures_util::stream::unfold((changes, chat), |(mut changes, chat)| async move {
        changes.changed().await.ok()?;
        let event = match chat.snapshot().await {
            Ok(snapshot) => SseEvent::new().event("messages").json_data(&snapshot),
            Err(error) => Err(error.into()),
        };
        Some((event, (changes, chat)))
    });
    Ok(
        Sse::new(futures_util::stream::once(async { Ok(initial) }).chain(updates))
            .keep_alive(KeepAlive::new()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_style_uses_bounded_configuration() {
        let settings = OverlaySettings::default();
        assert!((12..=72).contains(&settings.font_size_px));
        assert!(settings.background_opacity_percent <= 100);
    }
}
