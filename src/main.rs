use eframe::egui;
use egui::{Color32, ColorImage, TextureHandle, TextureOptions, Vec2};
use image::{GrayImage, Luma};
use noise::{
    BasicMulti, Billow, Fbm, HybridMulti, MultiFractal, NoiseFn, OpenSimplex, Perlin, RidgedMulti,
    SuperSimplex, Value, Worley,
};
use rand::Rng;
use std::path::PathBuf;

// ── Noise algorithm selector ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum NoiseType {
    Perlin,
    OpenSimplex,
    SuperSimplex,
    Value,
    Worley,
}

impl NoiseType {
    const ALL: &'static [Self] = &[
        Self::Perlin,
        Self::OpenSimplex,
        Self::SuperSimplex,
        Self::Value,
        Self::Worley,
    ];

    fn label(&self) -> &'static str {
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
enum FractalType {
    None,
    Fbm,
    Billow,
    RidgedMulti,
    HybridMulti,
    BasicMulti,
}

impl FractalType {
    const ALL: &'static [Self] = &[
        Self::None,
        Self::Fbm,
        Self::Billow,
        Self::RidgedMulti,
        Self::HybridMulti,
        Self::BasicMulti,
    ];

    fn label(&self) -> &'static str {
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
enum PostProcess {
    None,
    Terrace,
    Power,
    Invert,
    Abs,
    Clamp,
}

impl PostProcess {
    const ALL: &'static [Self] = &[
        Self::None,
        Self::Terrace,
        Self::Power,
        Self::Invert,
        Self::Abs,
        Self::Clamp,
    ];

    fn label(&self) -> &'static str {
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
enum ColorMode {
    Grayscale,
    Terrain,
    Heatmap,
}

impl ColorMode {
    const ALL: &'static [Self] = &[Self::Grayscale, Self::Terrain, Self::Heatmap];

    fn label(&self) -> &'static str {
        match self {
            Self::Grayscale => "Grayscale",
            Self::Terrain => "Terrain",
            Self::Heatmap => "Heatmap",
        }
    }

    fn sample(&self, t: f32) -> Color32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Grayscale => {
                let v = (t * 255.0) as u8;
                Color32::from_rgb(v, v, v)
            }
            Self::Terrain => {
                // deep water → shallow → sand → grass → rock → snow
                if t < 0.30 {
                    lerp_color(Color32::from_rgb(20, 40, 120), Color32::from_rgb(50, 100, 200), t / 0.30)
                } else if t < 0.40 {
                    lerp_color(Color32::from_rgb(50, 100, 200), Color32::from_rgb(210, 200, 150), (t - 0.30) / 0.10)
                } else if t < 0.60 {
                    lerp_color(Color32::from_rgb(60, 160, 50), Color32::from_rgb(30, 100, 30), (t - 0.40) / 0.20)
                } else if t < 0.80 {
                    lerp_color(Color32::from_rgb(100, 80, 60), Color32::from_rgb(140, 130, 120), (t - 0.60) / 0.20)
                } else {
                    lerp_color(Color32::from_rgb(180, 180, 180), Color32::from_rgb(255, 255, 255), (t - 0.80) / 0.20)
                }
            }
            Self::Heatmap => {
                if t < 0.25 {
                    lerp_color(Color32::from_rgb(0, 0, 80), Color32::from_rgb(0, 80, 255), t / 0.25)
                } else if t < 0.50 {
                    lerp_color(Color32::from_rgb(0, 200, 100), Color32::from_rgb(255, 255, 0), (t - 0.25) / 0.25)
                } else if t < 0.75 {
                    lerp_color(Color32::from_rgb(255, 200, 0), Color32::from_rgb(255, 60, 0), (t - 0.50) / 0.25)
                } else {
                    lerp_color(Color32::from_rgb(255, 60, 0), Color32::from_rgb(255, 255, 255), (t - 0.75) / 0.25)
                }
            }
        }
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 };
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

// ── Application state ───────────────────────────────────────────────────────

struct HeightmapApp {
    // Generation params
    noise_type: NoiseType,
    fractal_type: FractalType,
    seed: u32,
    octaves: u32,
    frequency: f64,
    lacunarity: f64,
    persistence: f64,
    offset_x: f64,
    offset_y: f64,

