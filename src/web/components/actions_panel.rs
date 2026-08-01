use crate::web::components::ui::button::{button, button_variants, ButtonSize, ButtonVariant};
use topcoat::{
    view::{attributes, component, view},
    Result,
};

#[component]
pub async fn actions_panel() -> Result {
    view! {
        <div class="flex flex-col sm:flex-row gap-4 justify-between items-center bg-surface p-4 border rounded-xl">
            <div class="flex gap-4">
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="submit"
                        formaction="/api/test-stream"
                        formmethod="post"
                        formnovalidate="formnovalidate"
                    },
                    "Test 15s Stream"
                )
                button(
                    variant: ButtonVariant::Outline,
                    attrs: attributes! {
                        type="submit"
                        formaction="/api/test-webhooks"
                        formmethod="post"
                        formnovalidate="formnovalidate"
                    },
                    "Test Webhooks"
                )
            </div>
            <div class="flex gap-4 w-full sm:w-auto">
                <a
                    href="/"
                    class=(format!("{} w-full sm:w-auto", button_variants(ButtonVariant::Secondary, ButtonSize::Md)))
                >"Revert"</a>
                button(
                    variant: ButtonVariant::Primary,
                    attrs: attributes! {
                        type="submit"
                        id="saveBtn"
                        name="action"
                        value="save"
                        class="w-full sm:w-auto min-w-[120px]"
                        formaction="/api/config"
                    },
                    "Save Configuration"
                )
            </div>
        </div>
    }
}
