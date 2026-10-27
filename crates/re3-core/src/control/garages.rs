//! `Garages.cpp` data: the garages, what they hold and their geometry (GTA III). The state machines are in `garage_update`.
use crate::entity::EntityRef;
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::automobile::BombType;
use glam::Vec3;

pub const NUM_GARAGES: usize = 32;
pub const NUM_GARAGE_STORED_CARS: usize = 6;
pub const TOTAL_COLLECTCARS_GARAGES: usize = 3;
pub const TOTAL_COLLECTCARS_CARS: usize = 16;
pub const SWITCH_GARAGE_DISTANCE_CLOSE: f32 = 40.0;

pub const CRUSHER_GARAGE_MIN: Vec3 = Vec3::new(1135.5, 57.0, -1.0);
pub const CRUSHER_GARAGE_MAX: Vec3 = Vec3::new(1149.5, 63.7, 3.5);

/// `eGarageState`.
pub mod garage_state {
    pub const FULLYCLOSED: u8 = 0;
    pub const OPENED: u8 = 1;
    pub const CLOSING: u8 = 2;
    pub const OPENING: u8 = 3;
    pub const OPENEDCONTAINSCAR: u8 = 4;
    pub const CLOSEDCONTAINSCAR: u8 = 5;
    pub const AFTERDROPOFF: u8 = 6;
}

/// `eGarageType`.
pub mod garage_type {
    pub const NONE: u8 = 0;
    pub const MISSION: u8 = 1;
    pub const BOMBSHOP1: u8 = 2;
    pub const BOMBSHOP2: u8 = 3;
    pub const BOMBSHOP3: u8 = 4;
    pub const RESPRAY: u8 = 5;
    pub const COLLECTORSITEMS: u8 = 6;
    pub const COLLECTSPECIFICCARS: u8 = 7;
    pub const COLLECTCARS_1: u8 = 8;
    pub const COLLECTCARS_2: u8 = 9;
    pub const COLLECTCARS_3: u8 = 10;
    pub const FORCARTOCOMEOUTOF: u8 = 11;
    pub const SIXTY_SECONDS: u8 = 12;
    pub const CRUSHER: u8 = 13;
    pub const MISSION_KEEPCAR: u8 = 14;
    pub const FOR_SCRIPT_TO_OPEN: u8 = 15;
    pub const HIDEOUT_ONE: u8 = 16;
    pub const HIDEOUT_TWO: u8 = 17;
    pub const HIDEOUT_THREE: u8 = 18;
    pub const FOR_SCRIPT_TO_OPEN_AND_CLOSE: u8 = 19;
    pub const KEEPS_OPENING_FOR_SPECIFIC_CAR: u8 = 20;
    pub const MISSION_KEEPCAR_REMAINCLOSED: u8 = 21;
}

pub fn is_hideout(ty: u8) -> bool {
    matches!(ty, garage_type::HIDEOUT_ONE | garage_type::HIDEOUT_TWO | garage_type::HIDEOUT_THREE)
}

/// The cars each of Craig's (the import/export) garages wants.
pub const CARS_TO_COLLECT_IN_CRAIGS_GARAGES: [[i16; TOTAL_COLLECTCARS_CARS]; TOTAL_COLLECTCARS_GARAGES] = [
    [mi::SECURICA, mi::MOONBEAM, mi::COACH, mi::FLATBED, mi::LINERUN, mi::TRASH, mi::PATRIOT, mi::MRWHOOP, mi::BLISTA, mi::MULE, mi::YANKEE, mi::BOBCAT, mi::DODO, mi::BUS, mi::RUMPO, mi::PONY],
    [mi::SENTINEL, mi::CHEETAH, mi::BANSHEE, mi::IDAHO, mi::INFERNUS, mi::TAXI, mi::KURUMA, mi::STRETCH, mi::PEREN, mi::STINGER, mi::MANANA, mi::LANDSTAL, mi::STALLION, mi::BFINJECT, mi::CABBIE, mi::ESPERANT],
    [mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::LANDSTAL, mi::CHEETAH, mi::TAXI, mi::ESPERANT, mi::SENTINEL, mi::IDAHO],
];

