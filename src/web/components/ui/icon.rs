use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub async fn trash_icon() -> Result {
    view! {
        <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M10 11v6M14 11v6" />
        </svg>
    }
}

#[component]
pub async fn eye_icon() -> Result {
    view! {
        <svg data-secret-visible="false" aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z" />
            <circle cx="12" cy="12" r="3" />
        </svg>
        <svg data-secret-visible="true" hidden="true" aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="m3 3 18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 4.2A10.8 10.8 0 0 1 12 4c6.5 0 10 8 10 8a17.7 17.7 0 0 1-2 3M6.6 6.6C3.5 8.6 2 12 2 12s3.5 8 10 8a9.8 9.8 0 0 0 4.1-.9" />
        </svg>
    }
}
