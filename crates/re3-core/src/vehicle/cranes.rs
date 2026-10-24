//! `CCrane`/`CCranes`: the port cranes and the crusher/military cranes that lift cars.
use super::base::VehicleType;
use crate::entity::EntityRef;
use crate::math::atan_of_xy;
use crate::modelinfo::indices_gta3::mi;
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2 as HALFPI, PI};

pub const NUM_CRANES: usize = 8;
const TWOPI: f32 = PI * 2.0;

pub const MAX_DISTANCE_TO_FIND_CRANE: f32 = 10.0;
pub const CRANE_UPDATE_RADIUS: f32 = 300.0;
pub const CRANE_MOVEMENT_PROCESSING_RADIUS: f32 = 150.0;
pub const CRUSHER_Z: f32 = -0.951;
pub const MILITARY_Z: f32 = 10.7862;
pub const DISTANCE_FROM_PLAYER_TO_REMOVE_VEHICLE: f32 = 5.0;
pub const DISTANCE_FROM_HOOK_TO_VEHICLE_TO_COLLECT: f32 = 0.5;
pub const CAR_REWARD_MILITARY_CRANE: i32 = 1500;
pub const CAR_MOVING_SPEED_THRESHOLD: f32 = 0.01;
pub const CRANE_SLOWDOWN_MULTIPLIER: f32 = 0.3;
pub const OSCILLATION_SPEED: f32 = 0.002;
pub const CAR_ROTATION_SPEED: f32 = 0.0035;
pub const CRANE_MOVEMENT_SPEED: f32 = 0.001;
pub const HOOK_ANGLE_MOVEMENT_SPEED: f32 = 0.004;
pub const HOOK_OFFSET_MOVEMENT_SPEED: f32 = 0.1;
pub const HOOK_HEIGHT_MOVEMENT_SPEED: f32 = 0.06;
pub const MESSAGE_SHOW_DURATION: u32 = 4000;
pub const MAX_DISTANCE: f32 = 99999.9;
pub const MIN_VALID_POSITION: f32 = -10000.0;
pub const DEFAULT_OFFSET: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CraneState {
    #[default]
    Idle,
    GoingTowardsTarget,
    LiftingTarget,
    GoingTowardsTargetOnlyHeight,
    RotatingTarget,
    DroppingTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CraneStatus {
    #[default]
    None,
    Activated,
    Deactivated,
}

#[derive(Debug, Clone, Default)]
pub struct Crane {
    /// The building the crane is made of.
    pub entity: Option<EntityRef>,
    /// Position of the crane building (its pivot).
    pub position: Vec3,
    pub random_seed: u16,
    pub hook: Option<EntityRef>,
    pub pickup_x1: f32,
    pub pickup_x2: f32,
    pub pickup_y1: f32,
    pub pickup_y2: f32,
    pub dropoff_target: Vec3,
    pub dropoff_heading: f32,
    pub pickup_angle: f32,
    pub dropoff_angle: f32,
    pub pickup_distance: f32,
    pub dropoff_distance: f32,
    pub pickup_height: f32,
    pub dropoff_height: f32,
    pub hook_angle: f32,
    pub hook_offset: f32,
    pub hook_height: f32,
    pub hook_init_pos: Vec3,
    pub hook_cur_pos: Vec3,
    pub hook_velocity: Vec2,
    pub vehicle_picked_up: Option<EntityRef>,
    pub time_for_next_check: u32,
    pub status: CraneStatus,
    pub state: CraneState,
    pub vehicles_collected: u8,
    pub is_crusher: bool,
    pub is_military_crane: bool,
    pub was_military_crane: bool,
    pub is_top: bool,
}

impl Crane {
    /// Where the hook hangs for the current angle, offset and height.
    pub fn hook_coordinates(&self) -> Vec3 {
        Vec3::new(self.hook_angle.cos() * self.hook_offset + self.position.x, self.hook_angle.sin() * self.hook_offset + self.position.y, self.hook_height)
    }

    /// `FindParametersForTarget`: angle, distance and height to reach a point.
    pub fn parameters_for_target(&self, p: Vec3) -> (f32, f32, f32) {
        let d = Vec2::new(p.x, p.y) - Vec2::new(self.position.x, self.position.y);
        (atan_of_xy(d.x, d.y), d.length(), p.z)
    }

    pub fn height_to_pickup(&self) -> f32 {
        4.0 + self.pickup_height + if self.is_crusher { 4.5 } else { 0.0 }
    }

    pub fn height_to_dropoff(&self) -> f32 {
        if self.is_crusher { 2.0 + self.dropoff_height + 3.0 } else { 2.0 + self.dropoff_height }
    }

    pub fn height_to_pickup_height(&self) -> f32 {
        self.pickup_height + if self.is_crusher { 7.0 } else { 4.0 }
    }

    pub fn height_to_dropoff_height(&self) -> f32 {
        self.dropoff_height + if self.is_crusher { 7.0 } else { 2.0 }
    }

    /// `GoTowardsTarget`: moves the hook angle, offset and height a step; true when all arrived.
    pub fn go_towards_target(&mut self, angle: f32, distance: f32, height: f32, speed_multiplier: f32, step: f32) -> bool {
        let mut angle_delta = angle - self.hook_angle;
        while angle_delta > PI {
            angle_delta -= TWOPI;
        }
        while angle_delta < -PI {
            angle_delta += TWOPI;
        }
        let angle_step = speed_multiplier * step * HOOK_ANGLE_MOVEMENT_SPEED;
        let angle_done = if angle_delta.abs() < angle_step {
            self.hook_angle = angle;
            true
        } else {
            if angle_delta < 0.0 {
                self.hook_angle -= angle_step;
                if self.hook_angle < 0.0 {
                    self.hook_angle += TWOPI;
                }
            } else {
                self.hook_angle += angle_step;
                if self.hook_angle > TWOPI {
                    self.hook_angle -= TWOPI;
                }
            }
            false
        };
        let offset_delta = distance - self.hook_offset;
        let offset_step = speed_multiplier * step * HOOK_OFFSET_MOVEMENT_SPEED;
        let offset_done = if offset_delta.abs() < offset_step {
            self.hook_offset = distance;
            true
        } else {
            self.hook_offset += if offset_delta < 0.0 { -offset_step } else { offset_step };
            false
        };
        let height_done = self.go_towards_height(height, speed_multiplier, step);
        angle_done && offset_done && height_done
    }

    /// `GoTowardsHeightTarget`.
    pub fn go_towards_height(&mut self, height: f32, speed_multiplier: f32, step: f32) -> bool {
        let delta = height - self.hook_height;
        let change = speed_multiplier * step * HOOK_HEIGHT_MOVEMENT_SPEED;
        if delta.abs() < change {
            self.hook_height = height;
            true
        } else {
            self.hook_height += if delta < 0.0 { -change } else { change };
            false
        }
    }

    /// `DoesCranePickUpThisCarType` (GTA III models).
    pub fn picks_up_car_type(&self, model: i16) -> bool {
        if self.is_crusher {
            // FIX_BUGS: the coach, not the Blista, is spared
            return !matches!(model, mi::FIRETRUCK | mi::TRASH | mi::COACH | mi::SECURICA | mi::BUS | mi::DODO | mi::RHINO);
        }
        if self.is_military_crane {
            return military_bit(model).is_some();
        }
        true
    }
}

fn military_bit(model: i16) -> Option<u32> {
    Some(match model {
        mi::FIRETRUCK => 1,
        mi::AMBULAN => 2,
        mi::ENFORCER => 4,
        mi::FBICAR => 8,
        mi::RHINO => 0x10,
        mi::BARRACKS => 0x20,
        mi::POLICE => 0x40,
        _ => return None,
    })
}

#[derive(Debug, Clone, Default)]
pub struct CraneManager {
    pub cranes: Vec<Crane>,
    pub cars_collected_military_crane: u32,
}

impl CraneManager {
    pub fn military_has_this_one(&self, model: i16) -> bool {
        military_bit(model).is_some_and(|b| self.cars_collected_military_crane & b != 0)
    }

    pub fn register_car_for_military_crane(&mut self, model: i16) {
        if let Some(b) = military_bit(model) {
            self.cars_collected_military_crane |= b;
        }
    }

    pub fn have_all_cars_been_collected(&self) -> bool {
        self.cars_collected_military_crane & 0x7F == 0x7F
    }

    /// The crane closest to (x, y) within 10 units.
    pub fn find_crane(&self, x: f32, y: f32) -> Option<usize> {
        let mut best = None;
        let mut min = MAX_DISTANCE;
        for (i, c) in self.cranes.iter().enumerate() {
            let d = (Vec2::new(x, y) - Vec2::new(c.position.x, c.position.y)).length();
            if d < min && d < MAX_DISTANCE_TO_FIND_CRANE {
                min = d;
                best = Some(i);
            }
        }
        best
    }

    pub fn is_car_being_carried(&self, car: EntityRef) -> bool {
        self.cranes.iter().any(|c| c.vehicle_picked_up == Some(car) && matches!(c.state, CraneState::GoingTowardsTargetOnlyHeight | CraneState::LiftingTarget | CraneState::RotatingTarget))
    }

    pub fn is_car_being_targeted(&self, car: EntityRef) -> bool {
        self.cranes.iter().any(|c| c.vehicle_picked_up == Some(car))
    }

    /// `IsThisCarPickedUp`.
    pub fn is_car_picked_up(&self, x: f32, y: f32, car: EntityRef) -> bool {
        self.cranes.iter().any(|c| {
            (Vec2::new(x, y) - Vec2::new(c.position.x, c.position.y)).length() < MAX_DISTANCE_TO_FIND_CRANE && c.vehicle_picked_up == Some(car) && matches!(c.state, CraneState::LiftingTarget | CraneState::RotatingTarget)
        })
    }
}

#[allow(dead_code)]
const _: (VehicleType, f32) = (VehicleType::Car, HALFPI);

#[cfg(test)]
mod tests {
    use super::*;

    fn crane() -> Crane {
        Crane { position: Vec3::new(100.0, 100.0, 0.0), hook_angle: 0.0, hook_offset: DEFAULT_OFFSET, hook_height: DEFAULT_OFFSET, ..Default::default() }
    }

    #[test]
    fn the_hook_hangs_at_its_angle_and_reach() {
        let mut c = crane();
        assert_eq!(c.hook_coordinates(), Vec3::new(120.0, 100.0, 20.0));
        c.hook_angle = HALFPI;
        let p = c.hook_coordinates();
        assert!((p.x - 100.0).abs() < 1e-4 && (p.y - 120.0).abs() < 1e-4);
    }

    #[test]
    fn targets_are_found_as_angle_distance_and_height() {
        let c = crane();
        let (a, d, h) = c.parameters_for_target(Vec3::new(100.0, 130.0, 5.0));
        assert!((a - HALFPI).abs() < 1e-4 && (d - 30.0).abs() < 1e-4 && h == 5.0);
    }

    #[test]
    fn the_hook_reaches_a_target_in_steps_and_the_short_way_round() {
        let mut c = crane();
        c.hook_angle = 0.1;
        // from 0.1 to 6.2 rad is shorter going backwards through zero
        let mut steps = 0;
        let before = c.hook_angle;
        assert!(!c.go_towards_target(6.2, 25.0, 30.0, 1.0, 1.0));
        assert!(c.hook_angle < before || c.hook_angle > 6.0, "{}", c.hook_angle);
        while !c.go_towards_target(6.2, 25.0, 30.0, 1.0, 1.0) {
            steps += 1;
            assert!(steps < 5000);
        }
        assert_eq!((c.hook_angle, c.hook_offset, c.hook_height), (6.2, 25.0, 30.0));
    }

    #[test]
    fn crane_types_pick_the_right_cars() {
        let mut c = crane();
        assert!(c.picks_up_car_type(mi::BUS));
        c.is_crusher = true;
        assert!(!c.picks_up_car_type(mi::BUS) && !c.picks_up_car_type(mi::COACH) && c.picks_up_car_type(mi::BLISTA));
        c.is_crusher = false;
        c.is_military_crane = true;
        assert!(c.picks_up_car_type(mi::POLICE) && !c.picks_up_car_type(mi::LANDSTAL));
        assert_eq!((c.height_to_pickup(), c.height_to_dropoff()), (4.0, 2.0));
        c.is_crusher = true;
        assert_eq!((c.height_to_pickup(), c.height_to_dropoff(), c.height_to_pickup_height()), (8.5, 5.0, 7.0));
    }

    #[test]
    fn the_military_crane_tracks_what_it_has_collected() {
        let mut m = CraneManager::default();
        assert!(!m.military_has_this_one(mi::POLICE));
        for model in [mi::FIRETRUCK, mi::AMBULAN, mi::ENFORCER, mi::FBICAR, mi::RHINO, mi::BARRACKS] {
            m.register_car_for_military_crane(model);
        }
        assert!(!m.have_all_cars_been_collected() && m.military_has_this_one(mi::RHINO));
        m.register_car_for_military_crane(mi::POLICE);
        m.register_car_for_military_crane(mi::LANDSTAL);
        assert!(m.have_all_cars_been_collected());
    }

    #[test]
    fn cranes_are_found_by_position() {
        let mut m = CraneManager::default();
        m.cranes.push(crane());
        assert_eq!(m.find_crane(105.0, 100.0), Some(0));
        assert_eq!(m.find_crane(200.0, 100.0), None);
    }
}
