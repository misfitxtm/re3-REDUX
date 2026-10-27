//! `EmergencyPed.cpp`: medics reviving the dead and firemen putting out fires.
use super::ctx::Px;
use super::data::*;
use super::types::ped_type;
use crate::anim::{assoc_group, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::general::{limit_angle, radian_angle_between_points};
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::base::{car_mission, driving_style};
use crate::world::World;
use glam::Vec3;
use std::f32::consts::FRAC_PI_4;

/// `EmergencyPedState`.
pub mod emergency_state {
    pub const READY: u16 = 0x0;
    pub const DETERMINE_NEXT_STATE: u16 = 0x1;
    pub const START_CPR: u16 = 0x2;
    pub const FACE_TO_PATIENT: u16 = 0x10;
    pub const PERFORM_CPR: u16 = 0x20;
    pub const STOP_CPR: u16 = 0x40;
    pub const STAND_STILL: u16 = 0x80;
    pub const STOP: u16 = 0x100;
}

/// The fields `CEmergencyPed` adds to `CPed`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmergencyData {
    pub revived_ped: Option<EntityRef>,
    pub state: u16,
    pub attended_accident: Option<usize>,
    pub attended_fire: Option<usize>,
    pub started_cpr: bool,
}

impl World {
    fn emergency(&self, r: EntityRef) -> &EmergencyData {
        self.ped(r).emergency.as_deref().expect("emergency data")
    }

    fn emergency_mut(&mut self, r: EntityRef) -> &mut EmergencyData {
        self.ped_mut(r).emergency.get_or_insert_with(Default::default)
    }

    /// `new CEmergencyPed(type)` with the model's skeleton at hand.
    pub fn create_emergency(&mut self, px: &mut Px, kind: usize, clump: Option<super::clump::PedClump>) -> Option<EntityRef> {
        let model = if kind == ped_type::FIREMAN { mi::FIREMAN } else { mi::MEDIC };
        let r = self.create_ped(px, kind, model, clump)?;
        let pd = self.ped_mut(r);
        pd.class = PedClass::Emergency;
        pd.emergency = Some(Box::default());
        Some(r)
    }

    /// `CEmergencyPed::InRange`: a victim within 30 m of the ambulance, or any if there is none.
    pub fn emergency_in_range(&self, r: EntityRef, victim: EntityRef) -> bool {
        match self.ped(r).my_vehicle {
            None => true,
            Some(v) => (self.epos(v) - self.epos(victim)).length() <= 30.0,
        }
    }

    /// The point between a victim's middle and head, where a medic goes to.
    fn victim_centre(&self, victim: EntityRef) -> Vec3 {
        (self.ped_node_position(victim, node::HEAD) + self.ped_node_position(victim, node::MID)) * 0.5
    }

    /// `CEmergencyPed::ProcessControl`.
    pub fn emergency_process_control(&mut self, px: &mut Px, r: EntityRef) {
        let zone = self.ent(r).phys.as_ref().map_or(0, |p| p.zone_level);
        if zone > 0 && zone as i32 != self.ents.collision_in_memory {
            return;
        }
        self.process_control(px, r);
        if self.ent(r).base.flags.was_postponed || self.ped(r).dying_or_dead() {
            return;
        }
        self.update_current_weapon(r);
        if self.ped(r).is_ped_in_control() && self.ped(r).moved.length() > 0.0 {
            self.avoid(px, r);
        }
        match self.ped(r).ped_state {
            state::SEEK_POS => {
                self.seek(px, r);
            }
            state::SEEK_ENTITY => match self.ped(r).seek_target {
                Some(t) => {
                    self.ped_mut(r).seek_pos = self.epos(t);
                    self.seek(px, r);
                }
                None => self.clear_seek(px, r),
            },
            _ => {}
        }
        match self.ped(r).ped_type {
            ped_type::EMERGENCY if self.ped(r).is_ped_in_control() || self.ped(r).ped_state == state::DRIVING => self.medic_ai(px, r),
            ped_type::FIREMAN if self.ped(r).is_ped_in_control() => self.fireman_ai(px, r),
            _ => {}
        }
    }

