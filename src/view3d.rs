use egui::{Color32, Mesh, Painter, Pos2, Rect};

use crate::types::ColorMode;

fn project(
    gx: f32,
    gy: f32,
    gz: f32,
    n_cells: f32,
    rot_sin: f32,
    rot_cos: f32,
    tilt_sin: f32,
    tilt_cos: f32,
    scale: f32,
    center: Pos2,
    elevation_scale: f32,
) -> Pos2 {
    let x = gx / n_cells - 0.5;
    let y = gy / n_cells - 0.5;
    let z = gz * elevation_scale * 0.35;

    let rx = x * rot_cos - y * rot_sin;
    let ry = x * rot_sin + y * rot_cos;

    Pos2::new(
        center.x + rx * scale,
        center.y + (ry * tilt_cos - z * tilt_sin) * scale,
    )
}

pub fn draw(
    data: &[f32],
    n: usize,
    painter: &Painter,
    rect: Rect,
    rot_deg: f32,
    elevation_scale: f32,
    color_mode: ColorMode,
) {
    if n < 2 || data.len() < n * n {
        return;
    }

    let n_cells = (n - 1) as f32;
    let center = rect.center();
    let scale = rect.width().min(rect.height()) * 0.65;
    let clip = rect.expand(rect.width().max(rect.height()) * 0.1);

    let rot = rot_deg.to_radians();
    let tilt = 38.0_f32.to_radians();
    let (rot_sin, rot_cos) = rot.sin_cos();
    let (tilt_sin, tilt_cos) = tilt.sin_cos();

    let proj = |gx: f32, gy: f32, gz: f32| -> Pos2 {
        project(
            gx,
            gy,
            gz,
            n_cells,
            rot_sin,
            rot_cos,
            tilt_sin,
            tilt_cos,
            scale,
            center,
            elevation_scale,
        )
    };

    // Back-to-front order by rotation quadrant
    let rot_norm = rot_deg.rem_euclid(360.0);
    let x_fwd = rot_norm < 90.0 || rot_norm >= 270.0;
    let y_fwd = rot_norm < 180.0;

    let cell_n = n - 1;
    let ys_vec: Vec<usize> = if y_fwd {
        (0..cell_n).collect()
    } else {
        (0..cell_n).rev().collect()
    };
    let xs_vec: Vec<usize> = if x_fwd {
        (0..cell_n).collect()
    } else {
        (0..cell_n).rev().collect()
    };

    let light = (0.5_f32, -0.35_f32, 1.0_f32);
    let l_len = (light.0 * light.0 + light.1 * light.1 + light.2 * light.2).sqrt();
    let inv_nc = 1.0 / n_cells;

    // Use a Mesh instead of convex_polygon to avoid egui's convexity assumptions
    let mut mesh = Mesh::default();

    for &gy in &ys_vec {
        for &gx in &xs_vec {
            let h00 = data[gy * n + gx];
            let h10 = data[gy * n + (gx + 1)];
            let h01 = data[(gy + 1) * n + gx];
            let h11 = data[(gy + 1) * n + (gx + 1)];

            let p00 = proj(gx as f32, gy as f32, h00);
            let p10 = proj(gx as f32 + 1.0, gy as f32, h10);
            let p11 = proj(gx as f32 + 1.0, gy as f32 + 1.0, h11);
            let p01 = proj(gx as f32, gy as f32 + 1.0, h01);

            // Skip quads with any vertex outside the clip zone
            if ![p00, p10, p11, p01].iter().all(|p| clip.contains(*p)) {
                continue;
            }

            let h_avg = (h00 + h10 + h01 + h11) * 0.25;

            // Lambert shading
            let dh_dx = ((h10 + h11) - (h00 + h01)) * 0.5;
            let dh_dy = ((h01 + h11) - (h00 + h10)) * 0.5;
            let sn = elevation_scale * inv_nc;
            let nx_n = -dh_dx * sn;
            let ny_n = -dh_dy * sn;
            let nz_n = inv_nc;
            let n_len = (nx_n * nx_n + ny_n * ny_n + nz_n * nz_n).sqrt().max(1e-6);
            let diff = (nx_n / n_len * light.0 / l_len
                + ny_n / n_len * light.1 / l_len
                + nz_n / n_len * light.2 / l_len)
                .clamp(-1.0, 1.0);
            let lum = 0.38 + 0.62 * diff.max(0.0);

            let base = color_mode.sample(h_avg);
            let color = Color32::from_rgb(
                (base.r() as f32 * lum).min(255.0) as u8,
                (base.g() as f32 * lum).min(255.0) as u8,
                (base.b() as f32 * lum).min(255.0) as u8,
            );

            // Two explicit triangles — no convexity requirement
            let i = mesh.vertices.len() as u32;
            mesh.colored_vertex(p00, color);
            mesh.colored_vertex(p10, color);
            mesh.colored_vertex(p11, color);
            mesh.colored_vertex(p01, color);
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i, i + 2, i + 3);
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}
