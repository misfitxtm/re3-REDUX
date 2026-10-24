//! `CAutomobile` controls: pad input, fire truck, tank, hydraulics, water, bus doors and horn.
// The per-wheel loops index several parallel arrays, as the original does.
#![allow(clippy::needless_range_loop)]
use super::{door_id, door_flag, node, Parts, BombType};
use crate::collision::ColModel;
use crate::entity::status;
use crate::general::Rng;
use crate::math::{atan_of_xy, dot, magnitude, magnitude_squared};
use crate::modelinfo::indices_gta3::mi;
use crate::timer::Timer;
use crate::vehicle::env::{CarEvent, CarSound, Particle, ParticleKind, ExplosionKind, Pad, PlayerView};
use crate::vehicle::floater::{Buoyancy, WaterLevel};
use crate::vehicle::handling::HandlingData;
use glam::Vec3;
use std::f32::consts::PI;

const TWOPI: f32 = PI * 2.0;

/// `DEGTORAD`.
fn deg_to_rad(x: f32) -> f32 {
    x * PI / 180.0
}

/// Inputs shared by the control functions.
pub struct ControlCtx<'a> {
    pub pad: &'a Pad,
    pub player: &'a PlayerView,
    pub timer: &'a Timer,
    pub rng: &'a mut Rng,
    pub events: &'a mut Vec<CarEvent>,
    pub handling: &'a HandlingData,
    pub model_index: i16,
}

