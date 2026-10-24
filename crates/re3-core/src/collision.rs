//! Collision shapes and the intersection tests (`Collision.cpp`, PC build: float vertices, no VU code).
//! Out-parameters (`point`, `mindist`) are kept as in the C++ so call sites port 1:1.
use crate::math::{cross, cross2d, dot, magnitude, magnitude_squared, normalise, Matrix};
use crate::surface;
use glam::{Vec2, Vec3};
use re3_formats::{col, datafile::Game};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
    pub surface: u8,
    pub piece: u8,
}

impl Sphere {
    pub fn new(radius: f32, center: Vec3, surface: u8, piece: u8) -> Self {
        Self { center, radius, surface, piece }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Box3 {
    pub min: Vec3,
    pub max: Vec3,
    pub surface: u8,
    pub piece: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Line {
    pub p0: Vec3,
    pub p1: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    XPos,
    XNeg,
    YPos,
    YNeg,
    ZPos,
    ZNeg,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub a: u16,
    pub b: u16,
    pub c: u16,
    pub surface: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrianglePlane {
    pub normal: Vec3,
    pub dist: f32,
    /// Largest component of the normal, used to pick the 2D projection.
    pub dir: Dir,
}

impl TrianglePlane {
    pub fn new(va: Vec3, vb: Vec3, vc: Vec3) -> Self {
        let normal = normalise(cross(vc - va, vb - va));
        let dist = dot(normal, va);
        let an = normal.abs();
        let dir = if an.x > an.y && an.x > an.z {
            if normal.x < 0.0 { Dir::XNeg } else { Dir::XPos }
        } else if an.y > an.z {
            if normal.y < 0.0 { Dir::YNeg } else { Dir::YPos }
        } else if normal.z < 0.0 {
            Dir::ZNeg
        } else {
            Dir::ZPos
        };
        Self { normal, dist, dir }
    }

    /// Signed distance from the plane.
    pub fn calc_point(&self, v: Vec3) -> f32 {
        dot(self.normal, v) - self.dist
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColPoint {
    pub point: Vec3,
    pub normal: Vec3,
    pub surface_a: u8,
    pub piece_a: u8,
    pub surface_b: u8,
    pub piece_b: u8,
    pub depth: f32,
}

/// A triangle remembered from a vertical-line test (for cars driving over it).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StoredCollPoly {
    pub verts: [Vec3; 3],
    pub valid: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColModel {
    pub bounding_sphere: Sphere,
    pub bounding_box: Box3,
    pub spheres: Vec<Sphere>,
    pub lines: Vec<Line>,
    pub boxes: Vec<Box3>,
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<Triangle>,
    /// One plane per triangle. The original builds these on demand in an LRU cache to save
    /// memory; they are deterministic, so they are simply kept here.
    pub planes: Vec<TrianglePlane>,
    pub level: i32,
}

impl ColModel {
    pub fn calculate_triangle_planes(&mut self) {
        let v = &self.vertices;
        self.planes = self.triangles.iter().map(|t| TrianglePlane::new(v[t.a as usize], v[t.b as usize], v[t.c as usize])).collect();
    }

    /// Converts a parsed `.col` model; fails if a face points past the vertex list.
    pub fn from_file(m: &col::ColModel) -> Result<Self, &'static str> {
        let vertices: Vec<Vec3> = m.vertices.iter().map(|v| Vec3::from(*v)).collect();
        let triangles = m
            .faces
            .iter()
            .map(|f| {
                let idx = |i: i32| u16::try_from(i).ok().filter(|&i| (i as usize) < vertices.len());
                Some(Triangle { a: idx(f.a)?, b: idx(f.b)?, c: idx(f.c)?, surface: f.surface })
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("collision face uses a vertex that does not exist")?;
        let mut model = Self {
            bounding_sphere: Sphere::new(m.bounding_radius, Vec3::from(m.bounding_centre), 0, 0),
            bounding_box: Box3 { min: Vec3::from(m.min), max: Vec3::from(m.max), surface: 0, piece: 0 },
            spheres: m.spheres.iter().map(|s| Sphere::new(s.radius, Vec3::from(s.centre), s.surface, s.piece)).collect(),
            lines: m.lines.iter().map(|l| Line { p0: Vec3::from(l.p0), p1: Vec3::from(l.p1) }).collect(),
            boxes: m.boxes.iter().map(|b| Box3 { min: Vec3::from(b.min), max: Vec3::from(b.max), surface: b.surface, piece: b.piece }).collect(),
            vertices,
            triangles,
            planes: vec![],
            level: 0,
        };
        model.calculate_triangle_planes();
        Ok(model)
    }
}

pub fn test_sphere_sphere(s1: &Sphere, s2: &Sphere) -> bool {
    let d = s1.radius + s2.radius;
    magnitude_squared(s1.center - s2.center) < d * d
}

pub fn test_sphere_box(sph: &Sphere, b: &Box3) -> bool {
    !(sph.center.x + sph.radius < b.min.x
        || sph.center.x - sph.radius > b.max.x
        || sph.center.y + sph.radius < b.min.y
        || sph.center.y - sph.radius > b.max.y
        || sph.center.z + sph.radius < b.min.z
        || sph.center.z - sph.radius > b.max.z)
}

fn inside_open(p: Vec3, b: &Box3) -> bool {
    p.x > b.min.x && p.x < b.max.x && p.y > b.min.y && p.y < b.max.y && p.z > b.min.z && p.z < b.max.z
}

/// The six slab crossings shared by `test_line_box` and `process_line_box`:
/// (t, hit point, outward normal) for every face the segment crosses inside the face's extent.
fn line_box_hits(line: &Line, b: &Box3) -> [Option<(f32, Vec3, Vec3)>; 6] {
    let (p0, p1) = (line.p0, line.p1);
    let mut out = [None; 6];
    // min x
    if (b.min.x - p1.x) * (b.min.x - p0.x) < 0.0 {
        let t = (b.min.x - p0.x) / (p1.x - p0.x);
        let y = p0.y + (p1.y - p0.y) * t;
        if y > b.min.y && y < b.max.y {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z {
                out[0] = Some((t, Vec3::new(b.min.x, y, z), Vec3::new(-1.0, 0.0, 0.0)));
            }
        }
    }
    // max x
    if (p1.x - b.max.x) * (p0.x - b.max.x) < 0.0 {
        let t = (p0.x - b.max.x) / (p0.x - p1.x);
        let y = p0.y + (p1.y - p0.y) * t;
        if y > b.min.y && y < b.max.y {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z {
                out[1] = Some((t, Vec3::new(b.max.x, y, z), Vec3::new(1.0, 0.0, 0.0)));
            }
        }
    }
    // min y
    if (b.min.y - p0.y) * (b.min.y - p1.y) < 0.0 {
        let t = (b.min.y - p0.y) / (p1.y - p0.y);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z {
                out[2] = Some((t, Vec3::new(x, b.min.y, z), Vec3::new(0.0, -1.0, 0.0)));
            }
        }
    }
    // max y
    if (p0.y - b.max.y) * (p1.y - b.max.y) < 0.0 {
        let t = (p0.y - b.max.y) / (p0.y - p1.y);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let z = p0.z + (p1.z - p0.z) * t;
            if z > b.min.z && z < b.max.z {
                out[3] = Some((t, Vec3::new(x, b.max.y, z), Vec3::new(0.0, 1.0, 0.0)));
            }
        }
    }
    // min z
    if (b.min.z - p0.z) * (b.min.z - p1.z) < 0.0 {
        let t = (b.min.z - p0.z) / (p1.z - p0.z);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let y = p0.y + (p1.y - p0.y) * t;
            if y > b.min.y && y < b.max.y {
                out[4] = Some((t, Vec3::new(x, y, b.min.z), Vec3::new(0.0, 0.0, -1.0)));
            }
        }
    }
    // max z
    if (p0.z - b.max.z) * (p1.z - b.max.z) < 0.0 {
        let t = (p0.z - b.max.z) / (p0.z - p1.z);
        let x = p0.x + (p1.x - p0.x) * t;
        if x > b.min.x && x < b.max.x {
            let y = p0.y + (p1.y - p0.y) * t;
            if y > b.min.y && y < b.max.y {
                out[5] = Some((t, Vec3::new(x, y, b.max.z), Vec3::new(0.0, 0.0, 1.0)));
            }
        }
    }
    out
}

pub fn test_line_box(line: &Line, b: &Box3) -> bool {
    inside_open(line.p0, b) || inside_open(line.p1, b) || line_box_hits(line, b).iter().any(Option::is_some)
}

pub fn test_vertical_line_box(line: &Line, b: &Box3) -> bool {
    if line.p0.x <= b.min.x || line.p0.y <= b.min.y || line.p0.x >= b.max.x || line.p0.y >= b.max.y {
        return false;
    }
    if line.p0.z < line.p1.z {
        !(line.p0.z > b.max.z || line.p1.z < b.min.z)
    } else {
        !(line.p1.z > b.max.z || line.p0.z < b.min.z)
    }
}

type Axis = fn(Vec3) -> f32;

/// Projects a triangle and a point onto the plane's dominant axes, ordered so the
/// winding test below is the same for every direction.
fn project(dir: Dir, va: Vec3, vb: Vec3, vc: Vec3, p: Vec3) -> [Vec2; 4] {
    let xy = |v: Vec3, a: Axis, b: Axis| Vec2::new(a(v), b(v));
    let (x, y, z): (Axis, Axis, Axis) = (|v| v.x, |v| v.y, |v| v.z);
    let (a, b) = match dir {
        Dir::XPos | Dir::XNeg => (y, z),
        Dir::YPos | Dir::YNeg => (z, x),
        Dir::ZPos | Dir::ZNeg => (x, y),
    };
    let flipped = matches!(dir, Dir::XNeg | Dir::YNeg | Dir::ZNeg);
    let (v2, v3) = if flipped { (vb, vc) } else { (vc, vb) };
    [xy(va, a, b), xy(v2, a, b), xy(v3, a, b), xy(p, a, b)]
}

/// The three-edge inside test used by every point-in-triangle check.
fn inside_projected([v1, v2, v3, vt]: [Vec2; 4]) -> bool {
    !(cross2d(v2 - v1, vt - v1) < 0.0 || cross2d(v3 - v1, vt - v1) > 0.0 || cross2d(v3 - v2, vt - v2) < 0.0)
}

pub fn test_line_triangle(line: &Line, verts: &[Vec3], tri: &Triangle, plane: &TrianglePlane) -> bool {
    // if points are on the same side, no collision
    if plane.calc_point(line.p0) * plane.calc_point(line.p1) > 0.0 {
        return false;
    }
    let p0dist = dot(line.p1 - line.p0, plane.normal);
    // FIX_BUGS: a line lying in the plane is no collision
    if p0dist == 0.0 {
        return false;
    }
    let t = -plane.calc_point(line.p0) / p0dist;
    let p = line.p0 + (line.p1 - line.p0) * t;
    inside_projected(project(plane.dir, verts[tri.a as usize], verts[tri.b as usize], verts[tri.c as usize], p))
}

/// Ported as-is, including its odd scaling. A first point inside the sphere registers no hit.
pub fn test_line_sphere(line: &Line, sph: &Sphere) -> bool {
    let v01 = line.p1 - line.p0;
    let v0c = sph.center - line.p0;
    let linesq = magnitude_squared(v01);
    let projline = -2.0 * dot(v01, v0c);
    let tansq = 4.0 * linesq * (magnitude_squared(sph.center) - 2.0 * dot(sph.center, line.p0) + magnitude_squared(line.p0) - sph.radius * sph.radius);
    let diffsq = projline * projline - tansq;
    if diffsq < 0.0 {
        return false;
    }
    let f = (-projline - diffsq.sqrt()) / (2.0 * linesq);
    (0.0..=1.0).contains(&f)
}

/// Distance from `point` to the segment l0-l1.
pub fn dist_to_line(l0: Vec3, l1: Vec3, point: Vec3) -> f32 {
    let lensq = magnitude_squared(l1 - l0);
    let d = dot(point - l0, l1 - l0);
    if d <= 0.0 {
        return magnitude(point - l0);
    }
    if d >= lensq {
        return magnitude(point - l1);
    }
    let dist_sqr = magnitude_squared(point - l0) - d * d / lensq;
    if dist_sqr <= 0.0 { 0.0 } else { dist_sqr.sqrt() }
}

/// Like [`dist_to_line`] but also returns the closest point on the segment.
pub fn dist_to_line_closest(l0: Vec3, l1: Vec3, point: Vec3) -> (f32, Vec3) {
    let lensq = magnitude_squared(l1 - l0);
    let d = dot(point - l0, l1 - l0);
    let closest = if d <= 0.0 {
        l0
    } else if d >= lensq {
        l1
    } else {
        l0 + (l1 - l0) * (d / lensq)
    };
    (magnitude(point - closest), closest)
}

/// Which of the six regions around a triangle (in its own 2D frame) the sphere centre is in.
struct TriSectors {
    inside_ab: bool,
    inside_ac: bool,
    inside_bc: bool,
}

fn sphere_sectors(center: Vec3, va: Vec3, vb: Vec3, vc: Vec3, normal: Vec3) -> TriSectors {
    let mut vec2 = vb - va;
    let len = magnitude(vec2);
    vec2 *= 1.0 / len;
    let vec1 = cross(vec2, normal);
    let vac = vc - va;
    let vas = center - va;
    let b = Vec2::new(0.0, len);
    let c = Vec2::new(dot(vec1, vac), dot(vec2, vac));
    let s = Vec2::new(dot(vec1, vas), dot(vec2, vas));
    TriSectors { inside_ab: cross2d(s, b) >= 0.0, inside_ac: cross2d(c, s) >= 0.0, inside_bc: cross2d(s - b, c - b) >= 0.0 }
}

pub fn test_sphere_triangle(sphere: &Sphere, verts: &[Vec3], tri: &Triangle, plane: &TrianglePlane) -> bool {
    let planedist = plane.calc_point(sphere.center);
    if planedist.abs() > sphere.radius {
        return false;
    }
    let (va, vb, vc) = (verts[tri.a as usize], verts[tri.b as usize], verts[tri.c as usize]);
    let s = sphere_sectors(sphere.center, va, vb, vc, plane.normal);
    let dist = match s.inside_ab as u8 + s.inside_ac as u8 + s.inside_bc as u8 {
        1 => {
            // closest to a vertex
            let p = if s.inside_ab { vc } else if s.inside_ac { vb } else { va };
            magnitude(sphere.center - p)
        }
        2 => {
            // closest to an edge
            if !s.inside_ab {
                dist_to_line(va, vb, sphere.center)
            } else if !s.inside_ac {
                dist_to_line(va, vc, sphere.center)
            } else {
                dist_to_line(vb, vc, sphere.center)
            }
        }
        3 => planedist.abs(), // centre is above the triangle
        _ => return false,
    };
    dist < sphere.radius
}

pub fn process_sphere_sphere(s1: &Sphere, s2: &Sphere, point: &mut ColPoint, mindistsq: &mut f32) -> bool {
    let mut dist = s1.center - s2.center;
    let mut d = magnitude(dist) - s2.radius;
    let depth = s1.radius - d;
    if d < 0.0 {
        d = 0.0;
    }
    if d * d < *mindistsq && d < s1.radius {
        dist = normalise(dist);
        point.point = s1.center - dist * d;
        point.normal = dist;
        point.surface_a = s1.surface;
        point.piece_a = s1.piece;
        point.surface_b = s2.surface;
        point.piece_b = s2.piece;
        point.depth = depth;
        *mindistsq = d * d;
        return true;
    }
    false
}

pub fn process_sphere_box(sph: &Sphere, b: &Box3, point: &mut ColPoint, mindistsq: &mut f32) -> bool {
    if !test_sphere_box(sph, b) {
        return false;
    }
    let pos = |c: f32, lo: f32, hi: f32| if c < lo { 1 } else if c > hi { 2 } else { 0 };
    let (xpos, ypos, zpos) = (pos(sph.center.x, b.min.x, b.max.x), pos(sph.center.y, b.min.y, b.max.y), pos(sph.center.z, b.min.z, b.max.z));
    if xpos == 0 && ypos == 0 && zpos == 0 {
        // sphere centre is inside the box
        let p = (b.min + b.max) * 0.5;
        let dist = sph.center - p;
        let lensq = magnitude_squared(dist);
        if lensq < *mindistsq {
            point.normal = dist * (1.0 / lensq.sqrt());
            point.point = sph.center - point.normal;
            point.surface_a = sph.surface;
            point.piece_a = sph.piece;
            point.surface_b = b.surface;
            point.piece_b = b.piece;
            // distance to the closer side in each dimension
            let dx = if dist.x > 0.0 { b.max.x - sph.center.x } else { sph.center.x - b.min.x };
            let dy = if dist.y > 0.0 { b.max.y - sph.center.y } else { sph.center.y - b.min.y };
            let dz = if dist.z > 0.0 { b.max.z - sph.center.z } else { sph.center.z - b.min.z };
            point.depth = if dx > dy && dx > dz {
                dx
            } else if dy > dz {
                dy
            } else {
                dz
            };
            return true;
        }
    } else {
        let pick = |p: i32, lo: f32, hi: f32, c: f32| match p {
            1 => lo,
            2 => hi,
            _ => c,
        };
        let p = Vec3::new(pick(xpos, b.min.x, b.max.x, sph.center.x), pick(ypos, b.min.y, b.max.y, sph.center.y), pick(zpos, b.min.z, b.max.z, sph.center.z));
        let dist = sph.center - p;
        let lensq = magnitude_squared(dist);
        if lensq < *mindistsq {
            let len = lensq.sqrt();
            point.point = p;
            point.normal = dist * (1.0 / len);
            point.surface_a = sph.surface;
            point.piece_a = sph.piece;
            point.surface_b = b.surface;
            point.piece_b = b.piece;
            point.depth = sph.radius - len;
            *mindistsq = lensq;
            return true;
        }
    }
    false
}

pub fn process_line_box(line: &Line, b: &Box3, point: &mut ColPoint, mindist: &mut f32) -> bool {
    let mut mint = 1.0;
    let (mut p, mut normal) = (Vec3::ZERO, Vec3::ZERO);
    // faces are tried in the original's order; the first to reach a smaller t wins ties
    for (t, hit, n) in line_box_hits(line, b).into_iter().flatten() {
        if t < mint {
            mint = t;
            p = hit;
            normal = n;
        }
    }
    if mint >= *mindist {
        return false;
    }
    point.point = p;
    point.normal = normal;
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = b.surface;
    point.piece_b = b.piece;
    *mindist = mint;
    true
}

/// If `line.p0` lies inside the sphere, no collision is registered.
pub fn process_line_sphere(line: &Line, sphere: &Sphere, point: &mut ColPoint, mindist: &mut f32) -> bool {
    let v01 = line.p1 - line.p0;
    let v0c = sphere.center - line.p0;
    let linesq = magnitude_squared(v01);
    let projline = dot(v01, v0c);
    let tansq = (magnitude_squared(v0c) - sphere.radius * sphere.radius) * linesq;
    let diffsq = projline * projline - tansq;
    if diffsq < 0.0 {
        return false;
    }
    let t = (projline - diffsq.sqrt()) / linesq;
    if !(0.0..=1.0).contains(&t) || t >= *mindist {
        return false;
    }
    point.point = line.p0 + v01 * t;
    point.normal = normalise(point.point - sphere.center);
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = sphere.surface;
    point.piece_b = sphere.piece;
    *mindist = t;
    true
}

/// Vertical-line variant used by GTA III's `process_vertical_line` (bounding-rect early-outs).
pub fn process_vertical_line_triangle(
    line: &Line,
    verts: &[Vec3],
    tri: &Triangle,
    plane: &TrianglePlane,
    point: &mut ColPoint,
    mindist: &mut f32,
    poly: Option<&mut StoredCollPoly>,
) -> bool {
    let p0 = line.p0;
    let (va, vb, vc) = (verts[tri.a as usize], verts[tri.b as usize], verts[tri.c as usize]);
    if p0.x < va.x && p0.x < vb.x && p0.x < vc.x {
        return false;
    }
    if p0.x > va.x && p0.x > vb.x && p0.x > vc.x {
        return false;
    }
    if p0.y < va.y && p0.y < vb.y && p0.y < vc.y {
        return false;
    }
    if p0.y > va.y && p0.y > vb.y && p0.y > vc.y {
        return false;
    }
    let normal = plane.normal;
    if plane.calc_point(p0) * plane.calc_point(line.p1) > 0.0 {
        return false;
    }
    let h = (line.p1 - p0).z;
    let t = -plane.calc_point(p0) / (h * normal.z);
    if t >= *mindist {
        return false;
    }
    let p = Vec3::new(p0.x, p0.y, p0.z + h * t);
    if !inside_projected(project(plane.dir, va, vb, vc, p)) {
        return false;
    }
    point.point = p;
    point.normal = normal;
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = tri.surface;
    point.piece_b = 0;
    if let Some(poly) = poly {
        poly.verts = [va, vb, vc];
        poly.valid = true;
    }
    *mindist = t;
    true
}

/// `poly` is only used by Vice City, which stores the hit triangle here.
pub fn process_line_triangle(
    line: &Line,
    verts: &[Vec3],
    tri: &Triangle,
    plane: &TrianglePlane,
    point: &mut ColPoint,
    mindist: &mut f32,
    poly: Option<&mut StoredCollPoly>,
) -> bool {
    if plane.calc_point(line.p0) * plane.calc_point(line.p1) > 0.0 {
        return false;
    }
    let p0dist = dot(line.p1 - line.p0, plane.normal);
    if p0dist == 0.0 {
        return false; // FIX_BUGS: line lies in the plane
    }
    let t = -plane.calc_point(line.p0) / p0dist;
    if t >= *mindist {
        return false;
    }
    let p = line.p0 + (line.p1 - line.p0) * t;
    let (va, vb, vc) = (verts[tri.a as usize], verts[tri.b as usize], verts[tri.c as usize]);
    if !inside_projected(project(plane.dir, va, vb, vc, p)) {
        return false;
    }
    point.point = p;
    point.normal = plane.normal;
    point.surface_a = 0;
    point.piece_a = 0;
    point.surface_b = tri.surface;
    point.piece_b = 0;
    if let Some(poly) = poly {
        poly.verts = [va, vb, vc];
        poly.valid = true;
    }
    *mindist = t;
    true
}

pub fn process_sphere_triangle(
    sphere: &Sphere,
    verts: &[Vec3],
    tri: &Triangle,
    plane: &TrianglePlane,
    point: &mut ColPoint,
    mindistsq: &mut f32,
) -> bool {
    let planedist = plane.calc_point(sphere.center);
    let distsq = planedist * planedist;
    if planedist.abs() > sphere.radius || distsq > *mindistsq {
        return false;
    }
    let (va, vb, vc) = (verts[tri.a as usize], verts[tri.b as usize], verts[tri.c as usize]);
    let normal = plane.normal;
    let s = sphere_sectors(sphere.center, va, vb, vc, normal);
    let (dist, p) = match s.inside_ab as u8 + s.inside_ac as u8 + s.inside_bc as u8 {
        1 => {
            let p = if s.inside_ab { vc } else if s.inside_ac { vb } else { va };
            (magnitude(sphere.center - p), p)
        }
        2 => {
            if !s.inside_ab {
                dist_to_line_closest(va, vb, sphere.center)
            } else if !s.inside_ac {
                dist_to_line_closest(va, vc, sphere.center)
            } else {
                dist_to_line_closest(vb, vc, sphere.center)
            }
        }
        3 => (planedist.abs(), sphere.center - normal * planedist),
        _ => return false,
    };
    if dist >= sphere.radius || dist * dist >= *mindistsq {
        return false;
    }
    point.point = p;
    point.normal = normalise(sphere.center - p);
    point.surface_a = sphere.surface;
    point.piece_a = sphere.piece;
    point.surface_b = tri.surface;
    point.piece_b = 0;
    point.depth = sphere.radius - dist;
    *mindistsq = dist * dist;
    true
}

/// Whether surfaces of this kind should be skipped by line queries.
#[derive(Debug, Clone, Copy, Default)]
pub struct LineFilter {
    pub ignore_see_through: bool,
    /// Vice City only.
    pub ignore_shoot_through: bool,
}

impl LineFilter {
    fn skips(&self, game: Game, surf: u8) -> bool {
        (self.ignore_see_through && surface::is_see_through(surf)) || (game == Game::Vc && self.ignore_shoot_through && surface::is_shoot_through(surf))
    }
}

pub fn test_line_of_sight(game: Game, line: &Line, matrix: &Matrix, model: &ColModel, filter: LineFilter) -> bool {
    let inv = matrix.inverted();
    let newline = Line { p0: inv.transform_point(line.p0), p1: inv.transform_point(line.p1) };
    if !test_line_box(&newline, &model.bounding_box) {
        return false;
    }
    model.spheres.iter().filter(|s| !filter.skips(game, s.surface)).any(|s| test_line_sphere(&newline, s))
        || model.boxes.iter().filter(|b| !filter.skips(game, b.surface)).any(|b| test_line_box(&newline, b))
        || model.triangles.iter().zip(&model.planes).filter(|(t, _)| !filter.skips(game, t.surface)).any(|(t, p)| test_line_triangle(&newline, &model.vertices, t, p))
}

pub fn process_line_of_sight(game: Game, line: &Line, matrix: &Matrix, model: &ColModel, point: &mut ColPoint, mindist: &mut f32, filter: LineFilter) -> bool {
    let inv = matrix.inverted();
    let newline = Line { p0: inv.transform_point(line.p0), p1: inv.transform_point(line.p1) };
    if !test_line_box(&newline, &model.bounding_box) {
        return false;
    }
    let mut coldist = *mindist;
    for s in model.spheres.iter().filter(|s| !filter.skips(game, s.surface)) {
        process_line_sphere(&newline, s, point, &mut coldist);
    }
    for b in model.boxes.iter().filter(|b| !filter.skips(game, b.surface)) {
        process_line_box(&newline, b, point, &mut coldist);
    }
    for (t, p) in model.triangles.iter().zip(&model.planes) {
        if !filter.skips(game, t.surface) {
            process_line_triangle(&newline, &model.vertices, t, p, point, &mut coldist, None);
        }
    }
    if coldist < *mindist {
        point.point = matrix.transform_point(point.point);
        point.normal = matrix.rotate_vector(point.normal);
        *mindist = coldist;
        return true;
    }
    false
}

/// Straight-down (or up) line through a model, for ground height under cars and peds.
/// GTA III and Vice City differ in the details, hence the game switch.
// mirrors the C++ signature one for one
#[allow(clippy::too_many_arguments)]
pub fn process_vertical_line(
    game: Game,
    line: &Line,
    matrix: &Matrix,
    model: &ColModel,
    point: &mut ColPoint,
    mindist: &mut f32,
    filter: LineFilter,
    poly: Option<&mut StoredCollPoly>,
) -> bool {
    let vc = game == Game::Vc;
    let mut newline = Line { p0: matrix.inverse_transform_point(line.p0), p1: matrix.inverse_transform_point(line.p1) };
    if vc {
        if !test_line_box(&newline, &model.bounding_box) {
            return false;
        }
    } else {
        newline.p1.x = newline.p0.x;
        newline.p1.y = newline.p0.y;
        if !test_vertical_line_box(&newline, &model.bounding_box) {
            return false;
        }
    }
    // Vice City uses a shorter see-through list here (and checks no shoot-through)
    let skip = |surf: u8| {
        if vc {
            (filter.ignore_see_through && surface::is_see_through_vertical(surf)) || false
        } else {
            filter.ignore_see_through && surface::is_see_through(surf)
        }
    };
    let mut coldist = *mindist;
    for s in model.spheres.iter().filter(|s| !skip(s.surface)) {
        process_line_sphere(&newline, s, point, &mut coldist);
    }
    for b in model.boxes.iter().filter(|b| !skip(b.surface)) {
        process_line_box(&newline, b, point, &mut coldist);
    }
    let mut temp = StoredCollPoly::default();
    for (t, p) in model.triangles.iter().zip(&model.planes) {
        if skip(t.surface) {
            continue;
        }
        if vc {
            process_line_triangle(&newline, &model.vertices, t, p, point, &mut coldist, Some(&mut temp));
        } else {
            process_vertical_line_triangle(&newline, &model.vertices, t, p, point, &mut coldist, Some(&mut temp));
        }
    }
    if coldist < *mindist {
        point.point = matrix.transform_point(point.point);
        point.normal = matrix.rotate_vector(point.normal);
        if temp.valid
            && let Some(poly) = poly {
                *poly = temp;
                for v in &mut poly.verts {
                    *v = matrix.transform_point(*v);
                }
            }
        *mindist = coldist;
        return true;
    }
    false
}

/// Re-checks a stored triangle against a vertical line; clears `valid` once it no longer hits.
pub fn is_stored_poly_still_valid_vertical_line(pos: Vec3, z: f32, point: &mut ColPoint, poly: &mut StoredCollPoly) -> bool {
    if !poly.valid {
        return false;
    }
    let plane = TrianglePlane::new(poly.verts[0], poly.verts[1], poly.verts[2]);
    let (p0, p1) = (pos, Vec3::new(pos.x, pos.y, z));
    if plane.calc_point(p0) * plane.calc_point(p1) > 0.0 {
        poly.valid = false;
        return false;
    }
    let t = -plane.calc_point(p0) / dot(p1 - p0, plane.normal);
    let p = p0 + (p1 - p0) * t;
    if !inside_projected(project(plane.dir, poly.verts[0], poly.verts[1], poly.verts[2], p)) {
        poly.valid = false;
        return false;
    }
    point.point = p;
    poly.valid = true;
    true
}

pub const MAX_SPHERES: usize = 128;
pub const MAX_BOXES: usize = 32;
pub const MAX_LINES: usize = 16;
pub const MAX_TRIS: usize = 600;

/// Model A's spheres and lines against model B's spheres, boxes and triangles.
/// Returns the number of A's spheres that collide; points are in world space.
/// Only vehicles have line volumes (4, one per wheel).
pub fn process_col_models(
    matrix_a: &Matrix,
    model_a: &ColModel,
    matrix_b: &Matrix,
    model_b: &ColModel,
    spherepoints: &mut [ColPoint],
    linepoints: &mut [ColPoint],
    linedists: &mut [f32],
) -> usize {
    assert!(model_a.spheres.len() <= MAX_SPHERES && model_a.lines.len() <= MAX_LINES);
    // A space -> B space
    let mat_ab = matrix_b.inverted() * *matrix_a;
    let bsphere_ab = Sphere { radius: model_a.bounding_sphere.radius, center: mat_ab.transform_point(model_a.bounding_sphere.center), surface: 0, piece: 0 };
    if !test_sphere_box(&bsphere_ab, &model_b.bounding_box) {
        return 0;
    }
    // B space -> A space
    let mat_ba = matrix_a.inverted() * *matrix_b;
    let spheres_a: Vec<Sphere> = model_a.spheres.iter().map(|s| Sphere::new(s.radius, mat_ab.transform_point(s.center), s.surface, s.piece)).collect();
    let lines_a: Vec<Line> = model_a.lines.iter().map(|l| Line { p0: mat_ab.transform_point(l.p0), p1: mat_ab.transform_point(l.p1) }).collect();
    let sphere_idx_a: Vec<usize> = (0..spheres_a.len()).filter(|&i| test_sphere_box(&spheres_a[i], &model_b.bounding_box)).collect();
    // the original does not actually test lines against the box
    let line_idx_a: Vec<usize> = (0..lines_a.len()).collect();
    if sphere_idx_a.is_empty() && line_idx_a.is_empty() {
        return 0;
    }
    let sphere_idx_b: Vec<usize> = (0..model_b.spheres.len())
        .filter(|&i| {
            let s = Sphere { radius: model_b.spheres[i].radius, center: mat_ba.transform_point(model_b.spheres[i].center), surface: 0, piece: 0 };
            test_sphere_box(&s, &model_a.bounding_box)
        })
        .collect();
    let box_idx_b: Vec<usize> = (0..model_b.boxes.len()).filter(|&i| test_sphere_box(&bsphere_ab, &model_b.boxes[i])).collect();
    let tri_idx_b: Vec<usize> =
        (0..model_b.triangles.len()).filter(|&i| test_sphere_triangle(&bsphere_ab, &model_b.vertices, &model_b.triangles[i], &model_b.planes[i])).collect();
    assert!(sphere_idx_b.len() <= MAX_SPHERES && box_idx_b.len() <= MAX_BOXES && tri_idx_b.len() <= MAX_TRIS);
    if sphere_idx_b.is_empty() && box_idx_b.is_empty() && tri_idx_b.is_empty() {
        return 0;
    }

    let mut num_collisions = 0;
    for &ia in &sphere_idx_a {
        let mut coldist = 1.0e24_f32;
        let mut collided = false;
        let pt = &mut spherepoints[num_collisions];
        for &j in &sphere_idx_b {
            collided |= process_sphere_sphere(&spheres_a[ia], &model_b.spheres[j], pt, &mut coldist);
        }
        for &j in &box_idx_b {
            collided |= process_sphere_box(&spheres_a[ia], &model_b.boxes[j], pt, &mut coldist);
        }
        for &j in &tri_idx_b {
            collided |= process_sphere_triangle(&spheres_a[ia], &model_b.vertices, &model_b.triangles[j], &model_b.planes[j], pt, &mut coldist);
        }
        if collided {
            num_collisions += 1;
        }
    }
    for p in &mut spherepoints[..num_collisions] {
        p.point = matrix_b.transform_point(p.point);
        p.normal = matrix_b.rotate_vector(p.normal);
    }

    let mut collided = vec![false; line_idx_a.len()];
    for (k, &i) in line_idx_a.iter().enumerate() {
        for &j in &sphere_idx_b {
            collided[k] |= process_line_sphere(&lines_a[i], &model_b.spheres[j], &mut linepoints[i], &mut linedists[i]);
        }
        for &j in &box_idx_b {
            collided[k] |= process_line_box(&lines_a[i], &model_b.boxes[j], &mut linepoints[i], &mut linedists[i]);
        }
        for &j in &tri_idx_b {
            collided[k] |= process_line_triangle(&lines_a[i], &model_b.vertices, &model_b.triangles[j], &model_b.planes[j], &mut linepoints[i], &mut linedists[i], None);
        }
    }
    for (k, &i) in line_idx_a.iter().enumerate() {
        if collided[k] {
            linepoints[i].point = matrix_b.transform_point(linepoints[i].point);
            linepoints[i].normal = matrix_b.rotate_vector(linepoints[i].normal);
        }
    }
    num_collisions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new(x, y, z)
    }

    fn unit_cube() -> Box3 {
        Box3 { min: v(-1., -1., -1.), max: v(1., 1., 1.), surface: 4, piece: 2 }
    }

    #[test]
    fn spheres() {
        let a = Sphere::new(1.0, v(0., 0., 0.), 1, 0);
        assert!(test_sphere_sphere(&a, &Sphere::new(1.0, v(1.5, 0., 0.), 2, 0)));
        assert!(!test_sphere_sphere(&a, &Sphere::new(1.0, v(2.5, 0., 0.), 2, 0)));
        let (mut p, mut d) = (ColPoint::default(), 1.0e24_f32);
        assert!(process_sphere_sphere(&a, &Sphere::new(1.0, v(1.5, 0., 0.), 2, 3), &mut p, &mut d));
        // a's centre is 1.5 from b's centre, 0.5 outside b: overlap depth 1 - 0.5
        assert_eq!((p.depth, p.normal, p.surface_a, p.surface_b, p.piece_b), (0.5, v(-1., 0., 0.), 1, 2, 3));
        assert_eq!(d, 0.25);
    }

    #[test]
    fn sphere_vs_box_outside_and_inside() {
        let b = unit_cube();
        let (mut p, mut d) = (ColPoint::default(), 1.0e24_f32);
        assert!(process_sphere_box(&Sphere::new(0.5, v(1.25, 0., 0.), 0, 0), &b, &mut p, &mut d));
        assert_eq!((p.point, p.normal, p.depth), (v(1., 0., 0.), v(1., 0., 0.), 0.25));
        assert_eq!((p.surface_b, p.piece_b), (4, 2));
        // centre inside the box: pushed out through the nearest-axis side
        let (mut p, mut d) = (ColPoint::default(), 1.0e24_f32);
        assert!(process_sphere_box(&Sphere::new(0.5, v(0.5, 0.1, 0.0), 0, 0), &b, &mut p, &mut d));
        // depth is the largest of the three distances to the nearest side: dx 0.5, dy 0.9, dz 1.0
        assert_eq!(p.depth, 1.0);
        assert!(!process_sphere_box(&Sphere::new(0.5, v(3., 0., 0.), 0, 0), &b, &mut ColPoint::default(), &mut 1.0e24));
    }

    #[test]
    fn lines_vs_box_and_sphere() {
        let b = unit_cube();
        let line = Line { p0: v(-3., 0., 0.), p1: v(3., 0., 0.) };
        assert!(test_line_box(&line, &b));
        let (mut p, mut d) = (ColPoint::default(), 1.0);
        assert!(process_line_box(&line, &b, &mut p, &mut d));
        assert_eq!((p.point, p.normal, d), (v(-1., 0., 0.), v(-1., 0., 0.), 1.0 / 3.0));
        assert!(!test_line_box(&Line { p0: v(-3., 5., 0.), p1: v(3., 5., 0.) }, &b));
        assert!(test_line_box(&Line { p0: v(0., 0., 0.), p1: v(0., 0., 0.5) }, &b)); // inside
        // vertical
        assert!(test_vertical_line_box(&Line { p0: v(0., 0., 5.), p1: v(0., 0., -5.) }, &b));
        assert!(!test_vertical_line_box(&Line { p0: v(2., 0., 5.), p1: v(2., 0., -5.) }, &b));

        let s = Sphere::new(1.0, v(0., 0., 0.), 5, 6);
        let (mut p, mut d) = (ColPoint::default(), 1.0);
        assert!(process_line_sphere(&line, &s, &mut p, &mut d));
        assert_eq!((p.point, p.normal, p.surface_b), (v(-1., 0., 0.), v(-1., 0., 0.), 5));
        assert!(test_line_sphere(&line, &s));
        // starting inside the sphere registers nothing
        assert!(!test_line_sphere(&Line { p0: v(0., 0., 0.), p1: v(5., 0., 0.) }, &s));
        assert!(!process_line_sphere(&Line { p0: v(0., 0., 0.), p1: v(5., 0., 0.) }, &s, &mut ColPoint::default(), &mut 1.0));
    }

    fn floor_triangle() -> (Vec<Vec3>, Triangle, TrianglePlane) {
        // the game's normal is cross(c - a, b - a), so +z means clockwise seen from above
        let verts = vec![v(0., 0., 0.), v(0., 4., 0.), v(4., 0., 0.)];
        let tri = Triangle { a: 0, b: 1, c: 2, surface: 9 };
        let plane = TrianglePlane::new(verts[0], verts[1], verts[2]);
        (verts, tri, plane)
    }

    #[test]
    fn triangle_plane_and_line() {
        let (verts, tri, plane) = floor_triangle();
        assert_eq!((plane.normal, plane.dist, plane.dir), (v(0., 0., 1.), 0.0, Dir::ZPos));
        let down = Line { p0: v(1., 1., 2.), p1: v(1., 1., -2.) };
        assert!(test_line_triangle(&down, &verts, &tri, &plane));
        let (mut p, mut d) = (ColPoint::default(), 1.0);
        assert!(process_line_triangle(&down, &verts, &tri, &plane, &mut p, &mut d, None));
        assert_eq!((p.point, p.normal, d, p.surface_b), (v(1., 1., 0.), v(0., 0., 1.), 0.5, 9));
        // outside the triangle (beyond the hypotenuse), parallel to the plane, same side
        assert!(!test_line_triangle(&Line { p0: v(3., 3., 2.), p1: v(3., 3., -2.) }, &verts, &tri, &plane));
        assert!(!test_line_triangle(&Line { p0: v(1., 1., 1.), p1: v(2., 2., 1.) }, &verts, &tri, &plane));
        assert!(!test_line_triangle(&Line { p0: v(1., 1., 0.), p1: v(2., 2., 0.) }, &verts, &tri, &plane)); // in-plane
        // stored polygon
        let mut poly = StoredCollPoly::default();
        let (mut p, mut d) = (ColPoint::default(), 1.0);
        assert!(process_vertical_line_triangle(&down, &verts, &tri, &plane, &mut p, &mut d, Some(&mut poly)));
        assert!(poly.valid && poly.verts[1] == verts[1]);
        let mut pt = ColPoint::default();
        assert!(is_stored_poly_still_valid_vertical_line(v(1., 1., 2.), -2.0, &mut pt, &mut poly));
        assert_eq!(pt.point, v(1., 1., 0.));
        assert!(!is_stored_poly_still_valid_vertical_line(v(3., 3., 2.), -2.0, &mut pt, &mut poly));
        assert!(!poly.valid);
    }

    #[test]
    fn triangle_vs_sphere() {
        let (verts, tri, plane) = floor_triangle();
        let above = Sphere::new(0.5, v(1., 1., 0.25), 3, 0);
        assert!(test_sphere_triangle(&above, &verts, &tri, &plane));
        let (mut p, mut d) = (ColPoint::default(), 1.0e24_f32);
        assert!(process_sphere_triangle(&above, &verts, &tri, &plane, &mut p, &mut d));
        assert_eq!((p.point, p.normal, p.depth, p.surface_a, p.surface_b), (v(1., 1., 0.), v(0., 0., 1.), 0.25, 3, 9));
        // beside the long edge: touching the edge at distance 0.2
        let (mut p, mut d) = (ColPoint::default(), 1.0e24_f32);
        let side = Sphere::new(0.5, v(2.0 + 0.2 / 2f32.sqrt() * 1.0 + 0.0, 2.0 + 0.2 / 2f32.sqrt(), 0.0), 0, 0);
        assert!(process_sphere_triangle(&side, &verts, &tri, &plane, &mut p, &mut d));
        assert!((p.point - v(2., 2., 0.)).length() < 1e-5 && (p.depth - 0.3).abs() < 1e-5);
        // near a corner, far from everything else
        let corner = Sphere::new(0.5, v(-0.3, -0.3, 0.0), 0, 0);
        assert!(test_sphere_triangle(&corner, &verts, &tri, &plane));
        assert!(!test_sphere_triangle(&Sphere::new(0.5, v(-1., -1., 0.), 0, 0), &verts, &tri, &plane));
        assert!(!test_sphere_triangle(&Sphere::new(0.5, v(1., 1., 1.), 0, 0), &verts, &tri, &plane));
    }

    #[test]
    fn dist_to_line_cases() {
        let (a, b) = (v(0., 0., 0.), v(10., 0., 0.));
        assert_eq!(dist_to_line(a, b, v(5., 3., 0.)), 3.0);
        assert_eq!(dist_to_line(a, b, v(-4., 3., 0.)), 5.0);
        assert_eq!(dist_to_line(a, b, v(14., 3., 0.)), 5.0);
        assert_eq!(dist_to_line_closest(a, b, v(5., 3., 0.)), (3.0, v(5., 0., 0.)));
    }

    fn floor_model() -> ColModel {
        let (vertices, tri, _) = floor_triangle();
        let mut m = ColModel {
            bounding_box: Box3 { min: v(-1., -1., -1.), max: v(5., 5., 1.), surface: 0, piece: 0 },
            bounding_sphere: Sphere::new(4.0, v(1.5, 1.5, 0.), 0, 0),
            vertices,
            triangles: vec![tri, Triangle { surface: surface::GLASS, ..tri }],
            ..Default::default()
        };
        m.calculate_triangle_planes();
        m
    }

    #[test]
    fn model_line_queries_in_world_space() {
        let model = floor_model();
        // model placed at (10, 0, 5), turned 90 degrees about z
        let mut m = Matrix::rotate_z_only(std::f32::consts::FRAC_PI_2);
        m.pos = v(10., 0., 5.);
        let world = |p: Vec3| m.transform_point(p);
        let line = Line { p0: world(v(1., 1., 2.)), p1: world(v(1., 1., -2.)) };
        let (mut pt, mut d) = (ColPoint::default(), 1.0);
        assert!(process_line_of_sight(Game::Gta3, &line, &m, &model, &mut pt, &mut d, LineFilter::default()));
        assert!((pt.point - world(v(1., 1., 0.))).length() < 1e-4);
        assert!((pt.normal - m.rotate_vector(Vec3::Z)).length() < 1e-5 && d == 0.5);
        assert!(test_line_of_sight(Game::Gta3, &line, &m, &model, LineFilter::default()));
        // both triangles overlap; ignoring see-through glass still leaves the solid one
        let f = LineFilter { ignore_see_through: true, ..Default::default() };
        assert!(test_line_of_sight(Game::Gta3, &line, &m, &model, f));
        let mut glass_only = model.clone();
        glass_only.triangles.remove(0);
        glass_only.calculate_triangle_planes();
        assert!(!test_line_of_sight(Game::Gta3, &line, &m, &glass_only, f));
        assert!(test_line_of_sight(Game::Gta3, &line, &m, &glass_only, LineFilter::default()));

        // vertical: world z drop through the model's local frame
        let down = Line { p0: v(10.0, 0., 8.), p1: v(10.0, 0., 2.) };
        let m0 = Matrix::from_translation(v(1., 1., 5.));
        let drop = Line { p0: v(2.0, 2.0, 9.), p1: v(2.0, 2.0, 1.) };
        let _ = down;
        let (mut pt, mut d) = (ColPoint::default(), 1.0);
        let mut poly = StoredCollPoly::default();
        assert!(process_vertical_line(Game::Gta3, &drop, &m0, &model, &mut pt, &mut d, LineFilter::default(), Some(&mut poly)));
        assert_eq!(pt.point, v(2., 2., 5.));
        assert!(poly.valid && poly.verts[0] == v(1., 1., 5.));
        // Vice City takes the general line path and lands in the same place
        let (mut pt2, mut d2) = (ColPoint::default(), 1.0);
        assert!(process_vertical_line(Game::Vc, &drop, &m0, &model, &mut pt2, &mut d2, LineFilter::default(), None));
        assert_eq!(pt2.point, pt.point);
    }

    #[test]
    fn model_vs_model_spheres() {
        let mut a = ColModel { bounding_sphere: Sphere::new(2.0, v(0., 0., 0.), 0, 0), spheres: vec![Sphere::new(1.0, v(0., 0., 0.), 1, 0)], ..Default::default() };
        a.bounding_box = Box3 { min: v(-1., -1., -1.), max: v(1., 1., 1.), surface: 0, piece: 0 };
        let b = ColModel {
            bounding_box: Box3 { min: v(-5., -5., -5.), max: v(5., 5., 5.), surface: 0, piece: 0 },
            spheres: vec![Sphere::new(1.0, v(0., 0., 0.), 2, 0)],
            ..Default::default()
        };
        let (ma, mb) = (Matrix::from_translation(v(1.5, 0., 0.)), Matrix::IDENTITY);
        let mut pts = [ColPoint::default(); 4];
        let n = process_col_models(&ma, &a, &mb, &b, &mut pts, &mut [], &mut []);
        assert_eq!(n, 1);
        assert_eq!((pts[0].surface_a, pts[0].surface_b), (1, 2));
        assert!((pts[0].normal - v(1., 0., 0.)).length() < 1e-5 && (pts[0].depth - 0.5).abs() < 1e-5);
        // far apart: nothing
        let far = Matrix::from_translation(v(50., 0., 0.));
        assert_eq!(process_col_models(&far, &a, &mb, &b, &mut pts, &mut [], &mut []), 0);
    }
}
