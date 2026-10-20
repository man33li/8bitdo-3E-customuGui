//! GLFW cursor/window coordinates are logical screen coordinates. Raylib's
//! non-Apple HIGHDPI callbacks apply an additional inverse-DPI mouse scale,
//! although GLFW Wayland input is already logical. Do not use GetMousePosition
//! as an unscaled cursor source. Query native metrics every frame, including
//! after moving between differently scaled monitors.
#[derive(Clone, Copy, Debug)]
pub(super) struct Coordinates {
    pub scale: f32,
    pub layout: [f32; 2],
    pub pointer: [f32; 2],
}
impl Coordinates {
    pub fn new(raw: [f32; 2], window: [f32; 2], framebuffer: [f32; 2], dpi: f32, high_dpi: bool) -> Self {
        let scale = if high_dpi { dpi.max(1.0) } else { 1.0 };
        let layout = framebuffer.map(|size| size.max(1.0) / scale);
        let pointer = std::array::from_fn(|axis| raw[axis] * layout[axis] / window[axis].max(1.0));
        Self { scale, layout, pointer }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_cursor_matches_rendered_pixels_for_all_dpi_and_flag_semantics() {
        for dpi in [1.0_f32, 1.25, 1.3333, 2.0] {
            for high_dpi in [false, true] {
                // GLFW can keep or scale its framebuffer independently of the
                // raylib flag; input remains in native logical window units.
                for framebuffer_scale in [1.0, dpi] {
                    let window = [1180.0, 820.0];
                    let framebuffer = window.map(|v| (v * framebuffer_scale).round());
                    for raw in [[0.0, 0.0], [320.0, 240.0], [1180.0, 820.0]] {
                        let coordinates = Coordinates::new(raw, window, framebuffer, dpi, high_dpi);
                        for axis in 0..2 {
                            let drawn_pixel = coordinates.pointer[axis] * coordinates.scale;
                            let native_pixel = raw[axis] * framebuffer[axis] / window[axis];
                            assert!((drawn_pixel - native_pixel).abs() < 0.001,
                                "dpi={dpi} high_dpi={high_dpi} framebuffer_scale={framebuffer_scale} axis={axis}");
                        }
                        assert_eq!(coordinates.scale, if high_dpi { dpi } else { 1.0 });
                    }
                }
            }
        }
    }
    #[test]
    fn monitor_moves_recompute_without_cached_scale_or_mouse_scale() {
        let raw = [900.0, 650.0];
        for dpi in [1.3333, 1.0, 2.0, 1.25, 1.0] {
            let window = [1200.0, 900.0];
            let coordinates = Coordinates::new(raw, window, window.map(|v| v * dpi), dpi, true);
            assert!((coordinates.pointer[0] - raw[0]).abs() < 0.001);
            assert!((coordinates.pointer[1] - raw[1]).abs() < 0.001);
            assert!((coordinates.layout[0] - window[0]).abs() < 0.001);
        }
    }
}
