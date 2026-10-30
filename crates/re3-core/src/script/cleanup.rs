//! `CTheScripts::CleanUpThisPed/Vehicle/Object` and `CMissionCleanup::Process`: what a mission leaves behind when it ends.
use super::checks::{StuckCarCheck, UpsideDownCarCheck};
use super::{ScriptHooks, CLEANUP_CAR, CLEANUP_CHAR, CLEANUP_OBJECT, CLEANUP_UNUSED, MAX_CLEANUP};
use crate::entity::object::CreatedBy;
use crate::entity::{EntityRef, Kind};
use crate::ped::ctx::Px;
use crate::ped::data::{objective, state as ped_state, MISSION_CHAR, RANDOM_CHAR};
use crate::ped::types::ped_type;
use crate::vehicle::base::{car_mission, MISSION_VEHICLE, RANDOM_VEHICLE};
use crate::world::World;

impl World {
    /// `UndoEntityInvisibilitySettings`: everything the script hid is shown again.
    pub fn undo_entity_invisibility_settings(&mut self) {
        for r in std::mem::take(&mut self.ents.scripts.invisibility_settings).into_iter().flatten() {
            if let Some(e) = self.get_mut(r) {
                e.base.flags.is_visible = true;
            }
        }
    }

    /// `CleanUpThisPed`: a mission ped becomes an ordinary one again.
    pub fn clean_up_this_ped(&mut self, px: &mut Px, ped: EntityRef) {
        if self.try_ped(ped).is_none_or(|p| p.char_created_by != MISSION_CHAR) {
            return;
        }
        let now = self.now();
        let pd = self.ped_mut(ped);
        pd.char_created_by = RANDOM_CHAR;
        if pd.ped_type == ped_type::PROSTITUTE {
            pd.objective_timer = now + 30000;
        }
        if let (true, Some(car)) = (self.ped(ped).in_vehicle, self.ped(ped).my_vehicle) {
            let is_car = self.car_ref(car).is_some();
            if self.try_veh(car).is_some_and(|v| v.driver == Some(ped)) {
                if is_car {
                    self.join_car_with_road_system(px, car);
                    self.try_veh_mut(car).unwrap().auto_pilot.car_mission = car_mission::CRUISE;
                }
            } else if is_car {
                self.set_objective_entity(px, ped, objective::LEAVE_CAR, car);
                self.ped_mut(ped).flags.wander_path_after_exiting_car = true;
            }
        }
        let pd = self.ped(ped);
        let flees = matches!(pd.ped_state, ped_state::FLEE_ENTITY | ped_state::FLEE_POS).then_some((pd.ped_state, pd.move_state));
        self.clear_objective(px, ped);
        let pd = self.ped_mut(ped);
        pd.flags.responds_to_threats = true;
        pd.flags.script_objective_completed = false;
        self.clear_leader(px, ped);
        if self.ped(ped).is_ped_in_control() {
            let path = (px.rng.number() & 7) as i8;
            self.set_wander_path(px, ped, path);
        }
        if let Some((state, move_state)) = flees {
            let pd = self.ped_mut(ped);
            pd.ped_state = state;
            pd.set_move_state(move_state);
        }
        self.ents.population.total_mission_peds = self.ents.population.total_mission_peds.saturating_sub(1);
    }

    /// `CleanUpThisVehicle`.
    pub fn clean_up_this_vehicle(&mut self, car: EntityRef) {
        if self.try_veh(car).is_none_or(|v| v.created_by != MISSION_VEHICLE) {
            return;
        }
        let v = self.try_veh_mut(car).unwrap();
        v.flags.is_locked = false;
        v.created_by = RANDOM_VEHICLE;
        self.ents.car_ctrl.remove_from_interesting_vehicle_list(car);
        self.ents.car_ctrl.num_random_cars += 1;
        self.ents.car_ctrl.num_mission_cars -= 1;
    }

    /// `CleanUpThisObject`: the object stays for twenty seconds and goes.
    pub fn clean_up_this_object(&mut self, obj: EntityRef) {
        let now = self.now();
        let Some(Kind::Object(o)) = self.get_mut(obj).map(|e| &mut e.kind) else { return };
        if o.created_by != CreatedBy::Mission {
            return;
        }
        o.created_by = CreatedBy::Temp;
        o.end_of_life_time = now + 20000;
        o.ref_model_index = -1;
        o.use_vehicle_colours = false;
    }

    /// `CMissionCleanup::Process`: the world goes back to normal and everything the mission made is let go.
    pub fn process_mission_cleanup(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks) {
        self.ents.population.all_random_peds_this_type = -1;
        self.ents.population.ped_density_multiplier = 1.0;
        self.ents.car_ctrl.car_density_multiplier = 1.0;
        if let Some(w) = self.player_wanted_mut() {
            w.crime_sensitivity = 1.0;
            w.ignored_by_cops = false;
            w.ignored_by_everyone = false;
        }
        // camera restored and widescreen off, mission audio, weather, streaming requests and the help message are all outside the logic
        sh.end_of_mission_housekeeping();
        self.ents.onscreen_timer.disabled = false;
        self.make_player_safe(px, false);
        self.ents.scripts.store_vehicle_index = -1;
        self.ents.scripts.store_vehicle_was_random = true;
        self.ents.script_checks.upside_down = UpsideDownCarCheck::default();
        self.ents.script_checks.stuck = StuckCarCheck::default();
        for i in 0..MAX_CLEANUP {
            let (ty, id) = self.ents.scripts.cleanup.entities[i];
            match ty {
                CLEANUP_UNUSED => continue,
                CLEANUP_CAR => {
                    if let Some(v) = self.script_vehicle(id) {
                        self.clean_up_this_vehicle(v);
                    }
                }
                CLEANUP_CHAR => {
                    if let Some(p) = self.script_ped(id) {
                        self.clean_up_this_ped(px, p);
                    }
                }
                CLEANUP_OBJECT => {
                    if let Some(o) = self.script_object(id) {
                        self.clean_up_this_object(o);
                    }
                }
                _ => {}
            }
            self.ents.scripts.cleanup.entities[i] = (CLEANUP_UNUSED, 0);
            self.ents.scripts.cleanup.count -= 1;
        }
    }
}
