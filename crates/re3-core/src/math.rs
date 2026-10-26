//! The game's own matrix, quaternion and rectangle types. Operation order follows the C++
//! so results match float for float; `glam::Vec3` is used for plain vectors.
use glam::{Vec2, Vec3};

pub const PI: f32 = std::f32::consts::PI;
pub const TWO_PI: f32 = std::f32::consts::TAU;

/// `CVector::Normalise`: a zero vector becomes (1, 0, 0).
/// `CGeneral::GetATanOfXY`: the angle of (x, y) counter-clockwise from +x, in 0..2pi, built from
/// `atan2(t, 1)` pieces like the original.
pub fn atan_of_xy(x: f32, y: f32) -> f32 {
    use std::f32::consts::PI;
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    let (xabs, yabs) = (x.abs(), y.abs());
    if xabs < yabs {
        if y > 0.0 {
            if x > 0.0 { 0.5 * PI - (x / y).atan2(1.0) } else { 0.5 * PI + (-x / y).atan2(1.0) }
        } else if x > 0.0 {
            1.5 * PI + (x / -y).atan2(1.0)
        } else {
            1.5 * PI - (-x / -y).atan2(1.0)
        }
    } else if y > 0.0 {
        if x > 0.0 { (y / x).atan2(1.0) } else { PI - (y / -x).atan2(1.0) }
    } else if x > 0.0 {
        2.0 * PI - (-y / x).atan2(1.0)
    } else {
        PI + (-y / -x).atan2(1.0)
    }
}

pub fn normalise(v: Vec3) -> Vec3 {
    let sq = magnitude_squared(v);
    if sq > 0.0 {
        let inv = 1.0 / sq.sqrt();
        Vec3::new(v.x * inv, v.y * inv, v.z * inv)
    } else {
        Vec3::new(1.0, v.y, v.z)
    }
}

/// `CrossProduct`, written out so the rounding matches the original.
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}

pub fn magnitude_squared(v: Vec3) -> f32 {
    v.x * v.x + v.y * v.y + v.z * v.z
}

