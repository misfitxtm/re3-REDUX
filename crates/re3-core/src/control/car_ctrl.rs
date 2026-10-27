//! `CarCtrl.cpp`, part 1: the car counters, model choice, interesting cars and removal of far away cars.
use crate::entity::{status, EntityRef, Kind, PoolKind};
use crate::general::Rng;
use crate::modelinfo::indices_gta3::mi;
use crate::ped::ctx::Px;
use crate::ped::population::{CarCounts, ZoneInfo};
use crate::vehicle::base::{driving_style, MISSION_VEHICLE, PARKED_VEHICLE, PERMANENT_VEHICLE, RANDOM_VEHICLE};
use crate::world::World;
use glam::Vec3;

pub const LANE_WIDTH: f32 = 5.0;
pub const DISTANCE_TO_SPAWN_ROADBLOCK_PEDS: f32 = 51.0;
pub const MAX_CARS_TO_KEEP: usize = 2;
pub const MAX_CAR_MODELS_IN_ARRAY: usize = 256;
pub const GAME_SPEED_TO_METERS_PER_SECOND: f32 = 50.0;
pub const METERS_PER_SECOND_TO_GAME_SPEED: f32 = 1.0 / GAME_SPEED_TO_METERS_PER_SECOND;
pub const GAME_SPEED_TO_CARAI_SPEED: f32 = 60.0;
pub const TIME_COPS_WAIT_TO_EXIT_AFTER_STOPPING: u32 = 2500;
/// `DEFAULT_MAX_NUMBER_OF_CARS`.
pub const DEFAULT_MAX_NUMBER_OF_CARS: i32 = 12;
pub const TOTAL_CUSTOM_CLASSES: usize = 7;

/// `eCarClass`.
pub mod car_class {
    pub const POOR: i32 = 0;
    pub const RICH: i32 = 1;
    pub const EXEC: i32 = 2;
    pub const WORKER: i32 = 3;
    pub const SPECIAL: i32 = 4;
    pub const BIG: i32 = 5;
    pub const TAXI: i32 = 6;
    pub const MAFIA: i32 = 8;
    pub const TRIAD: i32 = 9;
    pub const DIABLO: i32 = 10;
    pub const YAKUZA: i32 = 11;
    pub const YARDIE: i32 = 12;
    pub const COLOMB: i32 = 13;
    pub const NINES: i32 = 14;
    pub const GANG8: i32 = 15;
    pub const GANG9: i32 = 16;
    pub const COPS: i32 = 17;
}

/// `CCarCtrl`'s statics.
#[derive(Debug, Clone)]
pub struct CarCtrl {
    pub num_law_enforcer_cars: i32,
    pub num_ambulances_on_duty: i32,
    pub num_firetrucks_on_duty: i32,
    pub num_random_cars: i32,
    pub num_mission_cars: i32,
    pub num_parked_cars: i32,
    pub num_permanent_cars: i32,
    pub cars_generated_around_camera: bool,
    pub car_density_multiplier: f32,
    pub count_down_to_cars_at_start: i8,
    pub max_number_of_cars_in_use: i32,
    pub last_time_law_enforcer_created: u32,
    pub last_time_fire_truck_created: u32,
    pub last_time_ambulance_created: u32,
    pub total_num_of_cars_of_rating: [i32; TOTAL_CUSTOM_CLASSES],
    pub next_car_of_rating: [i32; TOTAL_CUSTOM_CLASSES],
    pub car_arrays: [[i32; MAX_CAR_MODELS_IN_ARRAY]; TOTAL_CUSTOM_CLASSES],
    pub cars_to_keep: [Option<EntityRef>; MAX_CARS_TO_KEEP],
    pub cars_to_keep_time: [u32; MAX_CARS_TO_KEEP],
    /// `CarNumberMultiplier` of the ini file.
    pub car_number_multiplier: f32,
}

