//! `CAutomobile`: cars. State, construction, suspension setup, doors and the small helpers.
//! The per-frame control lives in [`control`], damage in [`damage`].
//! Clump nodes (`RwFrame`s) are replaced by the node positions the model info carries;
//! visibility and animation of the parts belongs to the renderer.
// The per-wheel loops index several parallel arrays, as the original does.
#![allow(clippy::needless_range_loop)]
use super::base::{CarLock, VehicleData, VehicleType, WheelState};
use super::damage::{door_status, DamageManager};
use super::door::Door;
use super::handling::{flags as hflags, HandlingData};
use crate::collision::{ColModel, ColPoint, Line};
use crate::entity::physical::Physical;
use crate::entity::EntityRef;
use crate::general::Rng;
use crate::modelinfo::indices_gta3::mi;
use glam::Vec3;

pub mod control;
pub mod damage;

/// `eCarNodes`.
pub mod node {
    pub const WHEEL_RF: usize = 1;
    pub const WHEEL_RM: usize = 2;
    pub const WHEEL_RB: usize = 3;
    pub const WHEEL_LF: usize = 4;
    pub const WHEEL_LM: usize = 5;
    pub const WHEEL_LB: usize = 6;
    pub const BUMP_FRONT: usize = 7;
    pub const BUMP_REAR: usize = 8;
    pub const WING_RF: usize = 9;
    pub const WING_RR: usize = 10;
    pub const DOOR_RF: usize = 11;
    pub const DOOR_RR: usize = 12;
    pub const WING_LF: usize = 13;
    pub const WING_LR: usize = 14;
    pub const DOOR_LF: usize = 15;
    pub const DOOR_LR: usize = 16;
    pub const BONNET: usize = 17;
    pub const BOOT: usize = 18;
    pub const WINDSCREEN: usize = 19;
    pub const COUNT: usize = 20;
}

/// `eDoors`.
pub mod door_id {
    pub const BONNET: usize = 0;
    pub const BOOT: usize = 1;
    pub const FRONT_LEFT: usize = 2;
    pub const FRONT_RIGHT: usize = 3;
    pub const REAR_LEFT: usize = 4;
    pub const REAR_RIGHT: usize = 5;
}

pub const CARWHEEL_FRONT_LEFT: usize = 0;
pub const CARWHEEL_REAR_LEFT: usize = 1;
pub const CARWHEEL_FRONT_RIGHT: usize = 2;
pub const CARWHEEL_REAR_RIGHT: usize = 3;

/// `CAR_DOOR_FLAG_*`.
pub mod door_flag {
    pub const LF: i8 = 1;
    pub const LR: i8 = 2;
    pub const RF: i8 = 4;
    pub const RR: i8 = 8;
}

/// `GetCarDoorFlag`.
pub fn car_door_flag(carnode: usize) -> i8 {
    match carnode {
        node::DOOR_LF => door_flag::LF,
        node::DOOR_LR => door_flag::LR,
        node::DOOR_RF => door_flag::RF,
        node::DOOR_RR => door_flag::RR,
        _ => 0,
    }
}

/// `eBombType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BombType {
    #[default]
    None,
    Timed,
    OnIgnition,
    Remote,
    TimedActive,
    OnIgnitionActive,
}

/// `CAR_PIECE_*`: the part id in a collision point's piece.
pub mod piece {
    pub const BONNET: u16 = 1;
    pub const BOOT: u16 = 2;
    pub const BUMP_FRONT: u16 = 3;
    pub const BUMP_REAR: u16 = 4;
    pub const DOOR_LF: u16 = 5;
    pub const DOOR_RF: u16 = 6;
    pub const DOOR_LR: u16 = 7;
    pub const DOOR_RR: u16 = 8;
    pub const WING_LF: u16 = 9;
    pub const WING_RF: u16 = 10;
    pub const WING_LR: u16 = 11;
    pub const WING_RR: u16 = 12;
    pub const WHEEL_LF: u16 = 13;
    pub const WHEEL_RF: u16 = 14;
    pub const WHEEL_LR: u16 = 15;
    pub const WHEEL_RR: u16 = 16;
    pub const WINDSCREEN: u16 = 17;
}

