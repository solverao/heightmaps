use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use rand::Rng;
use std::path::PathBuf;

use crate::app::HeightmapApp;
use crate::types::{
    BlendMode, ColorMode, ErosionMaskType, FalloffShape, FractalType, NoiseType, PostProcess,
};
use crate::view3d;

const HIST_BINS: usize = 64;
const HIST_W: f32 = 200.0;
const HIST_H: f32 = 80.0;
const HIST_PAD: f32 = 10.0;

fn draw_histogram(data: &[f32], color_mode: ColorMode, painter: &egui::Painter, rect: Rect) {
    if data.is_empty() {
        return;
    }

    // Build bins
    let mut bins = [0u32; HIST_BINS];
    for &v in data {
        let b = ((v * HIST_BINS as f32) as usize).min(HIST_BINS - 1);
        bins[b] += 1;
    }
    let max_count = *bins.iter().max().unwrap_or(&1).max(&1) as f32;

    // Position: bottom-right of the rect
    let x0 = rect.right() - HIST_W - HIST_PAD;
    let y0 = rect.bottom() - HIST_H - HIST_PAD;
    let bg = Rect::from_min_size(
        Pos2::new(x0 - 4.0, y0 - 4.0),
        Vec2::new(HIST_W + 8.0, HIST_H + 8.0),
    );

    painter.rect_filled(bg, 4.0, Color32::from_black_alpha(160));
    painter.rect_stroke(
        bg,
        4.0,
        Stroke::new(1.0, Color32::from_white_alpha(30)),
        egui::StrokeKind::Middle,
    );

    let bin_w = HIST_W / HIST_BINS as f32;
    for (i, &count) in bins.iter().enumerate() {
        let t = i as f32 / (HIST_BINS - 1) as f32;
        let bar_h = (count as f32 / max_count) * HIST_H;
        let bx = x0 + i as f32 * bin_w;
        let by = y0 + HIST_H - bar_h;

        let base = color_mode.sample(t);
        let col = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 210);
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(bx, by), Vec2::new(bin_w.max(1.0), bar_h)),
            0.0,
            col,
        );
    }

    // Axis labels
    let font = egui::FontId::proportional(9.0);
    let label_color = Color32::from_white_alpha(140);
    painter.text(
        Pos2::new(x0, y0 - 1.0),
        egui::Align2::LEFT_BOTTOM,
        "0",
        font.clone(),
        label_color,
    );
    painter.text(
        Pos2::new(x0 + HIST_W, y0 - 1.0),
        egui::Align2::RIGHT_BOTTOM,
        "1",
        font.clone(),
        label_color,
    );
    painter.text(
        Pos2::new(x0 - 3.0, y0),
        egui::Align2::RIGHT_TOP,
        "max",
        font.clone(),
        label_color,
    );
}

