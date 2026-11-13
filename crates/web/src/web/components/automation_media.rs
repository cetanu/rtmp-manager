use super::ui::{
    button::{ButtonSize, ButtonVariant, button_variants},
    card::{card, card_content, card_header, card_title},
    confirm_delete::confirm_delete,
};
use rtmp_core::{chat::automations::alerts, server::state::AppHandle};
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, attributes, component, view},
};

#[component]
pub async fn automation_media(cx: &Cx) -> Result<impl View> {
    let app: &AppHandle = context(cx);
    let library = alerts::list(app.config.database())
        .await
        .map_err(topcoat::Error::from_anyhow)?;
    let count = library.len();
    let overlay_url = format!(
        "/overlay/alerts?key={}",
        app.config.get().web_auth.overlay_token
    );
    let outline = button_variants(ButtonVariant::Outline, ButtonSize::Md);
    let primary = button_variants(ButtonVariant::Primary, ButtonSize::Md);
    Ok(view! {
        <section
            id="automation-media"
            class="pb-4"
            aria-label="Media"
            data-media-library=""
        >
            card(
                card_header(
                    <div class="flex flex-wrap items-center justify-between gap-3 pb-3">
                        <div class="flex items-center gap-3">
                            card_title("capacity used")
                            <span class="font-mono text-[10px] text-muted-foreground">
                                (format!("{count} / 100"))
                            </span>
                        </div>
                        <div class="flex flex-wrap items-center gap-2">
                            <a
                                href=(overlay_url.clone())
                                target="_blank"
                                rel="noopener"
                                class=(outline.clone())
                            >
                                "OBS overlay ↗"
                            </a>
                            <form
                                method="post"
                                action="/api/alerts/media"
                                enctype="multipart/form-data"
                                data-media-form=""
                            >
                                <label class=(primary) for="alert-media-upload">
                                    "Upload"
                                </label>
                                <input
                                    id="alert-media-upload"
                                    aria-label="Upload alert media"
                                    class="sr-only"
                                    type="file"
                                    name="media"
                                    required="required"
                                    accept=".png,.gif,.jpg,.jpeg,.webp,.mp4,.mp3,.wav,.ogg"
                                />
                            </form>
                        </div>
                    </div>
                )
                card_content(
                    <p class="mb-4 text-xs text-muted-foreground">
                        "Supported formats: PNG, GIF, JPEG, WebP, MP4, MP3, WAV or Ogg · Size limit: 10 MiB"
                    </p>
                    <p class="mb-4 text-xs text-muted-foreground">
                        "Set the OBS browser source to your stream canvas size. Use position: #{x: 70, y: 10, width: 25, height: 30} in overlay.show to place an alert using canvas percentages."
                    </p>
                    <p
                        data-media-status=""
                        role="status"
                        class="mb-3 hidden text-sm text-muted-foreground"
                    ></p>
                    if library.is_empty() {
                        <div
                            class="flex flex-col items-center justify-center gap-2 rounded-sm border border-dashed border-border bg-white/[0.02] px-5 py-8 text-center"
                        >
                            <p class="text-sm font-medium">"No files yet"</p>
                        </div>
                    } else {
                        <div
                            class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4"
                        >
                            #[key(media.name)]
                            for media in library {
                                let image = media.content_type.starts_with("image/");
                                let video = media.content_type.starts_with("video/");
                                let url = format!("/api/alerts/media?name={}", media.name);
                                let snippet = format!(
                                    "overlay.show(#{{ {}: \"{}\", position: #{{x: 25, y: 25, width: 50, height: 50}}, duration_ms: 8000 }});",
                                    if image {
                                        "image"
                                    } else if video {
                                        "video"
                                    } else {
                                        "sound"
                                    },
                                    media.name,
                                );
                                <article
                                    class="flex min-w-0 flex-col overflow-hidden rounded-sm border border-border bg-background/40"
                                    data-media-item=""
                                >
                                    <div
                                        class="flex h-36 items-center justify-center overflow-hidden bg-white/[0.03] p-3"
                                    >
                                        if image {
                                            <img
                                                src=(url)
                                                alt=(media.name.clone())
                                                loading="lazy"
                                                class="max-h-full max-w-full object-contain"
                                            />
                                        } else if video {
                                            <video
                                                controls="controls"
                                                preload="metadata"
                                                aria-label=(format!("Preview {}", media.name))
                                                src=(url)
                                                class="max-h-full max-w-full"
                                            ></video>
                                        } else {
                                            <div
                                                class="flex w-full min-w-0 flex-col items-center gap-3"
                                            >
                                                <span
                                                    class="font-mono text-[10px] tracking-[0.16em] text-muted-foreground uppercase"
                                                >
                                                    "Sound"
                                                </span>
                                                <audio
                                                    style="color-scheme:dark"
                                                    controls="controls"
                                                    preload="none"
                                                    aria-label=(format!("Preview {}", media.name))
                                                    src=(url)
                                                    class="h-8 w-full min-w-0"
                                                ></audio>
                                            </div>
                                        }
                                    </div>
                                    <div
                                        class="flex flex-col gap-3 border-t border-border p-3"
                                    >
                                        <div class="min-w-0">
                                            <p
                                                class="truncate font-mono text-xs"
                                                title=(media.name.clone())
                                            >
                                                (media.name.clone())
                                            </p>
                                            <p
                                                class="mt-1 font-mono text-[10px] text-muted-foreground"
                                            >
                                                (media.content_type)
                                            </p>
                                        </div>
                                        <div
                                            class="flex flex-wrap items-center justify-between gap-2"
                                        >
                                            <form method="post" action=(format!("/api/alerts/media/repeats?name={}&allow={}", media.name, !media.allow_repeats)) data-media-form="">
                                                <button type="submit" class=(outline.clone()) aria-label=(format!("{} repeats for {}", if media.allow_repeats { "Disable" } else { "Allow" }, media.name))>
                                                    (if media.allow_repeats { "Repeats allowed" } else { "Repeats blocked" })
                                                </button>
                                            </form>
                                            <button
                                                type="button"
                                                data-copy-script=(snippet)
                                                class=(outline.clone())
                                            >
                                                "Copy script"
                                            </button>
                                            <form
                                                method="post"
                                                action=(format!(
                                                    "/api/alerts/media/delete?name={}",
                                                    media.name,
                                                ))
                                                data-media-form=""
                                            >
                                                confirm_delete(attrs: attributes! { type="submit" })
                                            </form>
                                        </div>
                                    </div>
                                </article>
                            }
                        </div>
                    }
                )
            )
        </section>
    })
}