pub const CARS_TO_COLLECT_IN_60_SECONDS: [i16; 5] = [mi::CHEETAH, mi::TAXI, mi::ESPERANT, mi::SENTINEL, mi::IDAHO];

/// `CStoredCar`: a car kept in a safe house.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StoredCar {
    pub model_index: i32,
    pub pos: Vec3,
    pub angle: Vec3,
    pub bullet_proof: bool,
    pub fire_proof: bool,
    pub explosion_proof: bool,
    pub collision_proof: bool,
    pub melee_proof: bool,
    pub primary_colour: u8,
    pub secondary_colour: u8,
    pub radio_station: u8,
    pub variation_a: i8,
    pub variation_b: i8,
    pub bomb_type: BombType,
}

impl StoredCar {
    pub fn has_car(&self) -> bool {
        self.model_index != 0
    }
}

/// What the garage tests of an entity need: where it is and the spheres of its collision model in the world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Footprint {
    pub pos: Vec3,
    pub bound_radius: f32,
    pub spheres: Vec<(Vec3, f32)>,
}

/// `CGarage`. The doors are entities; the pool indices of the original are covered by the handles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Garage {
    pub ty: u8,
    pub state: u8,
    pub closing_without_target_car: bool,
    pub deactivated: bool,
    pub respray_happened: bool,
    pub target_model_index: i32,
    pub door1: Option<EntityRef>,
    pub door2: Option<EntityRef>,
    pub recreate_door_on_next_refresh: bool,
    pub rotated_door: bool,
    pub camera_follows_player: bool,
    pub x1: f32,
    pub x2: f32,
    pub y1: f32,
    pub y2: f32,
    pub z1: f32,
    pub z2: f32,
    pub door_pos: f32,
    pub door_height: f32,
    pub door1_x: f32,
    pub door1_y: f32,
    pub door2_x: f32,
    pub door2_y: f32,
    pub door1_z: f32,
    pub door2_z: f32,
    pub time_to_start_action: u32,
    pub collected_cars_state: u8,
    pub target: Option<EntityRef>,
}

impl Garage {
    pub fn is_open(&self) -> bool {
        self.state == garage_state::OPENED || self.state == garage_state::OPENEDCONTAINSCAR
    }

    pub fn is_closed(&self) -> bool {
        self.state == garage_state::FULLYCLOSED
    }

    pub fn is_used(&self) -> bool {
        self.ty != garage_type::NONE
    }

    pub fn centre_x(&self) -> f32 {
        (self.x1 + self.x2) / 2.0
    }

    pub fn centre_y(&self) -> f32 {
        (self.y1 + self.y2) / 2.0
    }

    /// `IsFar` with the original's FIX_BUGS: from the middle of the garage.
    pub fn is_far(&self, camera: Vec3) -> bool {
        (camera.x - self.centre_x()).abs() > SWITCH_GARAGE_DISTANCE_CLOSE || (camera.y - self.centre_y()).abs() > SWITCH_GARAGE_DISTANCE_CLOSE
    }

    /// The point is inside the garage's box (strictly).
    pub fn contains(&self, p: Vec3) -> bool {
        p.x > self.x1 && p.x < self.x2 && p.y > self.y1 && p.y < self.y2 && p.z > self.z1 && p.z < self.z2
    }

    fn sphere_overlaps(&self, c: Vec3, r: f32) -> bool {
        c.x + r > self.x1 && c.x - r < self.x2 && c.y + r > self.y1 && c.y - r < self.y2 && c.z + r > self.z1 && c.z - r < self.z2
    }

    fn sphere_leaves(&self, c: Vec3, r: f32) -> bool {
        c.x + r < self.x1 || c.x - r > self.x2 || c.y + r < self.y1 || c.y - r > self.y2 || c.z + r < self.z1 || c.z - r > self.z2
    }

    /// `IsEntityEntirelyInside`: in the rectangle only.
    pub fn is_entity_entirely_inside(&self, f: &Footprint) -> bool {
        if f.pos.x < self.x1 || f.pos.x > self.x2 || f.pos.y < self.y1 || f.pos.y > self.y2 {
            return false;
        }
        f.spheres.iter().all(|&(c, r)| !(c.x - r < self.x1 || c.x + r > self.x2 || c.y - r < self.y1 || c.y + r > self.y2))
    }

