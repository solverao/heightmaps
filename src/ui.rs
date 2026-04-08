use egui::Vec2;
use rand::Rng;
use std::path::PathBuf;

use crate::app::HeightmapApp;
use crate::types::{BlendMode, ColorMode, FalloffShape, FractalType, NoiseType, PostProcess};

impl eframe::App for HeightmapApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Left panel: controls
        egui::SidePanel::left("controls")
            .min_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
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

                // ── Domain warp + Seamless ──
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.warp_enabled, "Domain warp").changed() {
                        self.dirty = true;
                    }
                    ui.add_space(8.0);
                    if ui.checkbox(&mut self.seamless_enabled, "Seamless").changed() {
                        self.dirty = true;
                    }
                });
                if self.warp_enabled {
                    ui.label("Strength");
                    if ui.add(egui::Slider::new(&mut self.warp_strength, 0.0..=2.0)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Warp frequency");
                    if ui.add(egui::Slider::new(&mut self.warp_frequency, 0.1..=10.0).logarithmic(true)).changed() {
                        self.dirty = true;
                    }
                }

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

                // ── Extra layers ──
                for i in 0..2usize {
                    egui::CollapsingHeader::new(format!("Capa {}", i + 2))
                        .id_salt(i)
                        .show(ui, |ui| {
                            if ui.checkbox(&mut self.layers[i].enabled, "Activa").changed() {
                                self.dirty = true;
                            }
                            if !self.layers[i].enabled { return; }

                            ui.label("Noise");
                            egui::ComboBox::from_id_salt(("l_noise", i))
                                .selected_text(self.layers[i].noise_type.label())
                                .show_ui(ui, |ui| {
                                    for &nt in NoiseType::ALL {
                                        if ui.selectable_value(&mut self.layers[i].noise_type, nt, nt.label()).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });

                            ui.label("Fractal");
                            egui::ComboBox::from_id_salt(("l_fractal", i))
                                .selected_text(self.layers[i].fractal_type.label())
                                .show_ui(ui, |ui| {
                                    for &ft in FractalType::ALL {
                                        if ui.selectable_value(&mut self.layers[i].fractal_type, ft, ft.label()).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });

                            ui.label("Blend");
                            egui::ComboBox::from_id_salt(("l_blend", i))
                                .selected_text(self.layers[i].blend_mode.label())
                                .show_ui(ui, |ui| {
                                    for &bm in BlendMode::ALL {
                                        if ui.selectable_value(&mut self.layers[i].blend_mode, bm, bm.label()).changed() {
                                            self.dirty = true;
                                        }
                                    }
                                });

                            ui.label("Weight");
                            if ui.add(egui::Slider::new(&mut self.layers[i].weight, 0.0..=1.0)).changed() {
                                self.dirty = true;
                            }
                            ui.label("Freq scale");
                            if ui.add(egui::Slider::new(&mut self.layers[i].frequency_scale, 0.1..=8.0).logarithmic(true)).changed() {
                                self.dirty = true;
                            }
                            ui.label("Seed offset");
                            if ui.add(egui::DragValue::new(&mut self.layers[i].seed_offset).speed(1)).changed() {
                                self.dirty = true;
                            }
                        });
                }

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

                // ── Falloff map ──
                if ui.checkbox(&mut self.falloff_enabled, "Falloff map (isla)").changed() {
                    self.dirty = true;
                }
                if self.falloff_enabled {
                    ui.label("Forma");
                    egui::ComboBox::from_id_salt("falloff_shape")
                        .selected_text(self.falloff_shape.label())
                        .show_ui(ui, |ui| {
                            for &s in FalloffShape::ALL {
                                if ui.selectable_value(&mut self.falloff_shape, s, s.label()).changed() {
                                    self.dirty = true;
                                }
                            }
                        });
                    ui.label("Radio interior (plano)");
                    if ui.add(egui::Slider::new(&mut self.falloff_inner, 0.0..=1.0)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Radio exterior (borde)");
                    if ui.add(egui::Slider::new(&mut self.falloff_outer, 0.0..=1.0)).changed() {
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
                ui.label("Export path (base)");
                ui.text_edit_singleline(&mut self.export_path);

                ui.add_space(4.0);
                ui.label("Normal map strength");
                ui.add(egui::Slider::new(&mut self.normal_strength, 1.0..=32.0));

                ui.add_space(4.0);
                // Derive sibling paths from the base path
                let base = PathBuf::from(&self.export_path);
                let stem = base.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                let dir  = base.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();

                ui.horizontal(|ui| {
                    if ui.button("💾 8-bit").on_hover_text(&self.export_path).clicked() {
                        self.export_status = Some(match self.export_png(base.clone()) {
                            Ok(()) => format!("Guardado 8-bit: {}", self.export_path),
                            Err(e) => e,
                        });
                    }
                    let path16 = dir.join(format!("{stem}_16.png"));
                    if ui.button("💾 16-bit").on_hover_text(path16.display().to_string()).clicked() {
                        self.export_status = Some(match self.export_png16(path16.clone()) {
                            Ok(()) => format!("Guardado 16-bit: {}", path16.display()),
                            Err(e) => e,
                        });
                    }
                    let path_nm = dir.join(format!("{stem}_normal.png"));
                    if ui.button("🗺 Normal map").on_hover_text(path_nm.display().to_string()).clicked() {
                        self.export_status = Some(match self.export_normal_png(path_nm.clone()) {
                            Ok(()) => format!("Guardado normal: {}", path_nm.display()),
                            Err(e) => e,
                        });
                    }
                });

                if let Some(status) = &self.export_status {
                    ui.add_space(4.0);
                    ui.label(status.as_str());
                }

                ui.add_space(8.0);
                ui.separator();
                ui.label(format!("Gen time: {:.1} ms", self.last_gen_ms));
                }); // ScrollArea
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
