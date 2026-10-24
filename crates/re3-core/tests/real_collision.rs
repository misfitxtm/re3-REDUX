//! Geometric self-consistency of the collision tests on every real collision model.
//! Runs when GTA3_DIR / VC_DIR are set; skipped otherwise.
use glam::Vec3;
use re3_core::{collision::*, math::*};
use re3_formats::{col::parse_col, datafile::*, dir::Archive};
use std::{env, fs, path::{Path, PathBuf}};

fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
        p = fs::read_dir(&p).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path();
    }
    Some(p)
}

fn load_models(root: &Path, dats: &[&str], archive: Option<&str>) -> Vec<ColModel> {
    let mut raw = vec![];
    for d in dats {
        let text = String::from_utf8_lossy(&fs::read(resolve(root, d).unwrap()).unwrap()).into_owned();
        for dir in parse_gta_dat(&text) {
            if let Directive::ColFile { path, .. } = dir {
                raw.extend(parse_col(&fs::read(resolve(root, &path).unwrap()).unwrap()).unwrap());
            }
        }
    }
    if let Some(a) = archive {
        let mut ar = Archive::open(&resolve(root, &format!("{a}.dir")).unwrap(), &resolve(root, &format!("{a}.img")).unwrap()).unwrap();
        for e in ar.dir.entries().to_vec() {
            if e.name.to_ascii_lowercase().ends_with(".col") {
                raw.extend(parse_col(&ar.read(&e).unwrap()).unwrap());
            }
        }
    }
    raw.iter().filter_map(|m| ColModel::from_file(m).ok()).collect()
}

fn near(a: Vec3, b: Vec3, tol: f32) -> bool {
    (a - b).length() <= tol
}

