//! `CBoat`: boats. State, pad input, water resistance and the wake trail.
use super::base::{VehicleData, VehicleType};
use super::handling::HandlingData;
use super::env::Pad;
use crate::entity::physical::Physical;
use crate::entity::EntityRef;
use crate::general::Rng;
use crate::math::{dot, Matrix};
use glam::{Vec2, Vec3};

pub const INVALID_ORIENTATION: f32 = -9999.99;
pub const MAX_WAKE_LENGTH: f32 = 50.0;
pub const MIN_WAKE_INTERVAL: f32 = 1.0;
pub const WAKE_LIFETIME: f32 = 400.0;
const WAKE_POINTS: usize = 32;

/// `eBoatNodes`.
pub mod node {
    pub const MOVING: usize = 1;
    pub const RUDDER: usize = 2;
    pub const WINDSCREEN: usize = 3;
}

#[derive(Debug, Clone)]
pub struct BoatData {
    pub thrust_z: f32,
    pub thrust_y: f32,
    pub move_res: Vec3,
    pub turn_res: Vec3,
    pub moving_rotation: f32,
    /// Position of the "moving" node (the propeller), if the clump has one.
    pub moving_node: Option<Vec3>,
    pub in_water: bool,
    pub propeller_in_water: bool,
    pub is_anchored: bool,
    pub orientation: f32,
    pub damage: f32,
    pub set_on_fire_entity: Option<EntityRef>,
    pub accelerate: f32,
    pub brake: f32,
    pub steering_left_right: f32,
    pub pad_id: u8,
    pub volume_under_water: f32,
    pub buoyance_point: Vec3,
    pub prev_volume_under_water: f32,
    pub delta_volume_under_water: i16,
    pub num_wake_points: u16,
    pub wake_points: [Vec2; WAKE_POINTS],
    pub wake_point_life_time: [f32; WAKE_POINTS],
}

pub struct NewBoat {
    pub vehicle: VehicleData,
    pub physical: Physical,
}

impl BoatData {
    fn new() -> Self {
        Self {
            thrust_z: 0.25,
            thrust_y: 0.35,
            move_res: Vec3::new(0.7, 0.998, 0.999),
            turn_res: Vec3::new(0.85, 0.96, 0.96),
            moving_rotation: 0.0,
            moving_node: None,
            in_water: true,
            propeller_in_water: true,
            is_anchored: true,
            orientation: INVALID_ORIENTATION,
            damage: 0.0,
            set_on_fire_entity: None,
            accelerate: 0.0,
            brake: 0.0,
            steering_left_right: 0.0,
            pad_id: 0,
            volume_under_water: 7.0,
            buoyance_point: Vec3::ZERO,
            prev_volume_under_water: 7.0,
            delta_volume_under_water: 0,
            num_wake_points: 0,
            wake_points: [Vec2::ZERO; WAKE_POINTS],
            wake_point_life_time: [0.0; WAKE_POINTS],
        }
    }

    /// `ProcessControlInputs`.
    pub fn process_control_inputs(&mut self, veh: &mut VehicleData, h: &HandlingData, pad: &Pad) {
        self.pad_id = 0;
        self.brake += (pad.state.brake as f32 / 255.0 - self.brake) * 0.1;
        self.brake = self.brake.clamp(0.0, 1.0);
        if self.brake < 0.05 {
            self.brake = 0.0;
            self.accelerate += (pad.state.accelerate as f32 / 255.0 - self.accelerate) * 0.1;
            self.accelerate = self.accelerate.clamp(0.0, 1.0);
        } else {
            self.accelerate = -self.brake * 0.2;
        }
        self.steering_left_right += (-(pad.state.steering_left_right as f32) / 128.0 - self.steering_left_right) * 0.2;
        self.steering_left_right = self.steering_left_right.clamp(-1.0, 1.0);
        let sq = if self.steering_left_right < 0.0 { -(self.steering_left_right * self.steering_left_right) } else { self.steering_left_right * self.steering_left_right };
        veh.steer_angle = h.steering_lock * (sq * std::f32::consts::PI / 180.0);
        veh.gas_pedal = self.accelerate;
    }

