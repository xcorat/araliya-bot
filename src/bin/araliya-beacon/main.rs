
//! `araliya-beacon` — floating GPU-canvas beacon widget.
//!
//! A small, always-on-top, borderless, transparent window rendered entirely via
//! `vello` (2D GPU vector renderer) on top of a `wgpu` surface. No widget tree.
//!
//! Architecture:
//!   - Main thread:    winit event loop + vello/wgpu rendering
//!   - Tokio thread:   IPC socket client (sends commands to daemon)
//!   - Channel bridge: `EventLoopProxy<UiMessage>` for thread-safe UI updates
//!
//! Interaction:
//!   - Click + drag anywhere on the hex body → moves the window (OS-native drag)
//!   - Click the "ping" button (right circle) → sends Status to daemon

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

// ── Messages from the tokio thread to the UI thread ────────────────────────

#[derive(Debug)]
enum UiMessage {
    IpcResult(String),
}

// ── Render state (wgpu + vello) ────────────────────────────────────────────

struct RenderState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

// ── Application state ───────────────────────────────────────────────────────

struct BeaconApp {
    window: Option<Arc<Window>>,
    render: Option<RenderState>,
    proxy: EventLoopProxy<UiMessage>,
    // Latest daemon response, shown via dot colour
    status_text: Option<String>,
    // Current cursor position in physical pixels
    cursor: (f64, f64),
    // State for distinguishing click vs drag on the hex body
    mouse_pressed: bool,
    is_dragging: bool,
    press_pos: Option<(f64, f64)>,
    drag_start_win_pos: Option<(i32, i32)>,
    // Tracked araliya-gpui child process (singleton)
    gpui_child: Option<std::process::Child>,
    // Tokio runtime for async IPC calls
    rt: tokio::runtime::Runtime,
}

impl BeaconApp {
    fn new(proxy: EventLoopProxy<UiMessage>) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("beacon-ipc")
            .build()
            .expect("tokio runtime");