    // Output
    resolution: u32,
    export_resolution: u32,

    // Post-process
    post_process: PostProcess,
    terrace_levels: u32,
    power_exp: f32,
    clamp_min: f32,
    clamp_max: f32,

    // Preview
    color_mode: ColorMode,
    preview_texture: Option<TextureHandle>,
    heightmap_data: Vec<f32>, // 0..1 normalized
    dirty: bool,

    // Status
    last_gen_ms: f64,
}

impl Default for HeightmapApp {
    fn default() -> Self {
        Self {
            noise_type: NoiseType::Perlin,
            fractal_type: FractalType::Fbm,
            seed: 42,
            octaves: 6,
            frequency: 2.0,
            lacunarity: 2.0,
            persistence: 0.5,
            offset_x: 0.0,
            offset_y: 0.0,
            resolution: 256,
            export_resolution: 1024,
            post_process: PostProcess::None,
            terrace_levels: 8,
            power_exp: 2.0,
            clamp_min: 0.2,
            clamp_max: 0.8,
            color_mode: ColorMode::Grayscale,
            preview_texture: None,
            heightmap_data: Vec::new(),
            dirty: true,
            last_gen_ms: 0.0,
        }
    }
}

impl HeightmapApp {
    /// Build a sampler closure that captures the noise struct(s) once.
    fn build_sampler(&self) -> Box<dyn Fn(f64, f64) -> f64> {
        let s = self.seed;
        let freq = self.frequency;
        let octaves = self.octaves as usize;
        let lacunarity = self.lacunarity;
        let persistence = self.persistence;
        let offset_x = self.offset_x;
        let offset_y = self.offset_y;
        let noise_type = self.noise_type;

        match self.fractal_type {
            FractalType::None => match noise_type {
                NoiseType::Perlin => {
                    let n = Perlin::new(s);
                    Box::new(move |x, y| n.get([x * freq, y * freq]))
                }
                NoiseType::OpenSimplex => {
                    let n = OpenSimplex::new(s);
                    Box::new(move |x, y| n.get([x * freq, y * freq]))
                }
                NoiseType::SuperSimplex => {
                    let n = SuperSimplex::new(s);
                    Box::new(move |x, y| n.get([x * freq, y * freq]))
                }
                NoiseType::Value => {
                    let n = Value::new(s);
                    Box::new(move |x, y| n.get([x * freq, y * freq]))
                }
                NoiseType::Worley => {
                    let n = Worley::new(s);
                    Box::new(move |x, y| n.get([x * freq, y * freq]))
                }
            },
            FractalType::Fbm => {
                let n = Fbm::<Perlin>::new(s)
                    .set_octaves(octaves)
                    .set_frequency(freq)
                    .set_lacunarity(lacunarity)
                    .set_persistence(persistence);
                Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
            }
            FractalType::Billow => {
                let n = Billow::<Perlin>::new(s)
                    .set_octaves(octaves)
                    .set_frequency(freq)
                    .set_lacunarity(lacunarity)
                    .set_persistence(persistence);
                Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
            }
            FractalType::RidgedMulti => {
                let n = RidgedMulti::<Perlin>::new(s)
                    .set_octaves(octaves)
                    .set_frequency(freq)
                    .set_lacunarity(lacunarity);
                Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
            }
            FractalType::HybridMulti => {
                let n = HybridMulti::<Perlin>::new(s)
                    .set_octaves(octaves)
                    .set_frequency(freq)
                    .set_lacunarity(lacunarity)
                    .set_persistence(persistence);
                Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
            }
            FractalType::BasicMulti => {
                let n = BasicMulti::<Perlin>::new(s)
                    .set_octaves(octaves)
                    .set_frequency(freq)
                    .set_lacunarity(lacunarity)
                    .set_persistence(persistence);
                Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
            }
        }
    }

