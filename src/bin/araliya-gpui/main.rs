mod api;
mod state;
mod components;

use gpui::*;
use gpui_component::Root;
use std::sync::Arc;

use crate::api::ApiClient;
use crate::state::{AppState, load_layout_prefs};
use crate::components::AppView;

fn main() {
    let app = Application::new();

    app.run(move |cx| {
        gpui_component::init(cx);

        let api_client = Arc::new(ApiClient::new("http://127.0.0.1:8080".to_string()));
        let layout = load_layout_prefs().map(|prefs| prefs.into_layout()).unwrap_or_default();
        let app_state = AppState::with_layout(api_client, layout);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| AppView::new(app_state, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Ok::<_, Box<dyn std::error::Error>>(())
        })
        .detach();
    });
}
