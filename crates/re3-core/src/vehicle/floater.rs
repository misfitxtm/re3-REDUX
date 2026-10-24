//! `cBuoyancy`: how deep a body sits in water and the upward push that follows.
//! The water surface comes from [`WaterLevel`], implemented by the water mesh later.
use crate::collision::ColModel;
use crate::math::Matrix;
use glam::Vec3;

pub trait WaterLevel: std::fmt::Debug {
    /// Height of the water at (x, y), or None if there is none. `ignore_height` skips the check
    /// that the point is below the surface (`CWaterLevel::GetWaterLevel`'s last argument).
    fn height(&self, x: f32, y: f32, z: f32, ignore_height: bool) -> Option<f32>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoatModel {
    Predator,
    Speeder,
    Reefer,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Above,
    In,
    Under,
}

/// Share of a boat's bounding box that is hull, rear to bow in a 3x3 grid.
const BOAT_VOLUME: [f32; 9] = [0.75, 0.9, 0.75, 0.95, 1.0, 0.95, 0.3, 0.7, 0.3];

#[derive(Debug, Clone)]
pub struct Buoyancy {
    position: Vec3,
    matrix: Matrix,
    position_z: Vec3,
    water_level: f32,
    /// Added to the water height (set by boats riding waves).
    pub water_level_inc: f32,
    buoyancy: f32,
    dim_max: Vec3,
    dim_min: Vec3,
    num_partial_volumes: f32,
    have_volume: bool,
    step: Vec3,
    step_ratio: Vec3,
    num_steps: f32,
    pub flip_average: bool,
    is_boat: bool,
    volume_under_water: f32,
    impulse_point: Vec3,
}

impl Default for Buoyancy {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            matrix: Matrix::IDENTITY,
            position_z: Vec3::ZERO,
            water_level: 0.0,
            water_level_inc: 0.0,
            buoyancy: 0.0,
            dim_max: Vec3::ZERO,
            dim_min: Vec3::ZERO,
            num_partial_volumes: 1.0,
            have_volume: false,
            step: Vec3::ZERO,
            step_ratio: Vec3::ZERO,
            num_steps: 2.0,
            flip_average: false,
            is_boat: false,
            volume_under_water: 0.0,
            impulse_point: Vec3::ZERO,
        }
    }
}

impl Buoyancy {
    /// Share of the body under water after the last [`Buoyancy::process`].
    pub fn volume_under_water(&self) -> f32 {
        self.volume_under_water
    }

    /// `ProcessBuoyancy`: the point (relative to the body) and impulse of the water's push, if any.
    #[allow(clippy::too_many_arguments)] // the original reads these from the physical
    pub fn process(
        &mut self,
        water: &dyn WaterLevel,
        matrix: &Matrix,
        col: &ColModel,
        boat: Option<BoatModel>,
        touching_water: bool,
        buoyancy: f32,
        time_step: f32,
    ) -> Option<(Vec3, Vec3)> {
        self.num_steps = 2.0;
        let p = matrix.pos;
        self.water_level = water.height(p.x, p.y, p.z, touching_water)?;
        self.matrix = *matrix;
        self.pre_calc_setup(matrix.pos, col, boat, buoyancy);
        self.simple_calc(water);
        let force = self.force(time_step);
        // a boat always counts as in the water
        match (force, self.is_boat) {
            (Some(f), _) => Some(f),
            (None, true) => Some((Vec3::ZERO, Vec3::ZERO)),
            (None, false) => None,
        }
    }

    fn pre_calc_setup(&mut self, position: Vec3, col: &ColModel, boat: Option<BoatModel>, buoyancy: f32) {
        self.is_boat = boat.is_some();
        self.dim_min = col.bounding_box.min;
        self.dim_max = col.bounding_box.max;
        match boat {
            Some(BoatModel::Predator) | Some(BoatModel::Other) => {
                self.dim_max.y *= 0.9;
                self.dim_min.y *= 0.9;
            }
            Some(BoatModel::Speeder) => {
                self.dim_max.y *= 1.1;
                self.dim_min.y *= 0.9;
            }
            Some(BoatModel::Reefer) => self.dim_min.y *= 0.9,
            None => {}
        }
        self.step = (self.dim_max - self.dim_min) / self.num_steps;
        let s = self.step;
        self.step_ratio = if s.z > s.x && s.z > s.y {
            Vec3::new(s.x / s.z, s.y / s.z, 1.0)
        } else if s.y > s.x && s.y > s.z {
            Vec3::new(s.x / s.y, 1.0, s.z / s.y)
        } else {
            Vec3::new(1.0, s.y / s.x, s.z / s.x)
        };
        self.have_volume = false;
        self.num_partial_volumes = 1.0;
        self.volume_under_water = 0.0;
        self.impulse_point = Vec3::ZERO;
        self.position = position;
        self.position_z = Vec3::new(0.0, 0.0, position.z);
        self.buoyancy = buoyancy;
        self.water_level += self.water_level_inc;
    }

