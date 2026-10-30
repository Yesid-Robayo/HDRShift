//! Iconos dibujados por código (sin imágenes externas): el de la app y el de la bandeja. El símbolo
//! es un disco mitad apagado y mitad encendido, el rango dinámico, con rayos cuando hay HDR.
//! Rasterizador mínimo con supermuestreo 4×4.

/// Color RGBA lineal (0..1) sin premultiplicar.
#[derive(Clone, Copy)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    pub const fn hex(rgb: u32) -> Self {
        Rgba(((rgb >> 16) & 0xff) as f32 / 255.0, ((rgb >> 8) & 0xff) as f32 / 255.0, (rgb & 0xff) as f32 / 255.0, 1.0)
    }

    fn lerp(a: Rgba, b: Rgba, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        Rgba(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
    }
}

const WHITE: Rgba = Rgba::hex(0xffffff);
const INK: Rgba = Rgba::hex(0x1c1c28);
const BRAND_A: Rgba = Rgba::hex(0x0b4f9e);
const BRAND_B: Rgba = Rgba::hex(0x0e7fc9);
const BRAND_C: Rgba = Rgba::hex(0x12b5cb);
const ACTIVE: Rgba = Rgba::hex(0x5cc0ff);
const ACTIVE_ON_LIGHT: Rgba = Rgba::hex(0x0b6fb8);
const ALERT: Rgba = Rgba::hex(0xff5d6c);

pub struct Canvas {
    pub size: u32,
    /// RGBA premultiplicado.
    px: Vec<[f32; 4]>,
}

fn rounded_rect(x: f32, y: f32, r: [f32; 5]) -> bool {
    let [x0, y0, x1, y1, rad] = r;
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + rad, x1 - rad);
    let cy = y.clamp(y0 + rad, y1 - rad);
    (x - cx).powi(2) + (y - cy).powi(2) <= rad * rad
}

fn in_circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

/// Segmento con puntas redondas.
fn in_capsule(x: f32, y: f32, a: (f32, f32), b: (f32, f32), r: f32) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    in_circle(x, y, a.0 + dx * t, a.1 + dy * t, r)
}

impl Canvas {
    pub fn new(size: u32) -> Self {
        Self { size, px: vec![[0.0; 4]; (size * size) as usize] }
    }

    /// Rellena la figura `inside` (coordenadas 0..1) con el color `paint(x, y)`.
    fn fill(&mut self, inside: impl Fn(f32, f32) -> bool, paint: impl Fn(f32, f32) -> Rgba) {
        let s = self.size as f32;
        const N: usize = 4;
        for py in 0..self.size {
            for px in 0..self.size {
                let mut hits = 0;
                for sy in 0..N {
                    for sx in 0..N {
                        let x = (px as f32 + (sx as f32 + 0.5) / N as f32) / s;
                        let y = (py as f32 + (sy as f32 + 0.5) / N as f32) / s;
                        if inside(x, y) {
                            hits += 1;
                        }
                    }
                }
                if hits == 0 {
                    continue;
                }
                let cov = hits as f32 / (N * N) as f32;
                let c = paint((px as f32 + 0.5) / s, (py as f32 + 0.5) / s);
                let a = c.3 * cov;
                let dst = &mut self.px[(py * self.size + px) as usize];
                // "source over" premultiplicado
                dst[0] = c.0 * a + dst[0] * (1.0 - a);
                dst[1] = c.1 * a + dst[1] * (1.0 - a);
                dst[2] = c.2 * a + dst[2] * (1.0 - a);
                dst[3] = a + dst[3] * (1.0 - a);
            }
        }
    }

    /// Píxeles RGBA de 8 bits sin premultiplicar (formato de egui y de los PNG).
    pub fn rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            let a = p[3];
            let un = |v: f32| if a > 0.0 { (v / a).clamp(0.0, 1.0) } else { 0.0 };
            out.extend([
                (un(p[0]) * 255.0).round() as u8,
                (un(p[1]) * 255.0).round() as u8,
                (un(p[2]) * 255.0).round() as u8,
                (a * 255.0).round() as u8,
            ]);
        }
        out
    }

    /// Disco con la mitad derecha llena, centrado en (0.5, 0.5).
    fn half_disc(&mut self, r: f32, stroke: f32, color: Rgba) {
        let inner = r - stroke;
        self.fill(|x, y| in_circle(x, y, 0.5, 0.5, r) && (x >= 0.5 || !in_circle(x, y, 0.5, 0.5, inner)), |_, _| color);
    }

    /// Ocho rayos entre los radios `from` y `to`.
    fn rays(&mut self, from: f32, to: f32, width: f32, color: Rgba) {
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            let (dx, dy) = (a.cos(), a.sin());
            let p0 = (0.5 + dx * from, 0.5 + dy * from);
            let p1 = (0.5 + dx * to, 0.5 + dy * to);
            self.fill(|x, y| in_capsule(x, y, p0, p1, width), |_, _| color);
        }
    }
}