#[test]
fn collision_agrees_with_geometry() {
    for (var, game, dats, archive) in [
        ("GTA3_DIR", Game::Gta3, &["data/default.dat", "data/gta3.dat"][..], None),
        ("VC_DIR", Game::Vc, &["data/default.dat", "data/gta_vc.dat"][..], Some("models/gta3")),
    ] {
        let Ok(root) = env::var(var) else { continue };
        let models = load_models(Path::new(&root), dats, archive);
        let (mut tris, mut boxes, mut spheres, mut skipped) = (0, 0, 0, 0);
        for m in &models {
            for (t, plane) in m.triangles.iter().zip(&m.planes) {
                let (va, vb, vc) = (m.vertices[t.a as usize], m.vertices[t.b as usize], m.vertices[t.c as usize]);
                let area = cross(vb - va, vc - va).length() * 0.5;
                if area < 1e-3 || !plane.normal.is_finite() {
                    skipped += 1;
                    continue;
                }
                tris += 1;
                let c = (va + vb + vc) / 3.0;
                let n = plane.normal;
                // straight through the centroid
                let line = Line { p0: c + n * 0.5, p1: c - n * 0.5 };
                let (mut pt, mut d) = (ColPoint::default(), 1.0);
                assert!(test_line_triangle(&line, &m.vertices, t, plane), "{var}: centroid line missed");
                assert!(process_line_triangle(&line, &m.vertices, t, plane, &mut pt, &mut d, None));
                assert!((d - 0.5).abs() < 1e-3 && near(pt.point, c, 1e-2) && near(pt.normal, n, 1e-5), "{var}: wrong centroid hit");
                // well outside the triangle, in its plane
                let out = c + (c - va) * 3.0;
                assert!(!test_line_triangle(&Line { p0: out + n * 0.5, p1: out - n * 0.5 }, &m.vertices, t, plane), "{var}: line outside the triangle hit");
                // sphere hovering above the centroid
                let mut pt = ColPoint::default();
                let mut dsq = 1.0e24_f32;
                let s = Sphere::new(0.5, c + n * 0.25, 7, 0);
                assert!(test_sphere_triangle(&s, &m.vertices, t, plane));
                assert!(process_sphere_triangle(&s, &m.vertices, t, plane, &mut pt, &mut dsq), "{var}: sphere above triangle missed");
                assert!((pt.depth - 0.25).abs() < 1e-3 && near(pt.point, c, 1e-2) && near(pt.normal, n, 1e-4), "{var}: wrong sphere depth");
                assert!(!test_sphere_triangle(&Sphere::new(0.5, c + n * 1.0, 7, 0), &m.vertices, t, plane));
            }
            for b in &m.boxes {
                let size = b.max - b.min;
                if size.min_element() < 0.05 {
                    skipped += 1;
                    continue;
                }
                boxes += 1;
                let mid = (b.min + b.max) * 0.5;
                let mut pt = ColPoint::default();
                let mut dsq = 1.0e24_f32;
                assert!(process_sphere_box(&Sphere::new(0.5, Vec3::new(b.max.x + 0.2, mid.y, mid.z), 0, 0), b, &mut pt, &mut dsq));
                assert!((pt.depth - 0.3).abs() < 1e-4 && pt.normal == Vec3::X && (pt.point.x - b.max.x).abs() < 1e-6, "{var}: sphere/box");
                let line = Line { p0: Vec3::new(b.max.x + 5.0, mid.y, mid.z), p1: Vec3::new(b.min.x - 5.0, mid.y, mid.z) };
                let (mut pt, mut d) = (ColPoint::default(), 1.0);
                assert!(test_line_box(&line, b) && process_line_box(&line, b, &mut pt, &mut d));
                assert!(pt.normal == Vec3::X && (pt.point.x - b.max.x).abs() < 1e-5 && (d - 5.0 / (size.x + 10.0)).abs() < 1e-5, "{var}: line/box");
            }
            for sp in &m.spheres {
                if sp.radius < 0.05 {
                    skipped += 1;
                    continue;
                }
                spheres += 1;
                let line = Line { p0: sp.center + Vec3::X * (sp.radius + 3.0), p1: sp.center - Vec3::X * (sp.radius + 3.0) };
                let (mut pt, mut d) = (ColPoint::default(), 1.0);
                assert!(process_line_sphere(&line, sp, &mut pt, &mut d), "{var}: line/sphere");
                assert!(near(pt.point, sp.center + Vec3::X * sp.radius, 1e-3) && near(pt.normal, Vec3::X, 1e-4));
                assert!(test_line_sphere(&line, sp));
            }
            // a model always collides with itself where its spheres sit
            if !m.spheres.is_empty() && m.spheres.len() <= MAX_SPHERES && m.boxes.len() <= MAX_BOXES && m.triangles.len() <= MAX_TRIS && m.lines.len() <= MAX_LINES && m.spheres.iter().all(|s| s.radius > 0.01) {
                let mut pts = vec![ColPoint::default(); m.spheres.len()];
                let n = process_col_models(&Matrix::IDENTITY, m, &Matrix::IDENTITY, m, &mut pts, &mut vec![ColPoint::default(); m.lines.len()], &mut vec![1.0; m.lines.len()]);
                assert!(n > 0, "{var}: model does not collide with itself");
            }
            // world-space queries through a rotated and moved model
            if !m.triangles.is_empty() && !m.planes.is_empty() {
                let mut mat = Matrix::from_euler(0.4, -0.3, 1.1);
                mat.pos = Vec3::new(100.0, -20.0, 7.0);
                let t = &m.triangles[0];
                let c = (m.vertices[t.a as usize] + m.vertices[t.b as usize] + m.vertices[t.c as usize]) / 3.0;
                let n = m.planes[0].normal;
                if n.is_finite() {
                    let wl = Line { p0: mat.transform_point(c + n * 0.5), p1: mat.transform_point(c - n * 0.5) };
                    let (mut pt, mut d) = (ColPoint::default(), 1.0);
                    if process_line_of_sight(game, &wl, &mat, m, &mut pt, &mut d, LineFilter::default()) {
                        assert!(d <= 0.5 + 1e-3, "{var}: line of sight found a hit farther than the known one");
                    }
                }
            }
        }
        println!("{var}: {} models, {tris} triangles, {boxes} boxes, {spheres} spheres checked, {skipped} degenerate skipped", models.len());
        assert!(tris > 1000 && boxes > 100 && spheres > 100);
    }
}
