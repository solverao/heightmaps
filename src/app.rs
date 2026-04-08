use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use image::{GrayImage, ImageBuffer, Luma, Rgb, RgbImage};
use noise::{
    BasicMulti, Billow, Fbm, HybridMulti, MultiFractal, NoiseFn, OpenSimplex, Perlin, RidgedMulti,
    SuperSimplex, Value, Worley,
};
use std::path::PathBuf;

use crate::types::{BlendMode, ColorMode, FalloffShape, FractalType, Layer, NoiseType, PostProcess};

fn default_export_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{}/heightmap.png", home)
}

// ── Sampler builder (free function) ────────────────────────────────────────

struct SamplerParams {
    seed: u32,
    noise_type: NoiseType,
    fractal_type: FractalType,
    frequency: f64,
    octaves: usize,
    lacunarity: f64,
    persistence: f64,
    offset_x: f64,
    offset_y: f64,
}

fn build_sampler_from(p: SamplerParams) -> Box<dyn Fn(f64, f64) -> f64> {
    let SamplerParams {
        seed: s, noise_type, fractal_type, frequency: freq,
        octaves, lacunarity, persistence, offset_x, offset_y,
    } = p;

    match fractal_type {
        FractalType::None => match noise_type {
            NoiseType::Perlin       => { let n = Perlin::new(s);       Box::new(move |x, y| n.get([x * freq, y * freq])) }
            NoiseType::OpenSimplex  => { let n = OpenSimplex::new(s);  Box::new(move |x, y| n.get([x * freq, y * freq])) }
            NoiseType::SuperSimplex => { let n = SuperSimplex::new(s); Box::new(move |x, y| n.get([x * freq, y * freq])) }
            NoiseType::Value        => { let n = Value::new(s);        Box::new(move |x, y| n.get([x * freq, y * freq])) }
            NoiseType::Worley       => { let n = Worley::new(s);       Box::new(move |x, y| n.get([x * freq, y * freq])) }
        },
        FractalType::Fbm => {
            let n = Fbm::<Perlin>::new(s).set_octaves(octaves).set_frequency(freq)
                .set_lacunarity(lacunarity).set_persistence(persistence);
            Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
        }
        FractalType::Billow => {
            let n = Billow::<Perlin>::new(s).set_octaves(octaves).set_frequency(freq)
                .set_lacunarity(lacunarity).set_persistence(persistence);
            Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
        }
        FractalType::RidgedMulti => {
            let n = RidgedMulti::<Perlin>::new(s).set_octaves(octaves).set_frequency(freq)
                .set_lacunarity(lacunarity);
            Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
        }
        FractalType::HybridMulti => {
            let n = HybridMulti::<Perlin>::new(s).set_octaves(octaves).set_frequency(freq)
                .set_lacunarity(lacunarity).set_persistence(persistence);
            Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
        }
        FractalType::BasicMulti => {
            let n = BasicMulti::<Perlin>::new(s).set_octaves(octaves).set_frequency(freq)
                .set_lacunarity(lacunarity).set_persistence(persistence);
            Box::new(move |x, y| n.get([x + offset_x, y + offset_y]))
        }
    }
}

// ── Seamless blend helper ───────────────────────────────────────────────────
//
// Samples `f` at (x,y), (x-1,y), (x,y-1), (x-1,y-1) and blends with
// smoothstep weights, forcing perfect tileability in both axes.
fn seamless_blend(f: &dyn Fn(f64, f64) -> f64, x: f64, y: f64) -> f64 {
    let smooth = |t: f64| t * t * (3.0 - 2.0 * t);
    let tx = smooth(x);
    let ty = smooth(y);
    let v00 = f(x,        y       );
    let v10 = f(x - 1.0,  y       );
    let v01 = f(x,        y - 1.0 );
    let v11 = f(x - 1.0,  y - 1.0 );
    v00 + (v10 - v00) * tx + (v01 - v00) * ty + (v00 - v10 - v01 + v11) * tx * ty
}

// ── Application state ───────────────────────────────────────────────────────

