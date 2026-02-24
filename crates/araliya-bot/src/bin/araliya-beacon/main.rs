//! `araliya-beacon` — floating GPU-canvas beacon widget.
//!
//! Fixed-size window (230×230 logical px). Main hex at bottom-right of the
//! canvas; three control hexes appear in the top-left transparent region on
//! hover or when pinned. No window resizing or moving.
//!
//! Architecture:
//!   Main thread:    winit event loop + vello/wgpu rendering
//!   Tokio thread:   IPC socket client
//!   Bridge:         EventLoopProxy<UiMessage>

mod ipc;
mod scene;

use std::num::NonZeroUsize;
use std::sync::Arc;

use pollster::FutureExt as _;
use vello::peniko::Color;
use vello::{AaConfig, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowLevel};

use scene::Hit;

#[derive(Debug)]
enum UiMessage { IpcResult(String) }

struct RenderState {
    surface:  wgpu::Surface<'static>,
    device:   wgpu::Device,
    queue:    wgpu::Queue,
    config:   wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

struct BeaconApp {
    window: Option<Arc<Window>>,
    render: Option<RenderState>,
    proxy:  EventLoopProxy<UiMessage>,

    status_text: Option<String>,
    cursor:      (f64, f64),

    // hover_over_main: cursor is over the main hex (used for drag / pin logic).
    // hover_over_any:  cursor is over main hex OR any control hex;
    //                  drives controls_visible() to keep controls up while
    //                  moving from the main hex toward a button.
    hover_over_main: bool,
    hover_over_any:  bool,
    extras_pinned:   bool,

    mouse_pressed: bool,
    is_dragging:   bool,
    press_pos:     Option<(f64, f64)>,

    gpui_child: Option<std::process::Child>,
    rt:         tokio::runtime::Runtime,
}

impl BeaconApp {
    fn new(proxy: EventLoopProxy<UiMessage>) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("beacon-ipc")
            .build()
            .expect("tokio runtime");
        Self {
            window: None, render: None, proxy,
            status_text: None,
            cursor: (0.0, 0.0),
            hover_over_main: false,
            hover_over_any:  false,
            extras_pinned:   false,
            mouse_pressed: false, is_dragging: false, press_pos: None,
            gpui_child: None, rt,
        }
    }

    fn controls_visible(&self) -> bool {
        self.hover_over_any || self.extras_pinned
    }

    // ── Launch sibling GUI ─────────────────────────────────────────────────

    fn launch_gpui(&mut self) {
        eprintln!("[beacon] launch_gpui");
        if let Some(child) = &mut self.gpui_child {
            match child.try_wait() {
                Ok(None)    => { let pid = child.id(); Self::foreground_by_pid(pid); return; }
                Ok(Some(s)) => { eprintln!("[beacon] gpui exited ({s})"); self.gpui_child = None; }
                Err(e)      => { eprintln!("[beacon] try_wait: {e}"); self.gpui_child = None; }
            }
        }
        let exe = std::env::current_exe().expect("current_exe");
        let bin = exe.parent().expect("exe parent").join("araliya-gpui");
        if !bin.exists() { eprintln!("[beacon] araliya-gpui not found"); return; }
        match std::process::Command::new(&bin).spawn() {
            Ok(c)  => { eprintln!("[beacon] spawned pid={}", c.id()); self.gpui_child = Some(c); }
            Err(e) => eprintln!("[beacon] spawn failed: {e}"),
        }
    }

    fn foreground_by_pid(pid: u32) {
        // TODO: replace with IPC focus command once araliya-gpui exposes one.
        let ok = std::process::Command::new("wmctrl")
            .args(["-p", "-a", &pid.to_string()])
            .status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            let _ = std::process::Command::new("xdotool")
                .args(["search", "--pid", &pid.to_string(), "windowactivate", "--sync", "%@"])
                .status();
        }
    }

    fn open_management_ui(&mut self) {
        eprintln!("[beacon] management UI — not yet implemented");
        // TODO: spawn or IPC to management UI.
    }

    // ── IPC ────────────────────────────────────────────────────────────────

    fn ping_status(&self) {
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let text = match ipc::send_command(ipc::Command::Status).await {
                Ok(s)  => s,
                Err(e) => format!("err: {e}"),
            };
            eprintln!("[beacon] status → {text}");
            let _ = proxy.send_event(UiMessage::IpcResult(text));
        });
    }

    // ── Rendering ─────────────────────────────────────────────────────────

    fn redraw(&mut self) {
        let cv          = self.controls_visible();
        let status_text = self.status_text.clone();

        let Some(rs)     = &mut self.render else { return };
        let Some(window) = &self.window     else { return };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 { return; }

        let tex = match rs.surface.get_current_texture() {
            Ok(t)  => t,
            Err(e) => { eprintln!("[beacon] surface: {e:?}"); window.request_redraw(); return; }
        };

        let mut vscene = Scene::new();
        scene::build(&mut vscene, status_text.as_deref(), cv);

        let params = vello::RenderParams {
            base_color: Color::from_rgba8(0, 0, 0, 0),
            width: size.width, height: size.height,
            antialiasing_method: AaConfig::Area,
        };
        if let Err(e) = rs.renderer.render_to_surface(&rs.device, &rs.queue, &vscene, &tex, &params) {
            eprintln!("[beacon] render: {e:?}");
        }
        tex.present();
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window { w.request_redraw(); }
    }

    // ── wgpu / vello init ──────────────────────────────────────────────────

    fn init_render(window: Arc<Window>) -> RenderState {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(), ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).block_on().expect("adapter");
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("beacon"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        }, None).block_on().expect("device");

        let caps = surface.get_capabilities(&adapter);
        let format = [wgpu::TextureFormat::Bgra8Unorm, wgpu::TextureFormat::Rgba8Unorm]
            .iter().find(|f| caps.formats.contains(f)).copied()
            .unwrap_or_else(|| caps.formats[0].remove_srgb_suffix());
        let alpha_mode = [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
        ].iter().find(|m| caps.alpha_modes.contains(m)).copied()
            .unwrap_or(caps.alpha_modes[0]);

        let sz = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width:  sz.width.max(1),
            height: sz.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let renderer = Renderer::new(&device, RendererOptions {
            surface_format: Some(format),
            use_cpu: false,
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: NonZeroUsize::new(1),
        }).expect("renderer");

        RenderState { surface, device, queue, config, renderer }
    }
}

