//! Geometría pura del flyout anclado sobre el ícono de la bandeja. Todo en píxeles físicos.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub const fn right(&self) -> i32 {
        self.x + self.w
    }
    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub const fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

/// Borde de la pantalla donde está la barra de tareas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

/// Deduce el borde de la barra: por lo que le quita al área de trabajo; si no quita nada
/// (barra con ocultado automático) por el borde más cercano al ícono.
pub fn taskbar_edge(monitor: Rect, work: Rect, icon: Rect) -> Edge {
    let insets = [
        (Edge::Bottom, monitor.bottom() - work.bottom()),
        (Edge::Top, work.y - monitor.y),
        (Edge::Left, work.x - monitor.x),
        (Edge::Right, monitor.right() - work.right()),
    ];
    let mut best = insets[0];
    for i in &insets[1..] {
        if i.1 > best.1 {
            best = *i;
        }
    }
    if best.1 > 0 {
        return best.0;
    }
    let (cx, cy) = icon.center();
    let dists = [
        (Edge::Bottom, monitor.bottom() - cy),
        (Edge::Top, cy - monitor.y),
        (Edge::Left, cx - monitor.x),
        (Edge::Right, monitor.right() - cx),
    ];
    let mut best = dists[0];
    for d in &dists[1..] {
        if d.1 < best.1 {
            best = *d;
        }
    }
    best.0
}

fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    if hi < lo {
        lo
    } else {
        v.clamp(lo, hi)
    }
}

/// Posición (esquina superior izquierda) de un flyout de `size` anclado al ícono, sin salirse del área de
/// trabajo. `gap` separa el flyout de la barra y de los bordes.
pub fn flyout_position(icon: Rect, monitor: Rect, work: Rect, size: (i32, i32), gap: i32) -> (i32, i32) {
    let (w, h) = size;
    let (cx, cy) = icon.center();
    let min_x = work.x + gap;
    let max_x = work.right() - w - gap;
    let min_y = work.y + gap;
    let max_y = work.bottom() - h - gap;
    match taskbar_edge(monitor, work, icon) {
        Edge::Bottom => (clamp(cx - w / 2, min_x, max_x), max_y.max(min_y)),
        Edge::Top => (clamp(cx - w / 2, min_x, max_x), min_y),
        Edge::Left => (min_x, clamp(cy - h / 2, min_y, max_y)),
        Edge::Right => (max_x.max(min_x), clamp(cy - h / 2, min_y, max_y)),
    }
}

