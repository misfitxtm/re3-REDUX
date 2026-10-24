//! Physics state and the force/impulse maths (`CPhysical`).
//! Methods take the entity's matrix and the frame step explicitly instead of reaching for globals.
use super::{Entity, EntityRef, EntityType};
use crate::math::{cross, magnitude_squared, Matrix};
use glam::Vec3;

pub const MAX_COLLISION_RECORDS: usize = 6;
pub const GRAVITY: f32 = 0.008;

/// Where a physical entity sits in a sector list (`CEntryInfoNode`): sector and list index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryInfo {
    pub sector: (usize, usize),
    pub list: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysFlags {
    pub is_heavy: bool,
    pub affected_by_gravity: bool,
    pub infinite_mass: bool,
    pub is_in_water: bool,
    pub hit_by_train: bool,
    pub skip_line_col: bool,
}

impl Default for PhysFlags {
    fn default() -> Self {
        Self { is_heavy: false, affected_by_gravity: true, infinite_mass: false, is_in_water: false, hit_by_train: false, skip_line_col: false }
    }
}

#[derive(Debug, Clone)]
pub struct Physical {
    pub audio_entity_id: i32,
    /// Treadable (road) the entity last touched: [car, ped].
    pub treadable: [Option<EntityRef>; 2],
    pub last_time_collided: u32,
    /// Velocity.
    pub move_speed: Vec3,
    /// Angular velocity.
    pub turn_speed: Vec3,
    pub move_friction: Vec3,
    pub turn_friction: Vec3,
    pub move_speed_avg: Vec3,
    pub turn_speed_avg: Vec3,
    pub mass: f32,
    /// Moment of inertia.
    pub turn_mass: f32,
    pub force_multiplier: f32,
    pub air_resistance: f32,
    pub elasticity: f32,
    pub buoyancy: f32,
    pub centre_of_mass: Vec3,
    pub entry_info: Vec<EntryInfo>,
    pub in_moving_list: bool,
    pub static_frames: u8,
    pub collision_records: Vec<EntityRef>,
    pub is_vehicle_being_shifted: bool,
    pub distance_travelled: f32,
    pub damage_impulse: f32,
    pub damage_entity: Option<EntityRef>,
    pub damage_normal: Vec3,
    pub damage_piece_type: i16,
    pub flags: PhysFlags,
    pub surface_touched: u8,
    pub zone_level: i8,
}

impl Default for Physical {
    /// `CPhysical::CPhysical`.
    fn default() -> Self {
        Self {
            audio_entity_id: -5,
            treadable: [None; 2],
            last_time_collided: 0,
            move_speed: Vec3::ZERO,
            turn_speed: Vec3::ZERO,
            move_friction: Vec3::ZERO,
            turn_friction: Vec3::ZERO,
            move_speed_avg: Vec3::ZERO,
            turn_speed_avg: Vec3::ZERO,
            mass: 0.0,
            turn_mass: 0.0,
            force_multiplier: 1.0,
            air_resistance: 0.0,
            elasticity: 0.0,
            buoyancy: 0.0,
            centre_of_mass: Vec3::ZERO,
            entry_info: vec![],
            in_moving_list: false,
            static_frames: 0,
            collision_records: vec![],
            is_vehicle_being_shifted: false,
            distance_travelled: 0.0,
            damage_impulse: 0.0,
            damage_entity: None,
            damage_normal: Vec3::ZERO,
            damage_piece_type: 0,
            flags: PhysFlags::default(),
            surface_touched: 0,
            zone_level: 0,
        }
    }
}

impl Physical {
    /// `GetSpeed(r)`: velocity of the point `r` relative to the centre.
    pub fn speed_at(&self, r: Vec3) -> Vec3 {
        self.move_speed + self.move_friction + cross(self.turn_friction + self.turn_speed, r)
    }

    pub fn speed(&self) -> Vec3 {
        self.speed_at(Vec3::ZERO)
    }