    /// Generate the full heightmap buffer at `res` x `res`.
    fn generate(&mut self, res: u32) -> Vec<f32> {
        let size = res as usize;
        let mut raw = vec![0.0f64; size * size];

        let mut min_v = f64::MAX;
        let mut max_v = f64::MIN;

        // Build the noise object once, then sample it for every pixel.
        let sampler = self.build_sampler();

        for y in 0..size {
            for x in 0..size {
                let nx = x as f64 / size as f64;
                let ny = y as f64 / size as f64;
                let v = sampler(nx, ny);
                raw[y * size + x] = v;
                if v < min_v { min_v = v; }
                if v > max_v { max_v = v; }
            }
        }

        // Normalize to 0..1
        let range = (max_v - min_v).max(1e-10);
        let mut data: Vec<f32> = raw.iter().map(|v| ((v - min_v) / range) as f32).collect();

        // Post-process
        for v in data.iter_mut() {
            *v = match self.post_process {
                PostProcess::None => *v,
                PostProcess::Terrace => {
                    let levels = self.terrace_levels.max(2) as f32;
                    (*v * levels).floor() / (levels - 1.0)
                }
                PostProcess::Power => v.powf(self.power_exp),
                PostProcess::Invert => 1.0 - *v,
                PostProcess::Abs => ((*v - 0.5) * 2.0).abs(),
                PostProcess::Clamp => {
                    let lo = self.clamp_min;
                    let hi = self.clamp_max.max(lo + 0.01);
                    ((*v).clamp(lo, hi) - lo) / (hi - lo)
                }
            };
        }

        data
    }

    fn rebuild_preview(&mut self, ctx: &egui::Context) {
        let start = std::time::Instant::now();
        let res = self.resolution;
        self.heightmap_data = self.generate(res);
        self.last_gen_ms = start.elapsed().as_secs_f64() * 1000.0;

        let size = res as usize;
        let pixels: Vec<Color32> = self.heightmap_data.iter()
            .map(|&v| self.color_mode.sample(v))
            .collect();

        let img = ColorImage { size: [size, size], pixels };
        let tex = ctx.load_texture("heightmap_preview", img, TextureOptions::NEAREST);
        self.preview_texture = Some(tex);
        self.dirty = false;
    }

    fn export_png(&mut self, path: PathBuf) {
        let res = self.export_resolution;
        let data = self.generate(res);
        let img = GrayImage::from_fn(res, res, |x, y| {
            let v = data[(y * res + x) as usize];
            Luma([(v * 255.0) as u8])
        });
        if let Err(e) = img.save(&path) {
            eprintln!("Error saving heightmap: {e}");
        } else {
            println!("Saved heightmap to {}", path.display());
        }
    }
}

// ── egui App impl ───────────────────────────────────────────────────────────