    /// The body is cut into 3x3 columns; each is sampled and summed.
    fn simple_calc(&mut self, water: &dyn WaterLevel) {
        let mut ix = 0;
        let mut x = self.dim_min.x;
        while x <= self.dim_max.x {
            let mut i = ix;
            let mut y = self.dim_min.y;
            while y <= self.dim_max.y {
                let (level, pos) = self.find_water_level(water, Vec3::new(x, y, 0.0));
                // the original reads past the table if rounding gives a fourth column
                let vol = if self.is_boat { BOAT_VOLUME.get(i).copied().unwrap_or(1.0) } else { 1.0 };
                if pos != Position::Above {
                    self.sum(level, vol);
                }
                i += 3;
                // a zero-size dimension would loop forever in the original
                if self.step.y <= 0.0 {
                    break;
                }
                y += self.step.y;
            }
            ix += 1;
            if self.step.x <= 0.0 {
                break;
            }
            x += self.step.x;
        }
        self.volume_under_water /= (self.dim_max.z - self.dim_min.z) * (self.num_steps + 1.0) * (self.num_steps + 1.0);
    }

    fn sum(&mut self, level: Vec3, vol_multiplier: f32) {
        let submerged = (level.z - self.dim_min.z).abs();
        // empty space in the box does not count
        let mut this = submerged - (1.0 - vol_multiplier);
        if this < 0.0 {
            return;
        }
        if self.is_boat {
            this *= vol_multiplier;
            if this < 0.5 {
                this = 2.0 * this * this;
            }
            if this < 1.0 {
                this *= this;
            }
            this *= this;
        }
        self.volume_under_water += this;
        let mut average = Vec3::new(level.x * self.step_ratio.x, level.y * self.step_ratio.y, (level.z + self.dim_min.z) / 2.0 * self.step_ratio.z);
        if self.flip_average {
            average = -average;
        }
        let fraction = 1.0 / self.num_partial_volumes;
        let remaining = 1.0 - fraction;
        self.impulse_point = self.impulse_point * remaining + average * this * fraction;
        self.num_partial_volumes += 1.0;
        self.have_volume = true;
    }

    /// Local x, y in; the level (local z) and where the body sits relative to it out.
    fn find_water_level(&self, water: &dyn WaterLevel, local: Vec3) -> (Vec3, Position) {
        let mut level = local;
        let mut pos = Position::In;
        let world = self.matrix.rotate_vector(local);
        // no water at a sample point reads as the surface far below, like an unset value
        let h = water.height(world.x + self.position.x, world.y + self.position.y, self.position.z, true).unwrap_or(0.0);
        level.z = h - (world.z + self.position_z.z);
        if level.z > self.dim_max.z {
            level.z = self.dim_max.z;
            pos = Position::Under;
        } else if level.z < self.dim_min.z {
            level.z = self.dim_min.z;
            pos = Position::Above;
        }
        (level, pos)
    }

    fn force(&self, time_step: f32) -> Option<(Vec3, Vec3)> {
        if !self.have_volume {
            return None;
        }
        let point = self.matrix.rotate_vector(self.impulse_point);
        Some((point, Vec3::new(0.0, 0.0, self.volume_under_water * self.buoyancy * time_step)))
    }
}

/// A world without water.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoWater;

impl WaterLevel for NoWater {
    fn height(&self, _: f32, _: f32, _: f32, _: bool) -> Option<f32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::Box3;

    #[derive(Debug)]
    struct Flat(f32);
    impl WaterLevel for Flat {
        fn height(&self, _: f32, _: f32, _: f32, _: bool) -> Option<f32> {
            Some(self.0)
        }
    }

    #[derive(Debug)]
    struct Dry;
    impl WaterLevel for Dry {
        fn height(&self, _: f32, _: f32, _: f32, _: bool) -> Option<f32> {
            None
        }
    }

    fn crate_col() -> ColModel {
        ColModel { bounding_box: Box3 { min: Vec3::splat(-1.0), max: Vec3::splat(1.0), surface: 0, piece: 0 }, ..Default::default() }
    }

    fn at(z: f32) -> Matrix {
        Matrix::from_translation(Vec3::new(0.0, 0.0, z))
    }

    #[test]
    fn no_water_no_push() {
        let mut b = Buoyancy::default();
        assert!(b.process(&Dry, &at(0.0), &crate_col(), None, false, 1.0, 1.0).is_none());
        // water far below: the body is dry and nothing is produced
        assert!(b.process(&Flat(-50.0), &at(0.0), &crate_col(), None, false, 1.0, 1.0).is_none());
    }

    #[test]
    fn a_fully_submerged_body_gets_the_full_push_straight_up() {
        let mut b = Buoyancy::default();
        let (point, impulse) = b.process(&Flat(50.0), &at(0.0), &crate_col(), None, false, 2.0, 1.0).unwrap();
        // nine columns, each 2 deep, over (box height 2 * 9): volume 1.0, times buoyancy 2
        assert!(impulse.x == 0.0 && impulse.y == 0.0);
        assert!((impulse.z - 2.0).abs() < 1e-5, "{impulse:?}");
        assert!(impulse.z > 0.0 && point.x.abs() < 1e-5 && point.y.abs() < 1e-5);
    }

    #[test]
    fn deeper_means_more_push_and_a_boat_always_reports() {
        let mut b = Buoyancy::default();
        let half = b.process(&Flat(0.0), &at(0.0), &crate_col(), None, false, 1.0, 1.0).unwrap().1.z;
        let full = b.process(&Flat(1.0), &at(0.0), &crate_col(), None, false, 1.0, 1.0).unwrap().1.z;
        assert!(full > half && half > 0.0);
        // a boat above the water still returns a (zero) result
        let r = b.process(&Flat(-50.0), &at(0.0), &crate_col(), Some(BoatModel::Other), false, 1.0, 1.0);
        assert_eq!(r, Some((Vec3::ZERO, Vec3::ZERO)));
    }
}
