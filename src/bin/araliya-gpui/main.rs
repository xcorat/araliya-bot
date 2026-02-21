mod api;
mod state;
mod components;

use gpui::*;
use gpui_component::Root;
use std::{borrow::Cow, sync::Arc};

use crate::api::ApiClient;
use crate::state::{AppState, load_layout_prefs};
use crate::components::AppView;

#[derive(Clone, Copy, Debug, Default)]
struct GpuiAssets;

impl AssetSource for GpuiAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "icons/bot.svg" => Some(include_bytes!("icons/bot.svg")),
            "icons/inbox.svg" => Some(include_bytes!("icons/inbox.svg")),
            "icons/square-terminal.svg" => Some(include_bytes!("icons/square-terminal.svg")),
            "icons/chart-pie.svg" => Some(include_bytes!("icons/chart-pie.svg")),
            "icons/settings-2.svg" => Some(include_bytes!("icons/settings-2.svg")),
            "icons/book-open.svg" => Some(include_bytes!("icons/book-open.svg")),
            _ => None,
        };

        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path != "icons" {
            return Ok(vec![]);
        }

        Ok(vec![
            "bot.svg".into(),
            "inbox.svg".into(),
            "square-terminal.svg".into(),
            "chart-pie.svg".into(),
            "settings-2.svg".into(),
            "book-open.svg".into(),
        ])
    }
}

fn main() {
    let app = Application::new().with_assets(GpuiAssets);

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
