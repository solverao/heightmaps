use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use image::{GrayImage, ImageBuffer, Luma, Rgb, RgbImage};
use noise::{
    BasicMulti, Billow, Fbm, HybridMulti, MultiFractal, NoiseFn, OpenSimplex, Perlin, RidgedMulti,
    SuperSimplex, Value, Worley,
};
use rand::prelude::*;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::path::PathBuf; // Para paralelismo

use crate::types::{
    BlendMode, ColorMode, ErosionMaskType, FalloffShape, FractalType, Layer, NoiseType,
    PostProcess, Preset,
};

fn default_export_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{}/heightmap.png", home)
}

// ── Voronoi F2−F1 (edge distance) noise ────────────────────────────────────

/// Hashes a grid cell (ix, iy, seed, component) → value in [0, 1].
fn hash_cell(ix: i64, iy: i64, seed: u32, which: u32) -> f64 {
    let mut h: u64 = (ix as u64)
        .wrapping_mul(2_654_435_761)
        ^ (iy as u64).wrapping_mul(805_459_861)
        ^ (seed as u64).wrapping_mul(1_234_567_891)
        ^ (which as u64).wrapping_mul(987_654_321);
    // MurmurHash3 finalizer
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^= h >> 33;
    h as f64 / u64::MAX as f64
}