    /// `IsEntityEntirelyInside3D`.
    pub fn is_entity_entirely_inside_3d(&self, f: &Footprint, margin: f32) -> bool {
        let p = f.pos;
        if p.x < self.x1 - margin || p.x > self.x2 + margin || p.y < self.y1 - margin || p.y > self.y2 + margin || p.z < self.z1 - margin || p.z > self.z2 + margin {
            return false;
        }
        f.spheres.iter().all(|&(c, r)| {
            !(c.x + r < self.x1 - margin || c.x - r > self.x2 + margin || c.y + r < self.y1 - margin || c.y - r > self.y2 + margin || c.z + r < self.z1 - margin || c.z - r > self.z2 + margin)
        })
    }

    /// `IsEntityEntirelyOutside`.
    pub fn is_entity_entirely_outside(&self, f: &Footprint, margin: f32) -> bool {
        let p = f.pos;
        if p.x > self.x1 - margin && p.x < self.x2 + margin && p.y > self.y1 - margin && p.y < self.y2 + margin {
            return false;
        }
        f.spheres.iter().all(|&(c, r)| !(c.x + r > self.x1 - margin && c.x - r < self.x2 + margin && c.y + r > self.y1 - margin && c.y - r < self.y2 + margin))
    }

    /// `IsEntityTouching3D`.
    pub fn is_entity_touching_3d(&self, f: &Footprint) -> bool {
        let (p, r) = (f.pos, f.bound_radius);
        if self.x1 - r > p.x || self.x2 + r < p.x || self.y1 - r > p.y || self.y2 + r < p.y || self.z1 - r > p.z || self.z2 + r < p.z {
            return false;
        }
        f.spheres.iter().any(|&(c, r)| self.sphere_overlaps(c, r))
    }

    /// `EntityHasASphereWayOutsideGarage`.
    pub fn entity_has_a_sphere_way_outside_garage(&self, f: &Footprint, margin: f32) -> bool {
        f.spheres.iter().any(|&(c, r)| self.sphere_leaves(c, r + margin))
    }

    /// `IsAnyOtherCarTouchingGarage` for one car: it touches and a sphere of it is inside.
    pub fn car_touches(&self, f: &Footprint) -> bool {
        self.is_entity_touching_3d(f) && f.spheres.iter().any(|&(c, r)| self.sphere_overlaps(c, r))
    }

    /// `IsAnyCarBlockingDoor` for one car: it touches and a sphere of it sticks out.
    pub fn car_blocks_door(&self, f: &Footprint) -> bool {
        self.is_entity_touching_3d(f) && f.spheres.iter().any(|&(c, r)| self.sphere_leaves(c, r))
    }

    /// `CalcDistToGarageRectangleSquared`.
    pub fn calc_dist_to_garage_rectangle_squared(&self, x: f32, y: f32) -> f32 {
        let dx = if x < self.x1 { self.x1 - x } else if x > self.x2 { x - self.x2 } else { 0.0 };
        let dy = if y < self.y1 { self.y1 - y } else if y > self.y2 { y - self.y2 } else { 0.0 };
        dx * dx + dy * dy
    }

    /// `CalcSmallestDistToGarageDoorSquared`.
    pub fn calc_smallest_dist_to_garage_door_squared(&self, x: f32, y: f32) -> f32 {
        let d1 = if self.door1.is_some() { (self.door1_x - x).powi(2) + (self.door1_y - y).powi(2) } else { 10000000.0 };
        let d2 = if self.door2.is_some() { (self.door2_x - x).powi(2) + (self.door2_y - y).powi(2) } else { 10000000.0 };
        d1.min(d2)
    }

    /// `OpenThisGarage`.
    pub fn open(&mut self) {
        if matches!(self.state, garage_state::FULLYCLOSED | garage_state::CLOSING | garage_state::CLOSEDCONTAINSCAR) {
            self.state = garage_state::OPENING;
        }
    }