impl Default for CarCtrl {
    /// `CCarCtrl::Init` plus the statics' initial values.
    fn default() -> Self {
        Self {
            num_law_enforcer_cars: 0,
            num_ambulances_on_duty: 0,
            num_firetrucks_on_duty: 0,
            num_random_cars: 0,
            num_mission_cars: 0,
            num_parked_cars: 0,
            num_permanent_cars: 0,
            cars_generated_around_camera: false,
            car_density_multiplier: 1.0,
            count_down_to_cars_at_start: 2,
            max_number_of_cars_in_use: DEFAULT_MAX_NUMBER_OF_CARS,
            last_time_law_enforcer_created: 0,
            last_time_fire_truck_created: 0,
            last_time_ambulance_created: 0,
            total_num_of_cars_of_rating: [0; TOTAL_CUSTOM_CLASSES],
            next_car_of_rating: [0; TOTAL_CUSTOM_CLASSES],
            car_arrays: [[0; MAX_CAR_MODELS_IN_ARRAY]; TOTAL_CUSTOM_CLASSES],
            cars_to_keep: [None; MAX_CARS_TO_KEEP],
            cars_to_keep_time: [0; MAX_CARS_TO_KEEP],
            car_number_multiplier: 1.0,
        }
    }
}

impl CarCtrl {
    /// `CCarCtrl::ReInit`: the counters and the interesting cars start over, the model arrays stay.
    pub fn reinit(&mut self) {
        let arrays = (self.car_arrays, self.total_num_of_cars_of_rating, self.max_number_of_cars_in_use, self.car_number_multiplier);
        *self = Self::default();
        (self.car_arrays, self.total_num_of_cars_of_rating, self.max_number_of_cars_in_use, self.car_number_multiplier) = arrays;
    }

    /// What `CPopulation` compares with.
    pub fn counts(&self) -> CarCounts {
        CarCounts {
            num_law_enforcer_cars: self.num_law_enforcer_cars,
            num_random_cars: self.num_random_cars,
            num_firetrucks_on_duty: self.num_firetrucks_on_duty,
            num_ambulances_on_duty: self.num_ambulances_on_duty,
            num_parked_cars: self.num_parked_cars,
            num_mission_cars: self.num_mission_cars,
            max_cars_in_use: self.max_number_of_cars_in_use,
            car_density_multiplier: self.car_density_multiplier,
        }
    }

    /// `AddToCarArray`: the model joins the cars of its class.
    pub fn add_to_car_array(&mut self, id: i32, class: i32) {
        let c = class as usize;
        self.car_arrays[c][self.total_num_of_cars_of_rating[c] as usize] = id;
        self.total_num_of_cars_of_rating[c] += 1;
    }

    /// `ChooseCarModel`: the next model of the class, then a random step on; -1 for a class without cars.
    pub fn choose_car_model(&mut self, rng: &mut Rng, class: i32) -> i32 {
        if !(0..=car_class::TAXI).contains(&class) {
            return -1;
        }
        let c = class as usize;
        let total = self.total_num_of_cars_of_rating[c];
        let model = self.car_arrays[c][self.next_car_of_rating[c] as usize];
        if total == 0 {
            // the original warns and then divides nothing; the index stays where it is
            return model;
        }
        self.next_car_of_rating[c] += rng.range_i32(1, total);
        while self.next_car_of_rating[c] >= total {
            self.next_car_of_rating[c] -= total;
        }
        model
    }

    /// `UpdateCarCount`: a vehicle of this origin appears or goes away.
    pub fn update_car_count(&mut self, created_by: u8, is_law_enforcer: bool, remove: bool) {
        let d = if remove { -1 } else { 1 };
        match created_by {
            RANDOM_VEHICLE => {
                if is_law_enforcer {
                    self.num_law_enforcer_cars += d;
                }
                self.num_random_cars += d;
            }
            MISSION_VEHICLE => self.num_mission_cars += d,
            PARKED_VEHICLE => self.num_parked_cars += d,
            PERMANENT_VEHICLE => self.num_permanent_cars += d,
            _ => {}
        }
    }

