//! `CHeli`: police and Catalina's helicopters. State and the path tables; the per-frame
//! control and the manager live in `world/helis.rs`.
use super::base::{VehicleData, VehicleType};
use crate::entity::physical::Physical;
use crate::entity::EntityRef;
use crate::general::Rng;
use glam::Vec3;

pub const NUM_HELIS: usize = 4;
pub const HELI_RANDOM0: usize = 0;
pub const HELI_RANDOM1: usize = 1;
pub const HELI_SCRIPT: usize = 2;
pub const HELI_CATALINA: usize = 3;

/// `eHeliNodes`.
pub mod node {
    pub const CHASSIS: usize = 1;
    pub const TOPROTOR: usize = 2;
    pub const BACKROTOR: usize = 3;
    pub const TAIL: usize = 4;
    pub const TOPKNOT: usize = 5;
    pub const SKID_LEFT: usize = 6;
    pub const SKID_RIGHT: usize = 7;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeliStatus {
    #[default]
    Hover,
    ChasePlayer,
    FlyAway,
    ShotDown,
    Hover2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeliType {
    #[default]
    Random,
    Script,
    Catalina,
}

pub const CATALINA_TARGET: [[f32; 3]; 7] = [
    [-478.0, 227.0, 77.0],
    [-677.0, 206.0, 66.0],
    [-907.0, 210.0, 60.0],
    [-1095.0, 242.0, 53.0],
    [-1152.0, 278.0, 51.0],
    [-1161.0, 341.0, 46.0],
    [-1161.0, 341.0, 30.0],
];
pub const DAM_PATH: [[f32; 3]; 6] = [
    [-1191.0, 350.0, 42.0],
    [-1176.0, 388.0, 37.0],
    [-1128.0, 429.0, 28.0],
    [-1072.0, 447.0, 28.0],
    [-1007.0, 449.0, 31.0],
    [-971.0, 416.0, 33.0],
];
pub const SHORT_PATH: [[f32; 3]; 4] = [[-974.0, 340.0, 41.0], [-1036.0, 312.0, 38.0], [-1112.0, 317.0, 32.0], [-1173.0, 294.0, 39.0]];
pub const LONG_PATH: [[f32; 3]; 7] = [
    [-934.0, 371.0, 57.0],
    [-905.0, 362.0, 90.0],
    [-906.0, 488.0, 105.0],
    [-1063.0, 548.0, 100.0],
    [-1204.0, 451.0, 81.0],
    [-1233.0, 346.0, 79.0],
    [-1207.0, 308.0, 70.0],
];

#[derive(Debug, Clone)]
pub struct HeliData {
    pub status: HeliStatus,
    pub search_light_x: f32,
    pub search_light_y: f32,
    pub explosion_timer: u32,
    pub rotation: f32,
    pub angular_speed: f32,
    pub target_z: f32,
    pub search_light_intensity: f32,
    pub heli_id: i8,
    pub heli_type: HeliType,
    pub path_state: i8,
    pub search_light_history_x: [f32; 6],
    pub search_light_history_y: [f32; 6],
    pub search_light_timer: u32,
    pub shoot_timer: u32,
    pub last_shot_time: u32,
    pub bullet_damage: u32,
    pub rotor_rotation: f32,
    pub heli_dust_z: [f32; 8],
    pub police_shout_timer: u32,
    pub target_offset: f32,
    pub test_right: bool,
    /// Local positions of the nodes the clump has (`eHeliNodes`, index 0 unused).
    pub node_positions: [Option<Vec3>; 8],
}

pub struct NewHeli {
    pub vehicle: VehicleData,
    pub physical: Physical,
}

/// `CHeli::CHeli`.
pub fn new_heli(time_ms: u32, created_by: u8, handling_id: usize, mi_data: &crate::modelinfo::VehicleData, rng: &mut Rng) -> NewHeli {
    let mut v = VehicleData::new(created_by, time_ms, VehicleType::Heli, rng);
    v.handling = handling_id;
    let mut node_positions = [None; 8];
    for (i, p) in mi_data.node_positions.iter().take(7).enumerate() {
        node_positions[i + 1] = *p;
    }
    v.sub = super::base::VehicleSub::Heli(Box::new(HeliData {
        status: HeliStatus::Hover,
        search_light_x: 0.0,
        search_light_y: 0.0,
        explosion_timer: 0,
        rotation: 0.0,
        angular_speed: 0.0,
        target_z: 0.0,
        search_light_intensity: 0.0,
        heli_id: 0,
        heli_type: HeliType::Random,
        path_state: 0,
        search_light_history_x: [0.0; 6],
        search_light_history_y: [0.0; 6],
        search_light_timer: time_ms,
        shoot_timer: 0,
        last_shot_time: 0,
        bullet_damage: 0,
        rotor_rotation: 0.0,
        heli_dust_z: [-50.0; 8],
        police_shout_timer: time_ms,
        target_offset: 0.0,
        test_right: true,
        node_positions,
    }));
    let physical = Physical { mass: 100_000_000.0, turn_mass: 100_000_000.0, air_resistance: 0.9994, elasticity: 0.05, ..Default::default() };
    NewHeli { vehicle: v, physical }
}

/// The statics of `CHeli`.
#[derive(Debug, Clone, Default)]
pub struct HeliManager {
    pub helis: [Option<EntityRef>; NUM_HELIS],
    pub num_random_helis: i16,
    pub test_for_new_random_helis_timer: u32,
    pub catalina_heli_on: bool,
    pub catalina_has_been_shot_down: bool,
    pub script_heli_on: bool,
    /// The function-static `PathPoint` of `ProcessControl`.
    pub path_point: i32,
    /// `nFrameGen` of the explosion debris.
    pub frame_gen: i32,
}