    /// `CloseThisGarage`.
    pub fn close(&mut self) {
        if matches!(self.state, garage_state::OPENED | garage_state::OPENING) {
            self.state = garage_state::CLOSING;
        }
    }

    /// `CGarage::PlayerArrestedOrDied`: the garages the player was using let go.
    pub fn player_arrested_or_died(&mut self) {
        use garage_state::*;
        use garage_type::*;
        match self.ty {
            MISSION | COLLECTORSITEMS | COLLECTSPECIFICCARS | COLLECTCARS_1 | COLLECTCARS_2 | COLLECTCARS_3 | FORCARTOCOMEOUTOF | SIXTY_SECONDS | MISSION_KEEPCAR | FOR_SCRIPT_TO_OPEN | HIDEOUT_ONE | HIDEOUT_TWO | HIDEOUT_THREE
            | FOR_SCRIPT_TO_OPEN_AND_CLOSE | KEEPS_OPENING_FOR_SPECIFIC_CAR | MISSION_KEEPCAR_REMAINCLOSED => {
                if matches!(self.state, OPENED | CLOSING | OPENING) {
                    self.state = CLOSING;
                }
            }
            BOMBSHOP1 | BOMBSHOP2 | BOMBSHOP3 | RESPRAY | CRUSHER => {
                if matches!(self.state, FULLYCLOSED | CLOSING | OPENING) {
                    self.state = OPENING;
                }
            }
            _ => {}
        }
    }

    pub fn craig_index(&self) -> usize {
        (self.ty - garage_type::COLLECTCARS_1) as usize
    }

    /// The slot of a model in Craig's list for this garage; the corpse hearse counts as a Manana.
    pub fn craig_slot(&self, model: i32) -> Option<usize> {
        let model = if model == mi::CORPSE as i32 { mi::MANANA as i32 } else { model };
        CARS_TO_COLLECT_IN_CRAIGS_GARAGES[self.craig_index()].iter().position(|&m| m as i32 == model)
    }

    pub fn does_60_seconds_need_this_car_at_all(model: i32) -> bool {
        CARS_TO_COLLECT_IN_60_SECONDS.iter().any(|&m| m as i32 == model)
    }

    /// `Does60SecondsNeedThisCar`: (sic) true once the car has been collected.
    pub fn does_60_seconds_need_this_car(&self, model: i32) -> bool {
        CARS_TO_COLLECT_IN_60_SECONDS.iter().position(|&m| m as i32 == model).is_some_and(|i| self.collected_cars_state & (1 << i) != 0)
    }

    pub fn mark_this_car_as_collected_for_60_seconds(&mut self, model: i32) {
        for (i, &m) in CARS_TO_COLLECT_IN_60_SECONDS.iter().enumerate() {
            if m as i32 == model {
                self.collected_cars_state |= 1 << i;
            }
        }
    }
}

/// `CGarages`' statics.
#[derive(Debug, Clone)]
pub struct Garages {
    pub bank_vans_collected: i32,
    pub bombs_are_free: bool,
    pub resprays_are_free: bool,
    pub cars_collected: i32,
    pub car_types_collected: [i32; TOTAL_COLLECTCARS_GARAGES],
    /// The handle of the car the crusher crushed last.
    pub crushed_car_id: i32,
    pub last_time_help_message: u32,
    pub message_number_in_string: i32,
    pub message_id: String,
    pub message_number_in_string2: i32,
    pub message_start_time: u32,
    pub message_end_time: u32,
    pub num_garages: usize,
    pub player_in_garage: bool,
    pub police_cars_collected: i32,
    pub garages: Vec<Garage>,
    pub safe_houses: [[StoredCar; NUM_GARAGE_STORED_CARS]; 3],
    pub cam_should_be_outside: bool,
    /// `TheCamera.pToGarageWeAreIn` and `pToGarageWeAreInForHackAvoidFirstPerson`.
    pub garage_we_are_in: Option<usize>,
    pub garage_we_are_in_for_hack_avoid_first_person: Option<usize>,
    pub garage_to_be_tidied: usize,
}

