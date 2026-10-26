//! `CVehicle`: state shared by every vehicle and the behaviour that does not depend on the kind.
//! GTA III behaviour. Peds, fires, audio and weapons are reached through events and arguments.
use super::handling::{HandlingData, HandlingMgr};
use crate::collision::{ColModel, StoredCollPoly};
use crate::entity::physical::{Physical, GRAVITY};
use crate::entity::{status, Entity, EntityRef};
use crate::general::Rng;
use crate::math::{cross, dot, magnitude, magnitude_squared, Matrix};
use crate::modelinfo::indices_gta3::mi;
use crate::timer::Timer;
use glam::Vec3;

/// `MISSION_CRUISE`.
pub const MISSION_CRUISE: u8 = 1;

pub const RANDOM_VEHICLE: u8 = 1;
pub const MISSION_VEHICLE: u8 = 2;
pub const PARKED_VEHICLE: u8 = 3;
pub const PERMANENT_VEHICLE: u8 = 4;

/// `eVehicleType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VehicleType {
    #[default]
    Car,
    Boat,
    Train,
    Heli,
    Plane,
    Bike,
}

/// `eCarLock`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CarLock {
    NotUsed,
    #[default]
    Unlocked,
    Locked,
    LockoutPlayerOnly,
    LockedPlayerInside,
    LockedInitially,
    ForceShutDoors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WheelState {
    /// Standing still or rolling normally.
    #[default]
    Normal,
    /// Rotating but not moving.
    Spinning,
    Skidding,
    /// Not rotating.
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightModel {
    Dodo,
    RcPlane,
    Heli,
    SeaPlane,
}

/// `eWeaponType` values the vehicle damage code distinguishes.
pub mod weapon {
    pub const UNARMED: i8 = 0;
    pub const BASEBALLBAT: i8 = 1;
    pub const COLT45: i8 = 2;
    pub const UZI: i8 = 3;
    pub const SHOTGUN: i8 = 4;
    pub const AK47: i8 = 5;
    pub const M16: i8 = 6;
    pub const SNIPERRIFLE: i8 = 7;
    pub const ROCKETLAUNCHER: i8 = 8;
    pub const FLAMETHROWER: i8 = 9;
    pub const MOLOTOV: i8 = 10;
    pub const GRENADE: i8 = 11;
    pub const DETONATOR: i8 = 12;
    pub const HELICANNON: i8 = 13;
    pub const TOTAL_INVENTORY_WEAPONS: i8 = 14;
    pub const UZI_DRIVEBY: i8 = 15;
    pub const EXPLOSION: i8 = 17;
    pub const RAMMEDBYCAR: i8 = 19;
}

/// The pad as the vehicle code reads it.
#[derive(Debug, Clone, Copy, Default)]
pub struct PadState {
    pub accelerate: i16,
    pub brake: i16,
    pub steering_left_right: i16,
    pub steering_up_down: i16,
    pub look_left: bool,
    pub look_right: bool,
    pub horn: bool,
    pub car_gun_up_down: i16,
    pub car_gun_left_right: i16,
}

/// What `CAutoPilot` fields this file touches; the rest comes with the car AI.
#[derive(Debug, Clone, Copy, Default)]
pub struct AutoPilot {
    pub car_mission: u8,
    pub temp_action: u8,
    /// `m_nTimeTempAction`.
    pub time_temp_action: u32,
    pub time_to_start_mission: u32,
    pub stay_in_current_level: bool,
    pub ignore_pathfinding: bool,
    pub slowed_down_because_of_cars: bool,
    pub slowed_down_because_of_peds: bool,
    pub max_traffic_speed: f32,
    /// `m_nCruiseSpeed`.
    pub cruise_speed: u8,
    /// `m_nDrivingStyle` (`eCarDrivingStyle`).
    pub driving_style: u8,
    /// `m_nPrevRouteNode`.
    pub prev_route_node: i32,
}

/// `eTempAction`.
pub mod temp_action {
    pub const NONE: u8 = 0;
    pub const WAIT: u8 = 1;
    pub const REVERSE: u8 = 2;
    pub const GOFORWARD: u8 = 8;
}

/// `eCarMission`, the ones the ped code sets.
pub mod car_mission {
    pub const NONE: u8 = 0;
    pub const CRUISE: u8 = 1;
    pub const RAMPLAYER_FARAWAY: u8 = 2;
}

/// `eCarDrivingStyle`.
pub mod driving_style {
    pub const STOP_FOR_CARS: u8 = 0;
    pub const SLOW_DOWN_FOR_CARS: u8 = 1;
    pub const AVOID_CARS: u8 = 2;
    pub const PLOUGH_THROUGH: u8 = 3;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VehicleFlags {
    pub is_law_enforcer: bool,
    pub is_ambulance_on_duty: bool,
    pub is_fire_truck_on_duty: bool,
    /// Locked by the script (cannot be removed).
    pub is_locked: bool,
    pub engine_on: bool,
    pub is_handbrake_on: bool,
    pub lights_on: bool,
    pub freebies: bool,
    pub is_van: bool,
    pub is_bus: bool,
    pub is_big: bool,
    pub low_vehicle: bool,
    pub comedy_controls: bool,
    pub warned_peds: bool,
    pub crane_message_done: bool,
    pub extended_range: bool,
    pub take_less_damage: bool,
    pub is_damaged: bool,
    pub has_been_owned_by_player: bool,
    pub fade_out: bool,
    pub is_being_car_jacked: bool,
    pub create_road_block_peds: bool,
    pub can_be_damaged: bool,
    pub using_special_col_model: bool,
    pub occupants_have_been_generated: bool,
    pub gun_switched_off: bool,
    pub vehicle_col_processed: bool,
    pub is_car_park_vehicle: bool,
    pub has_already_been_recorded: bool,
}

/// The kind-specific part of a vehicle.
#[derive(Debug, Clone, Default)]
pub enum VehicleSub {
    #[default]
    None,
    Car(Box<super::automobile::AutomobileData>),
    Boat(Box<super::boat::BoatData>),
    Heli(Box<super::heli::HeliData>),
    Plane(Box<super::plane::PlaneData>),
    Train(Box<super::train::TrainData>),
}

#[derive(Debug, Clone)]
pub struct VehicleData {
    pub sub: VehicleSub,
    /// Index into the handling table (`pHandling`).
    pub handling: usize,
    pub auto_pilot: AutoPilot,
    pub current_colour1: u8,
    pub current_colour2: u8,
    pub extras: [i8; 2],
    pub alarm_state: i16,
    pub mission_value: i16,
    pub driver: Option<EntityRef>,
    pub passengers: [Option<EntityRef>; 8],
    pub num_passengers: u8,
    pub num_getting_in: i8,
    pub getting_in_flags: i8,
    pub getting_out_flags: i8,
    pub num_max_passengers: u8,
    pub cur_ground_entity: Option<EntityRef>,
    pub steer_angle: f32,
    pub gas_pedal: f32,
    pub brake_pedal: f32,
    pub created_by: u8,
    pub flags: VehicleFlags,
    pub num_peds_use_it_as_cover: i8,
    pub ammo_in_clip: u8,
    pub pacman_pickups_carried: i8,
    pub roadblock_type: u8,
    pub roadblock_node: i16,
    /// 1000 is full; 250 is fire; 0 explodes.
    pub health: f32,
    pub current_gear: u8,
    pub change_gear_time: f32,
    pub gun_firing_time: u32,
    pub time_of_death: u32,
    pub time_blocked: u16,
    pub bomb_timer: i16,
    pub blow_up_entity: Option<EntityRef>,
    pub map_object_height_ahead: f32,
    pub map_object_height_behind: f32,
    pub door_lock: CarLock,
    /// `eWeaponType` of the last damage, -1 for none.
    pub last_weapon_damage: i8,
    pub radio_station: u8,
    pub rain_audio_counter: u8,
    pub rain_samples_counter: u8,
    /// `m_pCarFire`: the fire burning on the car (an index into the fire manager).
    pub car_fire: Option<usize>,
    pub car_horn_timer: u8,
    pub car_horn_pattern: u8,
    pub siren_or_alarm: bool,
    pub comedy_control_state: i8,
    /// Triangle under the front and rear of the car.
    pub coll_polys: [StoredCollPoly; 2],
    pub steer_input: f32,
    pub veh_type: VehicleType,
    /// Suspension compression of the four wheels, 1.0 fully extended (cars and bikes; moves with `Automobile`).
    pub suspension_spring_ratio: [f32; 4],
}

impl Default for VehicleData {
    /// `CVehicle::CVehicle(uint8 CreatedBy)` with a random-vehicle default.
    fn default() -> Self {
        Self::new(RANDOM_VEHICLE, 0, VehicleType::Car, &mut Rng::default())
    }
}

/// Things a vehicle asks the rest of the game to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VehicleEvent {
    /// The driver or a passenger should leave and flee (`OBJECTIVE_LEAVE_CAR`).
    LeaveCarAndFlee(EntityRef),
    /// The car should switch from simple to real physics and flee.
    /// Cruise speed in car-AI units (60 per game speed unit).
    SwitchToRealPhysics { cruise_speed: f32 },
    /// Engine catches fire (`ENGINE_STATUS_ON_FIRE`), started by this entity.
    EngineOnFire(Option<EntityRef>),
    BlowUp(Option<EntityRef>),
    /// Tick of a timed bomb.
    BombTick,
    /// An occupant is told to get out (`OBJECTIVE_LEAVE_CAR`).
    LeaveCar(EntityRef),
    /// The player dies with the vehicle (a boat explosion).
    KillPlayer,
    /// A police car was shot by the player: wanted level 1.
    WantedLevelNoDrop(u8),
}

impl VehicleData {
    pub fn new(created_by: u8, time_ms: u32, veh_type: VehicleType, rng: &mut Rng) -> Self {
        Self {
            sub: VehicleSub::None,
            handling: 0,
            auto_pilot: AutoPilot { time_to_start_mission: time_ms, cruise_speed: 10, ..Default::default() },
            current_colour1: 0,
            current_colour2: 0,
            extras: [0; 2],
            alarm_state: 0,
            mission_value: 0,
            driver: None,
            passengers: [None; 8],
            num_passengers: 0,
            num_getting_in: 0,
            getting_in_flags: 0,
            getting_out_flags: 0,
            num_max_passengers: 8,
            cur_ground_entity: None,
            steer_angle: 0.0,
            gas_pedal: 0.0,
            brake_pedal: 0.0,
            created_by,
            flags: VehicleFlags { engine_on: true, freebies: true, can_be_damaged: true, ..Default::default() },
            num_peds_use_it_as_cover: 0,
            ammo_in_clip: 0,
            pacman_pickups_carried: 0,
            roadblock_type: 0,
            roadblock_node: 0,
            health: 1000.0,
            current_gear: 1,
            change_gear_time: 0.0,
            gun_firing_time: 0,
            time_of_death: 0,
            time_blocked: 0,
            bomb_timer: 0,
            blow_up_entity: None,
            map_object_height_ahead: 0.0,
            map_object_height_behind: 0.0,
            door_lock: CarLock::Unlocked,
            last_weapon_damage: -1,
            // USERTRACK is 10 radio stations plus the user's tracks
            radio_station: (rng.number() % 10) as u8,
            rain_audio_counter: 0,
            rain_samples_counter: 0,
            car_fire: None,
            car_horn_timer: 0,
            car_horn_pattern: 0,
            siren_or_alarm: false,
            comedy_control_state: 0,
            coll_polys: [StoredCollPoly::default(); 2],
            steer_input: 0.0,
            veh_type,
            suspension_spring_ratio: [1.0; 4],
        }
    }

    pub fn is_car(&self) -> bool {
        self.veh_type == VehicleType::Car
    }

    pub fn is_boat(&self) -> bool {
        self.veh_type == VehicleType::Boat
    }

    pub fn is_train(&self) -> bool {
        self.veh_type == VehicleType::Train
    }

    pub fn is_heli(&self) -> bool {
        self.veh_type == VehicleType::Heli
    }

    pub fn is_plane(&self) -> bool {
        self.veh_type == VehicleType::Plane
    }

    pub fn is_bike(&self) -> bool {
        self.veh_type == VehicleType::Bike
    }

    /// `IsAlarmOn` (`FIX_BUGS`: a wrecked car's alarm is off).
    pub fn is_alarm_on(&self, entity_status: u8) -> bool {
        self.alarm_state != 0 && self.alarm_state != -1 && entity_status != status::WRECKED
    }

    pub fn is_taxi(model: i16) -> bool {
        model == mi::TAXI || model == mi::CABBIE || model == mi::BORGNINE
    }

    pub fn is_law_enforcement_vehicle(model: i16) -> bool {
        matches!(model, mi::FBICAR | mi::POLICE | mi::ENFORCER | mi::PREDATOR | mi::RHINO | mi::BARRACKS)
    }

    pub fn uses_siren(model: i16) -> bool {
        matches!(model, mi::FIRETRUCK | mi::AMBULAN | mi::FBICAR | mi::MRWHOOP | mi::POLICE | mi::ENFORCER | mi::PREDATOR)
    }

    /// `IsVehicleNormal`: an ordinary driven car (no emergency or special vehicle, no passengers).
    pub fn is_vehicle_normal(&self, model: i16, entity_status: u8) -> bool {
        if self.driver.is_none() || self.num_passengers != 0 || entity_status == status::WRECKED {
            return false;
        }
        !matches!(
            model,
            mi::FIRETRUCK | mi::AMBULAN | mi::TAXI | mi::POLICE | mi::ENFORCER | mi::BUS | mi::RHINO | mi::BARRACKS | mi::DODO | mi::COACH | mi::CABBIE | mi::RCBANDIT | mi::BORGNINE
        )
    }

    pub fn car_has_roof(&self, handling: &HandlingData) -> bool {
        if handling.flags & super::handling::flags::HAS_NO_ROOF == 0 {
            return true;
        }
        !(self.extras[0] != 0 && self.extras[1] != 0)
    }

    pub fn is_upside_down(m: &Matrix) -> bool {
        m.up.z <= -0.9
    }

    pub fn is_on_its_side(m: &Matrix) -> bool {
        !(m.right.z < 0.8 && m.right.z > -0.8)
    }

    /// `GetHeightAboveRoad`.
    pub fn height_above_road(col: &ColModel) -> f32 {
        -col.bounding_box.min.z
    }

    /// `CanBeDeleted`. `ped_ok(ped)` says whether that occupant is neither a mission ped nor out of
    /// the "driving or dead" states.
    pub fn can_be_deleted(&self, ped_ok: impl Fn(EntityRef) -> bool) -> bool {
        if self.num_getting_in != 0 || self.getting_out_flags != 0 {
            return false;
        }
        if self.driver.is_some_and(|d| !ped_ok(d)) {
            return false;
        }
        if self.passengers.iter().flatten().any(|&p| !ped_ok(p)) {
            return false;
        }
        !matches!(self.created_by, MISSION_VEHICLE | PERMANENT_VEHICLE)
    }

    pub fn can_ped_open_locks(&self, ped_is_player: bool) -> bool {
        if matches!(self.door_lock, CarLock::Locked | CarLock::LockedInitially | CarLock::LockedPlayerInside) {
            return false;
        }
        !(ped_is_player && self.door_lock == CarLock::LockoutPlayerOnly)
    }

    /// `CanPedEnterCar`: not when upright, nor while moving quickly.
    pub fn can_ped_enter_car(m: &Matrix, p: &Physical) -> bool {
        if m.up.z > 0.1 || m.up.z < -0.1 {
            return !(magnitude_squared(p.move_speed) > 0.2 * 0.2 || magnitude_squared(p.turn_speed) > 0.2 * 0.2);
        }
        false
    }

    pub fn can_ped_exit_car(m: &Matrix, p: &Physical) -> bool {
        let slow = |strict: bool| {
            let (mv, t) = (magnitude_squared(p.move_speed), p.turn_speed);
            if strict {
                !(mv > 0.005 || t.x.abs() > 0.01 || t.y.abs() > 0.01 || t.z.abs() > 0.01)
            } else {
                !(mv >= 0.005 || t.x.abs() >= 0.01 || t.y.abs() >= 0.01 || t.z.abs() >= 0.01)
            }
        };
        // the original really does use > in one case and >= in the other
        slow(m.up.z > 0.1 || m.up.z < -0.1)
    }

    pub fn change_law_enforcer_state(&mut self, enable: bool) -> Option<i32> {
        if enable && !self.flags.is_law_enforcer {
            self.flags.is_law_enforcer = true;
            return Some(1);
        }
        if !enable && self.flags.is_law_enforcer {
            self.flags.is_law_enforcer = false;
            return Some(-1);
        }
        None
    }

    // ---- occupants ----

    /// The turning push a newly seated occupant of `mass` gives the car (`SetDriver`, `AddPassenger`).
    fn occupant_force(phys: &mut Physical, m: &Matrix, ped_mass: f32, ped_pos: Vec3) {
        let at = Vec3::new(ped_pos.x - m.pos.x, ped_pos.y - m.pos.y, 0.0);
        phys.apply_turn_force(m, Vec3::new(0.0, 0.0, -0.2 * ped_mass), at);
    }

    pub fn set_driver(&mut self, driver: EntityRef, phys: &mut Physical, m: &Matrix, ped_mass: f32, ped_pos: Vec3) {
        self.driver = Some(driver);
        Self::occupant_force(phys, m, ped_mass, ped_pos);
    }

    pub fn add_passenger(&mut self, ped: EntityRef, phys: &mut Physical, m: &Matrix, ped_mass: f32, ped_pos: Vec3) -> bool {
        Self::occupant_force(phys, m, ped_mass, ped_pos);
        for i in 0..self.num_max_passengers as usize {
            if self.passengers[i].is_none() {
                self.passengers[i] = Some(ped);
                self.num_passengers += 1;
                return true;
            }
        }
        false
    }

    /// `AddPassenger(ped, n)`: a bus takes the first free seat.
    pub fn add_passenger_at(&mut self, ped: EntityRef, n: u8, phys: &mut Physical, m: &Matrix, ped_mass: f32, ped_pos: Vec3) -> bool {
        if self.flags.is_bus {
            return self.add_passenger(ped, phys, m, ped_mass, ped_pos);
        }
        Self::occupant_force(phys, m, ped_mass, ped_pos);
        if n < self.num_max_passengers && self.passengers[n as usize].is_none() {
            self.passengers[n as usize] = Some(ped);
            self.num_passengers += 1;
            return true;
        }
        false
    }

    /// `RemoveDriver` (`FIX_BUGS`: a wreck stays wrecked). Returns the new entity status.
    pub fn remove_driver(&mut self, current_status: u8) -> u8 {
        self.driver = None;
        if current_status == status::WRECKED { current_status } else { status::ABANDONED }
    }

    pub fn remove_passenger(&mut self, ped: EntityRef) {
        // trains seat everyone, other vehicles only up to their maximum
        let limit = if self.is_train() { self.passengers.len() } else { self.num_max_passengers as usize };
        for i in 0..limit {
            if self.passengers[i] == Some(ped) {
                self.passengers[i] = None;
                self.num_passengers -= 1;
                return;
            }
        }
    }

    // ---- per-frame helpers ----

    /// `ProcessCarAlarm`: counts the alarm down.
    pub fn process_car_alarm(&mut self, timer: &Timer) {
        if self.alarm_state == 0 || self.alarm_state == -1 {
            return;
        }
        let step = timer.time_step_in_milliseconds();
        if (self.alarm_state as u16 as u32) < step {
            self.alarm_state = 0;
        } else {
            self.alarm_state = self.alarm_state.wrapping_sub(step as i16);
        }
    }

    /// `ProcessDelayedExplosion`: counts a bomb or a burning wreck down, then blows up.
    pub fn process_delayed_explosion(&mut self, timer: &Timer, timed_bomb_active: bool) -> Vec<VehicleEvent> {
        let mut out = vec![];
        if self.bomb_timer == 0 {
            return out;
        }
        let tick = (timer.time_step / 60.0 * 1000.0) as i32;
        let prev = self.bomb_timer;
        self.bomb_timer = if tick > self.bomb_timer as i32 { 0 } else { (self.bomb_timer as i32 - tick) as i16 };
        if timed_bomb_active && (self.bomb_timer as u16 & 0xFE00) != (prev as u16 & 0xFE00) {
            out.push(VehicleEvent::BombTick);
        }
        if self.bomb_timer == 0 {
            out.push(VehicleEvent::BlowUp(self.blow_up_entity));
        }
        out
    }

    /// `IsSphereTouchingVehicle`.
    pub fn is_sphere_touching(m: &Matrix, col: &ColModel, sphere: Vec3, radius: f32) -> bool {
        let rel = sphere - m.pos;
        let (b, axes) = (&col.bounding_box, [m.right, m.forward, m.up]);
        (0..3).all(|i| {
            let d = dot(rel, axes[i]);
            !(b.min[i] - radius > d || b.max[i] + radius < d)
        })
    }

    /// `ProcessWheelRotation`.
    pub fn wheel_rotation(state: WheelState, fwd: Vec3, speed: Vec3, radius: f32, timer: &Timer) -> f32 {
        let angular_velocity = match state {
            WheelState::Spinning => -1.1,
            WheelState::Fixed => 0.0,
            _ => -dot(fwd, speed) / radius,
        };
        angular_velocity * timer.time_step
    }

    /// `ExtinguishCarFire`'s health part; the caller douses the fire and the engine.
    pub fn extinguish_health(&mut self) {
        self.health = self.health.max(300.0);
    }

    /// `InflictDamage`. `entity` is the car's own entity, `damaged_by` who hurt it,
    /// `player_is_attacker` whether that is the player (ped or vehicle).
    #[allow(clippy::too_many_arguments)] // the original reads these from globals
    pub fn inflict_damage(
        &mut self,
        entity: &mut Entity,
        model_is_police: bool,
        damaged_by: Option<EntityRef>,
        player_is_attacker: bool,
        weapon_type: i8,
        damage: f32,
        max_cruise_velocity: f32,
        occupants_flee: bool,
        rng: &mut Rng,
    ) -> Vec<VehicleEvent> {
        let mut out = vec![];
        if !self.flags.can_be_damaged {
            return out;
        }
        if entity.flags.only_damaged_by_player && !player_is_attacker {
            return out;
        }
        let mut frightens_driver = false;
        {
            use weapon::*;
            let f = &entity.flags;
            match weapon_type {
                UNARMED | BASEBALLBAT if f.melee_proof => return out,
                COLT45 | UZI | SHOTGUN | AK47 | M16 | SNIPERRIFLE | TOTAL_INVENTORY_WEAPONS | UZI_DRIVEBY => {
                    if f.bullet_proof {
                        return out;
                    }
                    frightens_driver = true;
                }
                ROCKETLAUNCHER | MOLOTOV | GRENADE | EXPLOSION => {
                    if f.explosion_proof {
                        return out;
                    }
                    frightens_driver = true;
                }
                FLAMETHROWER if f.fire_proof => return out,
                RAMMEDBYCAR if f.collision_proof => return out,
                _ => {}
            }
        }
        // FIX_BUGS: the probabilities are fractions of the random range, not 35000/70000
        let max = rng.max() as u32;
        let flee_in_car = max * 35 / 100;
        let flee_on_foot = max * 70 / 100;
        let seed = entity.random_seed as u32;
        if self.health > 0.0 {
            if self.created_by == RANDOM_VEHICLE
                && self.driver.is_some()
                && (entity.status == status::SIMPLE || entity.status == status::PHYSICS)
                && self.auto_pilot.car_mission == MISSION_CRUISE
                && seed < flee_in_car
            {
                out.push(VehicleEvent::SwitchToRealPhysics { cruise_speed: 60.0 * max_cruise_velocity });
                entity.status = status::PHYSICS;
            }
            self.last_weapon_damage = weapon_type;
            let old = self.health;
            if self.health > damage {
                self.health -= damage;
                if self.created_by == RANDOM_VEHICLE && (self.health < 200.0 || frightens_driver && seed > flee_on_foot) && occupants_flee && (entity.status == status::SIMPLE || entity.status == status::PHYSICS) {
                    if let Some(d) = self.driver {
                        entity.status = status::ABANDONED;
                        out.push(VehicleEvent::LeaveCarAndFlee(d));
                    }
                    out.extend(self.passengers[..self.num_max_passengers as usize].iter().flatten().map(|&p| VehicleEvent::LeaveCarAndFlee(p)));
                }
                if old >= 250.0 && self.health < 250.0 && self.is_car() {
                    out.push(VehicleEvent::EngineOnFire(damaged_by));
                }
            } else {
                self.health = 0.0;
                if weapon_type == weapon::EXPLOSION {
                    // the original's precedence makes this `(1000 + rand) & 0x7FF`, not 1000..3047
                    self.bomb_timer = ((1000 + rng.number() as i32) & 0x7FF) as i16;
                    self.blow_up_entity = damaged_by;
                } else {
                    out.push(VehicleEvent::BlowUp(damaged_by));
                }
            }
        }
        if model_is_police && player_is_attacker && damaged_by.is_some() && !self.flags.has_been_owned_by_player {
            out.push(VehicleEvent::WantedLevelNoDrop(1));
        }
        out
    }
}

/// `ProcessWheel`'s inputs for one wheel.
#[derive(Debug, Clone, Copy)]
pub struct Wheel {
    pub fwd: Vec3,
    pub right: Vec3,
    pub contact_speed: Vec3,
    pub contact_point: Vec3,
    pub wheels_on_ground: i32,
    pub thrust: f32,
    pub brake: f32,
    pub adhesion: f32,
    pub status: u8,
}

const BURST_SPEED_MAX: f32 = 0.3;
const BURST_TYRE_MOD: f32 = 0.1;

/// Static state of `CVehicle::ProcessWheel` that leaks between wheels and cars in the original.
#[derive(Debug, Clone, Copy, Default)]
pub struct WheelStatics {
    /// Set by the first skidding wheel and never cleared (the fix is behind `FIX_SIGNIFICANT_BUGS`, off in re3).
    pub already_skidding: bool,
    /// The function-static `WheelState[4]` of `CAutomobile::ProcessControl`, which keeps values
    /// between wheels, frames and cars.
    pub scratch_states: [WheelState; 4],
}

impl VehicleData {
    /// `ProcessWheel`: the force one wheel puts on the car, updating its state.
    #[allow(clippy::too_many_arguments)] // mirrors the original's inputs
    pub fn process_wheel(
        &self,
        phys: &mut Physical,
        m: &Matrix,
        handling: &HandlingData,
        mgr: &HandlingMgr,
        model_index: i16,
        w: &Wheel,
        state: &mut WheelState,
        statics: &mut WheelStatics,
        timer: &Timer,
        rng: &mut Rng,
    ) {
        let (mut fwd, mut right) = (0.0f32, 0.0f32);
        let braking = w.brake != 0.0;
        let thrust = if braking { 0.0 } else { w.thrust };
        let driving = thrust != 0.0;
        let mut brake = w.brake;

        let contact_fwd = dot(w.contact_speed, w.fwd);
        let contact_right = dot(w.contact_speed, w.right);

        if *state != WheelState::Normal {
            statics.already_skidding = true;
        }
        *state = WheelState::Normal;

        let mut adhesion = w.adhesion * timer.time_step;
        if statics.already_skidding {
            adhesion *= handling.traction_loss;
        }

        if contact_right != 0.0 {
            right = -contact_right / w.wheels_on_ground as f32;
            if w.status == super::damage::wheel_status::BURST {
                let fwd_speed = contact_fwd.min(BURST_SPEED_MAX);
                right += fwd_speed * rng.range_f32(-BURST_TYRE_MOD, BURST_TYRE_MOD);
            }
        }

        if driving {
            fwd = thrust;
            // limit sideways force
            right = right.clamp(-adhesion, adhesion);
        } else if contact_fwd != 0.0 {
            fwd = -contact_fwd / w.wheels_on_ground as f32;
            if !braking && self.gas_pedal < 0.01 {
                brake = if model_index == mi::RCBANDIT { 0.2 * mgr.wheel_friction / handling.mass } else { mgr.wheel_friction / handling.mass };
                brake *= timer.time_step_fix();
            }
            if brake > adhesion {
                if contact_fwd.abs() > 0.005 {
                    *state = WheelState::Fixed;
                }
            } else if fwd > 0.0 {
                fwd = fwd.min(brake);
            } else {
                fwd = fwd.max(-brake);
            }
        }

        let speed_sq = right * right + fwd * fwd;
        if adhesion * adhesion < speed_sq {
            if *state != WheelState::Fixed {
                *state = if driving && contact_fwd < 0.2 { WheelState::Spinning } else { WheelState::Skidding };
            }
            let l = speed_sq.sqrt();
            let traction_loss = if statics.already_skidding { 1.0 } else { handling.traction_loss };
            right *= adhesion * traction_loss / l;
            fwd *= adhesion * traction_loss / l;
        }

        if fwd != 0.0 || right != 0.0 {
            let direction = w.fwd * fwd + w.right * right;
            let speed = magnitude(direction);
            let direction = crate::math::normalise(direction);
            let impulse = speed * phys.mass;
            let turn_impulse = speed * phys.mass_at(w.contact_point, direction);
            phys.apply_move_force(direction * impulse);
            phys.apply_turn_force(m, direction * turn_impulse, w.contact_point);
        }
    }
}

// ---- flying ----

const RC_PROP_FALL_OFF: f32 = 3.0;
const RC_AERO_THRUST: f32 = 0.003;
const RC_SIDE_SLIP_MULT: f32 = 0.1;
const RC_RUDDER_MULT: f32 = 0.2;
const RC_YAW_MULT: f32 = -0.01;
const RC_ROLL_MULT: f32 = 0.02;
const RC_ROLL_STABILISE: f32 = -0.08;
const RC_PITCH_MULT: f32 = 0.005;
const RC_TAIL_MULT: f32 = 0.3;
const RC_FORM_LIFT_MULT: f32 = 0.02;
const RC_ATTACK_LIFT_MULT: f32 = 0.25;
const RC_AERO_RESISTANCE: Vec3 = Vec3::new(0.998, 0.998, 0.9);

const SEA_PROP_FALL_OFF: f32 = 2.3;
const SEA_THRUST: f32 = 0.002;
const SEA_SIDE_SLIP_MULT: f32 = 0.1;
const SEA_RUDDER_MULT: f32 = 0.01;
const SEA_YAW_MULT: f32 = -0.0003;
const SEA_ROLL_MULT: f32 = 0.0015;
const SEA_ROLL_STABILISE: f32 = -0.01;
const SEA_PITCH_MULT: f32 = 0.0002;
const SEA_TAIL_MULT: f32 = 0.01;
const SEA_FORM_LIFT_MULT: f32 = 0.012;
const SEA_ATTACK_LIFT_MULT: f32 = 0.1;
const SEA_AERO_RESISTANCE: Vec3 = Vec3::new(0.995, 0.995, 0.85);

const SPEED_RESISTANCE_Y: f32 = 500.0;

const HELI_MOVE_RES: Vec3 = Vec3::new(0.995, 0.995, 0.99);
const RC_HELI_MOVE_RES: Vec3 = Vec3::new(0.99, 0.99, 0.99);
const THRUST_VAR: f32 = 0.3;
const ROTOR_FALL_OFF: f32 = 0.75;
const STABILISE_VAR: f32 = 0.015;
const PITCH_VAR: f32 = 0.006;
const ROLL_VAR: f32 = 0.006;
const YAW_VAR: f32 = -0.001;
const HELI_RESISTANCE: Vec3 = Vec3::new(0.81, 0.85, 0.99);
const RC_HELI_RESISTANCE: Vec3 = Vec3::new(0.92, 0.92, 0.998);
const SPIN_SPEED_RES: f32 = 20.0;

/// Inputs of `FlyingControl` that live outside the physical.
pub struct Flight<'a> {
    pub pad: &'a PadState,
    pub col: &'a ColModel,
    pub entity_status: u8,
    pub is_sparrow: bool,
    /// `bCheat5`.
    pub cheat5: bool,
    pub steer_angle: f32,
    pub timer: &'a Timer,
}