    /// Effective mass seen by an impulse along `dir` applied at `pos`.
    pub fn mass_at(&self, pos: Vec3, dir: Vec3) -> f32 {
        1.0 / (magnitude_squared(cross(pos, dir)) / self.turn_mass + 1.0 / self.mass)
    }

    pub fn mass_tweak(&self, pos: Vec3, dir: Vec3, t: f32) -> f32 {
        1.0 / (magnitude_squared(cross(pos, dir)) / (self.turn_mass * t) + 1.0 / (self.mass * t))
    }

    pub fn apply_move_speed(&self, m: &mut Matrix, step: f32) {
        m.pos += self.move_speed * step;
    }

    /// Moves the matrix axes by their angular speed; this denormalises the matrix, as in the original.
    pub fn apply_turn_speed(&self, m: &mut Matrix, step: f32) {
        let turn = self.turn_speed * step;
        m.right += cross(turn, m.right);
        m.forward += cross(turn, m.forward);
        m.up += cross(turn, m.up);
    }

    /// "Force" is an impulse here.
    pub fn apply_move_force(&mut self, j: Vec3) {
        self.move_speed += j * (1.0 / self.mass);
    }

    /// `j` is the impulse, `p` the point relative to the model centre where it acts.
    pub fn apply_turn_force(&mut self, m: &Matrix, j: Vec3, p: Vec3) {
        let com = m.rotate_vector(self.centre_of_mass);
        let turn_impulse = cross(p - com, j);
        self.turn_speed += turn_impulse * (1.0 / self.turn_mass);
    }

    pub fn apply_friction_move_force(&mut self, j: Vec3) {
        self.move_friction += j * (1.0 / self.mass);
    }

    pub fn apply_friction_turn_force(&mut self, m: &Matrix, j: Vec3, p: Vec3) {
        let com = m.rotate_vector(self.centre_of_mass);
        let turn_impulse = cross(p - com, j);
        self.turn_friction += turn_impulse * (1.0 / self.turn_mass);
    }

    /// `spring_ratio`: 1.0 fully extended, 0.0 fully compressed.
    #[allow(clippy::too_many_arguments)] // mirrors the C++ signature (plus matrix and step)
    pub fn apply_spring_collision(&mut self, m: &Matrix, step: f32, spring_const: f32, dir: Vec3, point: Vec3, spring_ratio: f32, bias: f32) -> bool {
        let compression = 1.0 - spring_ratio;
        if compression > 0.0 {
            let step = step.min(3.0);
            let impulse = -GRAVITY * self.mass * step * spring_const * compression * bias * 2.0;
            self.apply_move_force(dir * impulse);
            self.apply_turn_force(m, dir * impulse, point);
        }
        true
    }

    pub fn apply_spring_dampening(&mut self, m: &Matrix, step: f32, damping: f32, dir: Vec3, point: Vec3, speed: Vec3) -> bool {
        let speed_a = crate::math::dot(speed, dir);
        let speed_b = crate::math::dot(self.speed_at(point), dir);
        // FIX_BUGS: nothing to damp
        if speed_b == 0.0 {
            return true;
        }
        let step = step.min(3.0);
        let mut impulse = -damping * (speed_a + speed_b) / 2.0 * self.mass * step * 0.53;
        let a = (self.turn_mass / ((magnitude_squared(point) + 1.0) * 2.0 * self.mass)).min(1.0);
        let b = (impulse / (speed_b * self.mass)).abs();
        if a < b {
            impulse *= a / b;
        }
        self.apply_move_force(dir * impulse);
        self.apply_turn_force(m, dir * impulse, point);
        true
    }

    pub fn apply_gravity(&mut self, step: f32) {
        if self.flags.affected_by_gravity {
            self.move_speed.z -= GRAVITY * step;
        }
    }

    /// Folds the accumulated friction impulses into the speeds.
    pub fn apply_friction(&mut self) {
        self.move_speed += self.move_friction;
        self.turn_speed += self.turn_friction;
        self.move_friction = Vec3::ZERO;
        self.turn_friction = Vec3::ZERO;
    }