#[derive(Debug, Clone, Default)]
pub struct AutomobileFlags {
    pub taxi_light: bool,
    pub driver_last_frame: bool,
    pub fixed_colour: bool,
    pub big_wheels: bool,
    /// No damage for non-player peds.
    pub water_tight: bool,
    pub not_damaged_upside_down: bool,
    pub more_resistant_to_damage: bool,
}

#[derive(Debug, Clone)]
pub struct AutomobileData {
    pub damage: DamageManager,
    pub doors: [Door; 6],
    /// Local position of each node the clump has (`m_aCarNodes != nil`).
    pub node_positions: [Option<Vec3>; node::COUNT],
    pub wheel_col_points: [ColPoint; 4],
    pub suspension_spring_ratio_prev: [f32; 4],
    /// Set to 4 when the wheel touches the ground, then counted down.
    pub wheel_timer: [f32; 4],
    pub wheel_skidmark_muddy: [bool; 4],
    pub wheel_skidmark_bloody: [bool; 4],
    pub wheel_rotation: [f32; 4],
    pub wheel_position: [f32; 4],
    pub wheel_speed: [f32; 4],
    pub bomb_type: BombType,
    pub flags: AutomobileFlags,
    pub bomb_rigger: Option<EntityRef>,
    pub hydraulic_state: u16,
    pub bus_door_timer_end: u32,
    pub bus_door_timer_start: u32,
    pub suspension_spring_length: [f32; 4],
    pub suspension_line_length: [f32; 4],
    pub height_above_road: f32,
    pub traction: f32,
    pub velocity_change_for_audio: f32,
    pub random_values: [f32; 6],
    pub fire_blow_up_timer: f32,
    /// Physicals touching the wheels.
    pub ground_physical: [Option<EntityRef>; 4],
    /// From the ground object to the contact point.
    pub ground_offset: [Vec3; 4],
    pub set_on_fire_entity: Option<EntityRef>,
    pub weapon_door_timer_left: f32,
    pub weapon_door_timer_right: f32,
    pub car_gun_lr: f32,
    pub car_gun_ud: f32,
    pub propeller_rotation: f32,
    pub wheels_on_ground: u8,
    pub drive_wheels_on_ground: u8,
    pub drive_wheels_on_ground_prev: u8,
    pub gas_pedal_audio: f32,
    pub wheel_state: [WheelState; 4],
}

