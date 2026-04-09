use egui::Color32;

// ── Noise algorithm selector ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoiseType {
    Perlin,
    OpenSimplex,
    SuperSimplex,
    Value,
    Worley,
}

impl NoiseType {
    pub const ALL: &'static [Self] = &[
        Self::Perlin,
        Self::OpenSimplex,
        Self::SuperSimplex,
        Self::Value,
        Self::Worley,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Perlin => "Perlin",
            Self::OpenSimplex => "Open Simplex",
            Self::SuperSimplex => "Super Simplex",
            Self::Value => "Value",
            Self::Worley => "Worley / Cellular",
        }
    }
}

// ── Fractal combiner selector ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FractalType {
    None,
    Fbm,
    Billow,
    RidgedMulti,
    HybridMulti,
    BasicMulti,
}

impl FractalType {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::Fbm,
        Self::Billow,
        Self::RidgedMulti,
        Self::HybridMulti,
        Self::BasicMulti,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None (raw)",
            Self::Fbm => "fBm",
            Self::Billow => "Billow",
            Self::RidgedMulti => "Ridged Multi",
            Self::HybridMulti => "Hybrid Multi",
            Self::BasicMulti => "Basic Multi",
        }
    }
}

// ── Post-processing operations ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PostProcess {
    None,
    Terrace,
    Power,
    Invert,
    Abs,
    Clamp,
}

impl PostProcess {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::Terrace,
        Self::Power,
        Self::Invert,
        Self::Abs,
        Self::Clamp,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Terrace => "Terrace / Posterize",
            Self::Power => "Power curve",
            Self::Invert => "Invert",
            Self::Abs => "Abs (ridges)",
            Self::Clamp => "Clamp range",
        }
    }
}

// ── Color ramp for preview ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorMode {
    Grayscale,
    Terrain,
    Heatmap,
}

impl ColorMode {
    pub const ALL: &'static [Self] = &[Self::Grayscale, Self::Terrain, Self::Heatmap];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Grayscale => "Grayscale",
            Self::Terrain => "Terrain",
            Self::Heatmap => "Heatmap",
        }
    }

    pub fn sample(&self, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Grayscale => {
                let v = (t * 255.0) as u8;
                Color32::from_rgb(v, v, v)
            }
            Self::Terrain => {
                // deep water → shallow → sand → grass → rock → snow
                if t < 0.30 {
                    lerp_color(
                        Color32::from_rgb(20, 40, 120),
                        Color32::from_rgb(50, 100, 200),
                        t / 0.30,
                    )
                } else if t < 0.40 {
                    lerp_color(
                        Color32::from_rgb(50, 100, 200),
                        Color32::from_rgb(210, 200, 150),
                        (t - 0.30) / 0.10,
                    )
                } else if t < 0.60 {
                    lerp_color(
                        Color32::from_rgb(60, 160, 50),
                        Color32::from_rgb(30, 100, 30),
                        (t - 0.40) / 0.20,
                    )
                } else if t < 0.80 {
                    lerp_color(
                        Color32::from_rgb(100, 80, 60),
                        Color32::from_rgb(140, 130, 120),
                        (t - 0.60) / 0.20,
                    )
                } else {
                    lerp_color(
                        Color32::from_rgb(180, 180, 180),
                        Color32::from_rgb(255, 255, 255),
                        (t - 0.80) / 0.20,
                    )
                }
            }
            Self::Heatmap => {
                if t < 0.25 {
                    lerp_color(
                        Color32::from_rgb(0, 0, 80),
                        Color32::from_rgb(0, 80, 255),
                        t / 0.25,
                    )
                } else if t < 0.50 {
                    lerp_color(
                        Color32::from_rgb(0, 200, 100),
                        Color32::from_rgb(255, 255, 0),
                        (t - 0.25) / 0.25,
                    )
                } else if t < 0.75 {
                    lerp_color(
                        Color32::from_rgb(255, 200, 0),
                        Color32::from_rgb(255, 60, 0),
                        (t - 0.50) / 0.25,
                    )
                } else {
                    lerp_color(
                        Color32::from_rgb(255, 60, 0),
                        Color32::from_rgb(255, 255, 255),
                        (t - 0.75) / 0.25,
                    )
                }
            }
        }
    }
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 };
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

// ── Blend modes for layer compositing ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlendMode {
    Add,
    Multiply,
    Max,
    Min,
    Screen,
}

impl BlendMode {
    pub const ALL: &'static [Self] = &[
        Self::Add,
        Self::Multiply,
        Self::Max,
        Self::Min,
        Self::Screen,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Multiply => "Multiply",
            Self::Max => "Max",
            Self::Min => "Min",
            Self::Screen => "Screen",
        }
    }
}

// ── Falloff shape ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FalloffShape {
    Circle,
    Square,
}

impl FalloffShape {
    pub const ALL: &'static [Self] = &[Self::Circle, Self::Square];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Circle => "Círculo",
            Self::Square => "Cuadrado",
        }
    }
}

// ── Additional noise layer ──────────────────────────────────────────────────

pub struct Layer {
    pub enabled: bool,
    pub noise_type: NoiseType,
    pub fractal_type: FractalType,
    pub seed_offset: u32,
    pub frequency_scale: f64,
    pub weight: f32,
    pub blend_mode: BlendMode,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            enabled: false,
            noise_type: NoiseType::Perlin,
            fractal_type: FractalType::Fbm,
            seed_offset: 1,
            frequency_scale: 2.0,
            weight: 0.5,
            blend_mode: BlendMode::Add,
        }
    }
}