        Self {
            window: None,
            render: None,
            proxy,
            status_text: None,
            cursor: (0.0, 0.0),
            mouse_pressed: false,
            is_dragging: false,
            press_pos: None,
            drag_start_win_pos: None,
            gpui_child: None,
            rt,
        }
    }

    // ── Launch sibling GUI ──────────────────────────────────────────────────

    fn launch_gpui(&mut self) {
        eprintln!("[beacon] launch_gpui called");

        // ── Singleton check ───────────────────────────────────────────────
        if let Some(child) = &mut self.gpui_child {
            match child.try_wait() {
                Ok(None) => {
                    // Still running — foreground it instead of spawning again.
                    let pid = child.id();
                    eprintln!("[beacon] araliya-gpui already running (pid={pid}), foregrounding");
                    Self::foreground_by_pid(pid);
                    return;
                }
                Ok(Some(status)) => {
                    eprintln!("[beacon] previous araliya-gpui exited ({status}), respawning");
                    self.gpui_child = None;
                }
                Err(e) => {
                    eprintln!("[beacon] try_wait error: {e}, clearing child");
                    self.gpui_child = None;
                }
            }
        }

        // ── Resolve binary ────────────────────────────────────────────────
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[beacon] ERROR: can't resolve own exe path: {e}");
                return;
            }
        };
        let dir = match exe.parent() {
            Some(d) => d.to_path_buf(),
            None => {
                eprintln!("[beacon] ERROR: exe has no parent dir");
                return;
            }
        };
        let candidate = dir.join("araliya-gpui");
        eprintln!("[beacon] binary dir: {:?}", dir);
        eprintln!("[beacon] checking {:?} → exists={}", candidate, candidate.exists());

        if !candidate.exists() {
            eprintln!("[beacon] araliya-gpui not found at {:?}", candidate);
            return;
        }

        match std::process::Command::new(&candidate).spawn() {
            Ok(child) => {
                eprintln!("[beacon] spawned araliya-gpui pid={}", child.id());
                self.gpui_child = Some(child);
            }
            Err(e) => eprintln!("[beacon] ERROR spawn failed: {e}"),
        }
    }

    /// Best-effort window raise for a child process.
    ///
    /// On X11 desktops `wmctrl` is commonly available; on Wayland there is no
    /// standardised way to raise another app's window from outside it, so we
    /// just log that we tried.
    ///
    /// _TODO_: replace this entire function with a direct IPC call once
    /// araliya-gpui exposes a focus/raise command on its own Unix socket.
    /// At that point: connect to araliya-gpui's socket, send `{"cmd":"focus"}`,
    /// and remove the wmctrl/xdotool dependency entirely.
    fn foreground_by_pid(pid: u32) {
        // Try wmctrl (X11): raise the window whose _NET_WM_PID matches.
        let wmctrl = std::process::Command::new("wmctrl")
            .args(["-p", "-a", &pid.to_string()])
            .status();
        match wmctrl {
            Ok(s) if s.success() => {
                eprintln!("[beacon] wmctrl raised window for pid={pid}");
            }
            Ok(s) => {
                eprintln!("[beacon] wmctrl failed (exit={s}); trying xdotool");
                // Try xdotool as fallback (also X11).
                let _ = std::process::Command::new("xdotool")
                    .args(["search", "--pid", &pid.to_string(), "windowactivate", "--sync", "%@"])
                    .status();
            }
            Err(e) => {
                eprintln!("[beacon] wmctrl not available ({e}); window raise not supported on this compositor");
            }
        }
    }

    // ── IPC ────────────────────────────────────────────────────────────────

    fn ping_status(&self) {
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let result = ipc::send_command(ipc::Command::Status).await;
            let text = match result {
                Ok(s) => s,
                Err(e) => format!("err: {e}"),
            };
            eprintln!("[beacon] status → {text}");
            let _ = proxy.send_event(UiMessage::IpcResult(text));
        });
    }

    // ── Rendering ──────────────────────────────────────────────────────────

    fn redraw(&mut self) {
        let Some(rs) = &mut self.render else { return };
        let Some(window) = &self.window else { return };

        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let surface_texture = match rs.surface.get_current_texture() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[beacon] surface error: {e:?}");
                window.request_redraw();
                return;
            }
        };

        let mut scene = Scene::new();
        scene::build(&mut scene, self.status_text.as_deref());

        let render_params = vello::RenderParams {
            base_color: Color::from_rgba8(0, 0, 0, 0),
            width: size.width,
            height: size.height,
            antialiasing_method: AaConfig::Area,
        };

        if let Err(e) =
            rs.renderer
                .render_to_surface(&rs.device, &rs.queue, &scene, &surface_texture, &render_params)
        {
            eprintln!("[beacon] render error: {e:?}");
        }

        surface_texture.present();
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    // ── wgpu + vello init (called from resumed()) ──────────────────────────

    fn init_render(window: Arc<Window>) -> RenderState {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // Use all available backends; wgpu selects the best for the platform.
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Safety: window lives as long as the surface (both inside BeaconApp).
        let surface = instance
            .create_surface(window.clone())
            .expect("wgpu surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .block_on()
            .expect("wgpu adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("beacon"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .block_on()
            .expect("wgpu device");

        let caps = surface.get_capabilities(&adapter);
        eprintln!("[beacon] surface formats: {:?}", caps.formats);
        eprintln!("[beacon] alpha modes:     {:?}", caps.alpha_modes);

        // Vello only accepts Bgra8Unorm or Rgba8Unorm (not sRGB variants).
        // If the driver only exposes sRGB variants (e.g. llvmpipe), strip the
        // suffix so the surface is configured with a vello-compatible format.
        let format = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ]
        .iter()
        .find(|f| caps.formats.contains(f))
        .copied()
        .unwrap_or_else(|| {
            let fallback = caps.formats[0].remove_srgb_suffix();
            eprintln!(
                "[beacon] no non-sRGB format in caps; stripping sRGB suffix: {:?} → {:?}",
                caps.formats[0], fallback
            );
            fallback
        });

        // Pick the first alpha mode that supports transparency.
        let alpha_mode = [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
        ]
        .iter()
        .find(|m| caps.alpha_modes.contains(m))
        .copied()
        .unwrap_or(caps.alpha_modes[0]);

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let renderer = Renderer::new(
            &device,
            RendererOptions {
                surface_format: Some(format),
                use_cpu: false,
                antialiasing_support: vello::AaSupport::area_only(),
                num_init_threads: NonZeroUsize::new(1),
            },
        )
        .expect("vello renderer");

        RenderState { surface, device, queue, config, renderer }
    }
}

