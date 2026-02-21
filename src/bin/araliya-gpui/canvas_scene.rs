pub struct CanvasGeometry {
    pub center_x: f32,
    pub center_y: f32,
    pub hex_radius: f32,
    pub icon_radius: f32,
}

impl CanvasGeometry {
    pub fn from_size(width: f32, height: f32) -> Self {
        let shortest = width.min(height);
        let hex_radius = (shortest * 0.34).max(96.0);
        let icon_radius = (shortest * 0.065).max(20.0);
        Self {
            center_x: width * 0.5,
            center_y: height * 0.5,
            hex_radius,
            icon_radius,
        }
    }

    pub fn hex_vertices(&self) -> [(f32, f32); 6] {
        std::array::from_fn(|index| {
            let angle = std::f32::consts::FRAC_PI_3 * index as f32;
            (
                self.center_x + self.hex_radius * angle.cos(),
                self.center_y + self.hex_radius * angle.sin(),
            )
        })
    }

    pub fn icon_contains(&self, x: f32, y: f32) -> bool {
        let dx = x - self.center_x;
        let dy = y - self.center_y;
        dx * dx + dy * dy <= self.icon_radius * self.icon_radius
    }

    pub fn icon_glyph_half(&self) -> f32 {
        self.icon_radius * 0.52
    }
}

#[cfg(test)]
mod tests {
    use super::CanvasGeometry;

    #[test]
    fn hex_vertices_are_generated() {
        let geometry = CanvasGeometry::from_size(1200.0, 800.0);
        let vertices = geometry.hex_vertices();

        assert_eq!(vertices.len(), 6);
        let (x, y) = vertices[0];
        assert!(x > geometry.center_x);
        assert!((y - geometry.center_y).abs() < 1.0);
    }

    #[test]
    fn icon_hit_test_uses_circle_distance() {
        let geometry = CanvasGeometry::from_size(900.0, 700.0);

        assert!(geometry.icon_contains(geometry.center_x, geometry.center_y));
        assert!(!geometry.icon_contains(
            geometry.center_x + geometry.icon_radius * 2.0,
            geometry.center_y,
        ));
    }
}
