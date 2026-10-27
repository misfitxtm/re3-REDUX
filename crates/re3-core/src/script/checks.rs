//! `CUpsideDownCarCheck` and `CStuckCarCheck`: cars the script asks to be watched.
use crate::entity::{EntityRef, PoolKind};
use crate::math::magnitude;
use crate::world::World;
use glam::Vec3;

pub const MAX_UPSIDEDOWN_CAR_CHECKS: usize = 6;
pub const MAX_STUCK_CAR_CHECKS: usize = 6;
const UPSIDEDOWN_UP_THRESHOLD: f32 = -0.97;
const UPSIDEDOWN_MOVE_SPEED_THRESHOLD: f32 = 0.01;
const UPSIDEDOWN_TURN_SPEED_THRESHOLD: f32 = 0.02;
const UPSIDEDOWN_TIMER_THRESHOLD: u32 = 1000;

/// A vehicle a script knows by its handle.
pub fn vehicle_ref(handle: i32) -> EntityRef {
    EntityRef { pool: PoolKind::Vehicle, handle }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpsideDownCar {
    pub vehicle: i32,
    pub timer: u32,
}

/// `CUpsideDownCarCheck`. The script never uses it, but a car upside down for long enough fails the mission like a death does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpsideDownCarCheck {
    pub cars: [UpsideDownCar; MAX_UPSIDEDOWN_CAR_CHECKS],
}

impl Default for UpsideDownCarCheck {
    fn default() -> Self {
        Self { cars: [UpsideDownCar { vehicle: -1, timer: 0 }; MAX_UPSIDEDOWN_CAR_CHECKS] }
    }
}

impl UpsideDownCarCheck {
    pub fn are_any_cars_upside_down(&self) -> bool {
        self.cars.iter().any(|c| c.vehicle >= 0 && c.timer > UPSIDEDOWN_TIMER_THRESHOLD)
    }

    pub fn add_car_to_check(&mut self, id: i32) {
        if let Some(c) = self.cars.iter_mut().find(|c| c.vehicle < 0) {
            *c = UpsideDownCar { vehicle: id, timer: 0 };
        }
    }

    pub fn remove_car_from_check(&mut self, id: i32) {
        for c in self.cars.iter_mut().filter(|c| c.vehicle == id) {
            *c = UpsideDownCar { vehicle: -1, timer: 0 };
        }
    }

    pub fn has_car_been_upside_down_for_a_while(&self, id: i32) -> bool {
        self.cars.iter().find(|c| c.vehicle == id).is_some_and(|c| c.timer > UPSIDEDOWN_TIMER_THRESHOLD)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StuckCar {
    pub vehicle: i32,
    pub pos: Vec3,
    pub last_check: i32,
    pub radius: f32,
    pub stuck_time: u32,
    pub stuck: bool,
}

impl Default for StuckCar {
    /// `stuck_car_data::Reset`.
    fn default() -> Self {
        Self { vehicle: -1, pos: Vec3::splat(-5000.0), last_check: -1, radius: 0.0, stuck_time: 0, stuck: false }
    }
}

/// `CStuckCarCheck`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StuckCarCheck {
    pub cars: [StuckCar; MAX_STUCK_CAR_CHECKS],
}

impl StuckCarCheck {
    pub fn remove_car_from_check(&mut self, id: i32) {
        for c in self.cars.iter_mut().filter(|c| c.vehicle == id) {
            *c = StuckCar::default();
        }
    }

    pub fn has_car_been_stuck_for_a_while(&self, id: i32) -> bool {
        self.cars.iter().find(|c| c.vehicle == id).is_some_and(|c| c.stuck)
    }
}

impl World {
    /// `IsCarUpsideDown`.
    pub fn is_car_upside_down(&self, r: EntityRef) -> bool {
        let Some(e) = self.get(r) else { return false };
        let Some(p) = e.phys.as_ref() else { return false };
        e.base.matrix.up.z <= UPSIDEDOWN_UP_THRESHOLD && magnitude(p.move_speed) < UPSIDEDOWN_MOVE_SPEED_THRESHOLD && magnitude(p.turn_speed) < UPSIDEDOWN_TURN_SPEED_THRESHOLD
    }

    /// `CUpsideDownCarCheck::UpdateTimers`.
    pub fn update_upside_down_timers(&mut self) {
        let step = (self.ents.timer.time_step / 50.0 * 1000.0) as u32;
        for i in 0..MAX_UPSIDEDOWN_CAR_CHECKS {
            let r = vehicle_ref(self.ents.script_checks.upside_down.cars[i].vehicle);
            let alive = self.ents.pools.get(r).is_some();
            let upside_down = alive && self.is_car_upside_down(r);
            let c = &mut self.ents.script_checks.upside_down.cars[i];
            match (alive, upside_down) {
                (true, true) => c.timer += step,
                (true, false) => c.timer = 0,
                (false, _) => *c = UpsideDownCar { vehicle: -1, timer: 0 },
            }
        }
    }

    /// `CStuckCarCheck::Process`.
    pub fn process_stuck_cars(&mut self) {
        let now = self.now();
        for i in 0..MAX_STUCK_CAR_CHECKS {
            let c = self.ents.script_checks.stuck.cars[i];
            if c.vehicle < 0 || now <= c.stuck_time.wrapping_add(c.last_check as u32) {
                continue;
            }
            let Some(e) = self.get(vehicle_ref(c.vehicle)) else {
                self.ents.script_checks.stuck.cars[i] = StuckCar::default();
                continue;
            };
            let pos = e.base.position();
            let c = &mut self.ents.script_checks.stuck.cars[i];
            c.stuck = magnitude(pos - c.pos) < c.radius;
            c.pos = pos;
            c.last_check = now as i32;
        }
    }

    /// `CStuckCarCheck::AddCarToCheck`.
    pub fn add_car_to_stuck_check(&mut self, id: i32, radius: f32, time: u32) {
        let now = self.now();
        let Some(pos) = self.get(vehicle_ref(id)).map(|e| e.base.position()) else { return };
        if let Some(c) = self.ents.script_checks.stuck.cars.iter_mut().find(|c| c.vehicle < 0) {
            *c = StuckCar { vehicle: id, pos, last_check: now as i32, radius, stuck_time: time, stuck: false };
        }
    }
}

/// The two checkers.
#[derive(Debug, Clone, Default)]
pub struct ScriptChecks {
    pub upside_down: UpsideDownCarCheck,
    pub stuck: StuckCarCheck,
}