    /// `RegisterVehicleOfInterest`: the two most recent ones are kept from being removed.
    pub fn register_vehicle_of_interest(&mut self, v: EntityRef, now: u32) {
        if let Some(i) = self.cars_to_keep.iter().position(|&c| c == Some(v)) {
            self.cars_to_keep_time[i] = now;
            return;
        }
        if let Some(i) = self.cars_to_keep.iter().position(|c| c.is_none()) {
            self.cars_to_keep[i] = Some(v);
            self.cars_to_keep_time[i] = now;
            return;
        }
        let mut oldest = (u32::MAX, 0);
        for i in 0..MAX_CARS_TO_KEEP {
            if self.cars_to_keep[i].is_some() && self.cars_to_keep_time[i] < oldest.0 {
                oldest = (self.cars_to_keep_time[i], i);
            }
        }
        self.cars_to_keep[oldest.1] = Some(v);
        self.cars_to_keep_time[oldest.1] = now;
    }

    pub fn is_this_vehicle_interesting(&self, v: EntityRef) -> bool {
        self.cars_to_keep.contains(&Some(v))
    }

    pub fn remove_from_interesting_vehicle_list(&mut self, v: EntityRef) {
        for c in self.cars_to_keep.iter_mut().filter(|c| **c == Some(v)) {
            *c = None;
        }
    }

    pub fn clear_interesting_vehicle_list(&mut self) {
        self.cars_to_keep = [None; MAX_CARS_TO_KEEP];
    }
}

/// `ThisRoadObjectCouldMove`.
pub fn this_road_object_could_move(indices: &crate::modelinfo::Indices, model: i16) -> bool {
    model == indices.bridge_lift() || model == indices.bridge_road_segment()
}

/// `MapCouldMoveInThisArea`: the bridge moves up and down.
pub fn map_could_move_in_this_area(x: f32, y: f32) -> bool {
    x > -342.0 && x < -219.0 && y > -677.0 && y < -580.0
}

impl World {
    /// `ChooseModel`: a model of a class the zone picks, one that is loaded; the class comes back as well.
    pub fn choose_model(&mut self, px: &mut Px, zone: &ZoneInfo) -> (i32, i32) {
        use car_class::*;
        let mut model = -1;
        let mut class = POOR;
        while model == -1 || !px.hooks.model_loaded(model as i16) {
            let rnd = px.rng.range_i32(0, 1000);
            let t = &zone.car_threshold;
            let gt = &zone.gang_threshold;
            let mut pick_class = |c: i32| {
                class = c;
                c
            };
            model = if rnd < t[0] as i32 {
                let c = pick_class(POOR);
                self.ents.car_ctrl.choose_car_model(px.rng, c)
            } else if rnd < t[1] as i32 {
                let c = pick_class(RICH);
                self.ents.car_ctrl.choose_car_model(px.rng, c)
            } else if rnd < t[2] as i32 {
                let c = pick_class(EXEC);
                self.ents.car_ctrl.choose_car_model(px.rng, c)
            } else if rnd < t[3] as i32 {
                let c = pick_class(WORKER);
                self.ents.car_ctrl.choose_car_model(px.rng, c)
            } else if rnd < t[4] as i32 {
                let c = pick_class(SPECIAL);
                self.ents.car_ctrl.choose_car_model(px.rng, c)
            } else if rnd < t[5] as i32 {
                let c = pick_class(BIG);
                self.ents.car_ctrl.choose_car_model(px.rng, c)
            } else if rnd < zone.cop_threshold as i32 {
                pick_class(COPS);
                self.choose_police_car_model(px)
            } else if let Some(g) = (0..crate::ped::types::NUM_GANGS).find(|&g| rnd < gt[g] as i32) {
                pick_class(MAFIA + g as i32);
                self.choose_gang_car_model(px, g)
            } else {
                let c = pick_class(TAXI);
                self.ents.car_ctrl.choose_car_model(px.rng, c)
            };
        }
        (model, class)
    }