pub struct HeightmapApp {
    // Generation params
    pub noise_type: NoiseType,
    pub fractal_type: FractalType,
    pub seed: u32,
    pub octaves: u32,
    pub frequency: f64,
    pub lacunarity: f64,
    pub persistence: f64,
    pub offset_x: f64,
    pub offset_y: f64,

    // Domain warp
    pub warp_enabled: bool,
    pub warp_strength: f64,
    pub warp_frequency: f64,

    // Seamless tiling
    pub seamless_enabled: bool,

    // Extra layers
    pub layers: [Layer; 2],

    // Output
    pub resolution: u32,
    pub export_resolution: u32,

    // Post-process
    pub post_process: PostProcess,
    pub terrace_levels: u32,
    pub power_exp: f32,
    pub clamp_min: f32,
    pub clamp_max: f32,

    // Falloff map
    pub falloff_enabled: bool,
    pub falloff_inner: f32,
    pub falloff_outer: f32,
    pub falloff_shape: FalloffShape,

    // Preview
    pub color_mode: ColorMode,
    pub preview_texture: Option<TextureHandle>,
    pub heightmap_data: Vec<f32>,
    pub dirty: bool,

    // Export
    pub export_path: String,
    pub export_status: Option<String>,
    pub normal_strength: f32,

    // Status
    pub last_gen_ms: f64,
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
            warp_enabled: false,
            warp_strength: 0.3,
            warp_frequency: 2.0,
            seamless_enabled: false,
            layers: [Layer::default(), Layer { seed_offset: 2, ..Layer::default() }],
            resolution: 256,
            export_resolution: 1024,
            post_process: PostProcess::None,
            terrace_levels: 8,
            power_exp: 2.0,
            clamp_min: 0.2,
            clamp_max: 0.8,
            falloff_enabled: false,
            falloff_inner: 0.3,
            falloff_outer: 0.7,
            falloff_shape: FalloffShape::Circle,
            color_mode: ColorMode::Grayscale,
            preview_texture: None,
            heightmap_data: Vec::new(),
            dirty: true,
            export_path: default_export_path(),
            export_status: None,
            normal_strength: 8.0,
            last_gen_ms: 0.0,
        }
    }
}

impl HeightmapApp {
    fn main_sampler_params(&self) -> SamplerParams {
        SamplerParams {
            seed: self.seed,
            noise_type: self.noise_type,
            fractal_type: self.fractal_type,
            frequency: self.frequency,
            octaves: self.octaves as usize,
            lacunarity: self.lacunarity,
            persistence: self.persistence,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
        }
    }

    pub fn build_sampler(&self) -> Box<dyn Fn(f64, f64) -> f64> {
        build_sampler_from(self.main_sampler_params())
    }