impl eframe::App for HeightmapApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Left panel: controls
        egui::SidePanel::left("controls")
            .min_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("⛰ Heightmap Generator");
                ui.separator();

                // ── Noise ──
                ui.label("Noise algorithm");
                egui::ComboBox::from_id_salt("noise_type")
                    .selected_text(self.noise_type.label())
                    .show_ui(ui, |ui| {
                        for &nt in NoiseType::ALL {
                            if ui.selectable_value(&mut self.noise_type, nt, nt.label()).changed() {
                                self.dirty = true;
                            }
                        }
                    });

                ui.add_space(4.0);
                ui.label("Fractal combiner");
                egui::ComboBox::from_id_salt("fractal_type")
                    .selected_text(self.fractal_type.label())
                    .show_ui(ui, |ui| {
                        for &ft in FractalType::ALL {
                            if ui.selectable_value(&mut self.fractal_type, ft, ft.label()).changed() {
                                self.dirty = true;
                            }
                        }
                    });

                ui.add_space(8.0);
                ui.separator();

                // ── Parameters ──
                ui.label("Seed");
                ui.horizontal(|ui| {
                    if ui.add(egui::DragValue::new(&mut self.seed).speed(1)).changed() {
                        self.dirty = true;
                    }
                    if ui.button("🎲").clicked() {
                        self.seed = rand::thread_rng().gen();
                        self.dirty = true;
                    }
                });

                if self.fractal_type != FractalType::None {
                    ui.add_space(4.0);
                    ui.label("Octaves");
                    if ui.add(egui::Slider::new(&mut self.octaves, 1..=12)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Frequency");
                    if ui.add(egui::Slider::new(&mut self.frequency, 0.1..=20.0).logarithmic(true)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Lacunarity");
                    if ui.add(egui::Slider::new(&mut self.lacunarity, 1.0..=4.0)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Persistence");
                    if ui.add(egui::Slider::new(&mut self.persistence, 0.0..=1.0)).changed() {
                        self.dirty = true;
                    }
                } else {
                    ui.add_space(4.0);
                    ui.label("Frequency");
                    if ui.add(egui::Slider::new(&mut self.frequency, 0.1..=20.0).logarithmic(true)).changed() {
                        self.dirty = true;
                    }
                }

                ui.add_space(4.0);
                ui.label("Offset X / Y");
                ui.horizontal(|ui| {
                    if ui.add(egui::DragValue::new(&mut self.offset_x).speed(0.01).prefix("X: ")).changed() {
                        self.dirty = true;
                    }
                    if ui.add(egui::DragValue::new(&mut self.offset_y).speed(0.01).prefix("Y: ")).changed() {
                        self.dirty = true;
                    }
                });

                ui.add_space(8.0);
                ui.separator();

                // ── Post-process ──
                ui.label("Post-process");
                egui::ComboBox::from_id_salt("post_process")
                    .selected_text(self.post_process.label())
                    .show_ui(ui, |ui| {
                        for &pp in PostProcess::ALL {
                            if ui.selectable_value(&mut self.post_process, pp, pp.label()).changed() {
                                self.dirty = true;
                            }
                        }
                    });

                match self.post_process {
                    PostProcess::Terrace => {
                        ui.label("Levels");
                        if ui.add(egui::Slider::new(&mut self.terrace_levels, 2..=32)).changed() {
                            self.dirty = true;
                        }
                    }
                    PostProcess::Power => {
                        ui.label("Exponent");
                        if ui.add(egui::Slider::new(&mut self.power_exp, 0.1..=5.0)).changed() {
                            self.dirty = true;
                        }
                    }
                    PostProcess::Clamp => {
                        ui.label("Min / Max");
                        if ui.add(egui::Slider::new(&mut self.clamp_min, 0.0..=1.0).text("min")).changed() {
                            self.dirty = true;
                        }
                        if ui.add(egui::Slider::new(&mut self.clamp_max, 0.0..=1.0).text("max")).changed() {
                            self.dirty = true;
                        }
                    }
                    _ => {}
                }

                ui.add_space(8.0);
                ui.separator();

                // ── Preview settings ──
                ui.label("Preview color");
                egui::ComboBox::from_id_salt("color_mode")
                    .selected_text(self.color_mode.label())
                    .show_ui(ui, |ui| {
                        for &cm in ColorMode::ALL {
                            if ui.selectable_value(&mut self.color_mode, cm, cm.label()).changed() {
                                self.dirty = true;
                            }
                        }
                    });

                ui.add_space(4.0);
                ui.label("Preview resolution");
                if ui.add(egui::Slider::new(&mut self.resolution, 64..=512).suffix("px")).changed() {
                    self.dirty = true;
                }

                ui.add_space(8.0);
                ui.separator();

                // ── Export ──
                ui.label("Export resolution");
                ui.add(egui::Slider::new(&mut self.export_resolution, 256..=4096).suffix("px").logarithmic(true));

                ui.add_space(4.0);
                if ui.button("💾 Export PNG (grayscale)").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name("heightmap.png")
                        .save_file()
                    {
                        self.export_png(path);
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.label(format!("Gen time: {:.1} ms", self.last_gen_ms));
            });

        // Center: preview
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.dirty {
                self.rebuild_preview(ctx);
            }
            if let Some(tex) = &self.preview_texture {
                let avail = ui.available_size();
                let side = avail.x.min(avail.y);
                ui.centered_and_justified(|ui| {
                    ui.image(egui::load::SizedTexture::new(tex.id(), Vec2::splat(side)));
                });
            }
        });
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Heightmap Generator",
        options,
        Box::new(|_cc| Ok(Box::new(HeightmapApp::default()))),
    )
}