    /// `ChoosePoliceCarModel`: the cars the wanted level calls for, if their models are loaded.
    pub fn choose_police_car_model(&mut self, px: &mut Px) -> i32 {
        let w = self.player_wanted().cloned().unwrap_or_default();
        let loaded = |px: &mut Px, ms: &[i16]| ms.iter().all(|&m| px.hooks.model_loaded(m));
        if w.are_swat_required() && loaded(px, &[mi::ENFORCER, mi::POLICE]) {
            return if px.rng.number() & 0xF == 0 { mi::ENFORCER } else { mi::POLICE } as i32;
        }
        if w.are_fbi_required() && loaded(px, &[mi::FBICAR, mi::FBI]) {
            return mi::FBICAR as i32;
        }
        if w.are_army_required() && loaded(px, &[mi::RHINO, mi::BARRACKS, mi::ARMY]) {
            return if px.rng.true_false() { mi::BARRACKS } else { mi::RHINO } as i32;
        }
        mi::POLICE as i32
    }

    /// `ChooseGangCarModel`: the gang's car if both its ped models are loaded, else -1.
    pub fn choose_gang_car_model(&mut self, px: &mut Px, gang: usize) -> i32 {
        let first = mi::GANG01 + 2 * gang as i16;
        if px.hooks.model_loaded(first) && px.hooks.model_loaded(first + 1) {
            return self.ents.gangs.0[gang].vehicle_mi;
        }
        -1
    }

    /// An ambulance or fire truck stops being on duty: the flags drop and the counters with them.
    pub fn end_emergency_duty(&mut self, r: EntityRef) {
        let Some(v) = self.try_veh_mut(r) else { return };
        let (amb, fire) = (v.flags.is_ambulance_on_duty, v.flags.is_fire_truck_on_duty);
        v.flags.is_ambulance_on_duty = false;
        v.flags.is_fire_truck_on_duty = false;
        let cc = &mut self.ents.car_ctrl;
        cc.num_ambulances_on_duty -= amb as i32;
        cc.num_firetrucks_on_duty -= fire as i32;
    }

    /// `CVehicle::ChangeLawEnforcerState`: the flag and the count change together.
    pub fn change_law_enforcer_state(&mut self, r: EntityRef, enable: bool) {
        if let Some(d) = self.try_veh_mut(r).and_then(|v| v.change_law_enforcer_state(enable)) {
            self.ents.car_ctrl.num_law_enforcer_cars += d;
        }
    }

    /// `CountCarsOfType`.
    pub fn count_cars_of_type(&self, model: i16) -> i32 {
        self.ents.pools.vehicles.iter().filter(|(_, e)| e.base.model_index == model).count() as i32
    }

    /// `SwitchVehicleToRealPhysics`: a car that left the rails starts cruising on its own.
    pub fn switch_vehicle_to_real_physics(&mut self, r: EntityRef) {
        let now = self.now();
        if let Some(v) = self.try_veh_mut(r) {
            v.auto_pilot.car_mission = crate::vehicle::base::car_mission::CRUISE;
            v.auto_pilot.temp_action = crate::vehicle::base::temp_action::NONE;
            v.auto_pilot.anti_reverse_timer = now;
            v.auto_pilot.time_temp_action = now;
        }
    }

    /// `RemoveDistantCars`: every vehicle gets its chance to be removed; roadblock cars get their cops once the player is near.
    pub fn remove_distant_cars(&mut self, px: &mut Px) {
        let centre = self.find_player_centre_of_world(px);
        let pool = &self.ents.pools.vehicles;
        let refs: Vec<EntityRef> = pool.iter().map(|(i, _)| EntityRef { pool: PoolKind::Vehicle, handle: pool.handle_of(i) }).collect();
        for r in refs.into_iter().rev() {
            if self.get(r).is_none() {
                continue;
            }
            self.possibly_remove_vehicle(px, r);
            if self.get(r).is_some_and(|e| matches!(&e.kind, Kind::Vehicle(v) if v.flags.create_road_block_peds)) && (self.epos(r) - centre).truncate().length() < DISTANCE_TO_SPAWN_ROADBLOCK_PEDS {
                self.generate_road_block_cops_for_car(px, r);
                if let Some(v) = self.try_veh_mut(r) {
                    v.flags.create_road_block_peds = false;
                }
            }
        }
    }

