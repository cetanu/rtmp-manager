use crate::chat::Source;
use topcoat::{
    Result,
    view::{View, component, view},
};

struct SourceStyle {
    color: &'static str,
    icon: &'static str,
}

const SOURCE_STYLES: [(Source, SourceStyle); 4] = [
    (
        Source::Twitch,
        SourceStyle {
            color: "text-[#9146ff]",
            icon: r#"
            <path
                d="M4 3h5v7.2L16.2 3H22l-8.5 9 8.5 9h-5.8L9 13.7V21H4V3Z"
                fill="currentColor"
                stroke="none"
            />
        "#,
        },
    ),
    (
        Source::YouTube,
        SourceStyle {
            color: "text-[#ff0033]",
            icon: r#"
            <path
                d="M21.5 7.2a2.8 2.8 0 0 0-2-2C17.7 4.7 12 4.7 12 4.7s-5.7 0-7.5.5a2.8 2.8 0 0 0-2 2C2 9 2 12 2 12s0 3 .5 4.8a2.8 2.8 0 0 0 2 2c1.8.5 7.5.5 7.5.5s5.7 0 7.5-.5a2.8 2.8 0 0 0 2-2C22 15 22 12 22 12s0-3-.5-4.8Z"
            />
            <path d="m10 15 5-3-5-3v6Z" fill="currentColor" stroke="none" />
        "#,
        },
    ),
    (
        Source::Kick,
        SourceStyle {
            color: "text-[#53fc18]",
            icon: r#"
            <path d="M5 3h14v12l-4 4h-4l-3 2v-2H5V3Z" />
            <path d="M10 8v4M14 8v4" />
        "#,
        },
    ),
    (
        Source::X,
        SourceStyle {
            color: "text-sky-500",
            icon: r#"
            <path
                d="M21.742 21.75l-7.563-11.179 7.056-8.321h-2.456l-5.691 6.714-4.54-6.714H2.359l7.29 10.776L2.25 21.75h2.456l6.035-7.118 4.818 7.118h6.191-.008zM7.739 3.818L18.81 20.182h-2.447L5.29 3.818h2.447z"
            ></path>
        "#,
        },
    ),
];

fn source_style(source: Source) -> &'static SourceStyle {
    SOURCE_STYLES
        .iter()
        .find_map(|(candidate, style)| (*candidate == source).then_some(style))
        .expect("every chat source must have a style")
}

pub(crate) fn source_color(source: Source) -> &'static str {
    source_style(source).color
}

#[component]
pub(crate) async fn chat_source_icon(source: Source) -> Result<impl View> {
    let style = source_style(source);

    Ok(view! {
        <span class=(format!("mt-0.5 shrink-0 {}", style.color)) title=(source.to_string())>
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
                (style.icon)
            </svg>
        </span>
    })
}