impl Default for Garages {
    fn default() -> Self {
        Self {
            bank_vans_collected: 0,
            bombs_are_free: false,
            resprays_are_free: false,
            cars_collected: 0,
            car_types_collected: [0; TOTAL_COLLECTCARS_GARAGES],
            crushed_car_id: -1,
            last_time_help_message: 0,
            message_number_in_string: 0,
            message_id: String::new(),
            message_number_in_string2: 0,
            message_start_time: 0,
            message_end_time: 0,
            num_garages: 0,
            player_in_garage: false,
            police_cars_collected: 0,
            garages: vec![Garage::default(); NUM_GARAGES],
            safe_houses: [[StoredCar::default(); NUM_GARAGE_STORED_CARS]; 3],
            cam_should_be_outside: false,
            garage_we_are_in: None,
            garage_we_are_in_for_hack_avoid_first_person: None,
            garage_to_be_tidied: 0,
        }
    }
}

impl Garages {
    /// `TriggerMessage`: a message that is already showing is kept for a while longer rather than restarted.
    pub fn trigger_message(&mut self, text: &str, number1: i32, time: u32, number2: i32, now: u32) {
        if text == self.message_id && now >= self.message_start_time && now <= self.message_end_time {
            if now - self.message_start_time <= 500 {
                return;
            }
            self.message_start_time = now - 500;
            self.message_end_time = now - 500 + time;
        } else {
            self.message_id = text.to_owned();
            self.message_start_time = now;
            self.message_end_time = now + time;
        }
        self.message_number_in_string = number1;
        self.message_number_in_string2 = number2;
    }

    /// `PlayerArrestedOrDied`.
    pub fn player_arrested_or_died(&mut self) {
        for g in self.garages.iter_mut().filter(|g| g.is_used()) {
            g.player_arrested_or_died();
        }
        self.message_end_time = 0;
        self.message_start_time = 0;
    }

    /// `AddOne` without the doors: the garage is filled in and the doors are found by the caller.
    pub fn add_one_geometry(&mut self, p1: Vec3, p2: Vec3, ty: u8, target_id: i32) -> usize {
        let i = self.num_garages;
        self.num_garages += 1;
        if i >= NUM_GARAGES {
            return i;
        }
        self.garages[i] = Garage {
            ty,
            x1: p1.x.min(p2.x),
            x2: p1.x.max(p2.x),
            y1: p1.y.min(p2.y),
            y2: p1.y.max(p2.y),
            z1: p1.z.min(p2.z),
            z2: p1.z.max(p2.z),
            door1_z: p1.z,
            door2_z: p1.z,
            target_model_index: target_id,
            ..Default::default()
        };
        i
    }

    /// `ChangeGarageType`.
    pub fn change_garage_type(&mut self, garage: usize, ty: u8, model: i32) {
        let g = &mut self.garages[garage];
        (g.ty, g.target_model_index, g.state) = (ty, model, garage_state::FULLYCLOSED);
    }

    /// `SetTargetCarForMissonGarage`.
    pub fn set_target_car_for_misson_garage(&mut self, garage: usize, car: Option<EntityRef>) {
        let g = &mut self.garages[garage];
        g.target = car;
        if car.is_some() && g.state == garage_state::CLOSEDCONTAINSCAR {
            g.state = garage_state::FULLYCLOSED;
        }
    }

    pub fn has_car_been_dropped_off_yet(&self, garage: usize) -> bool {
        self.garages[garage].state == garage_state::CLOSEDCONTAINSCAR
    }

    pub fn de_activate_garage(&mut self, garage: usize) {
        self.garages[garage].deactivated = true;
    }

    /// `ActivateGarage`.
    pub fn activate_garage(&mut self, garage: usize) {
        let g = &mut self.garages[garage];
        g.deactivated = false;
        if g.ty == garage_type::FORCARTOCOMEOUTOF && g.state == garage_state::FULLYCLOSED {
            g.state = garage_state::OPENING;
        }
    }

    pub fn has_import_export_garage_collected_this_car(&self, garage: usize, car: u8) -> bool {
        self.car_types_collected[self.garages[garage].craig_index()] & (1 << car) != 0
    }