impl Parts<'_> {
    /// `ProcessControlInputs` (keyboard/pad steering; the mouse path is off by default in re3).
    pub fn process_control_inputs(&mut self, cx: &mut ControlCtx) {
        let step = cx.timer.time_step;
        let speed = dot(self.phys.move_speed, self.base.matrix.forward);
        self.veh.flags.is_handbrake_on = if cx.pad.exit_vehicle { true } else { cx.pad.hand_brake };

        // steering
        self.veh.steer_input += (-(cx.pad.state.steering_left_right as f32) / 128.0 - self.veh.steer_input) * 0.2 * step;
        self.veh.steer_input = self.veh.steer_input.clamp(-1.0, 1.0);

        // accelerate / brake
        let mut acceleration = (cx.pad.state.accelerate - cx.pad.state.brake) as f32 / 255.0;
        if cx.model_index == mi::DODO && acceleration < 0.0 {
            acceleration *= 0.3;
        }
        if speed.abs() < 0.01 {
            // standing still: go in the direction we want
            self.veh.gas_pedal = acceleration;
            self.veh.brake_pedal = 0.0;
        } else if speed * acceleration < 0.0 {
            // opposite directions: brake first
            self.veh.gas_pedal = 0.0;
            self.veh.brake_pedal = acceleration.abs();
        } else {
            self.veh.gas_pedal = acceleration;
            self.veh.brake_pedal = 0.0;
        }

        // turn the wheels
        let value = if self.veh.steer_input < 0.0 { -(self.veh.steer_input * self.veh.steer_input) } else { self.veh.steer_input * self.veh.steer_input };
        self.veh.steer_angle = deg_to_rad(cx.handling.steering_lock) * value;

        if self.veh.flags.comedy_controls {
            let rnd = cx.rng.number() % 10;
            match self.veh.comedy_control_state {
                0 => {
                    if rnd < 2 {
                        self.veh.comedy_control_state = 1;
                    } else if rnd < 4 {
                        self.veh.comedy_control_state = 2;
                    }
                }
                1 => {
                    self.veh.steer_angle += 0.05;
                    if rnd < 2 {
                        self.veh.comedy_control_state = 0;
                    }
                }
                2 => {
                    self.veh.steer_angle -= 0.05;
                    if rnd < 2 {
                        self.veh.comedy_control_state = 0;
                    }
                }
                _ => {}
            }
        } else {
            self.veh.comedy_control_state = 0;
        }

        // brake if the player is not in control
        if cx.pad.controls_disabled {
            self.veh.brake_pedal = 1.0;
            self.veh.flags.is_handbrake_on = true;
            self.veh.gas_pedal = 0.0;
            let speed = magnitude(self.phys.move_speed);
            if speed > 0.28 {
                self.phys.move_speed *= 0.28 / speed;
            }
        }
    }

    /// `FireTruckControl`: aims the cannon (the player's by pad, AI trucks at the nearest fire).
    /// `fire_pos` is the furthest fire in 10..35 units, if any.
    pub fn fire_truck_control(&mut self, cx: &mut ControlCtx, fire_pos: Option<Vec3>, is_player_vehicle: bool) {
        let step = cx.timer.time_step;
        let m = self.base.matrix;
        if is_player_vehicle {
            if !cx.pad.car_gun_fired {
                return;
            }
            self.car.car_gun_lr += cx.pad.state.car_gun_left_right as f32 * 0.00025 * step;
            self.car.car_gun_ud += cx.pad.state.car_gun_up_down as f32 * 0.0001 * step;
            self.car.car_gun_ud = self.car.car_gun_ud.clamp(0.05, 0.3);
            let cannon_pos = m.transform_point(Vec3::new(0.0, 1.5, 1.9));
            let mut dir = m.rotate_vector(self.cannon_dir());
            dir.z += (cx.rng.number() & 0xF) as f32 / 1000.0;
            cx.events.push(CarEvent::WaterCannon { pos: cannon_pos, dir });
        } else if self.base.status == status::PHYSICS {
            let Some(fire) = fire_pos else { return };
            // aim at the fire
            let target = atan_of_xy(fire.x - m.pos.x, fire.y - m.pos.y);
            let fwd = atan_of_xy(m.forward.x, m.forward.y);
            let target_cannon = fwd - target;
            let angle_delta = step * 0.01;
            let mut cannon_delta = target_cannon - self.car.car_gun_lr;
            while cannon_delta < PI {
                cannon_delta += TWOPI;
            }
            while cannon_delta > PI {
                cannon_delta -= TWOPI;
            }
            if cannon_delta.abs() < angle_delta {
                self.car.car_gun_lr = target_cannon;
            } else if cannon_delta > 0.0 {
                self.car.car_gun_lr += angle_delta;
            } else {
                self.car.car_gun_lr -= angle_delta;
            }
            // go up and down a bit
            let up_down = ((cx.timer.time_in_ms & 0xFFF) as f32 / 0x1000 as f32 * TWOPI).sin();
            self.car.car_gun_ud = 0.2 + 0.2 * up_down;
            // spray every once in a while
            if (cx.timer.time_in_ms >> 10) & 3 != 0 {
                let cannon_pos = m.transform_point(Vec3::new(0.0, 0.0, 2.2));
                let mut dir = m.rotate_vector(self.cannon_dir());
                dir.z += (cx.rng.number() & 0xF) as f32 / 1000.0;
                cx.events.push(CarEvent::WaterCannon { pos: cannon_pos, dir });
            }
        }
    }

    fn cannon_dir(&self) -> Vec3 {
        let (lr, ud) = (self.car.car_gun_lr, self.car.car_gun_ud);
        Vec3::new(lr.sin() * ud.cos(), lr.cos() * ud.cos(), ud.sin())
    }

    /// `TankControl`: turret rotation and firing for the player's tank.
    pub fn tank_control(&mut self, cx: &mut ControlCtx, is_player_vehicle: bool) {
        if !is_player_vehicle || !cx.player.playing {
            return;
        }
        let step = cx.timer.time_step;
        let turret_base = Vec3::new(0.0, -1.394, 2.296);
        let gun_end = Vec3::new(0.0, 1.813, 2.979);
        let base_to_end = gun_end - turret_base;

        let prev = self.car.car_gun_lr;
        self.car.car_gun_lr -= cx.pad.state.car_gun_left_right as f32 * 0.00015 * step;
        if self.car.car_gun_lr < 0.0 {
            self.car.car_gun_lr += TWOPI;
        }
        if self.car.car_gun_lr > TWOPI {
            self.car.car_gun_lr -= TWOPI;
        }
        if self.car.car_gun_lr != prev {
            cx.events.push(CarEvent::Sound { sound: CarSound::TankTurretRotate, volume: (self.car.car_gun_lr - prev).abs() });
        }

        if cx.pad.car_gun_just_down && cx.timer.time_in_ms > cx.player.time_tank_shot_gun + 800 {
            let lr = self.car.car_gun_lr;
            let turret_dir = self.base.matrix.rotate_vector(Vec3::new((-lr).sin(), (-lr).cos(), 0.0));
            let (c, s) = (lr.cos(), lr.sin());
            let rotated = Vec3::new(c * base_to_end.x - s * base_to_end.y, s * base_to_end.x + c * base_to_end.y, base_to_end.z - 1.0) + turret_base;
            let point1 = self.base.matrix.transform_point(rotated);
            self.phys.move_speed -= 0.06 * turret_dir;
            self.phys.move_speed.z += 0.05;
            cx.events.push(CarEvent::TankFire { origin: point1, dir: turret_dir, moving_fast: magnitude(self.phys.move_speed) > 0.08, move_speed: self.phys.move_speed });
        }
    }

    /// `ProcessBuoyancy`: lifts and slows the car in water, drowns occupants, splashes.
    #[allow(clippy::too_many_arguments)] // the original reads these from globals
    pub fn process_buoyancy(&mut self, cx: &mut ControlCtx, buoy: &mut Buoyancy, water: &dyn WaterLevel, col: &ColModel, wet_roads_unused: f32, splash_state: &mut SplashState) {
        let _ = wet_roads_unused;
        let step = cx.timer.time_step;
        let m = self.base.matrix;
        let result = buoy.process(water, &m, col, None, self.base.flags.touching_water, self.phys.buoyancy, step);
        // `bTouchingWater` is a base-entity flag in the original
        match result {
            Some((point, impulse)) => {
                self.base.flags.touching_water = true;
                self.phys.apply_move_force(impulse);
                self.phys.apply_turn_force(&m, impulse, point);
                let initial_speed = self.phys.move_speed;
                let time_step = step.max(0.01);
                let impulse_ratio = impulse.z / (crate::entity::physical::GRAVITY * self.phys.mass * time_step);
                let resistance = (1.0 - 0.05 * impulse_ratio).powf(step);
                self.phys.move_speed *= resistance;
                self.phys.turn_speed *= resistance;
                if impulse_ratio > 0.5 {
                    self.phys.flags.is_in_water = true;
                    if self.phys.move_speed.z < -0.1 {
                        self.phys.move_speed.z = -0.1;
                    }
                    if let Some(driver) = self.veh.driver {
                        cx.events.push(CarEvent::Drown { occupant: driver, is_driver: true, water_tight: self.car.flags.water_tight, amount: step });
                    }
                    for p in self.veh.passengers[..self.veh.num_max_passengers as usize].iter().flatten() {
                        cx.events.push(CarEvent::Drown { occupant: *p, is_driver: false, water_tight: self.car.flags.water_tight, amount: step });
                    }
                } else {
                    self.phys.flags.is_in_water = false;
                }

                if initial_speed.z < -0.3 && impulse.z > 0.3 {
                    cx.events.push(CarEvent::CarWaterSplash { pos: m.pos, up_speed: cx.rng.range_f32(0.15, 0.3), alpha: (cx.rng.range_i32(0, 32) + 128) as u8 });
                    splash_state.raindrops_at = cx.timer.time_in_ms + 300;
                    splash_state.circles_at = cx.timer.time_in_ms + 60;
                    if self.phys.move_speed.z < -0.2 {
                        self.phys.move_speed.z = -0.2;
                    }
                    cx.events.push(CarEvent::Sound { sound: CarSound::WaterFall, volume: 0.0 });
                }

                let now = cx.timer.time_in_ms;
                if splash_state.circles_at > 0 && splash_state.circles_at <= now {
                    let mut pos = m.pos;
                    if let Some(level) = water.height(pos.x, pos.y, pos.z, false) {
                        pos.z = level;
                    }
                    if pos.z != 0.0 {
                        splash_state.circles_at = 0;
                        pos.z += 1.0;
                        for _ in 0..4 {
                            let p = Vec3::new(pos.x + cx.rng.range_f32(-2.5, 2.5), pos.y + cx.rng.range_f32(-2.5, 2.5), pos.z);
                            cx.events.push(CarEvent::Particle(Particle::new(ParticleKind::RainSplashBiggrow, p, Vec3::ZERO, 0.0)));
                        }
                    }
                }
                if splash_state.raindrops_at > 0 && splash_state.raindrops_at <= now {
                    let mut pos = m.pos;
                    if let Some(level) = water.height(pos.x, pos.y, pos.z, false) {
                        pos.z = level;
                    }
                    if pos.z >= 0.0 {
                        splash_state.raindrops_at = 0;
                        pos.z += 0.5;
                        cx.events.push(CarEvent::SplashesAround { pos });
                    }
                }
            }
            None => {
                self.phys.flags.is_in_water = false;
                self.base.flags.touching_water = false;
                for i in 0..4 {
                    if self.veh.suspension_spring_ratio[i] < 1.0 && self.car.wheel_col_points[i].surface_b == crate::surface::WATER {
                        let pos = self.car.wheel_col_points[i].point + 0.3 * m.up - m.pos;
                        let mut v_speed = self.phys.speed_at(pos);
                        v_speed.z = 0.0;
                        // GTA_PS2_STUFF: puddle drag
                        let force = step * (self.phys.mass * (v_speed * -0.003));
                        self.phys.apply_move_force(force);
                        let f_speed = magnitude_squared(v_speed);
                        if f_speed > 0.05 * 0.05 {
                            let f_speed = f_speed.sqrt();
                            let size = ((if f_speed < 0.15 { 0.25 } else { 0.75 }) * f_speed).min(0.6);
                            let right = 0.2 * f_speed * m.right + 0.2 * v_speed;
                            let world = pos + m.pos;
                            let mut splash = Particle::new(ParticleKind::PedSplash, world, -0.5 * right, size);
                            splash.colour = Some([155, 155, 185, 196]);
                            splash.rotation_speed = cx.rng.range_f32(0.0, 10.0);
                            splash.rotation = cx.rng.range_f32(0.0, 90.0);
                            splash.frame = 1;
                            cx.events.push(CarEvent::Particle(splash));
                            let mut smoke = Particle::new(ParticleKind::RubberSmoke, world, -0.6 * right, size);
                            smoke.colour = Some([255, 255, 255, 255]);
                            cx.events.push(CarEvent::Particle(smoke));
                            if cx.timer.frame_counter & 0xF == 0 {
                                cx.events.push(CarEvent::Sound { sound: CarSound::CarSplash, volume: 2000.0 * f_speed });
                            }
                        }
                    }
                }
            }
        }
    }

    /// `OpenDoor` without the clump work: moves the door and fires the sounds.
    pub fn open_door(&mut self, door: usize, ratio: f32, events: &mut Vec<CarEvent>) {
        let was_closed = self.car.doors[door].is_closed();
        self.car.doors[door].open(ratio);
        if was_closed && self.car.doors[door].angle_when_closed() != self.car.doors[door].angle {
            events.push(CarEvent::Sound { sound: CarSound::DoorOpen(door as u8), volume: 0.0 });
        }
        if !was_closed && ratio == 0.0 {
            if self.car.damage.door_status(door) == crate::vehicle::damage::door_status::SWINGING {
                self.car.damage.set_door_status(door, crate::vehicle::damage::door_status::OK);
            }
            events.push(CarEvent::Sound { sound: CarSound::DoorClose(door as u8), volume: 0.0 });
        }
    }

    /// `ProcessAutoBusDoors`.
    pub fn process_auto_bus_doors(&mut self, now_ms: u32, events: &mut Vec<CarEvent>) {
        let end = self.car.bus_door_timer_end;
        let flags = self.veh.getting_in_flags;
        let doors = [(door_id::FRONT_LEFT, door_flag::LF), (door_id::FRONT_RIGHT, door_flag::RF)];
        if now_ms < end {
            if end != 0 && now_ms > end.wrapping_sub(500) {
                for (door, flag) in doors {
                    if !self.car.is_door_missing(door) && flags & flag == 0 {
                        if self.car.is_door_closed(door) {
                            self.car.bus_door_timer_end = now_ms;
                            self.open_door(door, 0.0, events);
                        } else {
                            let ratio = 1.0 - (now_ms - (self.car.bus_door_timer_end.wrapping_sub(500))) as f32 / 500.0;
                            self.open_door(door, ratio, events);
                        }
                    }
                }
            }
        } else if self.car.bus_door_timer_start != 0 {
            for (door, flag) in doors {
                if !self.car.is_door_missing(door) && flags & flag == 0 {
                    self.open_door(door, 0.0, events);
                }
            }
            self.car.bus_door_timer_start = 0;
            self.car.bus_door_timer_end = 0;
        }
    }

    /// `ProcessSwingingDoor`: lets a loose door swing with the car's motion.
    pub fn process_swinging_door(&mut self, door: usize) {
        if self.car.damage.door_status(door) != crate::vehicle::damage::door_status::SWINGING {
            return;
        }
        let (phys, m) = (&*self.phys, self.base.matrix);
        self.car.doors[door].process(phys, &m);
    }

    /// `PlayCarHorn`.
    pub fn play_car_horn(&mut self, rng: &mut Rng, events: &mut Vec<CarEvent>) {
        if self.veh.car_horn_timer != 0 {
            return;
        }
        let r = rng.number() & 7;
        if r < 2 {
            self.veh.car_horn_timer = 45;
        } else if r < 4 {
            if self.veh.driver.is_some() {
                events.push(CarEvent::Sound { sound: CarSound::AnnoyedDriver, volume: 0.0 });
            }
            self.veh.car_horn_timer = 45;
        } else if self.veh.driver.is_some() {
            events.push(CarEvent::Sound { sound: CarSound::AnnoyedDriver, volume: 0.0 });
        }
    }

    /// `PlayHornIfNecessary`; `stopped_for_light` is the AI's answer to `HasCarStoppedBecauseOfLight`.
    pub fn play_horn_if_necessary(&mut self, stopped_for_light: bool, rng: &mut Rng, events: &mut Vec<CarEvent>) {
        let ap = self.veh.auto_pilot;
        if (ap.slowed_down_because_of_peds || ap.slowed_down_because_of_cars) && !stopped_for_light {
            self.play_car_horn(rng, events);
        }
    }

    pub fn reduce_horn_counter(&mut self) {
        self.veh.car_horn_timer = self.veh.car_horn_timer.saturating_sub(1);
    }

    /// `PlaceOnRoadProperly`: levels the car on the road from the heights ahead and behind.
    /// `front_z`/`rear_z` are the ground heights found under the nose and tail (or `None`).
    pub fn place_on_road_properly(&mut self, col: &ColModel, front_z: Option<f32>, rear_z: Option<f32>, height_above_road: f32) {
        let len_fwd = col.bounding_box.max.y;
        let len_back = -col.bounding_box.min.y;
        let m = self.base.matrix;
        let front = Vec3::new(m.pos.x + m.forward.x * len_fwd, m.pos.y + m.forward.y * len_fwd, m.pos.z + 5.0);
        let front_z = front_z.unwrap_or(self.veh.map_object_height_ahead);
        let rear = Vec3::new(m.pos.x - m.forward.x * len_back, m.pos.y - m.forward.y * len_back, m.pos.z + 5.0);
        let rear_z = rear_z.unwrap_or(self.veh.map_object_height_behind);
        let len = len_fwd + len_back;
        let angle = ((front_z - rear_z) / len).atan();
        let (c, s) = (angle.cos(), angle.sin());
        let right = Vec3::new((front.y - rear.y) / len, -(front.x - rear.x) / len, 0.0);
        let forward = Vec3::new(-c * right.y, c * right.x, s);
        let up = crate::math::cross(right, forward);
        let mut nm = m;
        nm.right = right;
        nm.forward = forward;
        nm.up = up;
        nm.pos = Vec3::new((front.x + rear.x) / 2.0, (front.y + rear.y) / 2.0, (front_z + rear_z) / 2.0 + height_above_road);
        self.base.matrix = nm;
    }

    /// `IsRoomForPedToLeaveCar`'s geometry: the seat and door points the world checks use.
    /// Returns `(seat, door)` in world space.
    pub fn exit_points(&self, component: usize, front_seat: Vec3, back_seat: Vec3, door_pos: Vec3, door_offset: Option<Vec3>) -> (Vec3, Vec3) {
        let mut seat = match component {
            node::DOOR_RF => front_seat,
            node::DOOR_LF => Vec3::new(-front_seat.x, front_seat.y, front_seat.z),
            node::DOOR_RR => back_seat,
            _ => Vec3::new(-back_seat.x, back_seat.y, back_seat.z),
        };
        seat = self.base.matrix.transform_point(seat);
        let mut door = door_pos;
        if let Some(mut off) = door_offset {
            if component == node::DOOR_RF || component == node::DOOR_RR {
                off.x = -off.x;
            }
            door += self.base.matrix.rotate_vector(off);
        }
        if self.base.matrix.up.z < 0.0 {
            seat.z += 0.5;
            door.z += 0.5;
        }
        (seat, door)
    }

    /// `ProcessWheelRotation` for the four wheels of a simple car.
    pub fn simple_car_wheels(&mut self, timer: &Timer) {
        let rot = crate::vehicle::base::VehicleData::wheel_rotation(crate::vehicle::base::WheelState::Normal, self.base.matrix.forward, self.phys.move_speed, 0.35, timer);
        for r in &mut self.car.wheel_rotation {
            *r += rot;
        }
    }

    /// `ScanForCrimes`: a police car near an alarmed player car raises the wanted level.
    pub fn scan_for_crimes(&self, player: &PlayerView, events: &mut Vec<CarEvent>) {
        if player.vehicle.is_some() && player.vehicle_is_car && player.vehicle_alarm_on && magnitude_squared(player.vehicle_pos - self.base.position()) < 20.0 * 20.0 {
            events.push(CarEvent::Vehicle(crate::vehicle::base::VehicleEvent::WantedLevelNoDrop(1)));
        }
    }

    /// `BombType` activation by the player (car gun button) while driving.
    pub fn arm_bomb(&mut self, player_ped: Option<crate::entity::EntityRef>, events: &mut Vec<CarEvent>) {
        match self.car.bomb_type {
            BombType::Timed => {
                self.car.bomb_type = BombType::TimedActive;
                self.veh.bomb_timer = 7000;
                self.veh.blow_up_entity = player_ped;
                events.push(CarEvent::TriggerMessage("GA_12"));
                events.push(CarEvent::Sound { sound: CarSound::BombTimedActivated, volume: 1.0 });
            }
            BombType::OnIgnition => {
                self.car.bomb_type = BombType::OnIgnitionActive;
                events.push(CarEvent::TriggerMessage("GA_12"));
                events.push(CarEvent::Sound { sound: CarSound::BombOnIgnitionActivated, volume: 1.0 });
            }
            _ => {}
        }
    }

    /// Explosion kind for the tank shot (kept next to its only user).
    pub fn tank_explosion() -> ExplosionKind {
        ExplosionKind::TankGrenade
    }
}

