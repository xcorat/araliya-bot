use gpui::*;
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    Icon, IconName,
    ActiveTheme, h_flex, v_flex, Disableable, Sizable,
    scroll::ScrollableElement,
};

use crate::state::{
    ActivitySection, AppState, LayoutMode, SurfaceMode, DESKTOP_BREAKPOINT_PX, TABLET_BREAKPOINT_PX,
    save_layout_prefs,
};
use crate::canvas_scene::CanvasGeometry;

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

    fn activity_sections() -> [ActivitySection; 6] {
        [
            ActivitySection::Chat,
            ActivitySection::Memory,
            ActivitySection::Tools,
            ActivitySection::Status,
            ActivitySection::Settings,
            ActivitySection::Docs,
        ]
    }

    fn set_active_section(&mut self, section: ActivitySection, cx: &mut Context<Self>) {
        self.state.active_section = section;
        cx.notify();
    }

    fn section_icon(section: ActivitySection) -> IconName {
        match section {
            ActivitySection::Chat => IconName::Bot,
            ActivitySection::Memory => IconName::Inbox,
            ActivitySection::Tools => IconName::SquareTerminal,
            ActivitySection::Status => IconName::ChartPie,
            ActivitySection::Settings => IconName::Settings2,
            ActivitySection::Docs => IconName::BookOpen,
        }
    }

    fn toggle_left_panel(&mut self, cx: &mut Context<Self>) {
        self.state.layout.left_panel_open = !self.state.layout.left_panel_open;
        save_layout_prefs(&self.state.layout);
        cx.notify();
    }

    fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        self.state.layout.right_panel_open = !self.state.layout.right_panel_open;
        save_layout_prefs(&self.state.layout);
        cx.notify();
    }

    fn toggle_surface_mode(&mut self, cx: &mut Context<Self>) {
        self.state.surface_mode = match self.state.surface_mode {
            SurfaceMode::Canvas => SurfaceMode::Shell,
            SurfaceMode::Shell => SurfaceMode::Canvas,
        };
        cx.notify();
    }

    fn resolve_layout_mode(window: &Window) -> LayoutMode {
        let width = window.viewport_size().width;
        if width >= px(DESKTOP_BREAKPOINT_PX) {
            LayoutMode::Desktop
        } else if width >= px(TABLET_BREAKPOINT_PX) {
            LayoutMode::Tablet
        } else {
            LayoutMode::Compact
        }
    }

    fn format_token_count(value: u64) -> String {
        let chars: Vec<char> = value.to_string().chars().rev().collect();
        let mut out = String::new();
        for (index, ch) in chars.iter().enumerate() {
            if index > 0 && index % 3 == 0 {
                out.push(',');
            }
            out.push(*ch);
        }
        out.chars().rev().collect()
    }

    fn refresh_session_usage_totals(&mut self, session_id: String, cx: &mut Context<Self>) {
        let client = self.state.api_client.clone();
        cx.spawn(move |view: WeakEntity<AppView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = client.get_session_by_id(&session_id);
                view.update(&mut cx, |this, cx| {
                    if let Ok(res) = result {
                        this.state.session_usage_totals = res.session_usage_totals;
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn fetch_health(&mut self, cx: &mut Context<Self>) {
        let client = self.state.api_client.clone();
        cx.spawn(move |view: WeakEntity<AppView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if let Ok(health) = client.check_health() {
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
                let result = client.list_sessions();
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
        self.state.session_usage_totals = None;
        cx.notify();

        let client = self.state.api_client.clone();
        cx.spawn(move |view: WeakEntity<AppView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = client.get_session_by_id(&session_id);
                view.update(&mut cx, |this, cx| {
                    this.state.is_loading_messages = false;
                    if let Ok(res) = result {
                        this.state.messages = res.transcript;
                        this.state.session_usage_totals = res.session_usage_totals;
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn start_new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.active_section = ActivitySection::Chat;
        self.state.active_session_id = None;
        self.state.is_loading_messages = false;
        self.state.is_sending_message = false;
        self.state.input_text.clear();
        self.state.messages.clear();
        self.state.session_usage_totals = None;

        self.input_state.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });

        cx.notify();
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
                let result = client.send_message(text, session_id.clone());
                view.update(&mut cx, |this, cx| {
                    this.state.is_sending_message = false;
                    if let Ok(res) = result {
                        if this.state.active_session_id.is_none() {
                            this.state.active_session_id = Some(res.session_id.clone());
                            this.fetch_sessions(cx);
                        }
                        this.state.session_usage_totals = res.session_usage_totals;
                        this.state.messages.push(crate::api::SessionTranscriptMessage {
                            role: "assistant".to_string(),
                            content: res.reply,
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            tool_call_id: None,
                            tool_calls: None,
                        });

                        if this.state.session_usage_totals.is_none() {
                            this.refresh_session_usage_totals(res.session_id, cx);
                        }
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

    fn render_sessions_sidebar(
        &self,
        cx: &mut Context<Self>,
        width_px: f32,
        is_drawer: bool,
    ) -> impl IntoElement {
        let state = &self.state;
        let view = cx.entity().downgrade();

        let mut sidebar_menu = SidebarMenu::new();
        for session in &state.sessions {
            let is_active = state.active_session_id.as_ref() == Some(&session.session_id);
            let session_id = session.session_id.clone();
            let view = view.clone();
            let short_id: String = session.session_id.chars().take(8).collect();
            let label = format!("Session {}", short_id);

            sidebar_menu = sidebar_menu.child(
                SidebarMenuItem::new(label)
                    .active(is_active)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.select_session(session_id.clone(), cx);
                        })
                        .ok();
                    }),
            );
        }

        let mut header_row = h_flex()
            .justify_between()
            .items_center()
            .child(div().text_sm().font_weight(FontWeight::BOLD).child("Sessions"));
        if is_drawer {
            header_row = header_row.child(
                Button::new("close-sessions-drawer")
                    .label("Close")
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_left_panel(cx);
                            })
                            .ok();
                        }
                    }),
            );
        }

        Sidebar::new(gpui_component::Side::Left)
            .w(px(width_px))
            .h_full()
            .header(
                SidebarHeader::new().child(
                    v_flex()
                        .gap_1()
                        .child(header_row)
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if state.is_loading_sessions {
                                    "Loading..."
                                } else {
                                    "Active list"
                                }),
                        ),
                ),
            )
            .footer(
                Button::new("new-session")
                    .label("+ New Session")
                    .w_full()
                    .on_click({
                        let view = view.clone();
                        move |_, window, cx| {
                            view.update(cx, |this, cx| {
                                this.start_new_session(window, cx);
                            })
                            .ok();
                        }
                    }),
            )
            .child(SidebarGroup::new("History").child(sidebar_menu))
    }

    fn render_chat_messages(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.state;
        let theme = cx.theme();
        let message_max_width = match self.state.layout.mode {
            LayoutMode::Desktop => px(760.),
            LayoutMode::Tablet => px(620.),
            LayoutMode::Compact => px(460.),
        };

        let mut messages_view = v_flex().flex_1().overflow_y_scrollbar().p_4().gap_4();
        if state.is_loading_messages {
            messages_view = messages_view.child(div().child("Loading messages..."));
        } else if state.messages.is_empty() {
            messages_view = messages_view.child(div().child("No messages yet."));
        } else {
            for msg in &state.messages {
                let is_user = msg.role == "user";
                let is_error = msg.role == "error";
                let (bg_color, text_color) = if is_error {
                    (theme.accent, theme.accent_foreground)
                } else if is_user {
                    (theme.primary, theme.primary_foreground)
                } else {
                    (theme.secondary, theme.secondary_foreground)
                };

                let mut msg_container = v_flex().w_full();
                if is_user {
                    msg_container = msg_container.items_end();
                } else {
                    msg_container = msg_container.items_start();
                }

                messages_view = messages_view.child(msg_container.child(
                    div()
                        .max_w(message_max_width)
                        .p_3()
                        .rounded_lg()
                        .bg(bg_color)
                        .text_color(text_color)
                        .child(msg.content.clone()),
                ));
            }
        }

        messages_view
    }

    fn render_chat_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.state;
        v_flex()
            .flex_1()
            .h_full()
            .child(self.render_chat_messages(cx))
            .child(
                h_flex()
                    .p_4()
                    .gap_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(div().flex_1().child(Input::new(&self.input_state)))
                    .child(
                        Button::new("send-btn")
                            .label("Send")
                            .primary()
                            .disabled(
                                state.is_sending_message || state.input_text.trim().is_empty(),
                            )
                            .on_click({
                                let view = cx.entity().downgrade();
                                let input_state = self.input_state.clone();
                                move |_, window, cx| {
                                    view.update(cx, |this, cx| {
                                        this.send_message(cx);
                                    })
                                    .ok();
                                    input_state.update(cx, |i, cx| {
                                        i.set_value("", window, cx);
                                    });
                                }
                            }),
                    ),
            )
    }

    fn render_status_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = v_flex().flex_1().p_4().gap_3();
        if let Some(health) = &self.state.health_status {
            content = content
                .child(
                    div()
                        .p_3()
                        .rounded_lg()
                        .bg(cx.theme().secondary)
                        .text_color(cx.theme().secondary_foreground)
                        .child(format!("Supervisor: {}", health.status)),
                )
                .child(div().child(format!("Bot ID: {}", health.bot_id)))
                .child(div().child(format!("Model: {}", health.llm_model)))
                .child(div().child(format!("Provider: {}", health.llm_provider)))
                .child(div().child(format!("Sessions: {}", health.session_count)));
        } else {
            content = content.child(div().child("Health check in progress..."));
        }
        content
    }

    fn render_placeholder_panel(&self, title: String, subtitle: String) -> impl IntoElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_2()
            .child(div().text_lg().font_weight(FontWeight::BOLD).child(title))
            .child(div().text_sm().child(subtitle))
    }

    fn render_section_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = v_flex().flex_1().h_full();
        match self.state.active_section {
            ActivitySection::Chat => panel = panel.child(self.render_chat_view(cx)),
            ActivitySection::Status => panel = panel.child(self.render_status_view(cx)),
            ActivitySection::Memory => {
                panel = panel.child(self.render_placeholder_panel(
                    "Memory Inspector".to_string(),
                    "Read-only memory panel will appear here.".to_string(),
                ))
            }
            ActivitySection::Tools => {
                panel = panel.child(self.render_placeholder_panel(
                    "Tool Trace".to_string(),
                    "Session-wide tool timeline panel placeholder.".to_string(),
                ))
            }
            ActivitySection::Settings => {
                panel = panel.child(self.render_placeholder_panel(
                    "Settings".to_string(),
                    "Theme and API settings panel placeholder.".to_string(),
                ))
            }
            ActivitySection::Docs => {
                panel = panel.child(self.render_placeholder_panel(
                    "Docs".to_string(),
                    "Documentation viewer placeholder.".to_string(),
                ))
            }
        }
        panel
    }

    fn render_canvas_surface(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let left_open = self.state.layout.left_panel_open;
        let right_open = self.state.layout.right_panel_open;
        let hex_fill = cx.theme().secondary;
        let hex_outline = cx.theme().border;
        let icon_fill = if left_open {
            cx.theme().primary
        } else {
            cx.theme().accent
        };
        let icon_glyph = cx.theme().primary_foreground;

        v_flex()
            .size_full()
            .child(
                canvas(
                    |bounds, _, _| CanvasGeometry::from_size(f32::from(bounds.size.width), f32::from(bounds.size.height)),
                    move |bounds, geometry: CanvasGeometry, window, _| {
                        let to_point = |x: f32, y: f32| {
                            point(
                                px(f32::from(bounds.origin.x) + x),
                                px(f32::from(bounds.origin.y) + y),
                            )
                        };

                        let vertices = geometry.hex_vertices();
                        let mut hex_path = Path::new(to_point(vertices[0].0, vertices[0].1));
                        for &(x, y) in &vertices[1..] {
                            hex_path.line_to(to_point(x, y));
                        }
                        hex_path.line_to(to_point(vertices[0].0, vertices[0].1));
                        window.paint_path(hex_path.clone(), hex_outline);

                        let inset_vertices: Vec<(f32, f32)> = vertices
                            .iter()
                            .map(|(x, y)| {
                                let dx = x - geometry.center_x;
                                let dy = y - geometry.center_y;
                                let len = (dx * dx + dy * dy).sqrt();
                                let nx = if len > 0.0 { dx / len } else { 0.0 };
                                let ny = if len > 0.0 { dy / len } else { 0.0 };
                                (x - nx * 3.0, y - ny * 3.0)
                            })
                            .collect();
                        let mut inner_hex = Path::new(to_point(inset_vertices[0].0, inset_vertices[0].1));
                        for &(x, y) in &inset_vertices[1..] {
                            inner_hex.line_to(to_point(x, y));
                        }
                        inner_hex.line_to(to_point(inset_vertices[0].0, inset_vertices[0].1));
                        window.paint_path(inner_hex, hex_fill);

                        let circle_steps = 40usize;
                        let mut icon_points = Vec::with_capacity(circle_steps);
                        for index in 0..circle_steps {
                            let angle = std::f32::consts::TAU / circle_steps as f32 * index as f32;
                            icon_points.push((
                                geometry.center_x + geometry.icon_radius * angle.cos(),
                                geometry.center_y + geometry.icon_radius * angle.sin(),
                            ));
                        }
                        let mut icon_path = Path::new(to_point(icon_points[0].0, icon_points[0].1));
                        for &(x, y) in &icon_points[1..] {
                            icon_path.line_to(to_point(x, y));
                        }
                        icon_path.line_to(to_point(icon_points[0].0, icon_points[0].1));
                        let icon_visible = geometry.icon_contains(geometry.center_x, geometry.center_y);
                        if icon_visible {
                            window.paint_path(icon_path, icon_fill);
                        }

                        let glyph_half = geometry.icon_glyph_half();
                        let bar_thickness = (geometry.icon_radius * 0.24).max(3.0);

                        let h_bar = [
                            (geometry.center_x - glyph_half, geometry.center_y - bar_thickness * 0.5),
                            (geometry.center_x + glyph_half, geometry.center_y - bar_thickness * 0.5),
                            (geometry.center_x + glyph_half, geometry.center_y + bar_thickness * 0.5),
                            (geometry.center_x - glyph_half, geometry.center_y + bar_thickness * 0.5),
                        ];
                        let mut h_path = Path::new(to_point(h_bar[0].0, h_bar[0].1));
                        for &(x, y) in &h_bar[1..] {
                            h_path.line_to(to_point(x, y));
                        }
                        h_path.line_to(to_point(h_bar[0].0, h_bar[0].1));
                        window.paint_path(h_path, icon_glyph);

                        if !right_open {
                            let v_bar = [
                                (geometry.center_x - bar_thickness * 0.5, geometry.center_y - glyph_half),
                                (geometry.center_x + bar_thickness * 0.5, geometry.center_y - glyph_half),
                                (geometry.center_x + bar_thickness * 0.5, geometry.center_y + glyph_half),
                                (geometry.center_x - bar_thickness * 0.5, geometry.center_y + glyph_half),
                            ];
                            let mut v_path = Path::new(to_point(v_bar[0].0, v_bar[0].1));
                            for &(x, y) in &v_bar[1..] {
                                v_path.line_to(to_point(x, y));
                            }
                            v_path.line_to(to_point(v_bar[0].0, v_bar[0].1));
                            window.paint_path(v_path, icon_glyph);
                        }
                    },
                )
                .size_full(),
            )
            .child(
                h_flex()
                    .p_2()
                    .justify_between()
                    .items_center()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Canvas controls panel visibility and section focus."),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("canvas-toggle-sessions")
                                    .label(if self.state.layout.left_panel_open {
                                        "Hide Sessions"
                                    } else {
                                        "Show Sessions"
                                    })
                                    .on_click({
                                        let view = cx.entity().downgrade();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.toggle_left_panel(cx);
                                                this.state.active_section = ActivitySection::Chat;
                                            })
                                            .ok();
                                        }
                                    }),
                            )
                            .child(
                                Button::new("canvas-toggle-context")
                                    .label(if self.state.layout.right_panel_open {
                                        "Hide Context"
                                    } else {
                                        "Show Context"
                                    })
                                    .on_click({
                                        let view = cx.entity().downgrade();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.toggle_right_panel(cx);
                                                this.set_active_section(ActivitySection::Status, cx);
                                            })
                                            .ok();
                                        }
                                    }),
                            ),
                    ),
            )
    }

    fn render_main_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.state.surface_mode {
            SurfaceMode::Canvas => self.render_canvas_surface(cx).into_any_element(),
            SurfaceMode::Shell => self.render_section_content(cx).into_any_element(),
        }
    }

    fn render_right_panel(
        &self,
        cx: &mut Context<Self>,
        width_px: f32,
        is_drawer: bool,
    ) -> impl IntoElement {
        let mut title_row = h_flex()
            .justify_between()
            .items_center()
            .child(div().font_weight(FontWeight::BOLD).child("Context Panel"));

        if is_drawer {
            let view = cx.entity().downgrade();
            title_row = title_row.child(
                Button::new("close-context-drawer")
                    .label("Close")
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.toggle_right_panel(cx);
                        })
                        .ok();
                    }),
            );
        }

        let mut panel = v_flex()
            .w(px(width_px))
            .h_full()
            .p_3()
            .gap_2()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(title_row);

        match self.state.active_section {
            ActivitySection::Chat | ActivitySection::Tools => {
                let mut tool_calls_count = 0usize;
                for message in &self.state.messages {
                    if let Some(calls) = &message.tool_calls {
                        tool_calls_count += calls.len();
                    }
                }
                panel = panel
                    .child(div().text_sm().child(format!("Tool calls in transcript: {}", tool_calls_count)))
                    .child(div().text_sm().child(match self.state.session_usage_totals.as_ref() {
                        Some(usage) => format!(
                            "Session total tokens: {}",
                            Self::format_token_count(usage.total_tokens)
                        ),
                        None => "Session total tokens: Unavailable".to_string(),
                    }))
                    .child(div().text_sm().child(match self.state.session_usage_totals.as_ref() {
                        Some(usage) => format!("Session total spend (USD): ${:.4}", usage.estimated_cost_usd),
                        None => "Session total spend (USD): Unavailable".to_string(),
                    }))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Detailed tool trace view is the next step."));
            }
            ActivitySection::Status => {
                panel = panel.child(div().text_sm().child("Subsystem status expansion can render here."));
            }
            _ => {
                panel = panel.child(div().text_sm().child("Panel-specific controls will appear here."));
            }
        }

        panel
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session_short = self
            .state
            .active_session_id
            .as_deref()
            .map(|id| &id[..8])
            .unwrap_or("none");

        let left_group = h_flex()
            .items_center()
            .gap_3()
            .child(div().text_sm().font_weight(FontWeight::BOLD).child("Araliya"))
            .child(div().text_sm().child(format!("Session: {}", session_short)))
            .child(div().text_sm().child(format!("Messages: {}", self.state.messages.len())));

        let right_group = h_flex()
            .items_center()
            .gap_3()
            .child(div().text_sm().child(format!("Section: {}", self.state.active_section.label())))
            .child(div().text_sm().child(format!("Mode: {}", self.state.layout.mode.label())))
            .child(div().text_sm().child(format!("Surface: {}", self.state.surface_mode.label())));

        h_flex()
            .w_full()
            .p_2()
            .items_center()
            .justify_between()
            .bg(cx.theme().primary)
            .text_color(cx.theme().primary_foreground)
            .child(left_group)
            .child(right_group)
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.state.layout.mode = Self::resolve_layout_mode(window);

        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        let muted_foreground = cx.theme().muted_foreground;
        let is_desktop = matches!(self.state.layout.mode, LayoutMode::Desktop);
        let left_panel_width = self.state.layout.left_panel_width.clamp(220.0, 360.0);
        let right_panel_width = self.state.layout.right_panel_width.clamp(280.0, 420.0);
        let show_left_inline = is_desktop && self.state.layout.left_panel_open;
        let show_right_inline = is_desktop && self.state.layout.right_panel_open;
        let show_left_drawer = !is_desktop && self.state.layout.left_panel_open;
        let show_right_drawer = !is_desktop && self.state.layout.right_panel_open;
        let health_text = match &self.state.health_status {
            Some(h) => format!("{} · {}", h.status, h.llm_model),
            None => "Checking health...".to_string(),
        };

        let mut activity_rail = v_flex()
            .w(px(56.))
            .h_full()
            .p_2()
            .gap_2()
            .items_center()
            .justify_start()
            .border_r_1()
            .border_color(border);
        for (index, section) in Self::activity_sections().into_iter().enumerate() {
            let section_label = section.label().to_string();
            let icon = Self::section_icon(section);
            let is_active = self.state.active_section == section;
            let view = cx.entity().downgrade();
            let mut button = Button::new(("activity", index))
                .icon(Icon::new(icon).small())
                .compact()
                .tooltip(section_label.clone());
            if is_active {
                button = button.primary();
            }

            activity_rail = activity_rail.child(button.on_click(move |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.set_active_section(section, cx);
                })
                .ok();
            }));
        }

        let header = h_flex()
            .w_full()
            .p_3()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .items_center()
            .child(div().font_weight(FontWeight::BOLD).child("Araliya"))
            .child(div().text_sm().text_color(muted_foreground).child(self.state.active_section.label()))
            .child(div().flex_1())
            .child(div().text_sm().text_color(muted_foreground).child(self.state.layout.mode.label()))
            .child(div().text_sm().text_color(muted_foreground).child(health_text))
            .child(
                Button::new("toggle-surface")
                    .label(match self.state.surface_mode {
                        SurfaceMode::Canvas => "Use Shell",
                        SurfaceMode::Shell => "Use Canvas",
                    })
                    .on_click({
                        let view = cx.entity().downgrade();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_surface_mode(cx);
                            })
                            .ok();
                        }
                    }),
            )
            .child(
                Button::new("toggle-left")
                    .label(if self.state.layout.left_panel_open { "Hide Sessions" } else { "Show Sessions" })
                    .on_click({
                        let view = cx.entity().downgrade();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_left_panel(cx);
                            })
                            .ok();
                        }
                    }),
            )
            .child(
                Button::new("toggle-right")
                    .label(if self.state.layout.right_panel_open { "Hide Context" } else { "Show Context" })
                    .on_click({
                        let view = cx.entity().downgrade();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.toggle_right_panel(cx);
                            })
                            .ok();
                        }
                    }),
            );

        let mut panel_row = h_flex().flex_1().w_full().h_full();
        if show_right_drawer {
            panel_row = panel_row.child(
                v_flex()
                    .w_full()
                    .h_full()
                    .child(self.render_right_panel(cx, right_panel_width, true)),
            );
        } else if show_left_drawer {
            panel_row = panel_row.child(
                v_flex()
                    .w_full()
                    .h_full()
                    .child(self.render_sessions_sidebar(cx, left_panel_width, true)),
            );
        } else {
            if show_left_inline {
                panel_row = panel_row.child(self.render_sessions_sidebar(cx, left_panel_width, false));
            }

            panel_row = panel_row.child(v_flex().flex_1().h_full().child(self.render_main_panel(cx)));

            if show_right_inline {
                panel_row = panel_row.child(self.render_right_panel(cx, right_panel_width, false));
            }
        }

        h_flex()
            .size_full()
            .items_start()
            .bg(background)
            .text_color(foreground)
            .child(activity_rail)
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .child(header)
                    .child(panel_row)
                    .child(self.render_status_bar(cx))
            )
    }
}
