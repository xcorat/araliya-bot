//! Vello scene builder for the beacon widget.
//!
//! Single fixed canvas (230×230 logical px). The main hexagon is anchored at
//! the bottom-right of the canvas. When controls are visible (hover or pin),
//! three control hexagons are drawn in the top-left transparent region.
//! No window resizing; the window is created at this size and never changed.
//!
//!   ⬡ Close
//!       ⬡ UI
//!           ⬡ Settings
//!                    ⬡ main  ← fixed position

use vello::kurbo::{Affine, BezPath, Circle, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;

// ── Canvas size ──────────────────────────────────────────────────────────

pub const WINDOW_W: u32 = 230;
pub const WINDOW_H: u32 = 230;

// ── Main hex — fixed position (bottom-right of canvas) ───────────────────

const MAIN_R: f64 = 36.0;
const HEX_CX: f64 = 185.0;
const HEX_CY: f64 = 185.0;

// ── Control hexes ─────────────────────────────────────────────────────────

const CTRL_R:    f64 = 24.0;
const CTRL_STEP: f64 = 68.0;
const DIAG:      f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Index 1 = Settings (closest to main), 2 = UI, 3 = Close.
fn ctrl_centre(index: u32) -> (f64, f64) {
    let d = CTRL_STEP * index as f64;
    (HEX_CX - DIAG * d, HEX_CY - DIAG * d)
}

fn settings_centre() -> (f64, f64) { ctrl_centre(1) }
fn ui_centre()       -> (f64, f64) { ctrl_centre(2) }
fn close_centre()    -> (f64, f64) { ctrl_centre(3) }

// ── Hit testing ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit { MainHex, Close, Ui, Settings, Nothing }

fn hit_hex(cx: f64, cy: f64, r: f64, x: f64, y: f64) -> bool {
    let dx = (x - cx).abs();
    let dy = (y - cy).abs();
    let half_h = r * (3.0_f64.sqrt() / 2.0);
    if dx > r || dy > half_h { return false; }
    dx * 0.5 + dy * (3.0_f64.sqrt() / 2.0) <= r * (3.0_f64.sqrt() / 2.0)
}

/// What is the cursor at logical `(x, y)` touching?
///
/// Control hexes are only tested when `controls_visible` is true.
pub fn hit_test(x: f64, y: f64, controls_visible: bool) -> Hit {
    if controls_visible {
        let (cx, cy) = close_centre();
        if hit_hex(cx, cy, CTRL_R, x, y) { return Hit::Close; }
        let (ux, uy) = ui_centre();
        if hit_hex(ux, uy, CTRL_R, x, y) { return Hit::Ui; }
        let (sx, sy) = settings_centre();
        if hit_hex(sx, sy, CTRL_R, x, y) { return Hit::Settings; }
    }

    if hit_hex(HEX_CX, HEX_CY, MAIN_R, x, y) { return Hit::MainHex; }
    Hit::Nothing
}

// ── Scene builder ─────────────────────────────────────────────────────────

pub fn build(scene: &mut Scene, status: Option<&str>, controls_visible: bool) {
    // Main hex (always at HEX_CX, HEX_CY).
    let main = hex_path(HEX_CX, HEX_CY, MAIN_R);
    scene.fill(Fill::NonZero, Affine::IDENTITY,
        Color::from_rgba8(18, 18, 28, 230), None, &main);
    scene.stroke(&Stroke::new(2.0), Affine::IDENTITY,
        Color::from_rgba8(80, 160, 255, 200), None, &main);

    // Status dot.
    let dot = match status {
        None                             => Color::from_rgba8(80,  80,  100, 200),
        Some(s) if s.starts_with("err") => Color::from_rgba8(220, 60,  60,  240),
        Some(_)                          => Color::from_rgba8(60,  210, 120, 240),
    };
    scene.fill(Fill::NonZero, Affine::IDENTITY, dot, None,
        &Circle::new((HEX_CX, HEX_CY), 8.0));

    // Controls (only when visible).
    if controls_visible {
        draw_ctrl(scene, close_centre(),    CtrlKind::Close);
        draw_ctrl(scene, ui_centre(),       CtrlKind::Ui);
        draw_ctrl(scene, settings_centre(), CtrlKind::Settings);
        draw_connector(scene, settings_centre(), (HEX_CX, HEX_CY));
    }
}

// ── Control hex rendering ─────────────────────────────────────────────────

enum CtrlKind { Close, Ui, Settings }

fn draw_ctrl(scene: &mut Scene, (cx, cy): (f64, f64), kind: CtrlKind) {
    let path = hex_path(cx, cy, CTRL_R);
    scene.fill(Fill::NonZero, Affine::IDENTITY,
        Color::from_rgba8(12, 14, 26, 210), None, &path);
    let (stroke, icon) = match kind {
        CtrlKind::Close    => (Color::from_rgba8(220, 80,  80,  220), Color::from_rgba8(220, 80,  80,  200)),
        CtrlKind::Ui       => (Color::from_rgba8(80,  200, 255, 220), Color::from_rgba8(80,  200, 255, 200)),
        CtrlKind::Settings => (Color::from_rgba8(160, 100, 255, 220), Color::from_rgba8(160, 100, 255, 200)),
    };
    scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, stroke, None, &path);
    scene.fill(Fill::NonZero, Affine::IDENTITY, icon, None,
        &Circle::new((cx, cy), 4.0));
}

fn draw_connector(scene: &mut Scene, from: (f64, f64), to: (f64, f64)) {
    let dim = Color::from_rgba8(60, 100, 160, 80);
    for i in 1..10usize {
        let t = i as f64 / 10.0;
        scene.fill(Fill::NonZero, Affine::IDENTITY, dim, None,
            &Circle::new((
                from.0 + (to.0 - from.0) * t,
                from.1 + (to.1 - from.1) * t,
            ), 1.2));
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

pub fn hex_path(cx: f64, cy: f64, r: f64) -> BezPath {
    let mut path = BezPath::new();
    for i in 0..6_u32 {
        let angle = std::f64::consts::FRAC_PI_3 * i as f64 - std::f64::consts::FRAC_PI_2;
        let x = cx + r * angle.cos();
        let y = cy + r * angle.sin();
        if i == 0 { path.move_to((x, y)); } else { path.line_to((x, y)); }
    }
    path.close_path();
    path
}