pub fn magnitude(v: Vec3) -> f32 {
    magnitude_squared(v).sqrt()
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub fn cross2d(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// 4x3 transform: `right`, `forward`, `up` axes (the C++ names; RenderWare calls them
/// right/up/at) and a position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub right: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub pos: Vec3,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Self = Self { right: Vec3::X, forward: Vec3::Y, up: Vec3::Z, pos: Vec3::ZERO };

    pub fn from_translation(p: Vec3) -> Self {
        Self { pos: p, ..Self::IDENTITY }
    }

    pub fn from_scale(s: f32) -> Self {
        Self { right: Vec3::new(s, 0.0, 0.0), forward: Vec3::new(0.0, s, 0.0), up: Vec3::new(0.0, 0.0, s), pos: Vec3::ZERO }
    }

    /// `CMatrix::Scale` with the FIX_BUGS behaviour (position untouched).
    pub fn scale(&mut self, s: f32) {
        self.right *= s;
        self.forward *= s;
        self.up *= s;
    }

    pub fn reset_orientation(&mut self) {
        *self = Self { pos: self.pos, ..Self::IDENTITY };
    }

    pub fn rotate_x_only(angle: f32) -> Self {
        let (s, c) = (angle.sin(), angle.cos());
        Self { right: Vec3::X, forward: Vec3::new(0.0, c, s), up: Vec3::new(0.0, -s, c), pos: Vec3::ZERO }
    }

    pub fn rotate_y_only(angle: f32) -> Self {
        let (s, c) = (angle.sin(), angle.cos());
        Self { right: Vec3::new(c, 0.0, -s), forward: Vec3::Y, up: Vec3::new(s, 0.0, c), pos: Vec3::ZERO }
    }

    pub fn rotate_z_only(angle: f32) -> Self {
        let (s, c) = (angle.sin(), angle.cos());
        Self { right: Vec3::new(c, s, 0.0), forward: Vec3::new(-s, c, 0.0), up: Vec3::Z, pos: Vec3::ZERO }
    }

    /// `SetRotate`: rotation from three Euler angles, zero position.
    pub fn from_euler(x: f32, y: f32, z: f32) -> Self {
        let (cx, sx, cy, sy, cz, sz) = (x.cos(), x.sin(), y.cos(), y.sin(), z.cos(), z.sin());
        Self {
            right: Vec3::new(cz * cy - (sz * sx) * sy, (cz * sx) * sy + sz * cy, -cx * sy),
            forward: Vec3::new(-sz * cx, cz * cx, sx),
            up: Vec3::new((sz * sx) * cy + cz * sy, sz * sy - (cz * sx) * cy, cx * cy),
            pos: Vec3::ZERO,
        }
    }

    pub fn rotate_x(&mut self, a: f32) {
        let (c, s) = (a.cos(), a.sin());
        let f = |v: Vec3| Vec3::new(v.x, c * v.y - s * v.z, c * v.z + s * v.y);
        (self.right, self.forward, self.up, self.pos) = (f(self.right), f(self.forward), f(self.up), f(self.pos));
    }

    pub fn rotate_y(&mut self, a: f32) {
        let (c, s) = (a.cos(), a.sin());
        let f = |v: Vec3| Vec3::new(c * v.x + s * v.z, v.y, c * v.z - s * v.x);
        (self.right, self.forward, self.up, self.pos) = (f(self.right), f(self.forward), f(self.up), f(self.pos));
    }

    pub fn rotate_z(&mut self, a: f32) {
        let (c, s) = (a.cos(), a.sin());
        let f = |v: Vec3| Vec3::new(c * v.x - s * v.y, c * v.y + s * v.x, v.z);
        (self.right, self.forward, self.up, self.pos) = (f(self.right), f(self.forward), f(self.up), f(self.pos));
    }

    /// `CMatrix::Rotate`: apply an Euler rotation to the whole matrix, position included.
    pub fn rotate(&mut self, x: f32, y: f32, z: f32) {
        let r = Self::from_euler(x, y, z);
        let (x1, x2, x3) = (r.right.x, r.right.y, r.right.z);
        let (y1, y2, y3) = (r.forward.x, r.forward.y, r.forward.z);
        let (z1, z2, z3) = (r.up.x, r.up.y, r.up.z);
        let f = |v: Vec3| Vec3::new(x1 * v.x + y1 * v.y + z1 * v.z, x2 * v.x + y2 * v.y + z2 * v.z, x3 * v.x + y3 * v.y + z3 * v.z);
        (self.right, self.forward, self.up, self.pos) = (f(self.right), f(self.forward), f(self.up), f(self.pos));
    }

    pub fn reorthogonalise(&mut self) {
        self.up = normalise(cross(self.right, self.forward));
        self.right = normalise(cross(self.forward, self.up));
        self.forward = cross(self.up, self.right);
    }

    /// `matrix * point`: rotate then translate.
    pub fn transform_point(&self, v: Vec3) -> Vec3 {
        let m = self;
        Vec3::new(
            m.right.x * v.x + m.forward.x * v.y + m.up.x * v.z + m.pos.x,
            m.right.y * v.x + m.forward.y * v.y + m.up.y * v.z + m.pos.y,
            m.right.z * v.x + m.forward.z * v.y + m.up.z * v.z + m.pos.z,
        )
    }

    /// `Multiply3x3(matrix, vec)`: rotate only.
    pub fn rotate_vector(&self, v: Vec3) -> Vec3 {
        let m = self;
        Vec3::new(
            m.right.x * v.x + m.forward.x * v.y + m.up.x * v.z,
            m.right.y * v.x + m.forward.y * v.y + m.up.y * v.z,
            m.right.z * v.x + m.forward.z * v.y + m.up.z * v.z,
        )
    }

    /// `Multiply3x3(vec, matrix)`: rotate by the transpose (the inverse rotation).
    pub fn rotate_vector_inverse(&self, v: Vec3) -> Vec3 {
        let m = self;
        Vec3::new(
            m.right.x * v.x + m.right.y * v.y + m.right.z * v.z,
            m.forward.x * v.x + m.forward.y * v.y + m.forward.z * v.z,
            m.up.x * v.x + m.up.y * v.y + m.up.z * v.z,
        )
    }

    /// `MultiplyInverse(matrix, point)`: the point in this matrix's local space.
    pub fn inverse_transform_point(&self, v: Vec3) -> Vec3 {
        self.rotate_vector_inverse(Vec3::new(v.x - self.pos.x, v.y - self.pos.y, v.z - self.pos.z))
    }

    /// `Invert`: assumes an orthonormal rotation, as the original does.
    pub fn inverted(&self) -> Self {
        let s = self;
        let right = Vec3::new(s.right.x, s.forward.x, s.up.x);
        let forward = Vec3::new(s.right.y, s.forward.y, s.up.y);
        let up = Vec3::new(s.right.z, s.forward.z, s.up.z);
        let mut p = Vec3::ZERO;
        p.x += right.x * s.pos.x;
        p.y += right.y * s.pos.x;
        p.z += right.z * s.pos.x;
        p.x += forward.x * s.pos.y;
        p.y += forward.y * s.pos.y;
        p.z += forward.z * s.pos.y;
        p.x += up.x * s.pos.z;
        p.y += up.y * s.pos.z;
        p.z += up.z * s.pos.z;
        Self { right, forward, up, pos: Vec3::new(-p.x, -p.y, -p.z) }
    }
}

/// `m1 * m2`: apply `m2`, then `m1`.
impl std::ops::Mul for Matrix {
    type Output = Matrix;
    fn mul(self, m2: Matrix) -> Matrix {
        let m1 = self;
        let col = |v: Vec3| {
            Vec3::new(
                m1.right.x * v.x + m1.forward.x * v.y + m1.up.x * v.z,
                m1.right.y * v.x + m1.forward.y * v.y + m1.up.y * v.z,
                m1.right.z * v.x + m1.forward.z * v.y + m1.up.z * v.z,
            )
        };
        Matrix { right: col(m2.right), forward: col(m2.forward), up: col(m2.up), pos: m1.transform_point(m2.pos) }
    }
}

/// `CRect`: note `top` is the smaller y and `bottom` the larger.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Rect {
    /// Empty rectangle that any point will grow (`CRect()`).
    pub const EMPTY: Self = Self { left: 1_000_000.0, bottom: -1_000_000.0, right: -1_000_000.0, top: 1_000_000.0 };

    /// `CRect(l, t, r, b)`.
    pub fn new(l: f32, t: f32, r: f32, b: f32) -> Self {
        Self { left: l, bottom: b, right: r, top: t }
    }
    pub fn contain_point(&mut self, v: Vec3) {
        self.left = self.left.min(v.x);
        self.right = self.right.max(v.x);
        self.top = self.top.min(v.y);
        self.bottom = self.bottom.max(v.y);
    }
    pub fn contain_rect(&mut self, r: &Rect) {
        self.left = self.left.min(r.left);
        self.right = self.right.max(r.right);
        self.top = self.top.min(r.top);
        self.bottom = self.bottom.max(r.bottom);
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.left && p.x <= self.right && p.y >= self.top && p.y <= self.bottom
    }
    pub fn contains_with_radius(&self, p: Vec2, r: f32) -> bool {
        p.x >= self.left - r && p.x <= self.right + r && p.y >= self.top - r && p.y <= self.bottom + r
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.left += x;
        self.right += x;
        self.bottom += y;
        self.top += y;
    }
    pub fn grow(&mut self, l: f32, r: f32, t: f32, b: f32) {
        self.left -= l;
        self.top -= t;
        self.right += r;
        self.bottom += b;
    }
    pub fn width(&self) -> f32 {
        self.right - self.left
    }
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    pub fn magnitude_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }
    pub fn normalise(&mut self) {
        let sq = self.magnitude_squared();
        if sq == 0.0 {
            self.w = 1.0;
        } else {
            let inv = 1.0 / sq.sqrt();
            (self.x, self.y, self.z, self.w) = (self.x * inv, self.y * inv, self.z * inv, self.w * inv);
        }
    }
    /// The conjugate (the C++ calls it Invert).
    pub fn conjugate(&self) -> Self {
        Self::new(-self.x, -self.y, -self.z, self.w)
    }
    pub fn dot(&self, o: &Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }
    pub fn scaled(&self, f: f32) -> Self {
        Self::new(self.x * f, self.y * f, self.z * f, self.w * f)
    }
    pub fn multiply(q1: &Self, q2: &Self) -> Self {
        Self {
            x: (q2.z * q1.y) - (q1.z * q2.y) + (q1.x * q2.w) + (q2.x * q1.w),
            y: (q2.x * q1.z) - (q1.x * q2.z) + (q1.y * q2.w) + (q2.y * q1.w),
            z: (q2.y * q1.x) - (q1.y * q2.x) + (q1.z * q2.w) + (q2.z * q1.w),
            w: (q2.w * q1.w) - (q2.x * q1.x) - (q2.y * q1.y) - (q2.z * q1.z),
        }
    }
    /// Spherical interpolation with the caller supplying `theta` and `1/sin(theta)`.
    pub fn slerp(q1: &Self, q2: &Self, theta: f32, inv_sin: f32, t: f32) -> Self {
        if theta == 0.0 {
            return *q2;
        }
        let (w1, w2) = if theta > PI / 2.0 {
            let theta = PI - theta;
            (((1.0 - t) * theta).sin() * inv_sin, -(t * theta).sin() * inv_sin)
        } else {
            (((1.0 - t) * theta).sin() * inv_sin, (t * theta).sin() * inv_sin)
        };
        let (a, b) = (q1.scaled(w1), q2.scaled(w2));
        Self::new(a.x + b.x, a.y + b.y, a.z + b.z, a.w + b.w)
    }
    /// `(axis, angle)`; like the original, the angle is `acos(w)`, i.e. half the rotation.
    pub fn to_axis_angle(&self) -> (Vec3, f32) {
        let angle = self.w.acos();
        let s = angle.sin();
        (Vec3::new(self.x * (1.0 / s), self.y * (1.0 / s), self.z * (1.0 / s)), angle)
    }
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let (hc, hs) = ((angle * 0.5).cos(), (angle * 0.5).sin());
        Self::new(axis.x * hs, axis.y * hs, axis.z * hs, hc)
    }
    /// Rotation part of a matrix (`right`, `forward`, `up` = RenderWare right, up, at).
    pub fn to_matrix(&self) -> Matrix {
        let (x2, y2, z2) = (self.x + self.x, self.y + self.y, self.z + self.z);
        let (x_2x, x_2y, x_2z) = (self.x * x2, self.x * y2, self.x * z2);
        let (y_2y, y_2z, z_2z) = (self.y * y2, self.y * z2, self.z * z2);
        let (w_2x, w_2y, w_2z) = (self.w * x2, self.w * y2, self.w * z2);
        Matrix {
            right: Vec3::new(1.0 - (y_2y + z_2z), x_2y + w_2z, x_2z - w_2y),
            forward: Vec3::new(x_2y - w_2z, 1.0 - (x_2x + z_2z), y_2z + w_2x),
            up: Vec3::new(x_2z + w_2y, y_2z - w_2x, 1.0 - (x_2x + y_2y)),
            pos: Vec3::ZERO,
        }
    }
    pub fn from_matrix(m: &Matrix) -> Self {
        let (r, u, a) = (m.right, m.forward, m.up);
        let f = u.y + r.x + a.z;
        if f >= 0.0 {
            let s = (f + 1.0).sqrt();
            let k = 0.5 / s;
            return Self::new((u.z - a.y) * k, (a.x - r.z) * k, (r.y - u.x) * k, 0.5 * s);
        }
        let f = r.x - u.y - a.z;
        if f >= 0.0 {
            let s = (f + 1.0).sqrt();
            let k = 0.5 / s;
            return Self::new(0.5 * s, (u.x + r.y) * k, (a.x + r.z) * k, (u.z - a.y) * k);
        }
        let f = u.y - r.x - a.z;
        if f >= 0.0 {
            let s = (f + 1.0).sqrt();
            let k = 0.5 / s;
            return Self::new((u.x - r.y) * k, 0.5 * s, (a.y + u.z) * k, (a.x - r.z) * k);
        }
        let f = a.z - (u.y + r.x);
        let s = (f + 1.0).sqrt();
        let k = 0.5 / s;
        Self::new((a.x + r.z) * k, (a.y + u.z) * k, 0.5 * s, (r.y - u.x) * k)
    }
    /// From three Euler angles (`CQuaternion::Set(f1, f2, f3)`), ported verbatim. It is not the exact
    /// inverse of [`Quat::to_euler`]: a round trip permutes the components (unverified against the C++ binary).
    pub fn from_euler(f1: f32, f2: f32, f3: f32) -> Self {
        let (c1, c2, c3) = ((f1 * 0.5).cos(), (f2 * 0.5).cos(), (f3 * 0.5).cos());
        let (s1, s2, s3) = ((f1 * 0.5).sin(), (f2 * 0.5).sin(), (f3 * 0.5).sin());
        Self::new(
            ((c2 * c1) * s3) - ((s2 * s1) * c3),
            ((s1 * c2) * c3) + ((s2 * c1) * s3),
            ((s2 * c1) * c3) - ((s1 * c2) * s3),
            ((c2 * c1) * c3) + ((s2 * s1) * s3),
        )
    }
    /// Euler angles `(f1, f2, f3)`, each in [0, 2π).
    pub fn to_euler(&self) -> (f32, f32, f32) {
        let m = self.to_matrix();
        let (r, u, a) = (m.right, m.forward, m.up);
        let mut f3 = r.y.atan2(u.y);
        if f3 < 0.0 {
            f3 += TWO_PI;
        }
        let (s, c) = (f3.sin(), f3.cos());
        let mut f1 = (-a.y).atan2(s * r.y + c * u.y);
        if f1 < 0.0 {
            f1 += TWO_PI;
        }
        let mut f2 = (-(r.z * c - u.z * s)).atan2(r.x * c - u.x * s);
        if f2 < 0.0 {
            f2 += TWO_PI;
        }
        (f1, f2, f3)
    }
}