    /// Generate the full heightmap buffer at `res` x `res`.
    pub fn generate(&mut self, res: u32) -> Vec<f32> {
        let size = res as usize;
        let n_px = size * size;

        let seamless     = self.seamless_enabled;
        let warp_enabled = self.warp_enabled;
        let warp_strength = self.warp_strength;

        // ── Warp samplers ───────────────────────────────────────────────────
        let (warp_x, warp_y): (Option<Box<dyn Fn(f64,f64)->f64>>, Option<Box<dyn Fn(f64,f64)->f64>>) =
            if warp_enabled {
                let wf = self.warp_frequency;
                let nx = Fbm::<Perlin>::new(self.seed.wrapping_add(997)).set_frequency(wf);
                let ny = Fbm::<Perlin>::new(self.seed.wrapping_add(1999)).set_frequency(wf);
                (
                    Some(Box::new(move |x, y| nx.get([x, y]))),
                    Some(Box::new(move |x, y| ny.get([x, y]))),
                )
            } else {
                (None, None)
            };

        // ── Main + layer samplers ───────────────────────────────────────────
        let main_fn = build_sampler_from(self.main_sampler_params());

        let base_seed = self.seed;
        let base_freq = self.frequency;
        let base_oct  = self.octaves as usize;
        let base_lac  = self.lacunarity;
        let base_per  = self.persistence;
        let base_ox   = self.offset_x;
        let base_oy   = self.offset_y;

        let active: Vec<(f32, BlendMode, Box<dyn Fn(f64,f64)->f64>)> = self.layers.iter()
            .filter(|l| l.enabled)
            .map(|l| {
                let p = SamplerParams {
                    seed: base_seed.wrapping_add(l.seed_offset.wrapping_mul(37)),
                    noise_type: l.noise_type,
                    fractal_type: l.fractal_type,
                    frequency: base_freq * l.frequency_scale,
                    octaves: base_oct,
                    lacunarity: base_lac,
                    persistence: base_per,
                    offset_x: base_ox,
                    offset_y: base_oy,
                };
                (l.weight, l.blend_mode, build_sampler_from(p))
            })
            .collect();

        // ── Macro: sample fn at (px,py) after applying domain warp ─────────
        // Using a macro avoids closure-capture lifetime issues while reusing logic.
        macro_rules! warped {
            ($f:expr, $px:expr, $py:expr) => {{
                let (wx, wy) = match (&warp_x, &warp_y) {
                    (Some(fx), Some(fy)) => (
                        $px + fx($px, $py) * warp_strength,
                        $py + fy($px, $py) * warp_strength,
                    ),
                    _ => ($px, $py),
                };
                $f(wx, wy)
            }};
        }

        // ── Sample all pixels ───────────────────────────────────────────────
        let mut main_raw  = vec![0.0f64; n_px];
        let mut main_min  = f64::MAX;
        let mut main_max  = f64::MIN;

        let n_active = active.len();
        let mut layer_raws: Vec<Vec<f64>> = (0..n_active).map(|_| vec![0.0f64; n_px]).collect();
        let mut layer_mins = vec![f64::MAX; n_active];
        let mut layer_maxs = vec![f64::MIN; n_active];

        for y in 0..size {
            for x in 0..size {
                let nx = x as f64 / size as f64;
                let ny = y as f64 / size as f64;
                let i  = y * size + x;

                let v = if seamless {
                    seamless_blend(&|px, py| warped!(&main_fn, px, py), nx, ny)
                } else {
                    warped!(&main_fn, nx, ny)
                };
                main_raw[i] = v;
                if v < main_min { main_min = v; }
                if v > main_max { main_max = v; }

                for (li, (_, _, sampler)) in active.iter().enumerate() {
                    let lv = if seamless {
                        seamless_blend(&|px, py| warped!(sampler, px, py), nx, ny)
                    } else {
                        warped!(sampler, nx, ny)
                    };
                    layer_raws[li][i] = lv;
                    if lv < layer_mins[li] { layer_mins[li] = lv; }
                    if lv > layer_maxs[li] { layer_maxs[li] = lv; }
                }
            }
        }

        // ── Normalize main to 0..1 ──────────────────────────────────────────
        let main_range = (main_max - main_min).max(1e-10);
        let mut data: Vec<f32> = main_raw.iter()
            .map(|&v| ((v - main_min) / main_range) as f32)
            .collect();

        // ── Blend layers ────────────────────────────────────────────────────
        for (li, (weight, blend_mode, _)) in active.iter().enumerate() {
            let range = (layer_maxs[li] - layer_mins[li]).max(1e-10);
            let w = *weight;
            for i in 0..n_px {
                let lv   = ((layer_raws[li][i] - layer_mins[li]) / range) as f32;
                let base = data[i];
                data[i] = match blend_mode {
                    BlendMode::Add      => (base + lv * w).clamp(0.0, 1.0),
                    BlendMode::Multiply => base * (1.0 - w + lv * w),
                    BlendMode::Max      => base * (1.0 - w) + base.max(lv) * w,
                    BlendMode::Min      => base * (1.0 - w) + base.min(lv) * w,
                    BlendMode::Screen   => {
                        let s = 1.0 - (1.0 - base) * (1.0 - lv);
                        base * (1.0 - w) + s * w
                    }
                };
            }
        }

        // ── Falloff map ─────────────────────────────────────────────────────
        if self.falloff_enabled {
            let inner = self.falloff_inner as f64;
            let outer = self.falloff_outer as f64;
            let shape = self.falloff_shape;
            for y in 0..size {
                for x in 0..size {
                    let nx = x as f64 / size as f64 - 0.5; // -0.5..0.5
                    let ny = y as f64 / size as f64 - 0.5;
                    let dist = match shape {
                        FalloffShape::Circle => (nx * nx + ny * ny).sqrt() * 2.0,
                        FalloffShape::Square => nx.abs().max(ny.abs()) * 2.0,
                    };
                    let t = ((dist - inner) / (outer - inner).max(1e-6)).clamp(0.0, 1.0);
                    let falloff = (1.0 - t * t * (3.0 - 2.0 * t)) as f32;
                    data[y * size + x] *= falloff;
                }
            }
        }

        // ── Post-process ────────────────────────────────────────────────────
        for v in data.iter_mut() {
            *v = match self.post_process {
                PostProcess::None    => *v,
                PostProcess::Terrace => {
                    let levels = self.terrace_levels.max(2) as f32;
                    (*v * levels).floor() / (levels - 1.0)
                }
                PostProcess::Power   => v.powf(self.power_exp),
                PostProcess::Invert  => 1.0 - *v,
                PostProcess::Abs     => ((*v - 0.5) * 2.0).abs(),
                PostProcess::Clamp   => {
                    let lo = self.clamp_min;
                    let hi = self.clamp_max.max(lo + 0.01);
                    ((*v).clamp(lo, hi) - lo) / (hi - lo)
                }
            };
        }

        data
    }