// ── ApplicationHandler ─────────────────────────────────────────────────────

impl ApplicationHandler<UiMessage> for BeaconApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }

        let attrs = WindowAttributes::default()
            .with_title("araliya-beacon")
            .with_inner_size(LogicalSize::new(scene::WINDOW_W, scene::WINDOW_H))
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_window_level(WindowLevel::AlwaysOnTop);

        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        self.render = Some(Self::init_render(window.clone()));
        self.window = Some(window);
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            // ── Cursor moved ──────────────────────────────────────────────
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().map(|w| w.scale_factor()).unwrap_or(1.0);
                let lx = position.x / scale;
                let ly = position.y / scale;
                self.cursor = (lx, ly);

                let prev_cv = self.controls_visible();
                let hit     = scene::hit_test(lx, ly, prev_cv);
                self.hover_over_main = hit == Hit::MainHex;
                self.hover_over_any  = hit != Hit::Nothing;
                let new_cv  = self.controls_visible();

                if new_cv != prev_cv {
                    self.request_redraw();
                }

                // Drag threshold.
                if self.mouse_pressed && !self.is_dragging {
                    if let Some((px, py)) = self.press_pos {
                        if (lx-px).powi(2) + (ly-py).powi(2) > 25.0 {
                            eprintln!("[beacon] drag");
                            self.is_dragging   = true;
                            self.extras_pinned = false; // clear pin on drag
                            if let Some(w) = &self.window { let _ = w.drag_window(); }
                        }
                    }
                }
            }

            // ── Cursor left — clear hover ──────────────────────────────────
            WindowEvent::CursorLeft { .. } => {
                let prev_cv = self.controls_visible();
                self.hover_over_main = false;
                self.hover_over_any  = false;
                if self.controls_visible() != prev_cv {
                    self.request_redraw();
                }
            }

            // ── Mouse press ───────────────────────────────────────────────
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let (lx, ly) = self.cursor;
                let hit = scene::hit_test(lx, ly, self.controls_visible());
                eprintln!("[beacon] press ({lx:.0},{ly:.0}) {hit:?}");
                if hit != Hit::Nothing {
                    self.mouse_pressed = true;
                    self.is_dragging   = false;
                    self.press_pos     = Some((lx, ly));
                }
            }

            // ── Mouse release ─────────────────────────────────────────────
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                let (lx, ly)     = self.cursor;
                let cv           = self.controls_visible();
                let hit          = scene::hit_test(lx, ly, cv);
                let was_pressed  = self.mouse_pressed;
                let was_dragging = self.is_dragging;
                self.mouse_pressed = false;
                self.is_dragging   = false;
                self.press_pos     = None;

                eprintln!("[beacon] release ({lx:.0},{ly:.0}) {hit:?} drag={was_dragging}");

                if was_pressed && !was_dragging {
                    match hit {
                        Hit::Close    => event_loop.exit(),
                        Hit::Ui       => self.launch_gpui(),
                        Hit::Settings => self.open_management_ui(),
                        Hit::MainHex  => {
                            self.extras_pinned = !self.extras_pinned;
                            eprintln!("[beacon] pin={}", self.extras_pinned);
                            self.request_redraw();
                        }
                        Hit::Nothing => {}
                    }
                }
                // drag-release: do nothing
            }

            // ── Surface resize (OS-initiated or from apply_window_size) ───
            WindowEvent::Resized(sz) => {
                if let Some(rs) = &mut self.render {
                    rs.config.width  = sz.width.max(1);
                    rs.config.height = sz.height.max(1);
                    rs.surface.configure(&rs.device, &rs.config);
                }
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => self.redraw(),

            _ => {}
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UiMessage) {
        match event {
            UiMessage::IpcResult(text) => {
                self.status_text = Some(text);
                self.request_redraw();
            }
        }
    }
}

fn main() {
    let event_loop = EventLoop::<UiMessage>::with_user_event().build().expect("event loop");
    let proxy = event_loop.create_proxy();
    let mut app = BeaconApp::new(proxy);
    event_loop.run_app(&mut app).expect("run");
}
