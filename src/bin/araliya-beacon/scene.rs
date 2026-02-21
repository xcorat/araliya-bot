
//! Vello scene builder for the beacon widget.
//!
//! Layout (160 × 80 logical px):
//!
//!   ┌────────────────────────────────────────────┐
//!   │          ╱‾‾╲         ╭──────╮            │
//!   │        ╱ DOT ╲        │  ●   │  beacon    │
//!   │       ╲  HEX ╱        │button│            │
//!   │         ╲──╱          ╰──────╯            │
//!   └────────────────────────────────────────────┘
//!    ← hex body (draggable) →← status button →
//!
//! The hex body is transparent-background, drawn over a fully-transparent
//! window. The "ping" button on the right triggers the IPC health call.

use vello::kurbo::{Affine, BezPath, Circle, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;

// ── Layout constants ──────────────────────────────────────────────────────

const HEX_CX: f64 = 66.0;
const HEX_CY: f64 = 40.0;
const HEX_R: f64 = 32.0;

/// Centre and radius of the "ping" status button.
pub const BTN_CX: f64 = 130.0;
pub const BTN_CY: f64 = 40.0;
pub const BTN_R: f64 = 18.0;

/// Returns true when (x, y) falls within the ping button.
pub fn is_button_hit(x: f64, y: f64) -> bool {
    let dx = x - BTN_CX;
    let dy = y - BTN_CY;
    dx * dx + dy * dy <= BTN_R * BTN_R
}

// ── Scene builder ─────────────────────────────────────────────────────────

/// Build one frame of the beacon scene into `scene`.
///
/// `status` is `None` while idle, `Some(text)` after the last IPC reply.
pub fn build(scene: &mut Scene, status: Option<&str>) {
    // ── Hex body ──────────────────────────────────────────────────────────

    let hex = hex_path(HEX_CX, HEX_CY, HEX_R);

    // Fill
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::rgba8(14, 14, 22, 210),
        None,
        &hex,
    );

    // Outline
    scene.stroke(
        &Stroke::new(1.5),
        Affine::IDENTITY,
        Color::rgba8(70, 170, 240, 180),
        None,
        &hex,
    );

    // ── Status indicator dot at hex centre ────────────────────────────────

    let dot_color = match status {
        None => Color::rgba8(70, 70, 90, 190),
        Some(s) if s.starts_with("err") => Color::rgba8(220, 60, 60, 230),
        Some(_) => Color::rgba8(50, 210, 110, 230),
    };

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        dot_color,
        None,
        &Circle::new((HEX_CX, HEX_CY), 9.0),
    );

    // Inner ring on dot
    scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        Color::rgba8(200, 200, 220, 100),
        None,
        &Circle::new((HEX_CX, HEX_CY), 9.0),
    );

    // ── Ping button ───────────────────────────────────────────────────────

    // Background pill
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::rgba8(20, 20, 36, 200),
        None,
        &Circle::new((BTN_CX, BTN_CY), BTN_R),
    );

    // Outer ring
    scene.stroke(
        &Stroke::new(1.5),
        Affine::IDENTITY,
        Color::rgba8(100, 160, 255, 200),
        None,
        &Circle::new((BTN_CX, BTN_CY), BTN_R),
    );

    // Inner send-pulse icon (three concentric arcs faking a signal icon)
    for (i, r) in [4.0_f64, 8.0, 12.0].iter().enumerate() {
        let a = 0.5 * (i as f64 + 1.0); // fade outer arcs
        let alpha = (180.0 * a / 3.0) as u8;
        scene.stroke(
            &Stroke::new(1.2),
            Affine::IDENTITY,
            Color::rgba8(100, 160, 255, alpha),
            None,
            &Circle::new((BTN_CX, BTN_CY), *r),
        );
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn hex_path(cx: f64, cy: f64 , r: f64) -> BezPath {
    let mut path = BezPath::new();
    for i in 0..6_u32 {
        let angle = std::f64::consts::FRAC_PI_3 * i as f64;
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