    /// `CEmergencyPed::FiremanAI` (with the original's fixes).
    pub fn fireman_ai(&mut self, px: &mut Px, r: EntityRef) {
        let pos = self.epos(r);
        let (nearest, fire_dist) = self.ents.fires.find_nearest(pos);
        match self.emergency(r).state {
            emergency_state::READY => {
                if let Some(f) = nearest {
                    self.send_fireman(px, r, f);
                    self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
                    self.emergency_mut(r).attended_fire = Some(f);
                    self.ents.fires.fires[f].firemen_putting_out += 1;
                }
            }
            emergency_state::DETERMINE_NEXT_STATE => {
                let attended = self.emergency(r).attended_fire;
                if let Some(f) = nearest.filter(|&f| Some(f) != attended) {
                    self.send_fireman(px, r, f);
                    if let Some(old) = attended {
                        let n = &mut self.ents.fires.fires[old].firemen_putting_out;
                        *n = n.saturating_sub(1);
                    }
                    self.ents.fires.fires[f].firemen_putting_out += 1;
                    self.emergency_mut(r).attended_fire = Some(f);
                } else if nearest.is_none() {
                    self.emergency_mut(r).state = emergency_state::STOP;
                }
                // "extinguishes" the fire, overriding the stop decision when the nearest fire is the attended one
                if fire_dist < 5.0 {
                    self.set_idle(px, r);
                    self.emergency_mut(r).state = emergency_state::STAND_STILL;
                }
            }
            emergency_state::STAND_STILL => {
                let attended = self.emergency(r).attended_fire;
                if attended.is_none_or(|f| !self.ents.fires.fires[f].is_ongoing) {
                    self.emergency_mut(r).state = emergency_state::STOP;
                }
                if let Some(f) = nearest
                    && Some(f) != attended
                    && (self.ents.fires.fires[f].pos - pos).length() < 30.0
                {
                    self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
                }
                self.say(px, r, sound::PED_EXTINGUISHING_FIRE);
            }
            emergency_state::STOP => {
                self.ped_mut(r).flags.is_running = false;
                if let Some(f) = self.emergency(r).attended_fire {
                    let n = &mut self.ents.fires.fires[f].firemen_putting_out;
                    *n = n.saturating_sub(1);
                }
                self.ped_mut(r).ped_state = state::NONE;
                let dir = (px.rng.number() & 7) as i8;
                self.set_wander_path(px, r, dir);
                self.emergency_mut(r).attended_fire = None;
                self.emergency_mut(r).state = emergency_state::READY;
                self.ped_mut(r).set_move_state(move_state::WALK);
            }
            _ => {}
        }
    }

    fn send_fireman(&mut self, px: &mut Px, r: EntityRef, fire: usize) {
        self.ped_mut(r).ped_state = state::NONE;
        let pos = self.ents.fires.fires[fire].pos;
        self.set_seek_pos(px, r, pos, 1.0);
        let pd = self.ped_mut(r);
        pd.set_move_state(move_state::RUN);
        pd.flags.is_running = true;
    }

