use crate::chat::Source;
use topcoat::{
    Result,
    view::{View, component, view},
};

pub(crate) const fn source_color(source: Source) -> &'static str {
    match source {
        Source::Twitch => "text-[#9146ff]",
        Source::YouTube => "text-[#ff0033]",
        Source::Kick => "text-[#53fc18]",
        Source::X => "text-sky-500",
    }
}

#[component]
pub(crate) async fn chat_source_icon(source: Source) -> Result<impl View> {
    let color = source_color(source);
    let svg = match source {
        Source::Twitch => {
            r#"
            <path
                d="M4 3h5v7.2L16.2 3H22l-8.5 9 8.5 9h-5.8L9 13.7V21H4V3Z"
                fill="currentColor"
                stroke="none"
            />
        "#
        }
        Source::YouTube => {
            r#"
            <path
                d="M21.5 7.2a2.8 2.8 0 0 0-2-2C17.7 4.7 12 4.7 12 4.7s-5.7 0-7.5.5a2.8 2.8 0 0 0-2 2C2 9 2 12 2 12s0 3 .5 4.8a2.8 2.8 0 0 0 2 2c1.8.5 7.5.5 7.5.5s5.7 0 7.5-.5a2.8 2.8 0 0 0 2-2C22 15 22 12 22 12s0-3-.5-4.8Z"
            />
            <path d="m10 15 5-3-5-3v6Z" fill="currentColor" stroke="none" />
        "#
        }
        Source::Kick => {
            r#"
            <path d="M5 3h14v12l-4 4h-4l-3 2v-2H5V3Z" />
            <path d="M10 8v4M14 8v4" />
        "#
        }
        Source::X => {
            r#"
            <path
                d="M21.742 21.75l-7.563-11.179 7.056-8.321h-2.456l-5.691 6.714-4.54-6.714H2.359l7.29 10.776L2.25 21.75h2.456l6.035-7.118 4.818 7.118h6.191-.008zM7.739 3.818L18.81 20.182h-2.447L5.29 3.818h2.447z"
            ></path>
        "#
        }
    };

    Ok(view! {
        <span class=(format!("mt-0.5 shrink-0 {color}")) title=(source.to_string())>
            <svg
                aria-hidden="true"
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                (svg)
            </svg>
        </span>
    })
}
