use eframe::egui::{Color32, Rgba};

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color32,
    pub bg2: Color32,
    pub panel: Color32,
    pub panel_stroke: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub danger: Color32,
    pub ok: Color32,
    pub bubble_me: Color32,
    pub bubble_other: Color32,
    pub radius: f32,
}

pub fn themes() -> [Theme; 4] { [
    Theme {
        name: "Neon",
        bg: Color32::from_rgb(9, 10, 18),
        bg2: Color32::from_rgb(18, 12, 34),
        panel: Color32::from_rgba_unmultiplied(20, 22, 38, 235),
        panel_stroke: Color32::from_rgba_unmultiplied(120, 90, 255, 90),
        accent: Color32::from_rgb(124, 92, 255),
        accent2: Color32::from_rgb(0, 229, 255),
        text: Color32::from_rgb(232, 234, 246),
        dim: Color32::from_rgb(130, 134, 160),
        danger: Color32::from_rgb(255, 84, 120),
        ok: Color32::from_rgb(64, 224, 152),
        bubble_me: Color32::from_rgb(66, 48, 132),
        bubble_other: Color32::from_rgb(28, 30, 48),
        radius: 14.0,
    },
    Theme {
        name: "Sunset",
        bg: Color32::from_rgb(24, 12, 16),
        bg2: Color32::from_rgb(46, 18, 26),
        panel: Color32::from_rgba_unmultiplied(40, 22, 30, 235),
        panel_stroke: Color32::from_rgba_unmultiplied(255, 140, 90, 90),
        accent: Color32::from_rgb(255, 120, 80),
        accent2: Color32::from_rgb(255, 200, 90),
        text: Color32::from_rgb(250, 240, 236),
        dim: Color32::from_rgb(175, 150, 145),
        danger: Color32::from_rgb(255, 70, 90),
        ok: Color32::from_rgb(120, 220, 160),
        bubble_me: Color32::from_rgb(140, 62, 46),
        bubble_other: Color32::from_rgb(52, 30, 36),
        radius: 14.0,
    },
    Theme {
        name: "Ocean",
        bg: Color32::from_rgb(8, 16, 24),
        bg2: Color32::from_rgb(8, 30, 44),
        panel: Color32::from_rgba_unmultiplied(12, 28, 42, 235),
        panel_stroke: Color32::from_rgba_unmultiplied(60, 190, 255, 90),
        accent: Color32::from_rgb(40, 160, 255),
        accent2: Color32::from_rgb(60, 240, 200),
        text: Color32::from_rgb(228, 240, 248),
        dim: Color32::from_rgb(120, 150, 168),
        danger: Color32::from_rgb(255, 96, 110),
        ok: Color32::from_rgb(70, 230, 170),
        bubble_me: Color32::from_rgb(26, 78, 128),
        bubble_other: Color32::from_rgb(18, 34, 46),
        radius: 14.0,
    },
    Theme {
        name: "Matrix",
        bg: Color32::from_rgb(4, 10, 6),
        bg2: Color32::from_rgb(6, 20, 10),
        panel: Color32::from_rgba_unmultiplied(8, 22, 12, 235),
        panel_stroke: Color32::from_rgba_unmultiplied(60, 255, 130, 90),
        accent: Color32::from_rgb(40, 230, 110),
        accent2: Color32::from_rgb(180, 255, 90),
        text: Color32::from_rgb(220, 245, 225),
        dim: Color32::from_rgb(110, 150, 120),
        danger: Color32::from_rgb(255, 90, 90),
        ok: Color32::from_rgb(80, 255, 150),
        bubble_me: Color32::from_rgb(20, 92, 48),
        bubble_other: Color32::from_rgb(12, 30, 18),
        radius: 8.0,
    },
] }

impl Theme {
    pub fn lerp(&self, other: &Theme, t: f32) -> Theme {
        Theme {
            name: other.name,
            bg: mix(self.bg, other.bg, t),
            bg2: mix(self.bg2, other.bg2, t),
            panel: mix(self.panel, other.panel, t),
            panel_stroke: mix(self.panel_stroke, other.panel_stroke, t),
            accent: mix(self.accent, other.accent, t),
            accent2: mix(self.accent2, other.accent2, t),
            text: mix(self.text, other.text, t),
            dim: mix(self.dim, other.dim, t),
            danger: mix(self.danger, other.danger, t),
            ok: mix(self.ok, other.ok, t),
            bubble_me: mix(self.bubble_me, other.bubble_me, t),
            bubble_other: mix(self.bubble_other, other.bubble_other, t),
            radius: self.radius + (other.radius - self.radius) * t,
        }
    }

    /// Üst üste binen yarı saydam neon katmanı.
    pub fn glow(&self, alpha: u8) -> Color32 {
        let [r, g, b, _] = self.accent.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, alpha)
    }
}

pub struct ThemeState {
    pub current: Theme,
    target_idx: usize,
    pub transitioning: bool,
}

impl Default for ThemeState {
    fn default() -> Self {
        Self {
            current: themes()[0].clone(),
            target_idx: 0,
            transitioning: false,
        }
    }
}

impl ThemeState {
    pub fn select(&mut self, idx: usize) {
        if idx < themes().len() {
            self.target_idx = idx;
        }
    }

    pub fn target_index(&self) -> usize {
        self.target_idx
    }

    /// Her karede çağır; renkleri hedef temaya doğru yumuşak geçirir.
    pub fn update(&mut self, dt: f32) {
        let all = themes();
        let target = &all[self.target_idx];
        let dist = color_dist(&self.current, target);
        if dist < 0.002 {
            self.current = target.clone();
            self.transitioning = false;
            return;
        }
        self.transitioning = true;
        let k = 1.0 - (-dt * 6.0).exp();
        self.current = self.current.lerp(target, k.min(1.0));
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ra = Rgba::from(a);
    let rb = Rgba::from(b);
    let r = ra.r() + (rb.r() - ra.r()) * t;
    let g = ra.g() + (rb.g() - ra.g()) * t;
    let bl = ra.b() + (rb.b() - ra.b()) * t;
    let al = ra.a() + (rb.a() - ra.a()) * t;
    Color32::from(Rgba::from_rgba_premultiplied(r, g, bl, al))
}

fn color_dist(a: &Theme, b: &Theme) -> f32 {
    fn d(x: Color32, y: Color32) -> f32 {
        let [r1, g1, b1, a1] = x.to_array();
        let [r2, g2, b2, a2] = y.to_array();
        let f = |p: u8, q: u8| ((p as f32 - q as f32) / 255.0).abs();
        f(r1, r2) + f(g1, g2) + f(b1, b2) + f(a1, a2)
    }
    d(a.bg, b.bg)
        + d(a.bg2, b.bg2)
        + d(a.accent, b.accent)
        + d(a.accent2, b.accent2)
        + d(a.text, b.text)
        + d(a.bubble_me, b.bubble_me)
}
