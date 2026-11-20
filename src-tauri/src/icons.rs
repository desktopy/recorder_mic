//! Íconos de la bandeja generados por código (RGBA), nítidos a 16/20/24/32 px y legibles sobre
//! barra oscura y clara. Tres variantes: reposo, grabando (punto coral) y en pausa (dos barras).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconState {
    Idle,
    Recording,
    Paused,
}

type Rgb = (f32, f32, f32);

const CORAL: Rgb = (255.0, 90.0, 78.0); // #FF5A4E
const FG_DARK_BAR: Rgb = (242.0, 242.0, 244.0); // glifo claro para barra oscura
const FG_LIGHT_BAR: Rgb = (28.0, 28.0, 32.0); // glifo oscuro para barra clara
const AMBER_DARK_BAR: Rgb = (245.0, 184.0, 75.0); // #F5B84B
const AMBER_LIGHT_BAR: Rgb = (207.0, 138.0, 10.0); // #CF8A0A

/// Cobertura antialias (0..1) de un punto con distancia con signo `sd` en píxeles de salida.
fn cov(sd: f32) -> f32 {
    (0.5 - sd).clamp(0.0, 1.0)
}

fn over(dst: &mut [f32; 4], color: Rgb, a: f32) {
    if a <= 0.0 {
        return;
    }
    let out_a = a + dst[3] * (1.0 - a);
    for (i, c) in [color.0, color.1, color.2].into_iter().enumerate() {
        dst[i] = (c * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

fn rounded_box_sd(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// Devuelve `size*size*4` bytes RGBA (alfa recto). `light_taskbar` = la barra de tareas usa tema claro.
pub fn render(state: IconState, size: u32, light_taskbar: bool) -> Vec<u8> {
    let n = size.max(8) as usize;
    let u = n as f32 / 32.0; // unidad de diseño: lienzo de 32×32
    let (fg, amber) = if light_taskbar { (FG_LIGHT_BAR, AMBER_LIGHT_BAR) } else { (FG_DARK_BAR, AMBER_DARK_BAR) };
    let c = n as f32 / 2.0;
    // Anillo (el «eco»): más grueso en tamaños pequeños para que no se pierda.
    let ring_t = if n <= 20 { 3.2 } else { 2.8 } * u;
    let ring_mid = 13.4 * u;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = (px - c, py - c);
            let d = (dx * dx + dy * dy).sqrt();
            let mut px_rgba = [0.0f32; 4];
            // anillo
            over(&mut px_rgba, fg, cov(((d - ring_mid).abs() - ring_t / 2.0) / 1.0));
            match state {
                IconState::Idle => over(&mut px_rgba, fg, cov(d - 4.2 * u)),
                IconState::Recording => over(&mut px_rgba, CORAL, cov(d - 6.6 * u)),
                IconState::Paused => {
                    let (hw, hh, off) = (1.9 * u, 5.6 * u, 3.5 * u);
                    for s in [-1.0f32, 1.0] {
                        over(&mut px_rgba, amber, cov(rounded_box_sd(px, py, c + s * off, c, hw, hh, 0.7 * u)));
                    }
                }
            }
            let i = (y * n + x) * 4;
            out[i] = px_rgba[0].round().clamp(0.0, 255.0) as u8;
            out[i + 1] = px_rgba[1].round().clamp(0.0, 255.0) as u8;
            out[i + 2] = px_rgba[2].round().clamp(0.0, 255.0) as u8;
            out[i + 3] = (px_rgba[3] * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(buf: &[u8], n: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * n + x) * 4;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn sizes_are_exact() {
        for s in [16u32, 20, 24, 32, 48] {
            for st in [IconState::Idle, IconState::Recording, IconState::Paused] {
                assert_eq!(render(st, s, false).len(), (s * s * 4) as usize);
            }
        }
    }

    #[test]
    fn recording_has_coral_center_at_16_and_32() {
        for n in [16usize, 32] {
            let b = render(IconState::Recording, n as u32, false);
            let p = px(&b, n, n / 2, n / 2);
            assert_eq!(p[3], 255, "centro opaco a {n}px");
            assert!(p[0] > 240 && (80..=100).contains(&p[1]) && (70..=90).contains(&p[2]), "coral a {n}px: {p:?}");
        }
    }

    #[test]
    fn corners_are_transparent_and_ring_is_opaque() {
        for st in [IconState::Idle, IconState::Recording, IconState::Paused] {
            let b = render(st, 32, false);
            assert_eq!(px(&b, 32, 0, 0)[3], 0);
            assert_eq!(px(&b, 32, 31, 31)[3], 0);
            // sobre el anillo (radio ≈ 13.4) a la izquierda del centro
            assert!(px(&b, 32, 2, 16)[3] > 200, "anillo visible en {st:?}");
        }
    }

    #[test]
    fn glyph_contrasts_with_taskbar_theme() {
        let dark = render(IconState::Idle, 16, false);
        let light = render(IconState::Idle, 16, true);
        assert!(px(&dark, 16, 8, 8)[0] > 200, "glifo claro sobre barra oscura");
        assert!(px(&light, 16, 8, 8)[0] < 60, "glifo oscuro sobre barra clara");
    }

    #[test]
    fn paused_differs_from_idle_and_recording() {
        let a = render(IconState::Idle, 16, false);
        let b = render(IconState::Paused, 16, false);
        let c = render(IconState::Recording, 16, false);
        assert_ne!(a, b);
        assert_ne!(b, c);
        // las dos barras dejan hueco en el centro exacto entre ellas
        let n = 32;
        let p = render(IconState::Paused, n as u32, false);
        assert!(px(&p, n, 15, 16)[3] < 40 || px(&p, n, 16, 16)[3] < 40, "separación entre barras");
    }
}