    pub fn is_garage_open(&self, garage: usize) -> bool {
        self.garages[garage].is_open()
    }

    pub fn is_garage_closed(&self, garage: usize) -> bool {
        self.garages[garage].is_closed()
    }

    pub fn has_this_car_been_collected(&self, garage: usize, id: u8) -> bool {
        self.garages[garage].collected_cars_state & (1 << id) != 0
    }

    pub fn open_garage(&mut self, garage: usize) {
        self.garages[garage].open();
    }

    pub fn close_garage(&mut self, garage: usize) {
        self.garages[garage].close();
    }

    /// `HasResprayHappened`: true once per respray.
    pub fn has_respray_happened(&mut self, garage: usize) -> bool {
        std::mem::take(&mut self.garages[garage].respray_happened)
    }

    /// `SetGarageDoorToRotate`.
    pub fn set_garage_door_to_rotate(&mut self, garage: usize) {
        let g = &mut self.garages[garage];
        if g.rotated_door {
            return;
        }
        g.rotated_door = true;
        g.door_height = g.door_height / 2.0 - 0.1;
    }

    pub fn set_leave_camera_for_this_garage(&mut self, garage: usize) {
        self.garages[garage].camera_follows_player = true;
    }

    pub fn has_car_been_crushed(&self, handle: i32) -> bool {
        self.crushed_car_id == handle
    }

    /// `IsPointInAGarageCameraZone`.
    pub fn is_point_in_a_garage_camera_zone(&self, p: Vec3) -> bool {
        self.garages.iter().any(|g| {
            let m = match g.ty {
                garage_type::NONE => return false,
                garage_type::COLLECTCARS_1 | garage_type::COLLECTCARS_2 | garage_type::COLLECTCARS_3 => 1.3,
                _ => 4.0,
            };
            g.x1 - m <= p.x && g.x2 + m >= p.x && g.y1 - m <= p.y && g.y2 + m >= p.y
        })
    }

    /// `IsPointWithinHideOutGarage`.
    pub fn is_point_within_hide_out_garage(&self, p: Vec3) -> bool {
        self.garages.iter().any(|g| is_hideout(g.ty) && g.contains(p))
    }

    /// `IsPointWithinAnyGarage`.
    pub fn is_point_within_any_garage(&self, p: Vec3) -> bool {
        self.garages.iter().any(|g| g.is_used() && g.contains(p))
    }

    /// `CountCarsInHideoutGarage`.
    pub fn count_cars_in_hideout_garage(&self, ty: u8) -> i32 {
        match ty {
            garage_type::HIDEOUT_ONE | garage_type::HIDEOUT_TWO | garage_type::HIDEOUT_THREE => self.safe_houses[(ty - garage_type::HIDEOUT_ONE) as usize].iter().filter(|c| c.has_car()).count() as i32,
            _ => 0,
        }
    }

    /// `FindMaxNumStoredCarsForGarage`.
    pub fn find_max_num_stored_cars_for_garage(ty: u8) -> i32 {
        match ty {
            garage_type::HIDEOUT_ONE => 1,
            garage_type::HIDEOUT_TWO => 2,
            garage_type::HIDEOUT_THREE => 3,
            _ => 0,
        }
    }

    /// `GetBombTypeForGarageType`.
    pub fn bomb_type_for_garage_type(ty: u8) -> BombType {
        match ty - garage_type::BOMBSHOP1 + 1 {
            1 => BombType::Timed,
            2 => BombType::OnIgnition,
            _ => BombType::Remote,
        }
    }