    /// `PossiblyRemoveVehicle`: far, unseen, wrecked for long, or abandoned in traffic: the car goes away.
    pub fn possibly_remove_vehicle(&mut self, px: &mut Px, r: EntityRef) {
        // FIX_BUGS: a locked vehicle stays
        if self.try_veh(r).is_none_or(|v| v.flags.is_locked) {
            return;
        }
        let centre = self.find_player_centre_of_world(px);
        let now = self.now();
        let pos = self.epos(r);
        let interesting = self.ents.car_ctrl.is_this_vehicle_interesting(r);
        let can_be_deleted = |w: &World| w.try_veh(r).is_some_and(|v| v.can_be_deleted(|p| w.try_ped(p).is_some_and(|p| p.can_be_deleted())));
        if !interesting && can_be_deleted(self) && !self.ents.cranes.is_car_being_targeted(r) {
            let v = self.try_veh(r).expect("vehicle");
            if v.flags.fade_out && v.clump_alpha == 0 {
                self.remove(r);
                self.delete(r);
                return;
            }
            let distance = (pos - centre).truncate().length();
            let gdm = px.hooks.generation_dist_multiplier();
            let cam = px.hooks.camera();
            let on_screen = px.hooks.is_on_screen(r);
            let model = self.ent(r).base.model_index;
            let v = self.try_veh(r).expect("vehicle");
            let mut threshold = 50.0;
            if on_screen || cam.looking_left_or_right || cam.looking_behind || cam.look_direction_zero || v.created_by == PARKED_VEHICLE || model == mi::AMBULAN || model == mi::FIRETRUCK || v.flags.is_law_enforcer || v.flags.is_car_park_vehicle {
                threshold = 130.0 * gdm;
            }
            if v.flags.extended_range {
                threshold *= 1.5;
            }
            if distance > threshold && !px.hooks.point_in_hide_out_garage(pos) {
                if on_screen && px.hooks.cull_zone_visible(r) {
                    self.try_veh_mut(r).expect("vehicle").flags.fade_out = true;
                } else {
                    self.remove(r);
                    self.delete(r);
                }
                return;
            }
        }
        let v = self.try_veh(r).expect("vehicle");
        let st = self.ent(r).base.status;
        let on_screen = px.hooks.is_on_screen(r);
        let started = now.wrapping_sub(v.auto_pilot.time_to_start_mission) > 5000;
        if (st == status::SIMPLE || (st == status::PHYSICS && v.auto_pilot.driving_style == driving_style::STOP_FOR_CARS))
            && started
            && !on_screen
            && (pos - centre).truncate().length() > 25.0
            && !interesting
            && !v.flags.is_locked
            && can_be_deleted(self)
            && !self.should_car_stop_for_light(r, true)
            && !self.should_car_stop_for_bridge(r)
            && !px.hooks.point_in_hide_out_garage(pos)
        {
            self.remove(r);
            self.delete(r);
            return;
        }
        let v = self.try_veh(r).expect("vehicle");
        if st == status::WRECKED && v.time_of_death != 0 && now > v.time_of_death.wrapping_add(60000) && !(on_screen && px.hooks.cull_zone_visible(r)) && (pos - centre).length_squared() > 7.5 * 7.5 && !px.hooks.point_in_hide_out_garage(pos) {
            self.remove(r);
            self.delete(r);
        }
    }