impl eframe::App for HeightmapApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Left panel: controls
        egui::SidePanel::left("controls")
            .min_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("⛰ Heightmap Generator");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("🎲 Randomize")
                                .on_hover_text("Aleatoriza todos los parámetros de generación")
                                .clicked()
                            {
                                self.randomize();
                            }
                        });
                    });
                    ui.separator();

                    // ── Vista ──
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!self.view_3d, "2D").clicked() {
                            self.view_3d = false;
                        }
                        if ui.selectable_label(self.view_3d, "3D").clicked() {
                            self.view_3d = true;
                        }
                    });

                    if self.view_3d {
                        ui.add_space(4.0);
                        ui.label("Rotación");
                        ui.add(egui::Slider::new(&mut self.view_rot, 0.0..=360.0).suffix("°"));
                        ui.label("Escala vertical");
                        ui.add(
                            egui::Slider::new(&mut self.elevation_scale, 0.05..=3.0)
                                .logarithmic(true),
                        );
                        ui.label("Resolución 3D");
                        if ui
                            .add(egui::Slider::new(&mut self.view3d_res, 16..=128).suffix("px"))
                            .changed()
                        {
                            self.view3d_dirty = true;
                        }
                    }

                    ui.separator();

                    // ── Noise ──
                    ui.label("Noise algorithm");
                    egui::ComboBox::from_id_salt("noise_type")
                        .selected_text(self.noise_type.label())
                        .show_ui(ui, |ui| {
                            for &nt in NoiseType::ALL {
                                if ui
                                    .selectable_value(&mut self.noise_type, nt, nt.label())
                                    .changed()
                                {
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
                                if ui
                                    .selectable_value(&mut self.fractal_type, ft, ft.label())
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                            }
                        });

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Domain warp + Seamless ──
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut self.warp_enabled, "Domain warp").changed() {
                            self.dirty = true;
                        }
                        ui.add_space(8.0);
                        if ui
                            .checkbox(&mut self.seamless_enabled, "Seamless")
                            .changed()
                        {
                            self.dirty = true;
                        }
                    });
                    if self.warp_enabled {
                        ui.label("Strength");
                        if ui
                            .add(egui::Slider::new(&mut self.warp_strength, 0.0..=2.0))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Warp frequency");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.warp_frequency, 0.1..=10.0)
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.add_space(4.0);
                        if ui
                            .checkbox(&mut self.warp2_enabled, "2ª pasada (warp-of-warp)")
                            .changed()
                        {
                            self.dirty = true;
                        }
                        if self.warp2_enabled {
                            ui.label("Strength 2");
                            if ui
                                .add(egui::Slider::new(&mut self.warp2_strength, 0.0..=2.0))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Frequency 2");
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.warp2_frequency, 0.1..=10.0)
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Parameters ──
                    ui.label("Seed");
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::DragValue::new(&mut self.seed).speed(1))
                            .changed()
                        {
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
                        if ui
                            .add(egui::Slider::new(&mut self.octaves, 1..=12))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Frequency");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.frequency, 0.1..=20.0)
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Lacunarity");
                        if ui
                            .add(egui::Slider::new(&mut self.lacunarity, 1.0..=4.0))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Persistence");
                        if ui
                            .add(egui::Slider::new(&mut self.persistence, 0.0..=1.0))
                            .changed()
                        {
                            self.dirty = true;
                        }
                    } else {
                        ui.add_space(4.0);
                        ui.label("Frequency");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.frequency, 0.1..=20.0)
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut self.chunk_mode, "Chunk mode").changed() {
                            self.dirty = true;
                        }
                    });

                    if self.chunk_mode {
                        // ── Chunk navigation ──
                        ui.add_space(4.0);
                        ui.label("Tamaño de chunk");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.chunk_size, 0.25..=4.0)
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }

                        ui.add_space(4.0);
                        // Row: ↑
                        ui.vertical_centered(|ui| {
                            if ui.button("  ↑  ").clicked() {
                                self.chunk_y -= 1;
                                self.dirty = true;
                            }
                        });
                        // Row: ← coord →
                        ui.horizontal(|ui| {
                            if ui.button(" ← ").clicked() {
                                self.chunk_x -= 1;
                                self.dirty = true;
                            }
                            ui.label(format!("X {:>3}  Y {:>3}", self.chunk_x, self.chunk_y));
                            if ui.button(" → ").clicked() {
                                self.chunk_x += 1;
                                self.dirty = true;
                            }
                        });
                        // Row: ↓
                        ui.vertical_centered(|ui| {
                            if ui.button("  ↓  ").clicked() {
                                self.chunk_y += 1;
                                self.dirty = true;
                            }
                        });

                        ui.add_space(4.0);
                        if ui.small_button("Reset (0, 0)").clicked() {
                            self.chunk_x = 0;
                            self.chunk_y = 0;
                            self.dirty = true;
                        }

                        let (ox, oy) = self.effective_offset();
                        ui.label(format!("Offset: X={ox:.2}  Y={oy:.2}"));

                        ui.add_space(6.0);
                        ui.separator();
                        ui.label("Exportación por lotes");
                        ui.horizontal(|ui| {
                            ui.label("X:");
                            ui.add(egui::DragValue::new(&mut self.batch_x_min).prefix("de ").speed(1));
                            ui.add(egui::DragValue::new(&mut self.batch_x_max).prefix("a ").speed(1));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Y:");
                            ui.add(egui::DragValue::new(&mut self.batch_y_min).prefix("de ").speed(1));
                            ui.add(egui::DragValue::new(&mut self.batch_y_max).prefix("a ").speed(1));
                        });
                        let nx = (self.batch_x_max - self.batch_x_min + 1).max(0) as usize;
                        let ny = (self.batch_y_max - self.batch_y_min + 1).max(0) as usize;
                        ui.weak(format!("{} chunks ({nx}×{ny})", nx * ny));
                        if ui.button("📦 Exportar lote").clicked() {
                            let base = PathBuf::from(&self.export_path);
                            let stem = base
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                            let dir = base
                                .parent()
                                .unwrap_or(std::path::Path::new("."))
                                .to_path_buf();
                            self.batch_status = Some(match self.export_chunks_batch(dir, stem) {
                                Ok(n) => (format!("✔ {n} chunks exportados"), true),
                                Err(e) => (format!("✖ Error: {e}"), false),
                            });
                        }
                        if let Some((msg, ok)) = &self.batch_status {
                            let color = if *ok {
                                egui::Color32::from_rgb(100, 220, 100)
                            } else {
                                egui::Color32::from_rgb(240, 80, 80)
                            };
                            ui.colored_label(color, msg.as_str());
                        }
                    } else {
                        // ── Manual offset ──
                        ui.add_space(4.0);
                        ui.label("Offset X / Y");
                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.offset_x)
                                        .speed(0.01)
                                        .prefix("X: "),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                            if ui
                                .add(
                                    egui::DragValue::new(&mut self.offset_y)
                                        .speed(0.01)
                                        .prefix("Y: "),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                        });
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Extra layers ──
                    for i in 0..2usize {
                        egui::CollapsingHeader::new(format!("Capa {}", i + 2))
                            .id_salt(i)
                            .show(ui, |ui| {
                                if ui.checkbox(&mut self.layers[i].enabled, "Activa").changed() {
                                    self.dirty = true;
                                }
                                if !self.layers[i].enabled {
                                    return;
                                }

                                ui.label("Noise");
                                egui::ComboBox::from_id_salt(("l_noise", i))
                                    .selected_text(self.layers[i].noise_type.label())
                                    .show_ui(ui, |ui| {
                                        for &nt in NoiseType::ALL {
                                            if ui
                                                .selectable_value(
                                                    &mut self.layers[i].noise_type,
                                                    nt,
                                                    nt.label(),
                                                )
                                                .changed()
                                            {
                                                self.dirty = true;
                                            }
                                        }
                                    });

                                ui.label("Fractal");
                                egui::ComboBox::from_id_salt(("l_fractal", i))
                                    .selected_text(self.layers[i].fractal_type.label())
                                    .show_ui(ui, |ui| {
                                        for &ft in FractalType::ALL {
                                            if ui
                                                .selectable_value(
                                                    &mut self.layers[i].fractal_type,
                                                    ft,
                                                    ft.label(),
                                                )
                                                .changed()
                                            {
                                                self.dirty = true;
                                            }
                                        }
                                    });

                                ui.label("Blend");
                                egui::ComboBox::from_id_salt(("l_blend", i))
                                    .selected_text(self.layers[i].blend_mode.label())
                                    .show_ui(ui, |ui| {
                                        for &bm in BlendMode::ALL {
                                            if ui
                                                .selectable_value(
                                                    &mut self.layers[i].blend_mode,
                                                    bm,
                                                    bm.label(),
                                                )
                                                .changed()
                                            {
                                                self.dirty = true;
                                            }
                                        }
                                    });

                                ui.label("Weight");
                                if ui
                                    .add(egui::Slider::new(&mut self.layers[i].weight, 0.0..=1.0))
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                                ui.label("Freq scale");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.layers[i].frequency_scale,
                                            0.1..=8.0,
                                        )
                                        .logarithmic(true),
                                    )
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                                ui.label("Seed offset");
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.layers[i].seed_offset)
                                            .speed(1),
                                    )
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                            });
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Hydraulic erosion ──
                    egui::CollapsingHeader::new("Erosión hidráulica")
                        .id_salt("erosion")
                        .show(ui, |ui| {
                            if ui.checkbox(&mut self.erosion_enabled, "Activa").changed() {
                                self.dirty = true;
                            }
                            if !self.erosion_enabled {
                                return;
                            }

                            ui.add_space(4.0);
                            ui.separator();
                            if ui
                                .checkbox(&mut self.erosion_mask_enabled, "Máscara de erosión")
                                .changed()
                            {
                                self.dirty = true;
                            }
                            if self.erosion_mask_enabled {
                                ui.label("Tipo de máscara");
                                egui::ComboBox::from_id_salt("erosion_mask_type")
                                    .selected_text(self.erosion_mask_type.label())
                                    .show_ui(ui, |ui| {
                                        for &mt in ErosionMaskType::ALL {
                                            if ui
                                                .selectable_value(
                                                    &mut self.erosion_mask_type,
                                                    mt,
                                                    mt.label(),
                                                )
                                                .changed()
                                            {
                                                self.dirty = true;
                                            }
                                        }
                                    });
                                let label = match self.erosion_mask_type {
                                    ErosionMaskType::Height => ("Altura mín", "Altura máx"),
                                    ErosionMaskType::Slope => ("Pendiente mín", "Pendiente máx"),
                                };
                                ui.label(label.0);
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.erosion_mask_min, 0.0..=1.0),
                                    )
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                                ui.label(label.1);
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.erosion_mask_max, 0.0..=1.0),
                                    )
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                                ui.weak("Solo erode dentro del rango. Aplica a erosión hidráulica y térmica.");
                            }
                            ui.separator();
                            ui.add_space(2.0);
                            ui.label("Gotas");
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.erosion_droplets, 1_000..=150_000)
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Inercia  (0 = gira rápido, 1 = recto)");
                            if ui
                                .add(egui::Slider::new(&mut self.erosion_inertia, 0.0..=0.99))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Capacidad de sedimento");
                            if ui
                                .add(egui::Slider::new(&mut self.erosion_capacity, 1.0..=20.0))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Deposición");
                            if ui
                                .add(egui::Slider::new(&mut self.erosion_deposition, 0.01..=1.0))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Velocidad de erosión");
                            if ui
                                .add(egui::Slider::new(
                                    &mut self.erosion_erosion_speed,
                                    0.01..=1.0,
                                ))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Evaporación");
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.erosion_evaporation, 0.001..=0.1)
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Radio de erosión");
                            if ui
                                .add(egui::Slider::new(&mut self.erosion_radius, 0..=6))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.add_space(2.0);
                            ui.weak(format!("~{} M iteraciones", self.erosion_droplets / 1000));
                        });

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Thermal erosion ──
                    egui::CollapsingHeader::new("Erosión térmica")
                        .id_salt("thermal")
                        .show(ui, |ui| {
                            if ui.checkbox(&mut self.thermal_enabled, "Activa").changed() {
                                self.dirty = true;
                            }
                            if !self.thermal_enabled {
                                return;
                            }
                            ui.add_space(2.0);
                            ui.label("Talud (ángulo de reposo)");
                            if ui
                                .add(egui::Slider::new(&mut self.thermal_talus, 0.01..=0.5))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Iteraciones");
                            if ui
                                .add(egui::Slider::new(&mut self.thermal_iterations, 1..=100))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.label("Fuerza");
                            if ui
                                .add(egui::Slider::new(&mut self.thermal_strength, 0.01..=1.0))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.add_space(2.0);
                            ui.weak("Derrumba pendientes pronunciadas → acantilados suaves");
                        });

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Gaussian blur ──
                    ui.horizontal(|ui| {
                        if ui
                            .checkbox(&mut self.blur_enabled, "Gaussian blur")
                            .changed()
                        {
                            self.dirty = true;
                        }
                    });
                    if self.blur_enabled {
                        ui.label("Sigma");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.blur_sigma, 0.3..=10.0)
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                        let radius = (self.blur_sigma * 3.0).ceil() as u32;
                        ui.weak(format!("Kernel {}×{}", radius * 2 + 1, radius * 2 + 1));
                    }

                    ui.add_space(4.0);

                    // ── Percentile normalize ──
                    ui.horizontal(|ui| {
                        if ui
                            .checkbox(&mut self.percentile_enabled, "Normalizar por percentil")
                            .changed()
                        {
                            self.dirty = true;
                        }
                    });
                    if self.percentile_enabled {
                        ui.label("Percentil bajo (recorte oscuro)");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.percentile_low, 0.0..=49.0).suffix("%"),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Percentil alto (recorte claro)");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.percentile_high, 51.0..=100.0)
                                    .suffix("%"),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Post-process ──
                    ui.label("Post-process");
                    egui::ComboBox::from_id_salt("post_process")
                        .selected_text(self.post_process.label())
                        .show_ui(ui, |ui| {
                            for &pp in PostProcess::ALL {
                                if ui
                                    .selectable_value(&mut self.post_process, pp, pp.label())
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                            }
                        });

                    match self.post_process {
                        PostProcess::Terrace => {
                            ui.label("Levels");
                            if ui
                                .add(egui::Slider::new(&mut self.terrace_levels, 2..=32))
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                        PostProcess::Power => {
                            ui.label("Exponent");
                            if ui
                                .add(egui::Slider::new(&mut self.power_exp, 0.1..=5.0))
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                        PostProcess::Clamp => {
                            ui.label("Min / Max");
                            if ui
                                .add(egui::Slider::new(&mut self.clamp_min, 0.0..=1.0).text("min"))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            if ui
                                .add(egui::Slider::new(&mut self.clamp_max, 0.0..=1.0).text("max"))
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                        _ => {}
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Falloff map ──
                    if ui
                        .checkbox(&mut self.falloff_enabled, "Falloff map (isla)")
                        .changed()
                    {
                        self.dirty = true;
                    }
                    if self.falloff_enabled {
                        ui.label("Forma");
                        egui::ComboBox::from_id_salt("falloff_shape")
                            .selected_text(self.falloff_shape.label())
                            .show_ui(ui, |ui| {
                                for &s in FalloffShape::ALL {
                                    if ui
                                        .selectable_value(&mut self.falloff_shape, s, s.label())
                                        .changed()
                                    {
                                        self.dirty = true;
                                    }
                                }
                            });
                        ui.label("Radio interior (plano)");
                        if ui
                            .add(egui::Slider::new(&mut self.falloff_inner, 0.0..=1.0))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Radio exterior (borde)");
                        if ui
                            .add(egui::Slider::new(&mut self.falloff_outer, 0.0..=1.0))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Irregularidad de orilla");
                        if ui
                            .add(egui::Slider::new(&mut self.falloff_edge_noise, 0.0..=0.5))
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Frecuencia de orilla");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.falloff_noise_freq, 0.5..=12.0)
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                        ui.label("Curva (suave ↔ pronunciado)");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.falloff_exponent, 0.2..=4.0)
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            self.dirty = true;
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Preview settings ──
                    ui.label("Preview color");
                    egui::ComboBox::from_id_salt("color_mode")
                        .selected_text(self.color_mode.label())
                        .show_ui(ui, |ui| {
                            for &cm in ColorMode::ALL {
                                if ui
                                    .selectable_value(&mut self.color_mode, cm, cm.label())
                                    .changed()
                                {
                                    self.dirty = true;
                                }
                            }
                        });

                    ui.add_space(4.0);
                    ui.label("Preview resolution");
                    if ui
                        .add(egui::Slider::new(&mut self.resolution, 64..=512).suffix("px"))
                        .changed()
                    {
                        self.dirty = true;
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Export ──
                    ui.label("Export resolution");
                    ui.add(
                        egui::Slider::new(&mut self.export_resolution, 256..=4096)
                            .suffix("px")
                            .logarithmic(true),
                    );

                    ui.add_space(4.0);
                    ui.label("Export path (base)");
                    ui.text_edit_singleline(&mut self.export_path);

                    ui.add_space(4.0);
                    ui.label("OBJ resolution");
                    ui.add(
                        egui::Slider::new(&mut self.export_obj_res, 32..=512)
                            .suffix("px")
                            .logarithmic(true),
                    );

                    ui.add_space(4.0);
                    ui.label("Normal map strength");
                    ui.add(egui::Slider::new(&mut self.normal_strength, 1.0..=32.0));

                    ui.add_space(4.0);
                    // Derive sibling paths from the base path
                    let base = PathBuf::from(&self.export_path);
                    let stem = base
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let dir = base
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf();

                    ui.horizontal(|ui| {
                        let path_obj = dir.join(format!("{stem}.obj"));
                        if ui
                            .button("📦 OBJ")
                            .on_hover_text(path_obj.display().to_string())
                            .clicked()
                        {
                            self.export_status = Some(match self.export_obj(path_obj.clone()) {
                                Ok(()) => (format!("✔ OBJ guardado: {}", path_obj.display()), true),
                                Err(e) => (format!("✖ Error: {e}"), false),
                            });
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui
                            .button("💾 8-bit")
                            .on_hover_text(&self.export_path)
                            .clicked()
                        {
                            self.export_status = Some(match self.export_png(base.clone()) {
                                Ok(()) => (format!("✔ 8-bit guardado: {}", self.export_path), true),
                                Err(e) => (format!("✖ Error: {e}"), false),
                            });
                        }
                        let path16 = dir.join(format!("{stem}_16.png"));
                        if ui
                            .button("💾 16-bit")
                            .on_hover_text(path16.display().to_string())
                            .clicked()
                        {
                            self.export_status = Some(match self.export_png16(path16.clone()) {
                                Ok(()) => (format!("✔ 16-bit guardado: {}", path16.display()), true),
                                Err(e) => (format!("✖ Error: {e}"), false),
                            });
                        }
                        let path_exr = dir.join(format!("{stem}.exr"));
                        if ui
                            .button("💾 EXR")
                            .on_hover_text(path_exr.display().to_string())
                            .clicked()
                        {
                            self.export_status = Some(match self.export_exr(path_exr.clone()) {
                                Ok(()) => (format!("✔ EXR guardado: {}", path_exr.display()), true),
                                Err(e) => (format!("✖ Error: {e}"), false),
                            });
                        }
                    });
                    ui.horizontal(|ui| {
                        let path_nm = dir.join(format!("{stem}_normal.png"));
                        if ui
                            .button("🗺 Normal map")
                            .on_hover_text(path_nm.display().to_string())
                            .clicked()
                        {
                            self.export_status =
                                Some(match self.export_normal_png(path_nm.clone()) {
                                    Ok(()) => (format!("✔ Normal guardado: {}", path_nm.display()), true),
                                    Err(e) => (format!("✖ Error: {e}"), false),
                                });
                        }
                        let path_slope = dir.join(format!("{stem}_slope.png"));
                        if ui
                            .button("📐 Slope map")
                            .on_hover_text(path_slope.display().to_string())
                            .clicked()
                        {
                            self.export_status =
                                Some(match self.export_slope_png(path_slope.clone()) {
                                    Ok(()) => (format!("✔ Slope guardado: {}", path_slope.display()), true),
                                    Err(e) => (format!("✖ Error: {e}"), false),
                                });
                        }
                        let path_wet = dir.join(format!("{stem}_wetness.png"));
                        if ui
                            .button("💧 Wetness map")
                            .on_hover_text(path_wet.display().to_string())
                            .clicked()
                        {
                            self.export_status =
                                Some(match self.export_wetness_png(path_wet.clone()) {
                                    Ok(()) => (format!("✔ Wetness guardado: {}", path_wet.display()), true),
                                    Err(e) => (format!("✖ Error: {e}"), false),
                                });
                        }
                    });

                    // ── Terrain3D ──
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label("Terrain3D (Godot)");
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label("Slope → rock");
                        ui.add(egui::Slider::new(&mut self.t3d_slope_thresh, 0.1..=0.9).step_by(0.05));
                    });
                    ui.horizontal(|ui| {
                        ui.label("High  → snow");
                        ui.add(egui::Slider::new(&mut self.t3d_high_thresh, 0.4..=0.99).step_by(0.05));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Low   → sand");
                        ui.add(egui::Slider::new(&mut self.t3d_low_thresh, 0.01..=0.5).step_by(0.05));
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let path_ctrl = dir.join(format!("{stem}_control.png"));
                        if ui
                            .button("🗺 Control map")
                            .on_hover_text(path_ctrl.display().to_string())
                            .clicked()
                        {
                            self.export_status =
                                Some(match self.export_terrain3d_control(path_ctrl.clone()) {
                                    Ok(()) => (format!("✔ Control guardado: {}", path_ctrl.display()), true),
                                    Err(e) => (format!("✖ Error: {e}"), false),
                                });
                        }
                        let path_color = dir.join(format!("{stem}_color.png"));
                        if ui
                            .button("🎨 Color map")
                            .on_hover_text(path_color.display().to_string())
                            .clicked()
                        {
                            self.export_status =
                                Some(match self.export_terrain3d_color(path_color.clone()) {
                                    Ok(()) => (format!("✔ Color guardado: {}", path_color.display()), true),
                                    Err(e) => (format!("✖ Error: {e}"), false),
                                });
                        }
                    });

                    if let Some((msg, ok)) = &self.export_status {
                        ui.add_space(4.0);
                        let color = if *ok {
                            egui::Color32::from_rgb(100, 220, 100)
                        } else {
                            egui::Color32::from_rgb(240, 80, 80)
                        };
                        ui.colored_label(color, msg.as_str());
                    }

                    ui.add_space(8.0);
                    ui.separator();

                    // ── Preset save / load ──
                    ui.label("Preset path");
                    ui.text_edit_singleline(&mut self.preset_path);
                    ui.horizontal(|ui| {
                        if ui.button("💾 Guardar preset").clicked() {
                            let path = PathBuf::from(&self.preset_path);
                            self.preset_status = Some(match self.save_preset(path) {
                                Ok(()) => format!("Preset guardado: {}", self.preset_path),
                                Err(e) => e,
                            });
                        }
                        if ui.button("📂 Cargar preset").clicked() {
                            let path = PathBuf::from(&self.preset_path);
                            self.preset_status = Some(match self.load_preset(path) {
                                Ok(()) => format!("Preset cargado: {}", self.preset_path),
                                Err(e) => e,
                            });
                        }
                    });
                    if let Some(status) = &self.preset_status {
                        ui.weak(status.as_str());
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!("Gen time: {:.1} ms", self.last_gen_ms));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut self.histogram_visible, "Histograma");
                        });
                    });
                }); // ScrollArea
            });

        // Center: preview
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.dirty {
                self.rebuild_preview(ctx);
            }

            if self.view_3d {
                if self.view3d_dirty {
                    self.rebuild_3d();
                }
                let rect = ui.available_rect_before_wrap();
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));
                view3d::draw(
                    &self.view3d_data,
                    self.view3d_res as usize,
                    &painter,
                    rect,
                    self.view_rot,
                    self.elevation_scale,
                    self.color_mode,
                );
                if self.histogram_visible && !self.heightmap_data.is_empty() {
                    draw_histogram(&self.heightmap_data, self.color_mode, &painter, rect);
                }
                // Histogram toggle button (top-right)
                let btn_rect = Rect::from_min_size(
                    rect.right_top() + Vec2::new(-34.0, 8.0),
                    Vec2::new(26.0, 16.0),
                );
                let btn_col = if self.histogram_visible {
                    Color32::from_rgba_unmultiplied(80, 140, 220, 200)
                } else {
                    Color32::from_black_alpha(150)
                };
                painter.rect_filled(btn_rect, 3.0, btn_col);
                painter.text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "hist",
                    egui::FontId::proportional(9.0),
                    Color32::WHITE,
                );
                let btn_resp = ui.allocate_rect(btn_rect, egui::Sense::click());
                if btn_resp.clicked() {
                    self.histogram_visible = !self.histogram_visible;
                }

                ui.allocate_rect(rect, egui::Sense::hover());
            } else if let Some(tex) = &self.preview_texture {
                let rect = ui.available_rect_before_wrap();
                let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                let painter = ui.painter_at(rect);

                // ── Scroll to zoom (centered on cursor) ──────────────────
                let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
                if response.hovered() && scroll != 0.0 {
                    let factor = (scroll * 0.002).exp();
                    let cursor = ctx
                        .input(|i| i.pointer.hover_pos())
                        .unwrap_or(rect.center());
                    // Zoom toward cursor: adjust pan so the point under the
                    // cursor stays fixed.
                    let before = (cursor - rect.center() - self.pan) / self.zoom;
                    self.zoom = (self.zoom * factor).clamp(0.5, 20.0);
                    self.pan = cursor - rect.center() - before * self.zoom;
                }

                // ── Drag to pan ──────────────────────────────────────────
                if response.dragged_by(egui::PointerButton::Primary) {
                    self.pan += response.drag_delta();
                }

                // ── Double-click to reset ────────────────────────────────
                if response.double_clicked() {
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }

                // ── Draw image with zoom/pan transform ───────────────────
                let base_side = rect.width().min(rect.height());
                let side = base_side * self.zoom;
                let center = rect.center() + self.pan;
                let img_rect = Rect::from_center_size(center, Vec2::splat(side));
                painter.image(
                    tex.id(),
                    img_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );

                // ── Zoom label ───────────────────────────────────────────
                let zoom_label = format!("{:.0}%", self.zoom * 100.0);
                painter.text(
                    rect.left_bottom() + Vec2::new(8.0, -8.0),
                    egui::Align2::LEFT_BOTTOM,
                    &zoom_label,
                    egui::FontId::proportional(11.0),
                    Color32::from_white_alpha(160),
                );

                // ── Histogram overlay ────────────────────────────────────
                if self.histogram_visible && !self.heightmap_data.is_empty() {
                    draw_histogram(&self.heightmap_data, self.color_mode, &painter, rect);
                }

                // ── Histogram toggle button (top-right) ──────────────────
                let btn_rect = Rect::from_min_size(
                    rect.right_top() + Vec2::new(-34.0, 8.0),
                    Vec2::new(26.0, 16.0),
                );
                let btn_resp = ui.allocate_rect(btn_rect, egui::Sense::click());
                let btn_col = if self.histogram_visible {
                    Color32::from_rgba_unmultiplied(80, 140, 220, 200)
                } else {
                    Color32::from_black_alpha(150)
                };
                painter.rect_filled(btn_rect, 3.0, btn_col);
                painter.text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "hist",
                    egui::FontId::proportional(9.0),
                    Color32::WHITE,
                );
                if btn_resp.clicked() {
                    self.histogram_visible = !self.histogram_visible;
                }
            }
        });
    }
}