/// `CVehicle::FlyingControl`.
pub fn flying_control(model: FlightModel, phys: &mut Physical, m: &Matrix, f: &Flight) {
    let step = f.timer.time_step;
    let (right, forward, up) = (m.right, m.forward, m.up);
    let com = |p: &Physical| m.rotate_vector(p.centre_of_mass);
    let (mass, turn_mass) = (phys.mass, phys.turn_mass);
    match model {
        FlightModel::Dodo => {
            // This seems pretty magic
            let mut move_speed = magnitude(phys.move_speed);
            let side_speed = dot(phys.move_speed, right);
            let side_impulse = -side_speed / move_speed;
            let fwd_speed = dot(phys.move_speed, forward);
            let magic = magnitude_squared(phys.move_speed) * (fwd_speed * fwd_speed);
            let turn_impulse = (side_impulse * 0.003 + f.steer_angle * 0.001) * magic * turn_mass * step;
            phys.apply_turn_force(m, right * turn_impulse, forward * -4.0);

            let impulse = side_impulse * 0.2 * magic * mass * step;
            phys.apply_move_force(right * impulse);
            phys.apply_turn_force(m, right * impulse, up * 2.0);

            move_speed = magnitude(phys.move_speed);
            let up_speed = dot(phys.move_speed, up);
            let up_impulse = -up_speed / move_speed;
            let turn_impulse = (up_impulse * 0.002 + -(f.pad.steering_up_down as f32) / 128.0 * 0.001) * magic * turn_mass * step;
            phys.apply_turn_force(m, up * turn_impulse, forward * -4.0);

            let mut impulse = (up_impulse * 3.5 + 0.5) * 0.05 * magic * mass * step;
            if GRAVITY * mass * step < impulse && m.pos.z > 100.0 {
                impulse = 0.9 * GRAVITY * mass * step;
            }
            let c = com(phys);
            phys.apply_move_force(up * impulse);
            phys.apply_turn_force(m, up * impulse, up * 2.0 + c);

            phys.turn_speed.y *= 0.9f32.powf(step);
            let ms = magnitude_squared(phys.move_speed);
            if ms > 1.5 * 1.5 {
                phys.move_speed *= 1.5 / ms.sqrt();
            }
            let ts = magnitude_squared(phys.turn_speed);
            if ts > 0.2 * 0.2 {
                phys.turn_speed *= 0.2 / ts.sqrt();
            }
        }
        FlightModel::RcPlane | FlightModel::SeaPlane => {
            let rc = model == FlightModel::RcPlane;
            let pick = |a: f32, b: f32| if rc { a } else { b };
            let thrust = pick(RC_AERO_THRUST, SEA_THRUST);
            let fall_off = pick(RC_PROP_FALL_OFF, SEA_PROP_FALL_OFF);

            let fwd_speed = dot(phys.move_speed, forward);
            let tail = forward * f.col.bounding_box.min.y;
            let mut pedal = (f.pad.accelerate - f.pad.brake) as f32 / 255.0;
            if fwd_speed > 0.1 || (rc && fwd_speed > 0.02) {
                pedal += 1.0;
            } else if fwd_speed > 0.0 && pedal < 0.0 {
                pedal = 0.0;
            }
            let thrust_accel = (pedal - fall_off * fwd_speed) * thrust;
            phys.apply_move_force(forward * thrust_accel * mass * step);

            // left/right
            let side_speed = -dot(phys.move_speed, right);
            let steer_lr = f.pad.steering_left_right as f32 / 128.0;
            let side_slip = side_speed.abs() * side_speed * pick(RC_SIDE_SLIP_MULT, SEA_SIDE_SLIP_MULT);
            phys.apply_move_force(right * (mass * side_slip * step));

            let yaw = -dot(phys.speed_at(tail), right);
            let yaw_accel = pick(RC_RUDDER_MULT, SEA_RUDDER_MULT) * yaw * yaw.abs() + pick(RC_YAW_MULT, SEA_YAW_MULT) * steer_lr * fwd_speed;
            phys.apply_turn_force(m, right * (yaw_accel * turn_mass * step), tail);

            let roll_accel = if rc {
                let mut dir = f.pad.look_right as i32 as f32;
                if f.pad.look_left {
                    dir = -1.0;
                }
                (0.5 * dir + steer_lr) * RC_ROLL_MULT
            } else {
                steer_lr * SEA_ROLL_MULT
            };
            phys.apply_turn_force(m, right * roll_accel * fwd_speed * turn_mass * step, up);

            let f_right = cross(forward, Vec3::new(0.0, 0.0, 1.0));
            let stabilise = if up.z > 0.0 { f_right } else { -f_right };
            let stab_dir = if right.z > 0.0 { -1.0 } else { 1.0 };
            let stab_speed = pick(RC_ROLL_STABILISE, SEA_ROLL_STABILISE) * stab_dir * (1.0 - dot(right, stabilise)) * (1.0 - forward.z.abs());
            // no time step here in the original either
            phys.apply_turn_force(m, right * (stab_speed * turn_mass), up);

            // up/down
            let tail_up = -dot(phys.speed_at(tail), up);
            let steer_ud = -(f.pad.steering_up_down as f32) / 128.0;
            let pitch_accel = pick(RC_TAIL_MULT, SEA_TAIL_MULT) * tail_up * tail_up.abs() + pick(RC_PITCH_MULT, SEA_PITCH_MULT) * steer_ud * fwd_speed;
            phys.apply_turn_force(m, up * (pitch_accel * turn_mass * step), tail);

            let lift = dot(phys.move_speed, up) / magnitude(phys.move_speed).max(0.01);
            let lift_accel = (pick(RC_FORM_LIFT_MULT, SEA_FORM_LIFT_MULT) - pick(RC_ATTACK_LIFT_MULT, SEA_ATTACK_LIFT_MULT) * lift) * (fwd_speed * fwd_speed);
            let mut lift_impulse = lift_accel * mass * step;
            if GRAVITY * step * mass < lift_impulse && (rc && m.pos.z > 50.0 || !rc && m.pos.z > 80.0) {
                lift_impulse = step * 0.9 * GRAVITY * mass;
            }
            phys.apply_move_force(up * lift_impulse);

            let res = if rc { RC_AERO_RESISTANCE } else { SEA_AERO_RESISTANCE };
            let (r_x, r_y, r_z) = (res.x.powf(step), res.y.powf(step), res.z.powf(step));
            let mut ts = m.rotate_vector_inverse(phys.turn_speed);
            ts.x *= r_x;
            let resistance = ts.y * (1.0 / (SPEED_RESISTANCE_Y * (ts.y * ts.y) + 1.0)) * r_y - ts.y;
            ts.z *= r_z;
            phys.turn_speed = m.rotate_vector(ts);
            let c = com(phys);
            phys.apply_turn_force(m, -up * resistance * turn_mass, right + c);
        }
        FlightModel::Heli => {
            let res = if f.is_sparrow { HELI_MOVE_RES } else { RC_HELI_MOVE_RES };
            phys.move_speed.x *= res.x.powf(step);
            phys.move_speed.y *= res.y.powf(step);
            phys.move_speed.z *= res.z.powf(step);
            if f.entity_status != status::PLAYER && f.entity_status != status::PLAYER_REMOTE {
                return;
            }
            let mut thrust = if f.cheat5 {
                f.pad.steering_up_down as f32 * THRUST_VAR / 128.0 + 0.95
            } else {
                THRUST_VAR * (f.pad.accelerate - 2 * f.pad.brake) as f32 / 255.0 + 0.95
            };
            thrust -= ROTOR_FALL_OFF * dot(phys.move_speed, up);
            if thrust > 0.9 && m.pos.z > 80.0 {
                thrust = 0.9;
            }
            phys.apply_move_force(up * GRAVITY * thrust * mass * step);

            if up.z > 0.0 {
                phys.apply_turn_force(m, -Vec3::new(up.x, up.y, 0.0) * STABILISE_VAR * turn_mass * step, up);
            }

            let (mut roll, mut pitch, mut yaw);
            if f.cheat5 {
                pitch = f.pad.car_gun_up_down as f32 / 128.0;
                roll = -(f.pad.steering_left_right as f32) / 128.0;
                yaw = f.pad.car_gun_left_right as f32 / 128.0;
            } else {
                pitch = f.pad.steering_up_down as f32 / 128.0;
                roll = f.pad.look_left as i32 as f32;
                if f.pad.look_right {
                    roll = -1.0;
                }
                yaw = f.pad.steering_left_right as f32 / 128.0;
            }
            if f.pad.horn {
                yaw = 0.0;
                pitch = (10.0 * dot(phys.move_speed, forward)).clamp(-200.0, 1.3);
                roll = (10.0 * dot(phys.move_speed, right)).clamp(-200.0, 1.3);
            }
            phys.apply_turn_force(m, up * pitch * PITCH_VAR * turn_mass * step, forward);
            phys.apply_turn_force(m, up * roll * ROLL_VAR * turn_mass * step, right);
            phys.apply_turn_force(m, forward * yaw * YAW_VAR * turn_mass * step, right);

            let res = if f.is_sparrow { HELI_RESISTANCE } else { RC_HELI_RESISTANCE };
            let (r_x, r_y, r_z) = (res.x.powf(step), res.y.powf(step), res.z.powf(step));
            let mut ts = m.rotate_vector_inverse(phys.turn_speed);
            let res_mult = (1.0 / (SPIN_SPEED_RES * (ts.z * ts.z) + 1.0) * r_z).powf(step);
            let resistance = ts.z * res_mult - ts.z;
            ts.x *= r_x;
            ts.y *= r_y;
            ts.z *= res_mult;
            phys.turn_speed = m.rotate_vector(ts);
            let c = com(phys);
            phys.apply_turn_force(m, -right * resistance * turn_mass, forward + c);
        }
    }
}

#[cfg(test)]
mod tests;