/// `CVector::Heading`: 0 faces +y, positive turns left.
pub fn heading(v: Vec3) -> f32 {
    (-v.x).atan2(v.y)
}

/// `CVector2D::Normalise` (no zero check, so a zero vector gives NaN like the original).
pub fn normalise_2d(v: Vec2) -> Vec2 {
    let inv = 1.0 / (v.x * v.x + v.y * v.y).sqrt();
    Vec2::new(v.x * inv, v.y * inv)
}

/// `CVector2D::NormaliseSafe`.
pub fn normalise_safe_2d(v: Vec2) -> Vec2 {
    let sq = v.x * v.x + v.y * v.y;
    if sq > 0.0 {
        let inv = 1.0 / sq.sqrt();
        Vec2::new(v.x * inv, v.y * inv)
    } else {
        Vec2::new(1.0, v.y)
    }
}

impl Matrix {
    /// `RwMatrixRotate(axis, degrees, rwCOMBINEPOSTCONCAT)`: rotates the axes and the position about `axis`.
    pub fn rotate_about_axis(&mut self, axis: Vec3, degrees: f32) {
        let q = glam::Quat::from_axis_angle(axis.normalize_or_zero(), degrees.to_radians());
        self.right = q * self.right;
        self.forward = q * self.forward;
        self.up = q * self.up;
        self.pos = q * self.pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).abs().max_element() < 1e-5
    }

    #[test]
    fn invert_undoes_transform() {
        let mut m = Matrix::from_euler(0.3, -0.7, 1.9);
        m.pos = Vec3::new(10.0, -4.0, 2.5);
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert!(close(m.inverted().transform_point(m.transform_point(p)), p));
        assert!(close(m.inverse_transform_point(m.transform_point(p)), p));
        let id = m * m.inverted();
        assert!(close(id.right, Vec3::X) && close(id.forward, Vec3::Y) && close(id.up, Vec3::Z) && close(id.pos, Vec3::ZERO));
    }

    #[test]
    fn mul_applies_right_hand_matrix_first() {
        let a = Matrix::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Matrix::rotate_z_only(PI / 2.0);
        let p = Vec3::new(1.0, 0.0, 0.0);
        assert!(close((a * b).transform_point(p), a.transform_point(b.transform_point(p))));
    }

    #[test]
    fn rotations_compose_like_the_original() {
        let mut m = Matrix::IDENTITY;
        m.rotate_z(PI / 2.0);
        assert!(close(m.right, Vec3::Y));
        let mut n = Matrix::from_translation(Vec3::X);
        n.rotate_x(PI / 2.0);
        n.rotate_y(0.0);
        assert!(close(n.pos, Vec3::X));
    }

    #[test]
    fn normalise_zero_vector_gives_x() {
        assert_eq!(normalise(Vec3::ZERO), Vec3::X);
        assert!(close(normalise(Vec3::new(0.0, 3.0, 4.0)), Vec3::new(0.0, 0.6, 0.8)));
    }

    #[test]
    fn quaternion_round_trips() {
        let m = Matrix::from_euler(0.2, 0.4, 0.6);
        let q = Quat::from_matrix(&m);
        let back = q.to_matrix();
        assert!(close(back.right, m.right) && close(back.forward, m.forward) && close(back.up, m.up));
        let (axis, angle) = Quat::from_axis_angle(Vec3::Z, 1.0).to_axis_angle();
        assert!((angle - 0.5).abs() < 1e-5 && close(axis, Vec3::Z));
        let (a, b, c) = Quat::from_euler(0.1, 0.2, 0.3).to_euler();
        assert!([a, b, c].iter().all(|x| (0.0..TWO_PI).contains(x)));

    }

    #[test]
    fn slerp_endpoints() {
        let (a, b) = (Quat::new(0.0, 0.0, 0.0, 1.0), Quat::from_axis_angle(Vec3::Z, 1.0));
        let theta = a.dot(&b).acos();
        let inv = 1.0 / theta.sin();
        let mid = Quat::slerp(&a, &b, theta, inv, 0.0);
        assert!((mid.w - 1.0).abs() < 1e-5);
        let end = Quat::slerp(&a, &b, theta, inv, 1.0);
        assert!((end.z - b.z).abs() < 1e-5);
    }

    #[test]
    fn rect_grows_around_points() {
        let mut r = Rect::EMPTY;
        r.contain_point(Vec3::new(1.0, 2.0, 0.0));
        r.contain_point(Vec3::new(-3.0, 5.0, 0.0));
        assert_eq!((r.left, r.right, r.top, r.bottom), (-3.0, 1.0, 2.0, 5.0));
        assert!(r.contains(Vec2::new(0.0, 3.0)) && !r.contains(Vec2::new(2.0, 3.0)));
    }
}