    /// `ApplyWaterResistance`.
    pub fn apply_water_resistance(&self, phys: &mut Physical, m: &Matrix, step: f32) {
        let fwd_speed = dot(phys.move_speed, m.forward);
        let resistance = 0.001 * (self.volume_under_water * self.volume_under_water) * phys.mass;
        let magic = ((fwd_speed * fwd_speed + 0.05) * resistance + 1.0).abs();
        let fx = (self.move_res.x / magic).powf(0.5 * step);
        let fy = (self.move_res.y / magic).powf(0.5 * step);
        let fz = (self.move_res.z / magic).powf(0.5 * step);
        let mut local = m.rotate_vector_inverse(phys.move_speed);
        local.x *= fx;
        local.y *= fy;
        local.z *= fz;
        let force = (fy - 1.0) * local.y * phys.mass;
        phys.move_speed = m.rotate_vector(local);
        phys.apply_turn_force(m, m.forward * force, -m.up);
        if phys.move_speed.z > 0.0 {
            phys.move_speed.z *= fz;
        } else {
            phys.move_speed.z *= (1.0 - fz) * 0.5 + fz;
        }
    }

    /// `PruneWakeTrail`: ages the wake points and drops dead ones.
    pub fn prune_wake_trail(&mut self, step: f32) {
        let mut i = 0;
        while i < WAKE_POINTS {
            if self.wake_point_life_time[i] <= 0.0 {
                break;
            }
            if self.wake_point_life_time[i] <= step {
                self.wake_point_life_time[i] = 0.0;
                break;
            }
            self.wake_point_life_time[i] -= step;
            i += 1;
        }
        self.num_wake_points = i as u16;
    }

    /// `AddWakePoint`: remembers where the boat has been (`pos` is the boat's position).
    pub fn add_wake_point(&mut self, pos: Vec3, point: Vec3) {
        if self.wake_point_life_time[0] > 0.0 {
            if (Vec2::new(pos.x, pos.y) - self.wake_points[0]).length_squared() < 1.0 {
                let mut i = (self.num_wake_points as usize).min(WAKE_POINTS - 1);
                while i != 0 {
                    self.wake_points[i] = self.wake_points[i - 1];
                    self.wake_point_life_time[i] = self.wake_point_life_time[i - 1];
                    i -= 1;
                }
                self.wake_points[0] = Vec2::new(point.x, point.y);
                self.wake_point_life_time[0] = 400.0;
                if (self.num_wake_points as usize) < WAKE_POINTS {
                    self.num_wake_points += 1;
                }
            }
        } else {
            self.wake_points[0] = Vec2::new(point.x, point.y);
            self.wake_point_life_time[0] = 400.0;
            self.num_wake_points = 1;
        }
    }

    /// `IsVertexAffectedByWake`: how strongly the wake of this boat lifts the water at `v`.
    pub fn vertex_affected_by_wake(&self, v: Vec3) -> f32 {
        const SHAPE_LENGTH: f32 = 0.4;
        const SHAPE_TIME: f32 = 0.05;
        const RANGE_MULT: f32 = 0.75;
        const TIME_MULT: f32 = 1.0 / WAKE_LIFETIME;
        for i in 0..self.num_wake_points as usize {
            let max_dist = (WAKE_LIFETIME - self.wake_point_life_time[i]) * SHAPE_TIME + i as f32 * SHAPE_LENGTH;
            let d = self.wake_points[i] - Vec2::new(v.x, v.y);
            let dist = d.length_squared();
            if dist < max_dist * max_dist {
                return 1.0 - (RANGE_MULT * (dist / (max_dist * max_dist)).sqrt() + (WAKE_LIFETIME - self.wake_point_life_time[i]) * TIME_MULT).min(1.0);
            }
        }
        0.0
    }
}

/// `CBoat::CBoat`.
pub fn new_boat(model: i16, created_by: u8, time_ms: u32, handling_id: usize, h: &HandlingData, mi_data: &mut crate::modelinfo::VehicleData, rng: &mut Rng) -> NewBoat {
    let mut v = VehicleData::new(created_by, time_ms, VehicleType::Boat, rng);
    v.handling = handling_id;
    v.num_max_passengers = super::automobile::max_passengers(model, mi_data.num_doors);
    let (c1, c2) = super::automobile::choose_vehicle_colour(mi_data, None);
    v.current_colour1 = c1;
    v.current_colour2 = c2;
    v.ammo_in_clip = 20;
    let mut b = BoatData::new();
    b.moving_node = mi_data.node_positions[node::MOVING - 1];
    v.sub = super::base::VehicleSub::Boat(Box::new(b));
    let physical = Physical {
        mass: h.mass,
        turn_mass: h.turn_mass / 2.0,
        centre_of_mass: h.centre_of_mass,
        air_resistance: h.dimension.x * h.dimension.z / h.mass,
        elasticity: 0.1,
        buoyancy: h.buoyancy,
        flags: crate::entity::physical::PhysFlags { is_in_water: true, ..Default::default() },
        ..Default::default()
    };
    NewBoat { vehicle: v, physical }
}