impl AutomobileData {
    /// The members `CAutomobile::CAutomobile` sets, apart from the base vehicle.
    fn new(handling: &HandlingData, mi: &crate::modelinfo::VehicleData, all_taxi_lights: bool, rng: &mut Rng) -> Self {
        use std::f32::consts::{FRAC_PI_2 as HALFPI, PI};
        let f = handling.flags;
        let (is_van, is_bus) = (f & hflags::IS_VAN != 0, f & hflags::IS_BUS != 0);
        let mut doors = [Door::default(); 6];
        if is_bus {
            doors[door_id::FRONT_LEFT].init(-HALFPI, 0.0, 0, 2);
            doors[door_id::FRONT_RIGHT].init(0.0, HALFPI, 1, 2);
        } else {
            doors[door_id::FRONT_LEFT].init(-PI * 0.4, 0.0, 0, 2);
            doors[door_id::FRONT_RIGHT].init(0.0, PI * 0.4, 1, 2);
        }
        if is_van {
            doors[door_id::REAR_LEFT].init(-HALFPI, 0.0, 1, 2);
            doors[door_id::REAR_RIGHT].init(0.0, HALFPI, 0, 2);
        } else {
            doors[door_id::REAR_LEFT].init(-PI * 0.4, 0.0, 0, 2);
            doors[door_id::REAR_RIGHT].init(0.0, PI * 0.4, 1, 2);
        }
        if f & hflags::REV_BONNET != 0 {
            doors[door_id::BONNET].init(-PI * 0.3, 0.0, 1, 0);
        } else {
            doors[door_id::BONNET].init(0.0, PI * 0.3, 1, 0);
        }
        if f & hflags::HANGING_BOOT != 0 {
            doors[door_id::BOOT].init(-PI * 0.4, 0.0, 0, 0);
        } else if f & hflags::TAILGATE_BOOT != 0 {
            doors[door_id::BOOT].init(0.0, HALFPI, 1, 0);
        } else {
            doors[door_id::BOOT].init(-PI * 0.3, 0.0, 1, 0);
        }
        let mut damage = DamageManager::default();
        if f & hflags::NO_DOORS != 0 {
            for d in [door_id::FRONT_LEFT, door_id::FRONT_RIGHT, door_id::REAR_LEFT, door_id::REAR_RIGHT] {
                damage.set_door_status(d, door_status::MISSING);
            }
        }
        let mut random_values = [0.0; 6];
        for v in &mut random_values {
            *v = rng.range_f32(-0.15, 0.15);
        }
        let mut node_positions = [None; node::COUNT];
        for (i, p) in mi.node_positions.iter().enumerate() {
            node_positions[i + 1] = *p;
        }
        Self {
            damage,
            doors,
            node_positions,
            wheel_col_points: [ColPoint::default(); 4],
            suspension_spring_ratio_prev: [1.0; 4],
            wheel_timer: [0.0; 4],
            wheel_skidmark_muddy: [false; 4],
            wheel_skidmark_bloody: [false; 4],
            wheel_rotation: [0.0; 4],
            wheel_position: [0.0; 4],
            wheel_speed: [0.0; 4],
            bomb_type: BombType::None,
            flags: AutomobileFlags { taxi_light: all_taxi_lights, ..Default::default() },
            bomb_rigger: None,
            hydraulic_state: 0,
            bus_door_timer_end: 0,
            bus_door_timer_start: 0,
            suspension_spring_length: [0.0; 4],
            suspension_line_length: [0.0; 4],
            height_above_road: 0.0,
            traction: 1.0,
            velocity_change_for_audio: 0.0,
            random_values,
            fire_blow_up_timer: 0.0,
            ground_physical: [None; 4],
            ground_offset: [Vec3::ZERO; 4],
            set_on_fire_entity: None,
            weapon_door_timer_left: 0.0,
            weapon_door_timer_right: 0.0,
            car_gun_lr: 0.0,
            car_gun_ud: 0.05,
            propeller_rotation: 0.0,
            wheels_on_ground: 0,
            drive_wheels_on_ground: 0,
            drive_wheels_on_ground_prev: 0,
            gas_pedal_audio: 0.0,
            wheel_state: [WheelState::Normal; 4],
        }
    }

    pub fn is_door_missing(&self, door: usize) -> bool {
        self.damage.door_status(door) == door_status::MISSING
    }

    pub fn is_door_closed(&self, door: usize) -> bool {
        self.doors[door].is_closed()
    }

    pub fn is_door_fully_open(&self, door: usize) -> bool {
        self.doors[door].is_fully_open() || self.is_door_missing(door)
    }

    /// `IsDoorReady`: closed, gone, or not currently being used by a ped getting in.
    pub fn is_door_ready(&self, door: usize, getting_in_flags: i8) -> bool {
        if self.doors[door].is_closed() || self.is_door_missing(door) {
            return true;
        }
        let flag = match door {
            door_id::FRONT_LEFT => door_flag::LF,
            door_id::FRONT_RIGHT => door_flag::RF,
            door_id::REAR_LEFT => door_flag::LR,
            door_id::REAR_RIGHT => door_flag::RR,
            _ => 0,
        };
        flag & getting_in_flags == 0
    }

    pub fn is_component_present(&self, comp: usize) -> bool {
        self.node_positions.get(comp).is_some_and(|p| p.is_some())
    }

    /// `ResetSuspension`.
    pub fn reset_suspension(&mut self, spring_ratio: &mut [f32; 4]) {
        for i in 0..4 {
            spring_ratio[i] = 1.0;
            self.wheel_timer[i] = 0.0;
            self.wheel_rotation[i] = 0.0;
            self.wheel_state[i] = WheelState::Normal;
        }
    }

    /// `GetAllWheelsOffGround`.
    pub fn all_wheels_off_ground(&self) -> bool {
        self.drive_wheels_on_ground == 0
    }

    /// `SetBusDoorTimer`: `kind` 0 opens and closes the doors, anything else only closes them.
    pub fn set_bus_door_timer(&mut self, now_ms: u32, timer: u32, kind: u8) {
        let timer = timer.max(1000);
        self.bus_door_timer_start = if kind == 0 { now_ms } else { now_ms.wrapping_sub(500) };
        self.bus_door_timer_end = self.bus_door_timer_start.wrapping_add(timer);
    }