    /// `CEmergencyPed::MedicAI`.
    pub fn medic_ai(&mut self, px: &mut Px, r: EntityRef) {
        if !self.ped(r).in_vehicle && self.ped(r).is_ped_in_control() {
            self.scan_for_threats(px, r);
            if let Some(t) = self.ped(r).threat_entity.filter(|&t| self.try_ped(t).is_some_and(|d| d.is_player())) {
                let pd = self.ped(t);
                if pd.weapons[pd.current_weapon as usize].is_type_melee() {
                    self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, t);
                } else {
                    self.set_flee(px, r, t, 6000);
                    self.say(px, r, sound::PED_FLEE_SPRINT);
                }
                return;
            }
        }
        if self.ped(r).in_vehicle {
            self.medic_in_vehicle(px, r);
        }
        if !self.ped(r).is_ped_in_control() {
            return;
        }
        match self.emergency(r).state {
            emergency_state::READY => self.medic_ready(px, r),
            emergency_state::DETERMINE_NEXT_STATE => self.medic_determine_next(px, r),
            emergency_state::START_CPR => self.medic_start_cpr(px, r),
            emergency_state::FACE_TO_PATIENT => self.medic_face_patient(px, r),
            emergency_state::PERFORM_CPR => self.medic_perform_cpr(px, r),
            emergency_state::STOP_CPR => {
                self.emergency_mut(r).state = emergency_state::STOP;
                self.ped_mut(r).flags.is_ducking = true;
            }
            emergency_state::STAND_STILL => {
                let revived = self.emergency(r).revived_ped;
                if revived.is_none_or(|p| self.try_ped(p).is_none_or(|d| d.health > 0.0)) {
                    self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
                } else if let Some(a) = self.emergency(r).attended_accident.map(|a| self.ents.accidents.accidents[a])
                    && (a.victim.is_none() || a.medics_performing_cpr == 0 || self.ents.accidents.unattended())
                {
                    self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
                }
            }
            emergency_state::STOP => {
                self.emergency_mut(r).started_cpr = false;
                self.ped_mut(r).ped_state = state::NONE;
                if let Some(a) = self.emergency_mut(r).attended_accident.take() {
                    let acc = &mut self.ents.accidents.accidents[a];
                    acc.victim = None;
                    acc.medics_attending = acc.medics_attending.saturating_sub(1);
                }
                let dir = (px.rng.number() & 7) as i8;
                self.set_wander_path(px, r, dir);
                self.emergency_mut(r).revived_ped = None;
                self.emergency_mut(r).state = emergency_state::READY;
                self.ped_mut(r).set_move_state(move_state::WALK);
            }
            _ => {}
        }
    }

    /// A medic in an ambulance stops at an accident, or drives off when everyone is aboard.
    fn medic_in_vehicle(&mut self, px: &mut Px, r: EntityRef) {
        let Some(veh) = self.ped(r).my_vehicle else { return };
        if !self.try_veh(veh).is_some_and(|v| v.is_car()) || self.ped(r).objective == objective::LEAVE_CAR {
            return;
        }
        let (near, dist) = self.find_nearest_accident(self.epos(veh));
        let speed = self.ent(veh).phys.as_ref().map_or(0.0, |p| p.move_speed.length());
        if near.is_some() && dist < 25.0 && speed < 0.01 {
            if let Some(v) = self.try_veh_mut(veh) {
                v.auto_pilot.car_mission = car_mission::NONE;
            }
            self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            self.say(px, r, sound::PED_LEAVE_VEHICLE);
        } else if self.try_veh(veh).is_some_and(|v| v.driver == Some(r) && v.auto_pilot.car_mission == car_mission::NONE) && self.ped(r).ped_state == state::DRIVING && px.rng.number() & 31 == 0 {
            let pd = self.ped(r);
            let wait = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().any(|&n| {
                self.try_ped(n).is_some_and(|d| {
                    d.ped_type == ped_type::EMERGENCY && matches!(d.ped_state, state::SEEK_CAR | state::ENTER_CAR) && d.my_vehicle == Some(veh)
                })
            });
            if !wait {
                px.hooks.join_car_with_road_system(veh);
                let on_duty = {
                    let v = self.try_veh_mut(veh).expect("vehicle");
                    v.auto_pilot.car_mission = car_mission::CRUISE;
                    v.siren_or_alarm = false;
                    v.auto_pilot.cruise_speed = 12;
                    v.auto_pilot.driving_style = driving_style::SLOW_DOWN_FOR_CARS;
                    std::mem::replace(&mut v.flags.is_ambulance_on_duty, false)
                };
                if on_duty {
                    self.ents.car_ctrl.num_ambulances_on_duty -= 1;
                }
            }
        }
    }

    fn medic_ready(&mut self, px: &mut Px, r: EntityRef) {
        let pos = self.epos(r);
        let (nearest, _) = self.find_nearest_accident(pos);
        if let Some(a) = nearest {
            let victim = self.ents.accidents.accidents[a].victim.expect("victim");
            let centre = self.victim_centre(victim);
            self.emergency_mut(r).revived_ped = Some(victim);
            self.set_seek_pos(px, r, centre, 1.0);
            self.set_objective(px, r, objective::NONE);
            self.ped_mut(r).flags.is_running = true;
            self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
            self.emergency_mut(r).attended_accident = Some(a);
            self.ents.accidents.accidents[a].medics_attending += 1;
            return;
        }
        let Some(veh) = self.ped(r).my_vehicle else {
            if self.ped(r).ped_state != state::WANDER_PATH {
                let dir = (px.rng.number() & 7) as i8;
                self.set_wander_path(px, r, dir);
            }
            return;
        };
        if self.ped(r).in_vehicle {
            return;
        }
        let (driver, getting_in) = self.try_veh(veh).map_or((None, 0), |v| (v.driver, v.getting_in_flags));
        let objective = self.ped(r).objective;
        if objective == super::data::objective::ENTER_CAR_AS_DRIVER || driver.is_some() || getting_in != 0 {
            if let Some(d) = driver.filter(|&d| self.ped(d).ped_type != ped_type::EMERGENCY).filter(|_| objective != super::data::objective::KILL_CHAR_ON_FOOT) {
                self.set_objective_entity(px, r, super::data::objective::KILL_CHAR_ON_FOOT, d);
            } else if !matches!(objective, super::data::objective::ENTER_CAR_AS_DRIVER | super::data::objective::ENTER_CAR_AS_PASSENGER | super::data::objective::KILL_CHAR_ON_FOOT) {
                self.set_objective_entity(px, r, super::data::objective::ENTER_CAR_AS_PASSENGER, veh);
            }
        } else {
            self.set_objective_entity(px, r, super::data::objective::ENTER_CAR_AS_DRIVER, veh);
        }
    }

    fn medic_determine_next(&mut self, px: &mut Px, r: EntityRef) {
        let pos = self.epos(r);
        let (nearest, dist) = self.find_nearest_accident(pos);
        match nearest {
            Some(a) => {
                let attended = self.emergency(r).attended_accident;
                if Some(a) != attended || self.ped(r).ped_state != state::SEEK_POS {
                    let victim = self.ents.accidents.accidents[a].victim.expect("victim");
                    self.emergency_mut(r).revived_ped = Some(victim);
                    if !self.emergency_in_range(r, victim) {
                        self.emergency_mut(r).state = emergency_state::STOP;
                        return;
                    }
                    let centre = self.victim_centre(victim);
                    let cpr = self.ents.accidents.accidents[a].medics_performing_cpr;
                    self.set_seek_pos(px, r, centre, cpr as f32 * 0.5 + 1.0);
                    self.set_objective(px, r, objective::NONE);
                    self.ped_mut(r).flags.is_running = true;
                    if let Some(old) = attended {
                        let n = &mut self.ents.accidents.accidents[old].medics_attending;
                        *n = n.saturating_sub(1);
                    }
                    self.ents.accidents.accidents[a].medics_attending += 1;
                    self.emergency_mut(r).attended_accident = Some(a);
                }
            }
            None => {
                self.emergency_mut(r).state = emergency_state::STOP;
                self.ped_mut(r).flags.is_running = false;
            }
        }
        if dist < 5.0 {
            let Some(victim) = self.emergency(r).revived_ped else { return };
            if self.try_ped(victim).is_some_and(|d| d.fire.is_some()) {
                self.ped_mut(r).flags.is_running = false;
                self.ped_mut(r).set_move_state(move_state::STILL);
            } else if dist < 4.5 {
                self.ped_mut(r).flags.is_running = false;
                self.ped_mut(r).set_move_state(move_state::WALK);
                let cpr_going = self.emergency(r).attended_accident.is_some_and(|a| self.ents.accidents.accidents[a].medics_performing_cpr != 0);
                if dist < 1.0 || cpr_going {
                    self.emergency_mut(r).state = emergency_state::START_CPR;
                }
            }
        }
    }

    fn medic_start_cpr(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let Some(patient) = self.emergency(r).revived_ped.filter(|&p| self.try_ped(p).is_some_and(|d| d.health <= 0.0 && !d.flags.fade_out)) else {
            self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
            return;
        };
        self.ped_mut(patient).bloody_footprint_count_or_death_time = now;
        let pd = self.ped_mut(r);
        pd.set_move_state(move_state::STILL);
        pd.ped_state = state::CPR;
        pd.last_ped_state = state::CPR;
        self.set_look_flag_entity(r, patient, false);
        self.set_look_timer(r, 500);
        self.say(px, r, sound::PED_HEALING);
        let attended = self.emergency(r).attended_accident.expect("accident");
        if self.ents.accidents.accidents[attended].medics_performing_cpr != 0 {
            self.set_idle(px, r);
            self.emergency_mut(r).state = emergency_state::STAND_STILL;
        } else {
            self.emergency_mut(r).state = emergency_state::FACE_TO_PATIENT;
            self.ped_blend_anim(r, assoc_group::STD, anim::MEDIC_CPR, 4.0);
            let pd = self.ped_mut(r);
            pd.vehicle_anim = Some(anim::MEDIC_CPR as i32);
            pd.flags.is_ducking = true;
        }
        self.set_look_timer(r, 2000);
        self.ents.accidents.accidents[attended].medics_performing_cpr += 1;
        self.emergency_mut(r).started_cpr = true;
    }

    /// Turns the medic towards the patient; true once close enough to the right heading.
    fn face_patient(&mut self, r: EntityRef, patient: EntityRef) {
        let c = self.victim_centre(patient);
        let me = self.epos(r);
        let dest = limit_angle(radian_angle_between_points(c.x, c.y, me.x, me.y));
        let pd = self.ped_mut(r);
        pd.rotation_dest = dest;
        pd.look_target = Some(patient);
        self.turn_body(r);
    }

    fn medic_face_patient(&mut self, _px: &mut Px, r: EntityRef) {
        let Some(patient) = self.emergency(r).revived_ped.filter(|&p| self.try_ped(p).is_some_and(|d| d.health <= 0.0)) else {
            self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
            return;
        };
        self.face_patient(r, patient);
        let pd = self.ped_mut(r);
        if (pd.rotation_cur - pd.rotation_dest).abs() < FRAC_PI_4 {
            self.emergency_mut(r).state = emergency_state::PERFORM_CPR;
        } else {
            let pd = self.ped_mut(r);
            pd.rotation_cur = (pd.rotation_cur + pd.rotation_dest) * 0.5;
        }
    }

    fn medic_perform_cpr(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let Some(patient) = self.emergency(r).revived_ped.filter(|&p| self.try_ped(p).is_some_and(|d| d.health <= 0.0)) else {
            self.emergency_mut(r).state = emergency_state::DETERMINE_NEXT_STATE;
            return;
        };
        self.face_patient(r, patient);
        if now <= self.ped(r).look_timer {
            self.ped_mut(r).set_move_state(move_state::STILL);
            return;
        }
        self.emergency_mut(r).state = emergency_state::STOP_CPR;
        let pd = self.ped_mut(r);
        pd.ped_state = state::NONE;
        pd.set_move_state(move_state::WALK);
        pd.vehicle_anim = None;
        if self.ped(patient).flags.body_part_just_came_off {
            return;
        }
        let pd = self.ped_mut(patient);
        pd.health = 100.0;
        pd.ped_state = state::NONE;
        pd.last_ped_state = state::WANDER_PATH;
        self.set_get_up(px, patient);
        self.get_mut(patient).expect("ped").base.flags.uses_collision = true;
        self.ped_mut(patient).set_move_state(move_state::WALK);
        self.restart_non_partial_anims(patient);
        let pd = self.ped_mut(patient);
        pd.flags.is_ped_die_anim_playing = false;
        pd.flags.knocked_up_into_air = false;
        pd.colliding_entity = None;
    }
}