    /// `FindDoorHeightForMI` from the height of the door's collision box.
    pub fn door_height_for_box_height(h: f32) -> f32 {
        h - 0.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn garage() -> Garage {
        Garage { ty: garage_type::MISSION, x1: 0.0, x2: 10.0, y1: 0.0, y2: 10.0, z1: 0.0, z2: 5.0, ..Default::default() }
    }

    fn at(pos: Vec3, radius: f32) -> Footprint {
        Footprint { pos, bound_radius: radius, spheres: vec![(pos, radius)] }
    }

    #[test]
    fn inside_outside_and_touching_follow_the_spheres_not_only_the_centre() {
        let g = garage();
        let (inside, edge, out) = (at(Vec3::new(5.0, 5.0, 1.0), 1.0), at(Vec3::new(9.5, 5.0, 1.0), 1.0), at(Vec3::new(14.0, 5.0, 1.0), 1.0));
        // (as in the original a sphere only has to be partly in)
        let centre_out = at(Vec3::new(10.5, 5.0, 1.0), 1.0);
        assert!(g.is_entity_entirely_inside_3d(&inside, 0.0) && g.is_entity_entirely_inside_3d(&edge, 0.0));
        assert!(!g.is_entity_entirely_inside_3d(&centre_out, 0.0) && g.is_entity_entirely_inside_3d(&centre_out, 1.0));
        assert!(g.is_entity_entirely_outside(&out, 0.0) && !g.is_entity_entirely_outside(&edge, 0.0));
        assert!(g.is_entity_touching_3d(&edge) && !g.is_entity_touching_3d(&out));
        // a car blocks the door when it touches and one of its spheres is wholly outside
        let blocking = Footprint { pos: Vec3::new(10.0, 5.0, 1.0), bound_radius: 3.0, spheres: vec![(Vec3::new(9.0, 5.0, 1.0), 1.0), (Vec3::new(12.0, 5.0, 1.0), 1.0)] };
        assert!(g.car_touches(&edge) && g.car_touches(&blocking) && g.car_blocks_door(&blocking) && !g.car_blocks_door(&edge) && !g.car_blocks_door(&inside));
        assert!(!g.entity_has_a_sphere_way_outside_garage(&edge, 0.0) && g.entity_has_a_sphere_way_outside_garage(&blocking, 0.0) && !g.entity_has_a_sphere_way_outside_garage(&blocking, 1.5));
        // the 2D inside test ignores height
        assert!(g.is_entity_entirely_inside(&at(Vec3::new(5.0, 5.0, 50.0), 1.0)));
    }

    #[test]
    fn distances_to_the_rectangle_and_the_doors() {
        let mut g = garage();
        assert_eq!((g.calc_dist_to_garage_rectangle_squared(5.0, 5.0), g.calc_dist_to_garage_rectangle_squared(13.0, 14.0), g.calc_dist_to_garage_rectangle_squared(-3.0, 5.0)), (0.0, 25.0, 9.0));
        assert_eq!(g.calc_smallest_dist_to_garage_door_squared(0.0, 0.0), 10000000.0);
        (g.door1, g.door1_x, g.door1_y) = (Some(EntityRef { pool: crate::entity::PoolKind::Object, handle: 1 }), 3.0, 4.0);
        assert_eq!(g.calc_smallest_dist_to_garage_door_squared(0.0, 0.0), 25.0);
    }

    #[test]
    fn a_message_that_is_showing_is_extended_but_not_restarted() {
        let mut gs = Garages::default();
        gs.trigger_message("GA_1", -1, 4000, -1, 1000);
        assert_eq!((gs.message_start_time, gs.message_end_time), (1000, 5000));
        gs.trigger_message("GA_1", -1, 4000, -1, 1300);
        assert_eq!(gs.message_end_time, 5000, "within half a second it is left alone");
        gs.trigger_message("GA_1", -1, 4000, -1, 2000);
        assert_eq!((gs.message_start_time, gs.message_end_time), (1500, 5500));
        gs.trigger_message("GA_2", 5, 1000, 6, 2100);
        assert_eq!((gs.message_id.as_str(), gs.message_start_time, gs.message_end_time, gs.message_number_in_string, gs.message_number_in_string2), ("GA_2", 2100, 3100, 5, 6));
        gs.player_arrested_or_died();
        assert_eq!((gs.message_start_time, gs.message_end_time), (0, 0));
    }

    #[test]
    fn open_and_close_only_act_from_the_states_that_make_sense() {
        let mut g = garage();
        g.state = garage_state::OPENED;
        g.open();
        assert_eq!(g.state, garage_state::OPENED);
        g.close();
        assert_eq!(g.state, garage_state::CLOSING);
        g.open();
        assert_eq!(g.state, garage_state::OPENING);
        g.state = garage_state::CLOSEDCONTAINSCAR;
        g.open();
        assert_eq!(g.state, garage_state::OPENING);
    }

    #[test]
    fn the_player_dying_closes_mission_garages_and_opens_the_shops() {
        let mut gs = Garages::default();
        gs.garages[0] = Garage { ty: garage_type::MISSION, state: garage_state::OPENING, ..Default::default() };
        gs.garages[1] = Garage { ty: garage_type::RESPRAY, state: garage_state::CLOSING, ..Default::default() };
        gs.garages[2] = Garage { ty: garage_type::MISSION, state: garage_state::FULLYCLOSED, ..Default::default() };
        gs.player_arrested_or_died();
        assert_eq!((gs.garages[0].state, gs.garages[1].state, gs.garages[2].state), (garage_state::CLOSING, garage_state::OPENING, garage_state::FULLYCLOSED));
    }

    #[test]
    fn the_garage_queries_and_the_camera_zones() {
        let mut gs = Garages::default();
        let i = gs.add_one_geometry(Vec3::new(10.0, 10.0, 5.0), Vec3::new(0.0, 0.0, 0.0), garage_type::HIDEOUT_ONE, 0);
        let j = gs.add_one_geometry(Vec3::new(100.0, 0.0, 0.0), Vec3::new(110.0, 10.0, 5.0), garage_type::COLLECTCARS_1, 0);
        assert_eq!((i, j, gs.num_garages, gs.garages[i].x1, gs.garages[i].z2), (0, 1, 2, 0.0, 5.0));
        assert!(gs.is_point_within_hide_out_garage(Vec3::new(5.0, 5.0, 1.0)) && !gs.is_point_within_hide_out_garage(Vec3::new(105.0, 5.0, 1.0)));
        assert!(gs.is_point_within_any_garage(Vec3::new(105.0, 5.0, 1.0)) && !gs.is_point_within_any_garage(Vec3::new(55.0, 5.0, 1.0)));
        // the camera zone is wider around an ordinary garage than around Craig's
        assert!(gs.is_point_in_a_garage_camera_zone(Vec3::new(13.5, 5.0, 0.0)) && !gs.is_point_in_a_garage_camera_zone(Vec3::new(14.5, 5.0, 0.0)));
        assert!(gs.is_point_in_a_garage_camera_zone(Vec3::new(98.8, 5.0, 0.0)) && !gs.is_point_in_a_garage_camera_zone(Vec3::new(98.6, 5.0, 0.0)));
        // a door that rotates is half as high, less a bit
        gs.garages[i].door_height = 4.0;
        gs.set_garage_door_to_rotate(i);
        gs.set_garage_door_to_rotate(i);
        assert_eq!(gs.garages[i].door_height, 1.9);
    }

    #[test]
    fn bombs_and_the_cars_a_hideout_may_keep_follow_the_type() {
        use crate::vehicle::automobile::BombType;
        assert_eq!([garage_type::BOMBSHOP1, garage_type::BOMBSHOP2, garage_type::BOMBSHOP3].map(Garages::bomb_type_for_garage_type), [BombType::Timed, BombType::OnIgnition, BombType::Remote]);
        assert_eq!([garage_type::HIDEOUT_ONE, garage_type::HIDEOUT_TWO, garage_type::HIDEOUT_THREE, garage_type::MISSION].map(Garages::find_max_num_stored_cars_for_garage), [1, 2, 3, 0]);
    }

    #[test]
    fn sixty_second_garages_remember_which_cars_they_have_had() {
        let mut g = garage();
        let cheetah = crate::modelinfo::indices_gta3::mi::CHEETAH as i32;
        assert!(Garage::does_60_seconds_need_this_car_at_all(cheetah) && !Garage::does_60_seconds_need_this_car_at_all(1));
        assert!(!g.does_60_seconds_need_this_car(cheetah));
        g.mark_this_car_as_collected_for_60_seconds(cheetah);
        assert!(g.does_60_seconds_need_this_car(cheetah));
    }
}
