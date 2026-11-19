use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

#[component]
pub async fn trash_icon(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <svg
            aria-hidden="true"
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
        >
            <path
                d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M10 11v6M14 11v6"
            />
        </svg>
    })
}
