
//! Vello scene builder for the beacon widget.
//!
//! Minimal: filled hexagon body + status dot + ping button circle.
//! Layout (160 × 80 logical px):
//!
//!   ┌──────────────────────┐
//!   │   ╱‾‾╲     ●        │
//!   │  ╱ ·  ╲  [ping]     │
//!   │  ╲    ╱             │
//!   │   ╲──╱              │
//!   └──────────────────────┘

use vello::kurbo::{Affine, BezPath, Circle, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;

// ── Layout constants ──────────────────────────────────────────────────────

const HEX_CX: f64 = 55.0;
const HEX_CY: f64 = 40.0;
const HEX_R: f64 = 30.0;

/// Centre and radius of the "ping" status button.
pub const BTN_CX: f64 = 128.0;
pub const BTN_CY: f64 = 40.0;
pub const BTN_R: f64 = 20.0;

/// Returns true when (x, y) falls within the ping button.
pub fn is_button_hit(x: f64, y: f64) -> bool {
    let dx = x - BTN_CX;
    let dy = y - BTN_CY;
    dx * dx + dy * dy <= BTN_R * BTN_R
}

// ── Scene builder ─────────────────────────────────────────────────────────

/// Build one frame of the beacon scene into `scene`.
///
/// `status` is `None` while idle, `Some(text)` after the last IPC reply (shown
/// as green dot), `Some("err:…")` shown as red dot.
pub fn build(scene: &mut Scene, status: Option<&str>) {
    // ── Hex body (filled polygon) ─────────────────────────────────────────

    let hex = hex_path(HEX_CX, HEX_CY, HEX_R);

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(18, 18, 28, 230),
        None,
        &hex,
    );
    scene.stroke(
        &Stroke::new(2.0),
        Affine::IDENTITY,
        Color::from_rgba8(80, 160, 255, 200),
        None,
        &hex,
    );

    // ── Status dot at hex centre ──────────────────────────────────────────

    let dot_color = match status {
        None => Color::from_rgba8(80, 80, 100, 200),
        Some(s) if s.starts_with("err") => Color::from_rgba8(220, 60, 60, 240),
        Some(_) => Color::from_rgba8(60, 210, 120, 240),
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        dot_color,
        None,
        &Circle::new((HEX_CX, HEX_CY), 8.0),
    );

    // ── Ping button (filled circle + outline) ─────────────────────────────

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(24, 24, 40, 220),
        None,
        &Circle::new((BTN_CX, BTN_CY), BTN_R),
    );
    scene.stroke(
        &Stroke::new(2.0),
        Affine::IDENTITY,
        Color::from_rgba8(80, 160, 255, 200),
        None,
        &Circle::new((BTN_CX, BTN_CY), BTN_R),
    );

    // tiny inner dot as "send" icon
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(120, 180, 255, 220),
        None,
        &Circle::new((BTN_CX, BTN_CY), 5.0),
    );
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn hex_path(cx: f64, cy: f64, r: f64) -> BezPath {
    let mut path = BezPath::new();
    for i in 0..6_u32 {
        // Start at top (subtract FRAC_PI_2 so flat edge is at top)
        let angle = std::f64::consts::FRAC_PI_3 * i as f64 - std::f64::consts::FRAC_PI_2;
        let x = cx + r * angle.cos();
        let y = cy + r * angle.sin();
        if i == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    path.close_path();
    path
}
