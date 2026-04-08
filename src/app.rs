use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use image::{GrayImage, ImageBuffer, Luma, Rgb, RgbImage};
use noise::{
    BasicMulti, Billow, Fbm, HybridMulti, MultiFractal, NoiseFn, OpenSimplex, Perlin, RidgedMulti,
    SuperSimplex, Value, Worley,
};
use rand::SeedableRng;
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
            NoiseType::Perlin       => { let n = Perlin::new(s);       Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq])) }
            NoiseType::OpenSimplex  => { let n = OpenSimplex::new(s);  Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq])) }
            NoiseType::SuperSimplex => { let n = SuperSimplex::new(s); Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq])) }
            NoiseType::Value        => { let n = Value::new(s);        Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq])) }
            NoiseType::Worley       => { let n = Worley::new(s);       Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq])) }
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

// ── Hydraulic erosion helpers ───────────────────────────────────────────────

fn hyd_sample(data: &[f32], n: usize, x: f32, y: f32) -> f32 {
    let xi = (x.floor() as usize).min(n - 2);
    let yi = (y.floor() as usize).min(n - 2);
    let fx = x - xi as f32;
    let fy = y - yi as f32;
    let h00 = data[yi * n + xi];
    let h10 = data[yi * n + xi + 1];
    let h01 = data[(yi + 1) * n + xi];
    let h11 = data[(yi + 1) * n + xi + 1];
    h00 * (1.0 - fx) * (1.0 - fy) + h10 * fx * (1.0 - fy)
        + h01 * (1.0 - fx) * fy   + h11 * fx * fy
}

fn hyd_gradient(data: &[f32], n: usize, x: f32, y: f32) -> (f32, f32) {
    let xi = (x.floor() as usize).min(n - 2);
    let yi = (y.floor() as usize).min(n - 2);
    let fx = x - xi as f32;
    let fy = y - yi as f32;
    let h00 = data[yi * n + xi];
    let h10 = data[yi * n + xi + 1];
    let h01 = data[(yi + 1) * n + xi];
    let h11 = data[(yi + 1) * n + xi + 1];
    let gx = (h10 - h00) * (1.0 - fy) + (h11 - h01) * fy;
    let gy = (h01 - h00) * (1.0 - fx) + (h11 - h10) * fx;
    (gx, gy)
}

fn hyd_deposit(data: &mut [f32], n: usize, x: f32, y: f32, amount: f32) {
    let xi = (x.floor() as usize).min(n - 2);
    let yi = (y.floor() as usize).min(n - 2);
    let fx = x - xi as f32;
    let fy = y - yi as f32;
    data[yi * n + xi]         += amount * (1.0 - fx) * (1.0 - fy);
    data[yi * n + xi + 1]     += amount * fx           * (1.0 - fy);
    data[(yi + 1) * n + xi]   += amount * (1.0 - fx) * fy;
    data[(yi + 1) * n + xi+1] += amount * fx           * fy;
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

    // Chunk navigation
    pub chunk_mode: bool,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub chunk_size: f64,

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
    pub falloff_edge_noise: f32,   // 0 = perfecto, >0 = orilla irregular
    pub falloff_noise_freq: f32,   // frecuencia del ruido de orilla
    pub falloff_exponent: f32,     // <1 suave, >1 pronunciado

    // 3D view
    pub view_3d: bool,
    pub view_rot: f32,
    pub elevation_scale: f32,
    pub view3d_res: u32,
    pub view3d_data: Vec<f32>,
    pub view3d_dirty: bool,

    // Gaussian blur
    pub blur_enabled: bool,
    pub blur_sigma: f32,

    // Percentile normalize
    pub percentile_enabled: bool,
    pub percentile_low: f32,
    pub percentile_high: f32,

    // Hydraulic erosion
    pub erosion_enabled: bool,
    pub erosion_droplets: u32,
    pub erosion_inertia: f32,
    pub erosion_capacity: f32,
    pub erosion_deposition: f32,
    pub erosion_erosion_speed: f32,
    pub erosion_evaporation: f32,

    // Preview
    pub color_mode: ColorMode,
    pub preview_texture: Option<TextureHandle>,
    pub heightmap_data: Vec<f32>,
    pub dirty: bool,

    // Export
    pub export_path: String,
    pub export_status: Option<String>,
    pub normal_strength: f32,

    // Histogram
    pub histogram_visible: bool,

    // 2D zoom / pan
    pub zoom: f32,
    pub pan: egui::Vec2,

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
            chunk_mode: false,
            chunk_x: 0,
            chunk_y: 0,
            chunk_size: 1.0,
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
            falloff_edge_noise: 0.15,
            falloff_noise_freq: 3.0,
            falloff_exponent: 1.0,
            view_3d: false,
            view_rot: 30.0,
            elevation_scale: 0.5,
            view3d_res: 64,
            view3d_data: Vec::new(),
            view3d_dirty: true,
            erosion_enabled: false,
            erosion_droplets: 30_000,
            erosion_inertia: 0.05,
            erosion_capacity: 8.0,
            erosion_deposition: 0.1,
            erosion_erosion_speed: 0.3,
            erosion_evaporation: 0.02,
            blur_enabled: false,
            blur_sigma: 1.5,
            percentile_enabled: false,
            percentile_low: 2.0,
            percentile_high: 98.0,
            color_mode: ColorMode::Grayscale,
            preview_texture: None,
            heightmap_data: Vec::new(),
            dirty: true,
            export_path: default_export_path(),
            export_status: None,
            normal_strength: 8.0,
            histogram_visible: true,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            last_gen_ms: 0.0,
        }
    }
}

