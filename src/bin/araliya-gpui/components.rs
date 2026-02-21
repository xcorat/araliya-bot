use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    ActiveTheme, h_flex, v_flex, Disableable,
    scroll::ScrollableElement,
};

use crate::state::AppState;

pub struct AppView {
    state: AppState,
    input_state: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    pub fn new(state: AppState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Type a message...")
        });

        let _subscriptions = vec![
            cx.subscribe_in(&input_state, window, {
                let input_state = input_state.clone();
                move |_this, _, ev: &InputEvent, _window, cx| match ev {
                    InputEvent::Change => {
                        let value = input_state.read(cx).value();
                        _this.state.input_text = value.to_string();
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => {
                        _this.send_message(cx);
                        input_state.update(cx, |i, cx| {
                            i.set_value("", _window, cx);
                        });
                    }
                    _ => {}
                }
            }),
        ];

        let mut view = Self {
            state,
            input_state,
            _subscriptions,
        };

        view.fetch_health(cx);
        view.fetch_sessions(cx);

        view
    }

    fn fetch_health(&mut self, cx: &mut Context<Self>) {
        let client = self.state.api_client.clone();
        cx.spawn(move |view: WeakEntity<AppView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Ok(health) = client.check_health().await {
                    view.update(&mut cx, |this, cx| {
                        this.state.health_status = Some(health);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn fetch_sessions(&mut self, cx: &mut Context<Self>) {
        self.state.is_loading_sessions = true;
        cx.notify();

        let client = self.state.api_client.clone();
        cx.spawn(move |view: WeakEntity<AppView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = client.list_sessions().await;
                view.update(&mut cx, |this, cx| {
                    this.state.is_loading_sessions = false;
                    if let Ok(res) = result {
                        this.state.sessions = res.sessions;
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn select_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.state.active_session_id = Some(session_id.clone());
        self.state.is_loading_messages = true;
        self.state.messages.clear();
        cx.notify();

        let client = self.state.api_client.clone();
        cx.spawn(move |view: WeakEntity<AppView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = client.get_session_by_id(&session_id).await;
                view.update(&mut cx, |this, cx| {
                    this.state.is_loading_messages = false;
                    if let Ok(res) = result {
                        this.state.messages = res.transcript;
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn send_message(&mut self, cx: &mut Context<Self>) {
        let text = self.state.input_text.trim().to_string();
        if text.is_empty() || self.state.is_sending_message {
            return;
        }

        self.state.is_sending_message = true;
        self.state.input_text.clear();
        self.state.messages.push(crate::api::SessionTranscriptMessage {
            role: "user".to_string(),
            content: text.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_call_id: None,
            tool_calls: None,
        });
        cx.notify();

        let client = self.state.api_client.clone();
        let session_id = self.state.active_session_id.clone();

        cx.spawn(move |view: WeakEntity<AppView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = client.send_message(text, session_id.clone()).await;
                view.update(&mut cx, |this, cx| {
                    this.state.is_sending_message = false;
                    if let Ok(res) = result {
                        if this.state.active_session_id.is_none() {
                            this.state.active_session_id = Some(res.session_id.clone());
                            this.fetch_sessions(cx);
                        }
                        this.state.messages.push(crate::api::SessionTranscriptMessage {
                            role: "assistant".to_string(),
                            content: res.reply,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            tool_call_id: None,
                            tool_calls: None,
                        });
                    } else {
                        this.state.messages.push(crate::api::SessionTranscriptMessage {
                            role: "error".to_string(),
                            content: "Failed to send message".to_string(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            tool_call_id: None,
                            tool_calls: None,
                        });
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.state;
        let theme = cx.theme();
        let view = cx.entity().downgrade();

        let mut sidebar_menu = SidebarMenu::new();
        for session in &state.sessions {
            let is_active = state.active_session_id.as_ref() == Some(&session.session_id);
            let session_id = session.session_id.clone();
            let view = view.clone();
            
            sidebar_menu = sidebar_menu.child(
                SidebarMenuItem::new(format!("Session {}", &session.session_id[..8]))
                    .active(is_active)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.select_session(session_id.clone(), cx);
                        }).ok();
                    })
            );
        }

        let health_text = match &state.health_status {
            Some(h) => format!("Status: {} | Bot: {}", h.status, h.bot_id),
            None => "Checking health...".to_string(),
        };

        let mut messages_view = v_flex().flex_1().overflow_y_scrollbar().p_4().gap_4();
        
        if state.is_loading_messages {
            messages_view = messages_view.child(div().child("Loading messages..."));
        } else if state.messages.is_empty() {
            messages_view = messages_view.child(div().child("No messages yet."));
        } else {
            for msg in &state.messages {
                let is_user = msg.role == "user";
                let bg_color = if is_user { theme.primary } else { theme.secondary };
                let text_color = if is_user { theme.primary_foreground } else { theme.secondary_foreground };

                let mut msg_container = v_flex().w_full();
                if is_user {
                    msg_container = msg_container.items_end();
                } else {
                    msg_container = msg_container.items_start();
                }

                messages_view = messages_view.child(
                    msg_container
                        .child(
                            div()
                                .max_w(px(600.))
                                .p_3()
                                .rounded_lg()
                                .bg(bg_color)
                                .text_color(text_color)
                                .child(msg.content.clone())
                        )
                );
            }
        }

        h_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                Sidebar::new(gpui_component::Side::Left)
                    .w(px(250.))
                    .header(
                        SidebarHeader::new().child(
                            v_flex()
                                .gap_2()
                                .child(div().text_xl().font_weight(FontWeight::BOLD).child("Araliya Bot"))
                                .child(div().text_sm().text_color(theme.muted_foreground).child(health_text))
                        )
                    )
                    .child(
                        SidebarGroup::new("Sessions").child(sidebar_menu)
                    )
            )
            .child(
                v_flex()
                    .flex_1()
                    .child(messages_view)
                    .child(
                        h_flex()
                            .p_4()
                            .gap_2()
                            .border_t_1()
                            .border_color(theme.border)
                            .child(
                                div().flex_1().child(Input::new(&self.input_state))
                            )
                            .child(
                                Button::new("send-btn")
                                    .label("Send")
                                    .primary()
                                    .disabled(state.is_sending_message || state.input_text.trim().is_empty())
                                    .on_click({
                                        let view = cx.entity().downgrade();
                                        let input_state = self.input_state.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.send_message(cx);
                                            }).ok();
                                            input_state.update(cx, |i, cx| {
                                                i.set_value("", window, cx);
                                            });
                                        }
                                    })
                            )
                    )
            )
    }
}