    pub fn apply_air_resistance(&mut self, step: f32) {
        if self.air_resistance > 0.1 {
            let f = self.air_resistance.powf(step);
            self.move_speed *= f;
            self.turn_speed *= f;
        } else {
            let f = (1.0 / (self.air_resistance * 0.5 * magnitude_squared(self.move_speed) + 1.0).abs()).powf(step);
            self.move_speed *= f;
            self.turn_speed *= 0.99;
        }
    }

    /// `AddCollisionRecord`'s bookkeeping part (alarm and treadable effects are handled by the caller).
    pub fn add_collision_record(&mut self, ent: EntityRef, time_ms: u32) {
        if self.collision_records.contains(&ent) {
            return;
        }
        if self.collision_records.len() < MAX_COLLISION_RECORDS {
            self.collision_records.push(ent);
        }
        self.last_time_collided = time_ms;
    }

    pub fn has_collided_with(&self, use_records: bool, ent: EntityRef) -> bool {
        use_records && self.collision_records.contains(&ent)
    }

    pub fn remove_refs_to(&mut self, ent: EntityRef) {
        self.collision_records.retain(|e| *e != ent);
        for t in &mut self.treadable {
            if *t == Some(ent) {
                *t = None;
            }
        }
        if self.damage_entity == Some(ent) {
            self.damage_entity = None;
        }
    }

    pub fn set_damaged_piece_record(&mut self, piece: u16, impulse: f32, entity: EntityRef, dir: Vec3) {
        self.damage_piece_type = piece as i16;
        self.damage_impulse = impulse;
        self.damage_entity = Some(entity);
        self.damage_normal = dir;
    }
}

/// The start-of-frame part of `CPhysical::ProcessControl`: resets per-frame state, lets objects
/// and ped-physics-free peds go to sleep, then applies gravity, friction and air resistance.
/// Returns false if the entity just became static and skips the rest of the frame.
pub fn process_control(e: &mut Entity, p: &mut Physical, step: f32) -> bool {
    if e.ty != EntityType::Ped {
        p.flags.is_in_water = false;
    }
    e.flags.has_contacted = false;
    e.flags.is_in_safe_position = false;
    e.flags.was_postponed = false;
    e.flags.has_hit_wall = false;
    if e.status == super::status::SIMPLE {
        return true;
    }
    p.collision_records.clear();
    e.flags.has_collided = false;
    p.damage_piece_type = 0;
    p.damage_impulse = 0.0;
    p.damage_entity = None;
    if !e.flags.is_stuck && (e.ty == EntityType::Object || (e.ty == EntityType::Ped && !e.flags.ped_physics)) {
        p.move_speed_avg = (p.move_speed_avg + p.move_speed) / 2.0;
        p.turn_speed_avg = (p.turn_speed_avg + p.turn_speed) / 2.0;
        let s = step * 0.003;
        if magnitude_squared(p.move_speed_avg) < s * s && magnitude_squared(p.turn_speed_avg) < s * s {
            p.static_frames += 1;
            if p.static_frames > 10 {
                p.static_frames = 10;
                e.flags.is_static = true;
                p.move_speed = Vec3::ZERO;
                p.turn_speed = Vec3::ZERO;
                p.move_friction = p.move_speed;
                p.turn_friction = p.turn_speed;
                return false;
            }
        } else {
            p.static_frames = 0;
        }
    }
    p.apply_gravity(step);
    p.apply_friction();
    p.apply_air_resistance(step);
    true
}

/// `UnsetIsInSafePosition`: undo this frame's motion.
pub fn unset_is_in_safe_position(e: &mut Entity, p: &mut Physical, step: f32) {
    p.move_speed = -p.move_speed;
    p.turn_speed = -p.turn_speed;
    p.apply_turn_speed(&mut e.matrix, step);
    p.apply_move_speed(&mut e.matrix, step);
    p.move_speed = -p.move_speed;
    p.turn_speed = -p.turn_speed;
    e.flags.is_in_safe_position = false;
}

#[cfg(test)]
mod tests;