/// Sin rectángulo del ícono (p. ej. aviso inicial): lo simula en la esquina de la barra, donde Windows
/// pone la bandeja por defecto.
pub fn fallback_icon(monitor: Rect, work: Rect, scale: f64) -> Rect {
    let s = |v: f64| (v * scale).round() as i32;
    let size = s(16.0);
    let edge = taskbar_edge(monitor, work, Rect::new(monitor.right(), monitor.bottom(), 0, 0));
    match edge {
        Edge::Bottom => Rect::new(work.right() - s(150.0), work.bottom(), size, size),
        Edge::Top => Rect::new(work.right() - s(150.0), work.y - size, size, size),
        Edge::Left => Rect::new(work.x - size, work.bottom() - s(150.0), size, size),
        Edge::Right => Rect::new(work.right(), work.bottom() - s(150.0), size, size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect::new(0, 0, 1920, 1080);
    const SIZE: (i32, i32) = (320, 220);
    const GAP: i32 = 12;

    fn inside(p: (i32, i32), size: (i32, i32), r: Rect) -> bool {
        p.0 >= r.x && p.1 >= r.y && p.0 + size.0 <= r.right() && p.1 + size.1 <= r.bottom()
    }

    #[test]
    fn bottom_taskbar_centers_over_icon_and_sits_above_bar() {
        let work = Rect::new(0, 0, 1920, 1032); // barra de 48 px abajo
        let icon = Rect::new(1700, 1040, 24, 40);
        assert_eq!(taskbar_edge(MON, work, icon), Edge::Bottom);
        let p = flyout_position(icon, MON, work, SIZE, GAP);
        assert_eq!(p, (1712 - 160, 1032 - 220 - GAP));
        assert!(inside(p, SIZE, work));
    }

    #[test]
    fn bottom_taskbar_clamps_at_right_edge() {
        let work = Rect::new(0, 0, 1920, 1032);
        let icon = Rect::new(1890, 1040, 24, 40);
        let p = flyout_position(icon, MON, work, SIZE, GAP);
        assert_eq!(p.0, 1920 - 320 - GAP);
        assert!(inside(p, SIZE, work));
    }

    #[test]
    fn top_taskbar_hangs_below_bar() {
        let work = Rect::new(0, 48, 1920, 1032);
        let icon = Rect::new(1700, 4, 24, 40);
        assert_eq!(taskbar_edge(MON, work, icon), Edge::Top);
        let p = flyout_position(icon, MON, work, SIZE, GAP);
        assert_eq!(p, (1712 - 160, 48 + GAP));
        assert!(inside(p, SIZE, work));
    }

    #[test]
    fn left_taskbar_opens_to_the_right_of_bar() {
        let work = Rect::new(60, 0, 1860, 1080);
        let icon = Rect::new(8, 700, 44, 24);
        assert_eq!(taskbar_edge(MON, work, icon), Edge::Left);
        let p = flyout_position(icon, MON, work, SIZE, GAP);
        assert_eq!(p, (60 + GAP, 712 - 110));
        assert!(inside(p, SIZE, work));
    }

    #[test]
    fn right_taskbar_opens_to_the_left_of_bar() {
        let work = Rect::new(0, 0, 1860, 1080);
        let icon = Rect::new(1868, 700, 44, 24);
        assert_eq!(taskbar_edge(MON, work, icon), Edge::Right);
        let p = flyout_position(icon, MON, work, SIZE, GAP);
        assert_eq!(p, (1860 - 320 - GAP, 712 - 110));
        assert!(inside(p, SIZE, work));
    }

    #[test]
    fn side_taskbar_clamps_vertically() {
        let work = Rect::new(60, 0, 1860, 1080);
        let p = flyout_position(Rect::new(8, 1060, 44, 24), MON, work, SIZE, GAP);
        assert_eq!(p.1, 1080 - 220 - GAP);
        let p = flyout_position(Rect::new(8, 2, 44, 24), MON, work, SIZE, GAP);
        assert_eq!(p.1, GAP);
    }

    #[test]
    fn autohide_taskbar_uses_nearest_edge() {
        // Área de trabajo = monitor completo: no hay inset; el ícono está pegado abajo.
        let icon = Rect::new(1700, 1050, 24, 30);
        assert_eq!(taskbar_edge(MON, MON, icon), Edge::Bottom);
        let p = flyout_position(icon, MON, MON, SIZE, GAP);
        assert_eq!(p, (1712 - 160, 1080 - 220 - GAP));
        assert_eq!(taskbar_edge(MON, MON, Rect::new(900, 0, 24, 30)), Edge::Top);
        assert_eq!(taskbar_edge(MON, MON, Rect::new(0, 500, 30, 24)), Edge::Left);
        assert_eq!(taskbar_edge(MON, MON, Rect::new(1890, 500, 30, 24)), Edge::Right);
    }

    #[test]
    fn dpi_scaled_sizes_and_secondary_monitor_offset() {
        // Monitor secundario a la derecha del primario, 150 % de escala: 320×220 lógicos = 480×330 físicos.
        let mon = Rect::new(1920, 0, 2560, 1440);
        let work = Rect::new(1920, 0, 2560, 1392);
        let icon = Rect::new(2400, 1400, 36, 40);
        let size = (480, 330);
        let gap = 18;
        let p = flyout_position(icon, mon, work, size, gap);
        assert_eq!(p, (2418 - 240, 1392 - 330 - gap));
        assert!(inside(p, size, work));
        // y pegado al borde izquierdo del monitor secundario no invade el primario
        let p = flyout_position(Rect::new(1925, 1400, 36, 40), mon, work, size, gap);
        assert_eq!(p.0, 1920 + gap);
    }

    #[test]
    fn fallback_icon_sits_in_the_taskbar_corner() {
        let work = Rect::new(0, 0, 1920, 1032);
        let f = fallback_icon(MON, work, 1.0);
        assert_eq!(f.y, 1032);
        let p = flyout_position(f, MON, work, SIZE, GAP);
        assert!(inside(p, SIZE, work));
        assert_eq!(p.1, 1032 - 220 - GAP);
    }
}