/// Returns F2 − F1 (edge distance), mapped to approximately [−1, 1].
fn voronoi_edge_noise(fx: f64, fy: f64, seed: u32) -> f64 {
    let ix = fx.floor() as i64;
    let iy = fy.floor() as i64;

    let mut f1 = f64::MAX;
    let mut f2 = f64::MAX;

    for dy in -2..=2i64 {
        for dx in -2..=2i64 {
            let cx = ix + dx;
            let cy = iy + dy;
            let px = cx as f64 + hash_cell(cx, cy, seed, 0);
            let py = cy as f64 + hash_cell(cx, cy, seed, 1);
            let d2 = (fx - px).powi(2) + (fy - py).powi(2);
            if d2 < f1 {
                f2 = f1;
                f1 = d2;
            } else if d2 < f2 {
                f2 = d2;
            }
        }
    }

    // (F2 - F1) is in [0, ~1.5]; remap to [-1, 1] for compatibility with other noise
    (f2.sqrt() - f1.sqrt()) * 2.0 - 0.5
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
    // VoronoiEdge ignores fractal type — always raw F2-F1 distance
    if p.noise_type == NoiseType::VoronoiEdge {
        let seed = p.seed;
        let freq = p.frequency;
        let (ox, oy) = (p.offset_x, p.offset_y);
        return Box::new(move |x, y| voronoi_edge_noise((x + ox) * freq, (y + oy) * freq, seed));
    }

    let SamplerParams {
        seed: s,
        noise_type,
        fractal_type,
        frequency: freq,
        octaves,
        lacunarity,
        persistence,
        offset_x,
        offset_y,
    } = p;

    match fractal_type {
        FractalType::None => match noise_type {
            NoiseType::Perlin => {
                let n = Perlin::new(s);
                Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq]))
            }
            NoiseType::OpenSimplex => {
                let n = OpenSimplex::new(s);
                Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq]))
            }
            NoiseType::SuperSimplex => {
                let n = SuperSimplex::new(s);
                Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq]))
            }
            NoiseType::Value => {
                let n = Value::new(s);
                Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq]))
            }
            NoiseType::Worley => {
                let n = Worley::new(s);
                Box::new(move |x, y| n.get([(x + offset_x) * freq, (y + offset_y) * freq]))
            }
            // Handled by early return above; unreachable
            NoiseType::VoronoiEdge => {
                Box::new(move |x, y| voronoi_edge_noise((x + offset_x) * freq, (y + offset_y) * freq, s))
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

// ── Seamless blend helper ───────────────────────────────────────────────────
//
// Samples `f` at (x,y), (x-1,y), (x,y-1), (x-1,y-1) and blends with
// smoothstep weights, forcing perfect tileability in both axes.
fn seamless_blend(f: &dyn Fn(f64, f64) -> f64, x: f64, y: f64) -> f64 {
    let smooth = |t: f64| t * t * (3.0 - 2.0 * t);
    let tx = smooth(x);
    let ty = smooth(y);
    let v00 = f(x, y);
    let v10 = f(x - 1.0, y);
    let v01 = f(x, y - 1.0);
    let v11 = f(x - 1.0, y - 1.0);
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

    // Domain warp second pass
    pub warp2_enabled: bool,
    pub warp2_strength: f64,
    pub warp2_frequency: f64,

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
    pub falloff_edge_noise: f32, // 0 = perfecto, >0 = orilla irregular
    pub falloff_noise_freq: f32, // frecuencia del ruido de orilla
    pub falloff_exponent: f32,   // <1 suave, >1 pronunciado

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

    // Erosion mask
    pub erosion_mask_enabled: bool,
    pub erosion_mask_type: ErosionMaskType,
    pub erosion_mask_min: f32,
    pub erosion_mask_max: f32,

    // Hydraulic erosion
    pub erosion_enabled: bool,
    pub erosion_droplets: u32,
    pub erosion_inertia: f32,
    pub erosion_capacity: f32,
    pub erosion_deposition: f32,
    pub erosion_erosion_speed: f32,
    pub erosion_evaporation: f32,
    pub erosion_radius: usize,

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

    // Thermal erosion
    pub thermal_enabled: bool,
    pub thermal_talus: f32,
    pub thermal_iterations: u32,
    pub thermal_strength: f32,

    // OBJ export
    pub export_obj_res: u32,

    // Preset save/load
    pub preset_path: String,
    pub preset_status: Option<String>,

    // Batch chunk export
    pub batch_x_min: i32,
    pub batch_x_max: i32,
    pub batch_y_min: i32,
    pub batch_y_max: i32,
    pub batch_status: Option<String>,

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
            warp2_enabled: false,
            warp2_strength: 0.2,
            warp2_frequency: 3.0,
            seamless_enabled: false,
            layers: [
                Layer::default(),
                Layer {
                    seed_offset: 2,
                    ..Layer::default()
                },
            ],
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
            erosion_mask_enabled: false,
            erosion_mask_type: ErosionMaskType::Height,
            erosion_mask_min: 0.3,
            erosion_mask_max: 1.0,
            erosion_enabled: false,
            erosion_droplets: 30_000,
            erosion_inertia: 0.05,
            erosion_capacity: 8.0,
            erosion_deposition: 0.1,
            erosion_erosion_speed: 0.3,
            erosion_evaporation: 0.02,
            erosion_radius: 3,
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
            thermal_enabled: false,
            thermal_talus: 0.08,
            thermal_iterations: 25,
            thermal_strength: 0.5,
            export_obj_res: 256,
            preset_path: {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                format!("{}/preset.json", home)
            },
            preset_status: None,
            batch_x_min: 0,
            batch_x_max: 2,
            batch_y_min: 0,
            batch_y_max: 2,
            batch_status: None,
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

        let seamless = self.seamless_enabled;
        let warp_enabled = self.warp_enabled;
        let warp_strength = self.warp_strength;
        let warp2_enabled = self.warp2_enabled;
        let warp2_strength = self.warp2_strength;

        // ── Warp samplers (pass 1) ──────────────────────────────────────────
        let (warp_x, warp_y): (
            Option<Box<dyn Fn(f64, f64) -> f64>>,
            Option<Box<dyn Fn(f64, f64) -> f64>>,
        ) = if warp_enabled {
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

        // ── Warp samplers (pass 2) ──────────────────────────────────────────
        let (warp2_x, warp2_y): (
            Option<Box<dyn Fn(f64, f64) -> f64>>,
            Option<Box<dyn Fn(f64, f64) -> f64>>,
        ) = if warp_enabled && warp2_enabled {
            let wf2 = self.warp2_frequency;
            let nx2 = Fbm::<Perlin>::new(self.seed.wrapping_add(3001)).set_frequency(wf2);
            let ny2 = Fbm::<Perlin>::new(self.seed.wrapping_add(4003)).set_frequency(wf2);
            (
                Some(Box::new(move |x, y| nx2.get([x, y]))),
                Some(Box::new(move |x, y| ny2.get([x, y]))),
            )
        } else {
            (None, None)
        };

        // ── Main + layer samplers ───────────────────────────────────────────
        let main_fn = self.build_sampler();

        let base_seed = self.seed;
        let base_freq = self.frequency;
        let base_oct = self.octaves as usize;
        let base_lac = self.lacunarity;
        let base_per = self.persistence;
        let (base_ox, base_oy) = self.effective_offset();

        let active: Vec<(f32, BlendMode, Box<dyn Fn(f64, f64) -> f64>)> = self
            .layers
            .iter()
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
                // Pass 1
                let (wx1, wy1) = match (&warp_x, &warp_y) {
                    (Some(fx), Some(fy)) => (
                        $px + fx($px, $py) * warp_strength,
                        $py + fy($px, $py) * warp_strength,
                    ),
                    _ => ($px, $py),
                };
                // Pass 2 (uses warped coords from pass 1 as input)
                let (wx, wy) = match (&warp2_x, &warp2_y) {
                    (Some(fx), Some(fy)) => (
                        wx1 + fx(wx1, wy1) * warp2_strength,
                        wy1 + fy(wx1, wy1) * warp2_strength,
                    ),
                    _ => (wx1, wy1),
                };
                $f(wx, wy)
            }};
        }

        // ── Sample all pixels ───────────────────────────────────────────────
        let mut main_raw = vec![0.0f64; n_px];
        let mut main_min = f64::MAX;
        let mut main_max = f64::MIN;

        let n_active = active.len();
        let mut layer_raws: Vec<Vec<f64>> = (0..n_active).map(|_| vec![0.0f64; n_px]).collect();
        let mut layer_mins = vec![f64::MAX; n_active];
        let mut layer_maxs = vec![f64::MIN; n_active];

        for y in 0..size {
            for x in 0..size {
                let nx = x as f64 / size as f64;
                let ny = y as f64 / size as f64;
                let i = y * size + x;

                let v = if seamless {
                    seamless_blend(&|px, py| warped!(&main_fn, px, py), nx, ny)
                } else {
                    warped!(&main_fn, nx, ny)
                };
                main_raw[i] = v;
                if v < main_min {
                    main_min = v;
                }
                if v > main_max {
                    main_max = v;
                }

                for (li, (_, _, sampler)) in active.iter().enumerate() {
                    let lv = if seamless {
                        seamless_blend(&|px, py| warped!(sampler, px, py), nx, ny)
                    } else {
                        warped!(sampler, nx, ny)
                    };
                    layer_raws[li][i] = lv;
                    if lv < layer_mins[li] {
                        layer_mins[li] = lv;
                    }
                    if lv > layer_maxs[li] {
                        layer_maxs[li] = lv;
                    }
                }
            }
        }

        // ── Normalize main to 0..1 ──────────────────────────────────────────
        let main_range = (main_max - main_min).max(1e-10);
        let mut data: Vec<f32> = main_raw
            .iter()
            .map(|&v| ((v - main_min) / main_range) as f32)
            .collect();

        // ── Blend layers ────────────────────────────────────────────────────
        for (li, (weight, blend_mode, _)) in active.iter().enumerate() {
            let range = (layer_maxs[li] - layer_mins[li]).max(1e-10);
            let w = *weight;
            for i in 0..n_px {
                let lv = ((layer_raws[li][i] - layer_mins[li]) / range) as f32;
                let base = data[i];
                data[i] = match blend_mode {
                    BlendMode::Add => (base + lv * w).clamp(0.0, 1.0),
                    BlendMode::Multiply => base * (1.0 - w + lv * w),
                    BlendMode::Max => base * (1.0 - w) + base.max(lv) * w,
                    BlendMode::Min => base * (1.0 - w) + base.min(lv) * w,
                    BlendMode::Screen => {
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
            let edge_noise = self.falloff_edge_noise as f64;
            let noise_freq = self.falloff_noise_freq as f64;
            let exponent = self.falloff_exponent as f64;

            // Two independent Perlin instances warp the distance field,
            // breaking the perfect geometry and creating organic coastlines.
            let warp_x = Perlin::new(self.seed.wrapping_add(555));
            let warp_y = Perlin::new(self.seed.wrapping_add(666));

            for y in 0..size {
                for x in 0..size {
                    let orig_nx = x as f64 / size as f64 - 0.5;
                    let orig_ny = y as f64 / size as f64 - 0.5;

                    // Perturb coordinates with noise before measuring distance
                    let nx = orig_nx
                        + warp_x.get([orig_nx * noise_freq, orig_ny * noise_freq]) * edge_noise;
                    let ny = orig_ny
                        + warp_y.get([orig_nx * noise_freq, orig_ny * noise_freq]) * edge_noise;

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
            if self.erosion_mask_enabled {
                let pre = data.clone();
                self.erode(&mut data, size);
                let mask = self.build_erosion_mask(&pre, size);
                for i in 0..data.len() {
                    data[i] = pre[i] + (data[i] - pre[i]) * mask[i];
                }
            } else {
                self.erode(&mut data, size);
            }
        }

        // ── Thermal erosion ─────────────────────────────────────────────────
        if self.thermal_enabled {
            if self.erosion_mask_enabled {
                let pre = data.clone();
                self.thermal_erode(&mut data, size);
                let mask = self.build_erosion_mask(&pre, size);
                for i in 0..data.len() {
                    data[i] = pre[i] + (data[i] - pre[i]) * mask[i];
                }
            } else {
                self.thermal_erode(&mut data, size);
            }
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
        let pixels: Vec<Color32> = self
            .heightmap_data
            .iter()
            .map(|&v| self.color_mode.sample(v))
            .collect();

        let img = ColorImage {
            size: [size, size],
            pixels,
        };
        let tex = ctx.load_texture("heightmap_preview", img, TextureOptions::NEAREST);
        self.preview_texture = Some(tex);
        self.dirty = false;
        self.view3d_dirty = true;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    fn gaussian_blur(&self, data: &mut Vec<f32>, size: usize) {
        let sigma = self.blur_sigma;
        let radius = (sigma * 3.0).ceil() as usize;
        let kernel_size = radius * 2 + 1;

        // Build 1D Gaussian kernel
        let mut kernel: Vec<f32> = (0..kernel_size)
            .map(|i| {
                let x = i as f32 - radius as f32;
                (-x * x / (2.0 * sigma * sigma)).exp()
            })
            .collect();
        let sum: f32 = kernel.iter().sum();
        kernel.iter_mut().for_each(|k| *k /= sum);

        let mut tmp = vec![0.0f32; size * size];

        // Horizontal pass
        for y in 0..size {
            for x in 0..size {
                let mut acc = 0.0f32;
                for (ki, &kv) in kernel.iter().enumerate() {
                    let sx =
                        (x as i32 + ki as i32 - radius as i32).clamp(0, size as i32 - 1) as usize;
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
                    let sy =
                        (y as i32 + ki as i32 - radius as i32).clamp(0, size as i32 - 1) as usize;
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

        let lo = sorted[((self.percentile_low / 100.0) * (n - 1) as f32) as usize];
        let hi =
            sorted[((self.percentile_high / 100.0) * (n - 1) as f32).min((n - 1) as f32) as usize];
        let range = (hi - lo).max(1e-10);

        for v in data.iter_mut() {
            *v = ((*v - lo) / range).clamp(0.0, 1.0);
        }
    }

    fn erode(&self, data: &mut Vec<f32>, size: usize) {
        let n = size;
        let inertia = self.erosion_inertia;
        let capacity = self.erosion_capacity;
        let deposit_k = self.erosion_deposition;
        let erode_k = self.erosion_erosion_speed;
        let evaporate = self.erosion_evaporation;
        let gravity = 10.0_f32;
        let max_steps = 64_usize;
        let min_slope = 0.001_f32;
        let radius = self.erosion_radius;

        let mut rng = rand::rngs::SmallRng::seed_from_u64(self.seed as u64);

        for _ in 0..self.erosion_droplets {
            let mut x = rng.gen::<f32>() * (n - 2) as f32 + 0.5;
            let mut y = rng.gen::<f32>() * (n - 2) as f32 + 0.5;
            let mut dir_x = 0.0f32;
            let mut dir_y = 0.0f32;
            let mut speed = 1.0f32;
            let mut water = 1.0f32;
            let mut sediment = 0.0f32;

            // #2: Leer gradiente y altura de la posición inicial una sola vez
            let (mut cur_gx, mut cur_gy, mut h_old) = self.get_gradient_and_height(data, n, x, y);

            for _ in 0..max_steps {
                // Inercia (usando gradiente cacheado)
                dir_x = dir_x * inertia - cur_gx * (1.0 - inertia);
                dir_y = dir_y * inertia - cur_gy * (1.0 - inertia);

                let len_sq = dir_x * dir_x + dir_y * dir_y;
                if len_sq > 0.0 {
                    let len = len_sq.sqrt();
                    dir_x /= len;
                    dir_y /= len;
                }

                let nx = x + dir_x;
                let ny = y + dir_y;

                if nx < 1.0 || nx >= (n - 2) as f32 || ny < 1.0 || ny >= (n - 2) as f32 {
                    break;
                }

                // #2: Leer siguiente posición una vez; guardar para la próxima iteración
                let (next_gx, next_gy, h_new) = self.get_gradient_and_height(data, n, nx, ny);
                let delta_h = h_new - h_old;

                let c = (-delta_h).max(min_slope) * speed * water * capacity;

                if sediment > c || delta_h > 0.0 {
                    let deposit = if delta_h > 0.0 {
                        delta_h.min(sediment)
                    } else {
                        (sediment - c) * deposit_k
                    };
                    sediment -= deposit;
                    // #3: Depositar distribuido en radio
                    self.deposit_with_radius(data, n, x, y, deposit, radius);
                } else {
                    let amount = ((c - sediment) * erode_k).min(-delta_h).max(0.0);
                    sediment += amount;
                    // #3: Erosionar distribuido en radio
                    self.deposit_with_radius(data, n, x, y, -amount, radius);
                }

                // #1: Física correcta de velocidad — subir cuesta ralentiza la gota
                speed = (speed * speed - delta_h * gravity).max(0.0).sqrt().max(0.01);
                water *= 1.0 - evaporate;
                x = nx;
                y = ny;

                // #2: Actualizar caché para la siguiente iteración
                cur_gx = next_gx;
                cur_gy = next_gy;
                h_old = h_new;

                if water < 0.01 {
                    break;
                }
            }
        }

        let min = data.iter().cloned().fold(f32::MAX, f32::min);
        let max = data.iter().cloned().fold(f32::MIN, f32::max);
        let range = (max - min).max(1e-10);
        for v in data.iter_mut() {
            *v = (*v - min) / range;
        }
    }

    fn deposit_with_radius(
        &self,
        data: &mut Vec<f32>,
        n: usize,
        x: f32,
        y: f32,
        amount: f32,
        radius: usize,
    ) {
        if radius == 0 {
            self.hyd_deposit_bilinear(data, n, x, y, amount);
            return;
        }

        let xi = x as i32;
        let yi = y as i32;
        let r = radius as i32;
        let frac_x = x - xi as f32;
        let frac_y = y - yi as f32;

        let mut cells: Vec<(usize, f32)> = Vec::with_capacity((2 * radius + 1).pow(2));
        let mut total_weight = 0.0f32;

        for dy in -r..=r {
            for dx in -r..=r {
                let cx = xi + dx;
                let cy = yi + dy;
                if cx < 0 || cy < 0 || cx >= n as i32 || cy >= n as i32 {
                    continue;
                }
                let dist = ((dx as f32 - frac_x).powi(2) + (dy as f32 - frac_y).powi(2)).sqrt();
                let w = (radius as f32 - dist).max(0.0);
                if w > 0.0 {
                    total_weight += w;
                    cells.push((cy as usize * n + cx as usize, w));
                }
            }
        }

        if total_weight > 0.0 {
            for (idx, w) in cells {
                data[idx] += amount * w / total_weight;
            }
        }
    }

    fn hyd_deposit_bilinear(&self, data: &mut Vec<f32>, n: usize, x: f32, y: f32, amount: f32) {
        let xi = (x as usize).min(n - 2); // Protegemos el (xi + 1)
        let yi = (y as usize).min(n - 2); // Protegemos el (yi + 1)
        let u = x - x.floor();
        let v = y - y.floor();

        let w00 = (1.0 - u) * (1.0 - v);
        let w10 = u * (1.0 - v);
        let w01 = (1.0 - u) * v;
        let w11 = u * v;

        data[yi * n + xi] += amount * w00;
        data[yi * n + xi + 1] += amount * w10;
        data[(yi + 1) * n + xi] += amount * w01;
        data[(yi + 1) * n + xi + 1] += amount * w11;
    }

    fn get_gradient_and_height(&self, data: &[f32], n: usize, x: f32, y: f32) -> (f32, f32, f32) {
        let xi = x as usize;
        let yi = y as usize;
        let u = x - xi as f32;
        let v = y - yi as f32;

        // Índices de los 4 vecinos
        let idx00 = yi * n + xi;
        let idx10 = idx00 + 1;
        let idx01 = (yi + 1) * n + xi;
        let idx11 = idx01 + 1;

        let h00 = data[idx00];
        let h10 = data[idx10];
        let h01 = data[idx01];
        let h11 = data[idx11];

        // Gradiente (derivadas parciales aproximadas)
        let gx = (h10 - h00) * (1.0 - v) + (h11 - h01) * v;
        let gy = (h01 - h00) * (1.0 - u) + (h11 - h10) * u;

        // Altura interpolada (bilineal)
        let height =
            h00 * (1.0 - u) * (1.0 - v) + h10 * u * (1.0 - v) + h01 * (1.0 - u) * v + h11 * u * v;

        (gx, gy, height)
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

    // ── Randomize all generation parameters ─────────────────────────────────
    pub fn randomize(&mut self) {
        let mut rng = rand::thread_rng();

        self.seed = rng.gen();
        self.noise_type = *NoiseType::ALL.choose(&mut rng).unwrap();
        self.fractal_type = *FractalType::ALL.choose(&mut rng).unwrap();
        self.octaves = rng.gen_range(2..=8);
        self.frequency = rng.gen_range(1.0_f64..=8.0);
        self.lacunarity = rng.gen_range(1.5_f64..=3.0);
        self.persistence = rng.gen_range(0.3_f64..=0.7);

        // domain warp: ~40% de probabilidad
        self.warp_enabled = rng.gen_bool(0.4);
        if self.warp_enabled {
            self.warp_strength = rng.gen_range(0.05_f64..=1.0);
            self.warp_frequency = rng.gen_range(0.5_f64..=6.0);
        }

        // post-process: elegir uno al azar (con más peso a None)
        self.post_process = if rng.gen_bool(0.5) {
            PostProcess::None
        } else {
            *PostProcess::ALL.choose(&mut rng).unwrap()
        };
        self.terrace_levels = rng.gen_range(4..=16);
        self.power_exp = rng.gen_range(0.3_f32..=4.0);

        // falloff: ~35%
        self.falloff_enabled = rng.gen_bool(0.35);
        if self.falloff_enabled {
            let inner: f32 = rng.gen_range(0.1..=0.45);
            self.falloff_inner = inner;
            self.falloff_outer = rng.gen_range((inner + 0.1).min(0.9)..=0.95);
            self.falloff_shape = *FalloffShape::ALL.choose(&mut rng).unwrap();
            self.falloff_edge_noise = rng.gen_range(0.0_f32..=0.3);
            self.falloff_noise_freq = rng.gen_range(1.0_f32..=8.0);
            self.falloff_exponent = rng.gen_range(0.4_f32..=2.5);
        }

        // capas adicionales: cada una ~30%
        for layer in self.layers.iter_mut() {
            layer.enabled = rng.gen_bool(0.3);
            if layer.enabled {
                layer.noise_type = *NoiseType::ALL.choose(&mut rng).unwrap();
                layer.fractal_type = *FractalType::ALL.choose(&mut rng).unwrap();
                layer.blend_mode = *BlendMode::ALL.choose(&mut rng).unwrap();
                layer.weight = rng.gen_range(0.2_f32..=0.8);
                layer.frequency_scale = rng.gen_range(0.5_f64..=4.0);
                layer.seed_offset = rng.gen_range(1..=500u32);
            }
        }

        self.dirty = true;
    }

    // ── Erosion mask ─────────────────────────────────────────────────────────
    /// Returns a per-pixel weight in [0, 1]: 1 = full erosion, 0 = no erosion.
    fn build_erosion_mask(&self, data: &[f32], size: usize) -> Vec<f32> {
        let lo = self.erosion_mask_min;
        let hi = self.erosion_mask_max.max(lo + 0.01);
        let feather = ((hi - lo) * 0.05).max(0.005);

        let smooth_edge = |v: f32, edge: f32, rising: bool| -> f32 {
            let t = ((v - (edge - feather)) / (2.0 * feather)).clamp(0.0, 1.0);
            let s = t * t * (3.0 - 2.0 * t);
            if rising { s } else { 1.0 - s }
        };

        let weight = |v: f32| -> f32 {
            if v <= lo - feather || v >= hi + feather {
                0.0
            } else if v >= lo && v <= hi {
                1.0
            } else if v < lo {
                smooth_edge(v, lo, true)
            } else {
                smooth_edge(v, hi, false)
            }
        };

        match self.erosion_mask_type {
            ErosionMaskType::Height => data.iter().map(|&v| weight(v)).collect(),
            ErosionMaskType::Slope => {
                let mut mask = vec![0.0f32; size * size];
                for y in 0..size {
                    for x in 0..size {
                        let get = |xi: i32, yi: i32| -> f32 {
                            data[yi.clamp(0, size as i32 - 1) as usize * size
                                + xi.clamp(0, size as i32 - 1) as usize]
                        };
                        let xi = x as i32;
                        let yi = y as i32;
                        let gx = get(xi + 1, yi) - get(xi - 1, yi);
                        let gy = get(xi, yi + 1) - get(xi, yi - 1);
                        let slope = (gx * gx + gy * gy).sqrt() * 0.5;
                        mask[y * size + x] = weight(slope);
                    }
                }
                mask
            }
        }
    }

    // ── Thermal erosion ─────────────────────────────────────────────────────
    fn thermal_erode(&self, data: &mut Vec<f32>, size: usize) {
        let talus = self.thermal_talus;
        let strength = self.thermal_strength;

        for _ in 0..self.thermal_iterations {
            let snapshot = data.clone();
            for y in 0..size {
                for x in 0..size {
                    let h = snapshot[y * size + x];
                    // 4-neighbor offsets using wrapping sub so bounds check catches usize overflow
                    let neighbors: [(usize, usize); 4] = [
                        (x.wrapping_sub(1), y),
                        (x + 1, y),
                        (x, y.wrapping_sub(1)),
                        (x, y + 1),
                    ];
                    let mut diffs = [0.0f32; 4];
                    let mut total_diff = 0.0f32;
                    for (i, &(nx, ny)) in neighbors.iter().enumerate() {
                        if nx < size && ny < size {
                            let diff = h - snapshot[ny * size + nx];
                            if diff > talus {
                                diffs[i] = diff - talus;
                                total_diff += diffs[i];
                            }
                        }
                    }
                    if total_diff > 0.0 {
                        let moved = total_diff * strength * 0.5;
                        data[y * size + x] -= moved;
                        for (i, &(nx, ny)) in neighbors.iter().enumerate() {
                            if nx < size && ny < size && diffs[i] > 0.0 {
                                data[ny * size + nx] += moved * (diffs[i] / total_diff);
                            }
                        }
                    }
                }
            }
        }

        let min = data.iter().cloned().fold(f32::MAX, f32::min);
        let max = data.iter().cloned().fold(f32::MIN, f32::max);
        let range = (max - min).max(1e-10);
        for v in data.iter_mut() {
            *v = (*v - min) / range;
        }
    }

    // ── OBJ export ──────────────────────────────────────────────────────────
    pub fn export_obj(&mut self, path: PathBuf) -> Result<(), String> {
        use std::fmt::Write as FmtWrite;
        let res = self.export_obj_res;
        let data = self.generate(res);
        let size = res as usize;
        let scale = self.elevation_scale;

        let mut out = String::with_capacity(size * size * 32);
        writeln!(out, "# Heightmap Generator export").unwrap();
        writeln!(out, "o heightmap").unwrap();

        for y in 0..size {
            for x in 0..size {
                let h = data[y * size + x];
                let xf = x as f32 / (size - 1).max(1) as f32;
                let zf = y as f32 / (size - 1).max(1) as f32;
                writeln!(out, "v {:.6} {:.6} {:.6}", xf, h * scale, zf).unwrap();
            }
        }

        for y in 0..(size - 1) {
            for x in 0..(size - 1) {
                let i0 = y * size + x + 1;
                let i1 = y * size + x + 2;
                let i2 = (y + 1) * size + x + 1;
                let i3 = (y + 1) * size + x + 2;
                writeln!(out, "f {i0} {i1} {i2}").unwrap();
                writeln!(out, "f {i1} {i3} {i2}").unwrap();
            }
        }

        std::fs::write(&path, out).map_err(|e| format!("Error OBJ: {e}"))
    }

    // ── Preset save / load ──────────────────────────────────────────────────
    pub fn save_preset(&self, path: PathBuf) -> Result<(), String> {
        let preset = Preset {
            noise_type: self.noise_type,
            fractal_type: self.fractal_type,
            seed: self.seed,
            octaves: self.octaves,
            frequency: self.frequency,
            lacunarity: self.lacunarity,
            persistence: self.persistence,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            chunk_mode: self.chunk_mode,
            chunk_x: self.chunk_x,
            chunk_y: self.chunk_y,
            chunk_size: self.chunk_size,
            warp_enabled: self.warp_enabled,
            warp_strength: self.warp_strength,
            warp_frequency: self.warp_frequency,
            warp2_enabled: self.warp2_enabled,
            warp2_strength: self.warp2_strength,
            warp2_frequency: self.warp2_frequency,
            seamless_enabled: self.seamless_enabled,
            layers: [
                Layer {
                    enabled: self.layers[0].enabled,
                    noise_type: self.layers[0].noise_type,
                    fractal_type: self.layers[0].fractal_type,
                    seed_offset: self.layers[0].seed_offset,
                    frequency_scale: self.layers[0].frequency_scale,
                    weight: self.layers[0].weight,
                    blend_mode: self.layers[0].blend_mode,
                },
                Layer {
                    enabled: self.layers[1].enabled,
                    noise_type: self.layers[1].noise_type,
                    fractal_type: self.layers[1].fractal_type,
                    seed_offset: self.layers[1].seed_offset,
                    frequency_scale: self.layers[1].frequency_scale,
                    weight: self.layers[1].weight,
                    blend_mode: self.layers[1].blend_mode,
                },
            ],
            resolution: self.resolution,
            export_resolution: self.export_resolution,
            post_process: self.post_process,
            terrace_levels: self.terrace_levels,
            power_exp: self.power_exp,
            clamp_min: self.clamp_min,
            clamp_max: self.clamp_max,
            falloff_enabled: self.falloff_enabled,
            falloff_inner: self.falloff_inner,
            falloff_outer: self.falloff_outer,
            falloff_shape: self.falloff_shape,
            falloff_edge_noise: self.falloff_edge_noise,
            falloff_noise_freq: self.falloff_noise_freq,
            falloff_exponent: self.falloff_exponent,
            erosion_mask_enabled: self.erosion_mask_enabled,
            erosion_mask_type: self.erosion_mask_type,
            erosion_mask_min: self.erosion_mask_min,
            erosion_mask_max: self.erosion_mask_max,
            erosion_enabled: self.erosion_enabled,
            erosion_droplets: self.erosion_droplets,
            erosion_inertia: self.erosion_inertia,
            erosion_capacity: self.erosion_capacity,
            erosion_deposition: self.erosion_deposition,
            erosion_erosion_speed: self.erosion_erosion_speed,
            erosion_evaporation: self.erosion_evaporation,
            erosion_radius: self.erosion_radius,
            thermal_enabled: self.thermal_enabled,
            thermal_talus: self.thermal_talus,
            thermal_iterations: self.thermal_iterations,
            thermal_strength: self.thermal_strength,
            blur_enabled: self.blur_enabled,
            blur_sigma: self.blur_sigma,
            percentile_enabled: self.percentile_enabled,
            percentile_low: self.percentile_low,
            percentile_high: self.percentile_high,
            color_mode: self.color_mode,
            normal_strength: self.normal_strength,
        };
        let json = serde_json::to_string_pretty(&preset)
            .map_err(|e| format!("Serialize error: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("Write error: {e}"))
    }

    pub fn load_preset(&mut self, path: PathBuf) -> Result<(), String> {
        let json = std::fs::read_to_string(&path).map_err(|e| format!("Read error: {e}"))?;
        let p: Preset = serde_json::from_str(&json).map_err(|e| format!("Parse error: {e}"))?;
        self.noise_type = p.noise_type;
        self.fractal_type = p.fractal_type;
        self.seed = p.seed;
        self.octaves = p.octaves;
        self.frequency = p.frequency;
        self.lacunarity = p.lacunarity;
        self.persistence = p.persistence;
        self.offset_x = p.offset_x;
        self.offset_y = p.offset_y;
        self.chunk_mode = p.chunk_mode;
        self.chunk_x = p.chunk_x;
        self.chunk_y = p.chunk_y;
        self.chunk_size = p.chunk_size;
        self.warp_enabled = p.warp_enabled;
        self.warp_strength = p.warp_strength;
        self.warp_frequency = p.warp_frequency;
        self.warp2_enabled = p.warp2_enabled;
        self.warp2_strength = p.warp2_strength;
        self.warp2_frequency = p.warp2_frequency;
        self.seamless_enabled = p.seamless_enabled;
        self.layers = p.layers;
        self.resolution = p.resolution;
        self.export_resolution = p.export_resolution;
        self.post_process = p.post_process;
        self.terrace_levels = p.terrace_levels;
        self.power_exp = p.power_exp;
        self.clamp_min = p.clamp_min;
        self.clamp_max = p.clamp_max;
        self.falloff_enabled = p.falloff_enabled;
        self.falloff_inner = p.falloff_inner;
        self.falloff_outer = p.falloff_outer;
        self.falloff_shape = p.falloff_shape;
        self.falloff_edge_noise = p.falloff_edge_noise;
        self.falloff_noise_freq = p.falloff_noise_freq;
        self.falloff_exponent = p.falloff_exponent;
        self.erosion_mask_enabled = p.erosion_mask_enabled;
        self.erosion_mask_type = p.erosion_mask_type;
        self.erosion_mask_min = p.erosion_mask_min;
        self.erosion_mask_max = p.erosion_mask_max;
        self.erosion_enabled = p.erosion_enabled;
        self.erosion_droplets = p.erosion_droplets;
        self.erosion_inertia = p.erosion_inertia;
        self.erosion_capacity = p.erosion_capacity;
        self.erosion_deposition = p.erosion_deposition;
        self.erosion_erosion_speed = p.erosion_erosion_speed;
        self.erosion_evaporation = p.erosion_evaporation;
        self.erosion_radius = p.erosion_radius;
        self.thermal_enabled = p.thermal_enabled;
        self.thermal_talus = p.thermal_talus;
        self.thermal_iterations = p.thermal_iterations;
        self.thermal_strength = p.thermal_strength;
        self.blur_enabled = p.blur_enabled;
        self.blur_sigma = p.blur_sigma;
        self.percentile_enabled = p.percentile_enabled;
        self.percentile_low = p.percentile_low;
        self.percentile_high = p.percentile_high;
        self.color_mode = p.color_mode;
        self.normal_strength = p.normal_strength;
        self.dirty = true;
        Ok(())
    }

    // ── Batch chunk export ───────────────────────────────────────────────────
    pub fn export_chunks_batch(&mut self, dir: PathBuf, stem: String) -> Result<usize, String> {
        let orig_x = self.chunk_x;
        let orig_y = self.chunk_y;
        let orig_mode = self.chunk_mode;
        self.chunk_mode = true;
        let mut count = 0usize;
        let x_range = self.batch_x_min..=self.batch_x_max;
        let y_range = self.batch_y_min..=self.batch_y_max;

        for cy in y_range {
            for cx in x_range.clone() {
                self.chunk_x = cx;
                self.chunk_y = cy;
                let path = dir.join(format!("{stem}_cx{cx}_cy{cy}.png"));
                self.export_png(path)?;
                count += 1;
            }
        }

        self.chunk_x = orig_x;
        self.chunk_y = orig_y;
        self.chunk_mode = orig_mode;
        Ok(count)
    }

    // ── EXR export (32-bit float) ────────────────────────────────────────────
    pub fn export_exr(&mut self, path: PathBuf) -> Result<(), String> {
        let res = self.export_resolution;
        let data = self.generate(res);
        // EXR requires Rgb<f32>; store the heightmap in all three channels
        // so engines that read R, G, or luminance all get the correct value.
        let img: ImageBuffer<Rgb<f32>, Vec<f32>> = ImageBuffer::from_fn(res, res, |x, y| {
            let v = data[(y * res + x) as usize];
            Rgb([v, v, v])
        });
        img.save(&path).map_err(|e| format!("Error EXR: {e}"))
    }

    // ── Slope map export ─────────────────────────────────────────────────────
    pub fn export_slope_png(&mut self, path: PathBuf) -> Result<(), String> {
        let res = self.export_resolution;
        let data = self.generate(res);
        let size = res as usize;
        let slope = self.compute_slope_map(&data, size);
        let img = GrayImage::from_fn(res, res, |x, y| {
            Luma([(slope[(y * res + x) as usize] * 255.0) as u8])
        });
        img.save(&path).map_err(|e| format!("Error slope: {e}"))
    }

    fn compute_slope_map(&self, data: &[f32], size: usize) -> Vec<f32> {
        let mut slope = vec![0.0f32; size * size];
        for y in 0..size {
            for x in 0..size {
                let get = |xi: i32, yi: i32| -> f32 {
                    data[yi.clamp(0, size as i32 - 1) as usize * size
                        + xi.clamp(0, size as i32 - 1) as usize]
                };
                let xi = x as i32;
                let yi = y as i32;
                // Central differences, scaled so pixel spacing = 1/size
                let gx = (get(xi + 1, yi) - get(xi - 1, yi)) * 0.5 * size as f32;
                let gy = (get(xi, yi + 1) - get(xi, yi - 1)) * 0.5 * size as f32;
                slope[y * size + x] = (gx * gx + gy * gy).sqrt();
            }
        }
        let max = slope.iter().cloned().fold(0.0_f32, f32::max).max(1e-10);
        slope.iter_mut().for_each(|v| *v /= max);
        slope
    }

    // ── Wetness map export ───────────────────────────────────────────────────
    pub fn export_wetness_png(&mut self, path: PathBuf) -> Result<(), String> {
        let res = self.export_resolution;
        // Generate full heightmap (including erosion if enabled) — droplets
        // follow already-carved channels for a more accurate wetness map.
        let data = self.generate(res);
        let size = res as usize;
        let wetness = self.erode_wetness_only(&data, size);
        let img = GrayImage::from_fn(res, res, |x, y| {
            Luma([(wetness[(y * res + x) as usize] * 255.0) as u8])
        });
        img.save(&path).map_err(|e| format!("Error wetness: {e}"))
    }

    /// Runs the droplet simulation in read-only mode and accumulates per-pixel
    /// water flow. Returns a normalized wetness buffer in [0, 1].
    fn erode_wetness_only(&self, data: &[f32], size: usize) -> Vec<f32> {
        let n = size;
        let inertia = self.erosion_inertia;
        let evaporate = self.erosion_evaporation;
        let gravity = 10.0_f32;
        let max_steps = 64_usize;

        let mut wetness = vec![0.0f32; n * n];
        let mut rng = rand::rngs::SmallRng::seed_from_u64(self.seed as u64);

        for _ in 0..self.erosion_droplets {
            let mut x = rng.gen::<f32>() * (n - 2) as f32 + 0.5;
            let mut y = rng.gen::<f32>() * (n - 2) as f32 + 0.5;
            let mut dir_x = 0.0f32;
            let mut dir_y = 0.0f32;
            let mut speed = 1.0f32;
            let mut water = 1.0f32;
            let (mut cur_gx, mut cur_gy, mut h_old) =
                self.get_gradient_and_height(data, n, x, y);

            for _ in 0..max_steps {
                // Accumulate water at current position (bilinear)
                let xi = x as usize;
                let yi = y as usize;
                if xi < n && yi < n {
                    let u = x - xi as f32;
                    let v = y - yi as f32;
                    let xi1 = (xi + 1).min(n - 1);
                    let yi1 = (yi + 1).min(n - 1);
                    wetness[yi * n + xi] += water * (1.0 - u) * (1.0 - v);
                    wetness[yi * n + xi1] += water * u * (1.0 - v);
                    wetness[yi1 * n + xi] += water * (1.0 - u) * v;
                    wetness[yi1 * n + xi1] += water * u * v;
                }

                dir_x = dir_x * inertia - cur_gx * (1.0 - inertia);
                dir_y = dir_y * inertia - cur_gy * (1.0 - inertia);
                let len_sq = dir_x * dir_x + dir_y * dir_y;
                if len_sq > 0.0 {
                    let len = len_sq.sqrt();
                    dir_x /= len;
                    dir_y /= len;
                }

                let nx = x + dir_x;
                let ny = y + dir_y;
                if nx < 1.0 || nx >= (n - 2) as f32 || ny < 1.0 || ny >= (n - 2) as f32 {
                    break;
                }

                let (next_gx, next_gy, h_new) = self.get_gradient_and_height(data, n, nx, ny);
                let delta_h = h_new - h_old;
                speed = (speed * speed - delta_h * gravity).max(0.0).sqrt().max(0.01);
                water *= 1.0 - evaporate;
                x = nx;
                y = ny;
                cur_gx = next_gx;
                cur_gy = next_gy;
                h_old = h_new;

                if water < 0.01 {
                    break;
                }
            }
        }

        // Normalize with sqrt to compress the high-end and reveal fine channels
        let max = wetness.iter().cloned().fold(0.0_f32, f32::max).max(1e-10);
        wetness.iter_mut().for_each(|v| *v = (*v / max).sqrt());
        wetness
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
            let dx = (get(xi + 1, yi - 1) + 2.0 * get(xi + 1, yi) + get(xi + 1, yi + 1)
                - get(xi - 1, yi - 1)
                - 2.0 * get(xi - 1, yi)
                - get(xi - 1, yi + 1))
                / 8.0;
            let dy = (get(xi - 1, yi + 1) + 2.0 * get(xi, yi + 1) + get(xi + 1, yi + 1)
                - get(xi - 1, yi - 1)
                - 2.0 * get(xi, yi - 1)
                - get(xi + 1, yi - 1))
                / 8.0;

            let nx = -dx * strength;
            let ny = -dy * strength;
            let nz = 1.0_f64;
            let len = (nx * nx + ny * ny + nz * nz).sqrt();

            let r = ((nx / len + 1.0) * 0.5 * 255.0) as u8;
            let g = ((ny / len + 1.0) * 0.5 * 255.0) as u8;
            let b = ((nz / len + 1.0) * 0.5 * 255.0) as u8;
            Rgb([r, g, b])
        });
        img.save(&path).map_err(|e| format!("Error: {e}"))
    }
}