impl HeightmapApp {
    pub fn effective_offset(&self) -> (f64, f64) {
        if self.chunk_mode {
            (
                self.chunk_x as f64 * self.chunk_size,
                self.chunk_y as f64 * self.chunk_size,
            )
        } else {
            (self.offset_x, self.offset_y)
        }
    }

    fn main_sampler_params(&self) -> SamplerParams {
        let (ox, oy) = self.effective_offset();
        SamplerParams {
            seed: self.seed,
            noise_type: self.noise_type,
            fractal_type: self.fractal_type,
            frequency: self.frequency,
            octaves: self.octaves as usize,
            lacunarity: self.lacunarity,
            persistence: self.persistence,
            offset_x: ox,
            offset_y: oy,
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
        let (base_ox, base_oy) = self.effective_offset();

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
            let inner     = self.falloff_inner as f64;
            let outer     = self.falloff_outer as f64;
            let shape     = self.falloff_shape;
            let edge_noise = self.falloff_edge_noise as f64;
            let noise_freq = self.falloff_noise_freq as f64;
            let exponent  = self.falloff_exponent as f64;

            // Two independent Perlin instances warp the distance field,
            // breaking the perfect geometry and creating organic coastlines.
            let warp_x = Perlin::new(self.seed.wrapping_add(555));
            let warp_y = Perlin::new(self.seed.wrapping_add(666));

            for y in 0..size {
                for x in 0..size {
                    let orig_nx = x as f64 / size as f64 - 0.5;
                    let orig_ny = y as f64 / size as f64 - 0.5;

                    // Perturb coordinates with noise before measuring distance
                    let nx = orig_nx + warp_x.get([orig_nx * noise_freq, orig_ny * noise_freq]) * edge_noise;
                    let ny = orig_ny + warp_y.get([orig_nx * noise_freq, orig_ny * noise_freq]) * edge_noise;

                    let dist = match shape {
                        FalloffShape::Circle => (nx * nx + ny * ny).sqrt() * 2.0,
                        FalloffShape::Square => nx.abs().max(ny.abs()) * 2.0,
                    };

                    let t = ((dist - inner) / (outer - inner).max(1e-6)).clamp(0.0, 1.0);
                    // Smoothstep, luego curva de potencia para controlar la pendiente
                    let smooth = 1.0 - t * t * (3.0 - 2.0 * t);
                    let falloff = smooth.powf(exponent) as f32;
                    data[y * size + x] *= falloff;
                }
            }
        }

        // ── Hydraulic erosion ───────────────────────────────────────────────
        if self.erosion_enabled {
            self.erode(&mut data, size);
        }

        // ── Gaussian blur ────────────────────────────────────────────────────
        if self.blur_enabled {
            self.gaussian_blur(&mut data, size);
        }

        // ── Percentile normalize ─────────────────────────────────────────────
        if self.percentile_enabled {
            self.percentile_normalize(&mut data);
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

    pub fn rebuild_3d(&mut self) {
        let res = self.view3d_res;
        self.view3d_data = self.generate(res);
        self.view3d_dirty = false;
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
        self.view3d_dirty = true;
        self.zoom = 1.0;
        self.pan  = egui::Vec2::ZERO;
    }

    fn gaussian_blur(&self, data: &mut Vec<f32>, size: usize) {
        let sigma = self.blur_sigma;
        let radius = (sigma * 3.0).ceil() as usize;
        let kernel_size = radius * 2 + 1;

        // Build 1D Gaussian kernel
        let mut kernel: Vec<f32> = (0..kernel_size).map(|i| {
            let x = i as f32 - radius as f32;
            (-x * x / (2.0 * sigma * sigma)).exp()
        }).collect();
        let sum: f32 = kernel.iter().sum();
        kernel.iter_mut().for_each(|k| *k /= sum);

        let mut tmp = vec![0.0f32; size * size];

        // Horizontal pass
        for y in 0..size {
            for x in 0..size {
                let mut acc = 0.0f32;
                for (ki, &kv) in kernel.iter().enumerate() {
                    let sx = (x as i32 + ki as i32 - radius as i32).clamp(0, size as i32 - 1) as usize;
                    acc += data[y * size + sx] * kv;
                }
                tmp[y * size + x] = acc;
            }
        }

        // Vertical pass
        for y in 0..size {
            for x in 0..size {
                let mut acc = 0.0f32;
                for (ki, &kv) in kernel.iter().enumerate() {
                    let sy = (y as i32 + ki as i32 - radius as i32).clamp(0, size as i32 - 1) as usize;
                    acc += tmp[sy * size + x] * kv;
                }
                data[y * size + x] = acc;
            }
        }
    }

    fn percentile_normalize(&self, data: &mut Vec<f32>) {
        let mut sorted = data.clone();
        sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let n = sorted.len();

        let lo = sorted[((self.percentile_low  / 100.0) * (n - 1) as f32) as usize];
        let hi = sorted[((self.percentile_high / 100.0) * (n - 1) as f32).min((n - 1) as f32) as usize];
        let range = (hi - lo).max(1e-10);

        for v in data.iter_mut() {
            *v = ((*v - lo) / range).clamp(0.0, 1.0);
        }
    }

    fn erode(&self, data: &mut Vec<f32>, size: usize) {
        use rand::Rng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.seed as u64 ^ 0xDEAD_BEEF);

        let n          = size;
        let inertia    = self.erosion_inertia;
        let capacity   = self.erosion_capacity;
        let deposit_k  = self.erosion_deposition;
        let erode_k    = self.erosion_erosion_speed;
        let evaporate  = self.erosion_evaporation;
        let gravity    = 10.0_f32;
        let max_steps  = 64_usize;
        let min_slope  = 0.001_f32;

        for _ in 0..self.erosion_droplets {
            // Random start in pixel space, away from borders
            let mut x = rng.gen::<f32>() * (n - 2) as f32 + 0.5;
            let mut y = rng.gen::<f32>() * (n - 2) as f32 + 0.5;
            let mut dir_x   = 0.0_f32;
            let mut dir_y   = 0.0_f32;
            let mut speed   = 1.0_f32;
            let mut water   = 1.0_f32;
            let mut sediment = 0.0_f32;

            for _ in 0..max_steps {
                // Gradient at current position
                let (gx, gy) = hyd_gradient(data, n, x, y);

                // Update direction with inertia
                dir_x = dir_x * inertia - gx * (1.0 - inertia);
                dir_y = dir_y * inertia - gy * (1.0 - inertia);
                let len = (dir_x * dir_x + dir_y * dir_y).sqrt().max(1e-6);
                dir_x /= len;
                dir_y /= len;

                let nx = x + dir_x;
                let ny = y + dir_y;

                // Stop if out of bounds
                if nx < 0.5 || nx >= (n - 1) as f32 - 0.5
                    || ny < 0.5 || ny >= (n - 1) as f32 - 0.5 {
                    break;
                }

                let h_old = hyd_sample(data, n, x, y);
                let h_new = hyd_sample(data, n, nx, ny);
                let delta_h = h_new - h_old;

                // Sediment capacity proportional to speed, water, and slope
                let c = (-delta_h).max(min_slope) * speed * water * capacity;

                if sediment > c || delta_h > 0.0 {
                    // Deposit: fill uphill gaps fully, otherwise deposit fraction
                    let deposit = if delta_h > 0.0 {
                        delta_h.min(sediment)
                    } else {
                        (sediment - c) * deposit_k
                    };
                    sediment -= deposit;
                    hyd_deposit(data, n, x, y, deposit);
                } else {
                    // Erode: remove sediment from current cell
                    let amount = ((c - sediment) * erode_k).min(-delta_h.max(0.0) + 0.01);
                    let amount = amount.max(0.0);
                    sediment  += amount;
                    hyd_deposit(data, n, x, y, -amount);
                }

                speed = (speed * speed + delta_h.abs() * gravity).sqrt().max(0.01);
                water *= 1.0 - evaporate;
                x = nx;
                y = ny;

                if water < 0.001 { break; }
            }
        }

        // Re-normalize to 0..1 after erosion shifts the range
        let min = data.iter().cloned().fold(f32::MAX, f32::min);
        let max = data.iter().cloned().fold(f32::MIN, f32::max);
        let range = (max - min).max(1e-10);
        for v in data.iter_mut() {
            *v = (*v - min) / range;
        }
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