// ── ApplicationHandler ──────────────────────────────────────────────────────

impl ApplicationHandler<UiMessage> for BeaconApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("araliya-beacon")
            .with_inner_size(LogicalSize::new(160_u32, 80_u32))
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_window_level(WindowLevel::AlwaysOnTop);

        let window = Arc::new(
            event_loop.create_window(attrs).expect("create window"),
        );

        let render = Self::init_render(window.clone());
        self.render = Some(render);
        self.window = Some(window);
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                // Manual drag: move window by the delta from press position.
                if self.mouse_pressed {
                    if let (Some((px, py)), Some((wx, wy))) =
                        (self.press_pos, self.drag_start_win_pos)
                    {
                        let dx = position.x - px;
                        let dy = position.y - py;
                        if dx.abs() > 3.0 || dy.abs() > 3.0 {
                            if !self.is_dragging {
                                eprintln!("[beacon] drag started");
                                self.is_dragging = true;
                            }
                            if let Some(w) = &self.window {
                                w.set_outer_position(winit::dpi::PhysicalPosition::new(
                                    wx + dx as i32,
                                    wy + dy as i32,
                                ));
                            }
                        }
                    }
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let is_btn = scene::is_button_hit(self.cursor.0, self.cursor.1);
                eprintln!("[beacon] LMB pressed at ({:.1},{:.1}) is_button={is_btn}",
                    self.cursor.0, self.cursor.1);
                if is_btn {
                    self.ping_status();
                } else {
                    self.mouse_pressed = true;
                    self.is_dragging = false;
                    self.press_pos = Some(self.cursor);
                    self.drag_start_win_pos = self.window
                        .as_ref()
                        .and_then(|w| w.outer_position().ok())
                        .map(|p| (p.x, p.y));
                    eprintln!("[beacon] press recorded, win_pos={:?}", self.drag_start_win_pos);
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                let is_btn = scene::is_button_hit(self.cursor.0, self.cursor.1);
                eprintln!("[beacon] LMB released at ({:.1},{:.1}) is_button={is_btn} mouse_pressed={} is_dragging={}",
                    self.cursor.0, self.cursor.1, self.mouse_pressed, self.is_dragging);
                if self.mouse_pressed && !is_btn {
                    if !self.is_dragging {
                        eprintln!("[beacon] click detected → launch_gpui");
                        self.launch_gpui();
                    }
                }
                self.mouse_pressed = false;
                self.is_dragging = false;
                self.press_pos = None;
                self.drag_start_win_pos = None;
            }

            WindowEvent::Resized(new_size) => {
                if let Some(rs) = &mut self.render {
                    rs.config.width = new_size.width.max(1);
                    rs.config.height = new_size.height.max(1);
                    rs.surface.configure(&rs.device, &rs.config);
                }
                self.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                self.redraw();
            }

            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UiMessage) {
        match event {
            UiMessage::IpcResult(text) => {
                self.status_text = Some(text);
                self.request_redraw();
            }
        }
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    let event_loop = EventLoop::<UiMessage>::with_user_event()
        .build()
        .expect("event loop");

    let proxy = event_loop.create_proxy();
    let mut app = BeaconApp::new(proxy);

    event_loop.run_app(&mut app).expect("run");
}