    pub fn rebuild_preview(&mut self, ctx: &egui::Context) {
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

    pub fn export_png(&mut self, path: PathBuf) -> Result<(), String> {
        let res = self.export_resolution;
        let data = self.generate(res);
        let img = GrayImage::from_fn(res, res, |x, y| {
            let v = data[(y * res + x) as usize];
            Luma([(v * 255.0) as u8])
        });
        img.save(&path).map_err(|e| format!("Error: {e}"))
    }

    pub fn export_png16(&mut self, path: PathBuf) -> Result<(), String> {
        let res = self.export_resolution;
        let data = self.generate(res);
        let img: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_fn(res, res, |x, y| {
            let v = data[(y * res + x) as usize];
            Luma([(v * 65535.0) as u16])
        });
        img.save(&path).map_err(|e| format!("Error: {e}"))
    }

    pub fn export_normal_png(&mut self, path: PathBuf) -> Result<(), String> {
        let res = self.export_resolution;
        let data = self.generate(res);
        let strength = self.normal_strength as f64;

        let get = |xi: i32, yi: i32| -> f64 {
            let xi = xi.clamp(0, res as i32 - 1) as u32;
            let yi = yi.clamp(0, res as i32 - 1) as u32;
            data[(yi * res + xi) as usize] as f64
        };

        let img: RgbImage = ImageBuffer::from_fn(res, res, |x, y| {
            let xi = x as i32;
            let yi = y as i32;

            // Sobel 3×3 gradient
            let dx = (get(xi+1, yi-1) + 2.0*get(xi+1, yi) + get(xi+1, yi+1)
                     - get(xi-1, yi-1) - 2.0*get(xi-1, yi) - get(xi-1, yi+1)) / 8.0;
            let dy = (get(xi-1, yi+1) + 2.0*get(xi, yi+1) + get(xi+1, yi+1)
                     - get(xi-1, yi-1) - 2.0*get(xi, yi-1) - get(xi+1, yi-1)) / 8.0;

            let nx = -dx * strength;
            let ny = -dy * strength;
            let nz = 1.0_f64;
            let len = (nx*nx + ny*ny + nz*nz).sqrt();

            let r = ((nx / len + 1.0) * 0.5 * 255.0) as u8;
            let g = ((ny / len + 1.0) * 0.5 * 255.0) as u8;
            let b = ((nz / len + 1.0) * 0.5 * 255.0) as u8;
            Rgb([r, g, b])
        });
        img.save(&path).map_err(|e| format!("Error: {e}"))
    }
}