    /// The position of the player's centre of the world for the generation code.
    pub fn car_generation_centre(&mut self, px: &mut Px) -> Vec3 {
        self.find_player_centre_of_world(px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_follow_the_origin() {
        let mut c = CarCtrl::default();
        c.update_car_count(RANDOM_VEHICLE, true, false);
        c.update_car_count(RANDOM_VEHICLE, false, false);
        c.update_car_count(MISSION_VEHICLE, false, false);
        c.update_car_count(PARKED_VEHICLE, false, false);
        c.update_car_count(PERMANENT_VEHICLE, false, false);
        assert_eq!((c.num_random_cars, c.num_law_enforcer_cars, c.num_mission_cars, c.num_parked_cars, c.num_permanent_cars), (2, 1, 1, 1, 1));
        c.update_car_count(RANDOM_VEHICLE, true, true);
        assert_eq!((c.num_random_cars, c.num_law_enforcer_cars), (1, 0));
    }

    #[test]
    fn a_class_hands_out_its_models_in_a_random_walk() {
        let mut c = CarCtrl::default();
        for m in [100, 101, 102] {
            c.add_to_car_array(m, car_class::RICH);
        }
        let mut rng = Rng::default();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..30 {
            let m = c.choose_car_model(&mut rng, car_class::RICH);
            assert!((100..=102).contains(&m));
            seen.insert(m);
        }
        assert_eq!(seen.len(), 3);
        // a class with no cars, and a gang class, give nothing sensible
        assert_eq!(c.choose_car_model(&mut rng, car_class::MAFIA), -1);
    }

    #[test]
    fn reinit_keeps_the_model_lists() {
        let mut c = CarCtrl::default();
        c.add_to_car_array(100, car_class::POOR);
        c.num_random_cars = 5;
        c.next_car_of_rating[0] = 1;
        c.reinit();
        assert_eq!((c.num_random_cars, c.next_car_of_rating[0], c.total_num_of_cars_of_rating[0], c.car_arrays[0][0]), (0, 0, 1, 100));
    }

    #[test]
    fn only_the_two_latest_cars_are_kept_of_interest() {
        let mut c = CarCtrl::default();
        let r = |h| EntityRef { pool: PoolKind::Vehicle, handle: h };
        c.register_vehicle_of_interest(r(1), 10);
        c.register_vehicle_of_interest(r(2), 20);
        c.register_vehicle_of_interest(r(1), 30);
        c.register_vehicle_of_interest(r(3), 40);
        // 2 was the oldest
        assert!(c.is_this_vehicle_interesting(r(1)) && c.is_this_vehicle_interesting(r(3)) && !c.is_this_vehicle_interesting(r(2)));
        c.remove_from_interesting_vehicle_list(r(1));
        assert!(!c.is_this_vehicle_interesting(r(1)));
        c.clear_interesting_vehicle_list();
        assert!(!c.is_this_vehicle_interesting(r(3)));
    }

    #[test]
    fn the_world_counts_vehicles_in_and_out() {
        use crate::entity::physical::Physical;
        use crate::entity::{Ent, Entity, EntityType};
        use crate::vehicle::base::{VehicleData, VehicleType};
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let mut rng = Rng::default();
        let mut v = VehicleData::new(RANDOM_VEHICLE, 0, VehicleType::Car, &mut rng);
        v.flags.is_law_enforcer = true;
        v.flags.is_ambulance_on_duty = true;
        w.ents.car_ctrl.num_ambulances_on_duty = 1;
        let r = w.insert(Ent::new(Entity::new(EntityType::Vehicle, &mut rng), Some(Physical::default()), Kind::Vehicle(Box::new(v)))).unwrap();
        assert_eq!((w.ents.car_ctrl.num_random_cars, w.ents.car_ctrl.num_law_enforcer_cars), (1, 1));
        w.change_law_enforcer_state(r, false);
        assert_eq!(w.ents.car_ctrl.num_law_enforcer_cars, 0);
        w.change_law_enforcer_state(r, false);
        assert_eq!(w.ents.car_ctrl.num_law_enforcer_cars, 0);
        w.delete(r);
        let cc = &w.ents.car_ctrl;
        assert_eq!((cc.num_random_cars, cc.num_ambulances_on_duty), (0, 0));
    }
}