/// `nGenerateRaindrops` / `nGenerateWaterCircles`, the function statics of `ProcessBuoyancy`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SplashState {
    pub raindrops_at: u32,
    pub circles_at: u32,
}

/// Hydraulic suspension limits (`HYDRAULIC_UPPER_EXT`, `HYDRAULIC_LOWER_EXT`).
const HYDRAULIC_UPPER_EXT: f32 = -0.12;
const HYDRAULIC_LOWER_EXT: f32 = 0.14;

/// What `HydraulicControl` needs from the model and the player's special collision model.
pub struct HydraulicEnv<'a> {
    /// The model's own collision model (with its four suspension lines).
    pub normal: &'a ColModel,
    /// The player's per-car copy (`CPlayerInfo::m_ColModel`), edited in place.
    pub special: &'a mut ColModel,
    /// `m_pVehicleEx`: the car currently using the special model.
    pub vehicle_ex: &'a mut Option<crate::entity::EntityRef>,
    pub wheel_positions: [Vec3; 4],
    pub wheel_scale: f32,
}

impl Parts<'_> {
    /// `HydraulicControl` (the Yardie Lobo's hydraulics). Always `None` for now: the original never
    /// releases the previous holder.
    pub fn hydraulic_control(&mut self, cx: &mut ControlCtx, he: &mut HydraulicEnv) -> Option<crate::entity::EntityRef> {
        let h = cx.handling;
        let wheel_radius = 0.5 * he.wheel_scale;
        let mut wheel_positions = [0.0f32; 4];
        let sprat = &mut self.veh.suspension_spring_ratio;

        if self.base.status != status::PLAYER {
            // reset the hydraulics of non-player cars
            if !self.veh.flags.using_special_col_model {
                return None;
            }
            for i in 0..4 {
                wheel_positions[i] = he.special.lines[i].p0.z - sprat[i] * self.car.suspension_line_length[i];
            }
            for i in 0..4 {
                self.car.suspension_spring_length[i] = h.suspension_upper_limit - h.suspension_lower_limit;
                self.car.suspension_line_length[i] = he.normal.lines[i].p0.z - he.normal.lines[i].p1.z;
                sprat[i] = (he.normal.lines[i].p0.z - wheel_positions[i]) / self.car.suspension_line_length[i];
                if sprat[i] > 1.0 {
                    sprat[i] = 1.0;
                }
            }
            if self.car.hydraulic_state == 0 {
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic1, volume: 0.0 });
            } else if self.car.hydraulic_state >= 100 {
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic2, volume: 0.0 });
            }
            if *he.vehicle_ex == Some(self.me) {
                *he.vehicle_ex = None;
            }
            self.veh.flags.using_special_col_model = false;
            self.car.hydraulic_state = 0;
            return None;
        }

        // player car
        let mut normal_upper = h.suspension_upper_limit;
        let mut normal_lower = h.suspension_lower_limit;
        let mut normal_spring = normal_upper - normal_lower;
        let extended_upper = normal_upper - 0.2;
        let extended_lower = normal_lower - 0.2;
        let extended_spring = extended_upper - extended_lower;

        if !self.veh.flags.using_special_col_model {
            // set up the special collision model
            // the original only clears the flag of `this` here (a comparison against itself), so the
            // previous holder keeps its flag
            *he.vehicle_ex = Some(self.me);
            *he.special = he.normal.clone();
            self.veh.flags.using_special_col_model = true;
            if self.car.velocity_change_for_audio > 0.1 {
                self.car.hydraulic_state = 20;
            } else {
                self.car.hydraulic_state = 0;
                normal_upper += HYDRAULIC_UPPER_EXT;
                normal_spring = normal_upper - (normal_lower + HYDRAULIC_LOWER_EXT);
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic2, volume: 0.0 });
            }
            let normal_line = normal_spring + wheel_radius;
            for i in 0..4 {
                wheel_positions[i] = he.normal.lines[i].p0.z - sprat[i] * self.car.suspension_line_length[i];
                let mut pos = he.wheel_positions[i];
                pos.z += normal_upper;
                he.special.lines[i].p0 = pos;
                pos.z -= normal_line;
                he.special.lines[i].p1 = pos;
                self.car.suspension_spring_length[i] = normal_spring;
                self.car.suspension_line_length[i] = normal_line;
                if sprat[i] < 1.0 {
                    sprat[i] = ((he.special.lines[i].p0.z - wheel_positions[i]) / self.car.suspension_line_length[i]).min(1.0);
                }
            }
            cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic2, volume: 0.0 });
            // widen the collision model
            let minz = he.wheel_positions[0].z + extended_lower - wheel_radius;
            if minz < he.special.bounding_box.min.z {
                he.special.bounding_box.min.z = minz;
            }
            let radius = magnitude(he.special.bounding_box.min).max(magnitude(he.special.bounding_box.max));
            if he.special.bounding_sphere.radius < radius {
                he.special.bounding_sphere.radius = radius;
            }
            return None;
        }

        if !cx.player.playing {
            return None;
        }

        let mut set_prev_ratio = false;
        if self.car.hydraulic_state < 20 && self.car.velocity_change_for_audio > 0.2 {
            if self.car.hydraulic_state == 0 {
                self.car.hydraulic_state = 20;
                for w in &mut self.car.wheel_position {
                    *w -= 0.06;
                }
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic1, volume: 0.0 });
                set_prev_ratio = true;
            } else {
                self.car.hydraulic_state += 1;
            }
        } else if self.car.hydraulic_state != 0 && self.car.hydraulic_state < 21 && self.car.velocity_change_for_audio < 0.1 {
            self.car.hydraulic_state -= 1;
            if self.car.hydraulic_state == 0 {
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic2, volume: 0.0 });
            }
        }

        if cx.pad.horn_just_down {
            // switch between normal and extended
            if self.car.hydraulic_state < 100 {
                self.car.hydraulic_state = 100;
            } else if self.car.velocity_change_for_audio > 0.1 {
                self.car.hydraulic_state = 20;
            } else {
                self.car.hydraulic_state = 0;
            }

            if self.car.hydraulic_state < 100 {
                if self.car.hydraulic_state == 0 {
                    normal_upper += HYDRAULIC_UPPER_EXT;
                    normal_lower += HYDRAULIC_LOWER_EXT;
                    normal_spring = normal_upper - normal_lower;
                }
                let normal_line = normal_spring + wheel_radius;
                for i in 0..4 {
                    wheel_positions[i] = he.special.lines[i].p0.z - sprat[i] * self.car.suspension_line_length[i];
                    let mut pos = he.wheel_positions[i];
                    pos.z += normal_upper;
                    he.special.lines[i].p0 = pos;
                    pos.z -= normal_line;
                    he.special.lines[i].p1 = pos;
                    self.car.suspension_spring_length[i] = normal_spring;
                    self.car.suspension_line_length[i] = normal_line;
                    if sprat[i] < 1.0 {
                        sprat[i] = ((he.special.lines[i].p0.z - wheel_positions[i]) / self.car.suspension_line_length[i]).min(1.0);
                    }
                }
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic2, volume: 0.0 });
            } else {
                let extended_line = extended_spring + wheel_radius;
                for i in 0..4 {
                    wheel_positions[i] = he.special.lines[i].p0.z - sprat[i] * self.car.suspension_line_length[i];
                    let mut pos = he.wheel_positions[i];
                    pos.z += extended_upper;
                    he.special.lines[i].p0 = pos;
                    pos.z -= extended_line;
                    he.special.lines[i].p1 = pos;
                    self.car.suspension_spring_length[i] = extended_spring;
                    self.car.suspension_line_length[i] = extended_line;
                    if sprat[i] < 1.0 {
                        sprat[i] = ((he.special.lines[i].p0.z - wheel_positions[i]) / self.car.suspension_line_length[i]).min(1.0);
                    }
                    set_prev_ratio = true;
                    self.car.wheel_position[i] -= 0.05;
                }
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic2, volume: 0.0 });
            }
        } else {
            let mut max_delta = 0.0f32;
            let rear = cx.pad.state.car_gun_up_down as f32 / 128.0;
            let front = -rear;
            let right = cx.pad.state.car_gun_left_right as f32 / 128.0;
            let left = -right;
            let mut susp = [0.0f32; 4];
            susp[super::CARWHEEL_FRONT_LEFT] = (front + left).max(0.0);
            susp[super::CARWHEEL_REAR_LEFT] = (rear + left).max(0.0);
            susp[super::CARWHEEL_FRONT_RIGHT] = (front + right).max(0.0);
            susp[super::CARWHEEL_REAR_RIGHT] = (rear + right).max(0.0);

            if self.car.hydraulic_state < 100 {
                // lowered: move the wheels up
                if self.car.hydraulic_state == 0 {
                    normal_upper += HYDRAULIC_UPPER_EXT;
                    normal_lower += HYDRAULIC_LOWER_EXT;
                    normal_spring = normal_upper - normal_lower;
                }
                for i in 0..4 {
                    susp[i] = susp[i].min(1.0);
                    let old_z = he.special.lines[i].p1.z;
                    let upper = susp[i] * (extended_upper - normal_upper) + normal_upper;
                    let spring = susp[i] * (extended_spring - normal_spring) + normal_spring;
                    let line = spring + wheel_radius;
                    wheel_positions[i] = he.special.lines[i].p0.z - sprat[i] * self.car.suspension_line_length[i];
                    let mut pos = he.wheel_positions[i];
                    pos.z += upper;
                    he.special.lines[i].p0 = pos;
                    pos.z -= line;
                    if (pos.z - he.special.lines[i].p1.z).abs() > max_delta.abs() {
                        max_delta = pos.z - he.special.lines[i].p1.z;
                    }
                    he.special.lines[i].p1 = pos;
                    self.car.suspension_spring_length[i] = spring;
                    self.car.suspension_line_length[i] = line;
                    if sprat[i] < 1.0 {
                        sprat[i] = ((he.special.lines[i].p0.z - wheel_positions[i]) / self.car.suspension_line_length[i]).min(1.0);
                        self.car.wheel_position[i] -= (old_z - he.special.lines[i].p1.z) * 0.3;
                    }
                }
            } else {
                if self.car.hydraulic_state < 104 {
                    self.car.hydraulic_state += 1;
                    for w in &mut self.car.wheel_position {
                        *w -= 0.1;
                    }
                }
                if self.car.velocity_change_for_audio < 0.1 {
                    normal_upper += HYDRAULIC_UPPER_EXT;
                    normal_lower += HYDRAULIC_LOWER_EXT;
                    normal_spring = normal_upper - normal_lower;
                }
                for i in 0..4 {
                    susp[i] = susp[i].min(1.0);
                    let upper = susp[i] * (normal_upper - extended_upper) + extended_upper;
                    let spring = susp[i] * (normal_spring - extended_spring) + extended_spring;
                    let line = spring + wheel_radius;
                    wheel_positions[i] = he.special.lines[i].p0.z - sprat[i] * self.car.suspension_line_length[i];
                    let mut pos = he.wheel_positions[i];
                    pos.z += upper;
                    he.special.lines[i].p0 = pos;
                    pos.z -= line;
                    if (pos.z - he.special.lines[i].p1.z).abs() > max_delta.abs() {
                        max_delta = pos.z - he.special.lines[i].p1.z;
                    }
                    he.special.lines[i].p1 = pos;
                    self.car.suspension_spring_length[i] = spring;
                    self.car.suspension_line_length[i] = line;
                    if sprat[i] < 1.0 {
                        sprat[i] = ((he.special.lines[i].p0.z - wheel_positions[i]) / self.car.suspension_line_length[i]).min(1.0);
                    }
                }
            }

            let limit_diff = extended_lower - normal_lower;
            if limit_diff != 0.0 && (max_delta / limit_diff).abs() > 0.01 {
                let f = ((max_delta + limit_diff) / 2.0 / limit_diff).clamp(0.0, 1.0);
                cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic3, volume: f });
                if !(0.4..=0.6).contains(&f) {
                    set_prev_ratio = true;
                }
                if f < 0.25 {
                    cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic2, volume: 0.0 });
                } else if f > 0.75 {
                    cx.events.push(CarEvent::Sound { sound: CarSound::Hydraulic1, volume: 0.0 });
                }
            }
        }

        if set_prev_ratio {
            for i in 0..4 {
                let r = 1.0 - self.car.suspension_spring_length[i] / self.car.suspension_line_length[i];
                self.car.suspension_spring_ratio_prev[i] = (sprat[i] - r) / (1.0 - r);
            }
        }
        None
    }
}
