//! Genera los íconos de Eco (círculo + ondas en #FF5A4E sobre #111214).
//! Uso: `cargo run -p eco-audio --example gen_icons -- <carpeta icons>`

use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use std::io::Write;

const BG: [f32; 3] = [17.0, 18.0, 20.0]; // #111214
const ACCENT: [f32; 3] = [255.0, 90.0, 78.0]; // #FF5A4E

/// Cobertura (0..1) de un anillo/disco en el punto (x,y) normalizado a 0..1.
fn ring(x: f32, y: f32, r_mid: f32, half_w: f32) -> bool {
    let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt();
    (d - r_mid).abs() <= half_w
}

/// Color RGBA de un punto del ícono (coordenadas normalizadas).
fn shade(x: f32, y: f32) -> [f32; 4] {
    // cuadrado redondeado de fondo
    let (cx, cy) = ((x - 0.5).abs(), (y - 0.5).abs());
    let (half, rad) = (0.5, 0.22);
    let (qx, qy) = ((cx - (half - rad)).max(0.0), (cy - (half - rad)).max(0.0));
    if (qx * qx + qy * qy).sqrt() > rad {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let mut c = BG;
    let mix = |c: &mut [f32; 3], a: f32| {
        for i in 0..3 {
            c[i] = c[i] * (1.0 - a) + ACCENT[i] * a;
        }
    };
    if ring(x, y, 0.0, 0.14) {
        mix(&mut c, 1.0); // punto central
    } else if ring(x, y, 0.255, 0.032) {
        mix(&mut c, 0.78); // primera onda
    } else if ring(x, y, 0.37, 0.026) {
        mix(&mut c, 0.42); // segunda onda
    }
    [c[0], c[1], c[2], 255.0]
}

fn render(size: u32) -> Vec<u8> {
    const SS: u32 = 4; // supermuestreo 4x4
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    let c = shade(x, y);
                    // promedio con alfa premultiplicado
                    let a = c[3] / 255.0;
                    acc[0] += c[0] * a;
                    acc[1] += c[1] * a;
                    acc[2] += c[2] * a;
                    acc[3] += a;
                }
            }
            let n = (SS * SS) as f32;
            let a = acc[3] / n;
            let (r, g, b) = if acc[3] > 0.0 { (acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]) } else { (0.0, 0.0, 0.0) };
            out.extend_from_slice(&[r.round() as u8, g.round() as u8, b.round() as u8, (a * 255.0).round() as u8]);
        }
    }
    out
}

fn png(size: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    PngEncoder::new(&mut buf)
        .write_image(&render(size), size, size, ExtendedColorType::Rgba8)
        .expect("png");
    buf
}

fn ico(sizes: &[u32]) -> Vec<u8> {
    let imgs: Vec<(u32, Vec<u8>)> = sizes.iter().map(|&s| (s, png(s))).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(imgs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * imgs.len() as u32;
    for (s, data) in &imgs {
        out.push(if *s >= 256 { 0 } else { *s as u8 });
        out.push(if *s >= 256 { 0 } else { *s as u8 });
        out.extend_from_slice(&[0, 0, 1, 0, 32, 0]);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &imgs {
        out.write_all(data).unwrap();
    }
    out
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "icons".into()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, size) in [
        ("32x32.png", 32),
        ("64x64.png", 64),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ] {
        std::fs::write(dir.join(name), png(size)).unwrap();
    }
    std::fs::write(dir.join("icon.ico"), ico(&[16, 24, 32, 48, 64, 128, 256])).unwrap();
    println!("íconos escritos en {}", dir.display());
}