fn brand_gradient(t: f32) -> Rgba {
    if t < 0.5 { Rgba::lerp(BRAND_A, BRAND_B, t * 2.0) } else { Rgba::lerp(BRAND_B, BRAND_C, (t - 0.5) * 2.0) }
}

/// Icono de la app: insignia con el degradado de marca y el disco con rayos.
pub fn app_icon(size: u32) -> Canvas {
    let mut c = Canvas::new(size);
    let bg = [0.03, 0.03, 0.97, 0.97, 0.24];
    c.fill(|x, y| rounded_rect(x, y, bg), |x, y| brand_gradient((x + y) / 2.0));
    if size >= 32 {
        c.rays(0.335, 0.385, 0.026, WHITE);
        c.half_disc(0.22, 0.05, WHITE);
    } else {
        // en tamaños pequeños los rayos serían ruido
        c.half_disc(0.29, 0.075, WHITE);
    }
    c
}

/// Estado que representa el icono de la bandeja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayState {
    /// Alguna sesión mantiene el HDR.
    pub active: bool,
    /// Queda HDR por restaurar.
    pub warn: bool,
    /// La barra de tareas es clara: el símbolo se dibuja oscuro para que se vea.
    pub light_taskbar: bool,
}

/// Icono de la bandeja: el disco, con rayos y en azul mientras HDRShift mantiene el HDR.
pub fn tray_icon(size: u32, st: TrayState) -> Canvas {
    let mut c = Canvas::new(size);
    let px = 1.0 / size as f32;
    let color = match (st.active, st.light_taskbar) {
        (true, false) => ACTIVE,
        (true, true) => ACTIVE_ON_LIGHT,
        (false, false) => WHITE,
        (false, true) => INK,
    };
    if st.active {
        c.rays(0.37, 0.44, (0.035f32).max(px * 0.6), color);
        c.half_disc(0.245, (0.075f32).max(px), color);
    } else {
        c.half_disc(0.36, (0.09f32).max(px * 1.2), color);
    }
    if st.warn {
        c.fill(|x, y| in_circle(x, y, 0.8, 0.8, 0.2), |_, _| ALERT);
    }
    c
}

/// Archivo .ico con varias resoluciones (entradas BMP de 32 bits). Lo usa `build.rs`.
#[allow(dead_code)]
pub fn ico_file(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_entry(&app_icon(s))).collect();
    let mut out = Vec::new();
    out.extend(0u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, img) in sizes.iter().zip(&images) {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        out.extend([dim, dim, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((img.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend(img);
    }
    out
}

/// Imagen BMP de icono: cabecera + BGRA de abajo arriba + máscara AND.
#[allow(dead_code)]
fn bmp_entry(c: &Canvas) -> Vec<u8> {
    let s = c.size;
    let rgba = c.rgba8();
    let mut out = Vec::new();
    out.extend(40u32.to_le_bytes());
    out.extend((s as i32).to_le_bytes());
    out.extend((s as i32 * 2).to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(32u16.to_le_bytes());
    out.extend([0u8; 24]);
    for y in (0..s).rev() {
        for x in 0..s {
            let i = ((y * s + x) * 4) as usize;
            out.extend([rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    let mask_row = (s.div_ceil(32) * 4) as usize;
    out.extend(vec![0u8; mask_row * s as usize]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tray_state_draws_at_every_size() {
        for size in [16u32, 20, 24, 32, 48] {
            for active in [false, true] {
                for warn in [false, true] {
                    for light_taskbar in [false, true] {
                        let c = tray_icon(size, TrayState { active, warn, light_taskbar });
                        assert!(c.rgba8().chunks(4).any(|p| p[3] > 0));
                    }
                }
            }
        }
    }

    #[test]
    fn the_icon_file_has_one_entry_per_size() {
        let ico = ico_file(&[16, 32]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 2);
    }
}