    /// `Fix`: repairs everything.
    pub fn fix(&mut self, handling: &HandlingData) {
        self.damage.reset();
        if handling.flags & hflags::NO_DOORS != 0 {
            for d in [door_id::FRONT_LEFT, door_id::FRONT_RIGHT, door_id::REAR_LEFT, door_id::REAR_RIGHT] {
                self.damage.set_door_status(d, door_status::MISSING);
            }
        }
    }

    /// `BurstTyre`: `wheel` is a `CAR_PIECE_WHEEL_*` id or a wheel index. True if the tyre burst.
    pub fn burst_tyre(&mut self, wheel: u8) -> bool {
        let wheel = match wheel as u16 {
            piece::WHEEL_LF => CARWHEEL_FRONT_LEFT,
            piece::WHEEL_RF => CARWHEEL_FRONT_RIGHT,
            piece::WHEEL_LR => CARWHEEL_REAR_LEFT,
            piece::WHEEL_RR => CARWHEEL_REAR_RIGHT,
            _ => wheel as usize,
        };
        if self.damage.wheel_status(wheel) == super::damage::wheel_status::OK {
            self.damage.set_wheel_status(wheel, super::damage::wheel_status::BURST);
            return true;
        }
        false
    }
}

/// `CAutomobile::SetupSuspensionLines`: the four suspension lines of the model's collision
/// model and the spring lengths they imply. Run once per model, before cars of it are made.
pub fn setup_suspension_lines(col: &mut ColModel, mi_data: &crate::modelinfo::VehicleData, h: &HandlingData, model_index: i16) -> SuspensionSetup {
    let mut out = SuspensionSetup::default();
    if col.lines.len() != 4 {
        col.lines = vec![Line::default(); 4];
    }
    for i in 0..4 {
        let mut p = mi_data.wheel_positions[i];
        out.wheel_position[i] = p.z;
        // uppermost wheel position
        p.z += h.suspension_upper_limit;
        col.lines[i].p0 = p;
        // lowermost wheel position, then the lowest point of the tyre
        p.z += h.suspension_lower_limit - h.suspension_upper_limit;
        p.z -= mi_data.wheel_scale * 0.5;
        col.lines[i].p1 = p;
        out.spring_length[i] = h.suspension_upper_limit - h.suspension_lower_limit;
        out.line_length[i] = col.lines[i].p0.z - col.lines[i].p1.z;
    }
    // the spring is somewhat compressed at rest
    out.height_above_road = -(col.lines[0].p0.z + (col.lines[0].p1.z - col.lines[0].p0.z) * (1.0 - 1.0 / (8.0 * h.suspension_force_level)));
    for i in 0..4 {
        out.wheel_position[i] = mi_data.wheel_scale * 0.5 - out.height_above_road;
    }
    // make the model include its suspension lines
    if col.bounding_box.min.z > col.lines[0].p1.z {
        col.bounding_box.min.z = col.lines[0].p1.z;
    }
    let radius = crate::math::magnitude(col.bounding_box.min).max(crate::math::magnitude(col.bounding_box.max));
    if col.bounding_sphere.radius < radius {
        col.bounding_sphere.radius = radius;
    }
    if model_index == mi::RCBANDIT {
        col.bounding_sphere.radius = 2.0;
        for s in &mut col.spheres {
            s.radius = 0.3;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SuspensionSetup {
    pub spring_length: [f32; 4],
    pub line_length: [f32; 4],
    pub height_above_road: f32,
    pub wheel_position: [f32; 4],
}

/// What a freshly built car consists of.
pub struct NewCar {
    pub vehicle: VehicleData,
    pub physical: Physical,
}

/// `CAutomobile::CAutomobile` for model `model_index`. `col` is the model's collision model
/// (already given suspension lines through [`setup_suspension_lines`]) and `suspension` its result.
#[allow(clippy::too_many_arguments)] // the original reads these from model info and globals
pub fn new_car(
    model_index: i16,
    created_by: u8,
    time_ms: u32,
    handling_id: usize,
    handling: &HandlingData,
    mi_data: &mut crate::modelinfo::VehicleData,
    suspension: &SuspensionSetup,
    all_taxi_lights: bool,
    player_colours: Option<(u8, u8)>,
    rng: &mut Rng,
) -> NewCar {
    let mut v = VehicleData::new(created_by, time_ms, VehicleType::Car, rng);
    v.handling = handling_id;
    v.num_max_passengers = max_passengers(model_index, mi_data.num_doors);
    let mut car = AutomobileData::new(handling, mi_data, all_taxi_lights, rng);
    let (c1, c2) = choose_vehicle_colour(mi_data, player_colours);
    v.current_colour1 = c1;
    v.current_colour2 = c2;
    v.flags.is_van = handling.flags & hflags::IS_VAN != 0;
    v.flags.is_big = handling.flags & hflags::IS_BIG != 0;
    v.flags.is_bus = handling.flags & hflags::IS_BUS != 0;
    v.flags.low_vehicle = handling.flags & hflags::IS_LOW != 0;

    car.suspension_spring_length = suspension.spring_length;
    car.suspension_line_length = suspension.line_length;
    car.height_above_road = suspension.height_above_road;
    car.wheel_position = suspension.wheel_position;
    v.suspension_spring_ratio = [1.0; 4];

    let physical = Physical {
        mass: handling.mass,
        turn_mass: handling.turn_mass,
        centre_of_mass: handling.centre_of_mass,
        air_resistance: handling.dimension.x * handling.dimension.z / handling.mass,
        elasticity: 0.05,
        buoyancy: handling.buoyancy,
        ..Default::default()
    };

    if v.door_lock == CarLock::Unlocked && matches!(model_index, mi::POLICE | mi::ENFORCER | mi::RHINO) {
        v.door_lock = CarLock::LockedInitially;
    }
    v.sub = super::base::VehicleSub::Car(Box::new(car));
    NewCar { vehicle: v, physical }
}

/// `GetMaximumNumberOfPassengersFromNumberOfDoors` (GTA III).
pub fn max_passengers(model: i16, num_doors: i32) -> u8 {
    let n = match model {
        mi::TRAIN => 3,
        mi::FIRETRUCK => 2,
        _ => num_doors,
    };
    if n == 0 {
        return if model == mi::RCBANDIT { 0 } else { 1 };
    }
    if model == mi::COACH {
        return 8;
    }
    (n - 1) as u8
}

/// `ChooseVehicleColour`: cycles through the model's colour table, skipping the player's current one.
pub fn choose_vehicle_colour(mi_data: &mut crate::modelinfo::VehicleData, player_colours: Option<(u8, u8)>) -> (u8, u8) {
    if mi_data.num_colours == 0 {
        return (0, 0);
    }
    let n = mi_data.num_colours as i32;
    let pick = |mi_data: &mut crate::modelinfo::VehicleData| {
        mi_data.last_colour_variation = (mi_data.last_colour_variation + 1) % n;
        let i = mi_data.last_colour_variation as usize;
        (mi_data.colours1[i], mi_data.colours2[i])
    };
    let mut c = pick(mi_data);
    if n > 1 && player_colours == Some(c) {
        c = pick(mi_data);
    }
    c
}

/// The pieces of one car entity, so the control code can work on them without the pool.
/// The automobile data is moved out of the vehicle while this exists: call [`Parts::finish`]
/// to put it back.
#[must_use = "call finish() to put the automobile data back"]
pub struct Parts<'a> {
    pub me: EntityRef,
    pub base: &'a mut crate::entity::Entity,
    pub phys: &'a mut Physical,
    pub veh: &'a mut VehicleData,
    pub car: Box<AutomobileData>,
}

impl<'a> Parts<'a> {
    /// The car's parts, if the entity is a car.
    pub fn of(me: EntityRef, e: &'a mut crate::entity::Ent) -> Option<Self> {
        let crate::entity::Ent { base, phys, kind, .. } = e;
        match (phys.as_mut(), kind) {
            (Some(phys), crate::entity::Kind::Vehicle(veh)) => match std::mem::take(&mut veh.sub) {
                super::base::VehicleSub::Car(car) => Some(Self { me, base, phys, veh, car }),
                other => {
                    veh.sub = other;
                    None
                }
            },
            _ => None,
        }
    }

    pub fn finish(self) {
        self.veh.sub = super::base::VehicleSub::Car(self.car);
    }

    /// `GetComponentWorldPosition`: `None` if the model lacks the node.
    pub fn component_world_position(&self, component: usize) -> Option<Vec3> {
        self.car.node_positions.get(component).copied().flatten().map(|p| self.base.matrix.transform_point(p))
    }
}
