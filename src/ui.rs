use egui::Vec2;
use rand::Rng;
use std::path::PathBuf;

use crate::app::HeightmapApp;
use crate::types::{BlendMode, ColorMode, FalloffShape, FractalType, NoiseType, PostProcess};
use crate::view3d;

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
                    ui.add(egui::Slider::new(&mut self.elevation_scale, 0.05..=3.0).logarithmic(true));
                    ui.label("Resolución 3D");
                    if ui.add(egui::Slider::new(&mut self.view3d_res, 16..=128).suffix("px")).changed() {
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
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.chunk_mode, "Chunk mode").changed() {
                        self.dirty = true;
                    }
                });

                if self.chunk_mode {
                    // ── Chunk navigation ──
                    ui.add_space(4.0);
                    ui.label("Tamaño de chunk");
                    if ui.add(egui::Slider::new(&mut self.chunk_size, 0.25..=4.0).logarithmic(true)).changed() {
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
                        if ui.button(" ← ").clicked() { self.chunk_x -= 1; self.dirty = true; }
                        ui.label(format!("X {:>3}  Y {:>3}", self.chunk_x, self.chunk_y));
                        if ui.button(" → ").clicked() { self.chunk_x += 1; self.dirty = true; }
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
                } else {
                    // ── Manual offset ──
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

                // ── Hydraulic erosion ──
                egui::CollapsingHeader::new("Erosión hidráulica")
                    .id_salt("erosion")
                    .show(ui, |ui| {
                        if ui.checkbox(&mut self.erosion_enabled, "Activa").changed() {
                            self.dirty = true;
                        }
                        if !self.erosion_enabled { return; }

                        ui.add_space(2.0);
                        ui.label("Gotas");
                        if ui.add(egui::Slider::new(&mut self.erosion_droplets, 1_000..=150_000).logarithmic(true)).changed() {
                            self.dirty = true;
                        }
                        ui.label("Inercia  (0 = gira rápido, 1 = recto)");
                        if ui.add(egui::Slider::new(&mut self.erosion_inertia, 0.0..=0.99)).changed() {
                            self.dirty = true;
                        }
                        ui.label("Capacidad de sedimento");
                        if ui.add(egui::Slider::new(&mut self.erosion_capacity, 1.0..=20.0)).changed() {
                            self.dirty = true;
                        }
                        ui.label("Deposición");
                        if ui.add(egui::Slider::new(&mut self.erosion_deposition, 0.01..=1.0)).changed() {
                            self.dirty = true;
                        }
                        ui.label("Velocidad de erosión");
                        if ui.add(egui::Slider::new(&mut self.erosion_erosion_speed, 0.01..=1.0)).changed() {
                            self.dirty = true;
                        }
                        ui.label("Evaporación");
                        if ui.add(egui::Slider::new(&mut self.erosion_evaporation, 0.001..=0.1).logarithmic(true)).changed() {
                            self.dirty = true;
                        }
                        ui.add_space(2.0);
                        ui.weak(format!("~{} M iteraciones", self.erosion_droplets / 1000));
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
                    ui.label("Irregularidad de orilla");
                    if ui.add(egui::Slider::new(&mut self.falloff_edge_noise, 0.0..=0.5)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Frecuencia de orilla");
                    if ui.add(egui::Slider::new(&mut self.falloff_noise_freq, 0.5..=12.0).logarithmic(true)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Curva (suave ↔ pronunciado)");
                    if ui.add(egui::Slider::new(&mut self.falloff_exponent, 0.2..=4.0).logarithmic(true)).changed() {
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

            if self.view_3d {
                if self.view3d_dirty {
                    self.rebuild_3d();
                }
                let rect = ui.available_rect_before_wrap();
                let painter = ui.painter_at(rect);
                // Dark background
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
                ui.allocate_rect(rect, egui::Sense::hover());
            } else if let Some(tex) = &self.preview_texture {
                let avail = ui.available_size();
                let side = avail.x.min(avail.y);
                ui.centered_and_justified(|ui| {
                    ui.image(egui::load::SizedTexture::new(tex.id(), Vec2::splat(side)));
                });
            }
        });
    }
}
