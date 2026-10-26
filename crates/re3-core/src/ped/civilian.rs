//! `CivilianPed.cpp`: how ordinary pedestrians react to threats and spend their frame.
use super::ctx::Px;
use super::data::*;
use super::pending::Investigate;
use super::types::{ped_flag, ped_type};
use crate::audio::sound;
use crate::control::wanted::crime;
use crate::entity::EntityRef;
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::surface;
use crate::world::{Kinds, World};
use glam::{Vec2, Vec3};
use std::f32::consts::TAU;

impl World {
    /// `new CCivilianPed(pedType, mi)` with the model's skeleton at hand.
    pub fn create_civilian(&mut self, px: &mut Px, ped_type: usize, model: i16, clump: Option<super::clump::PedClump>) -> Option<EntityRef> {
        let r = self.create_ped(px, ped_type, model, clump)?;
        self.ped_mut(r).class = PedClass::Civilian;
        Some(r)
    }

    /// `CCivilianPed::CivilianAI`.
    pub fn civilian_ai(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        if now <= pd.flee_timer || pd.objective != objective::NONE && !pd.flags.responds_to_threats || !pd.is_ped_in_control() {
            if pd.objective == objective::GUARD_SPOT {
                return;
            }
            if matches!(pd.objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS) && pd.ped_in_objective.is_some_and(|t| self.try_ped(t).is_some_and(|d| d.is_player())) {
                return;
            }
            if now <= pd.look_timer {
                return;
            }
            let flag = self.scan_for_threats(px, r);
            if flag == ped_flag::EXPLOSION {
                let (p, me) = (self.ped(r).event_or_threat, self.epos(r));
                self.set_look_flag_direction(r, radian_angle_between_points(p.x, p.y, me.x, me.y), true);
                self.set_look_timer(r, 500);
            } else if flag == ped_flag::GUN
                && let Some(t) = self.ped(r).threat_entity
            {
                self.set_look_flag_entity(r, t, true);
                self.set_look_timer(r, 500);
            }
            return;
        }
        let flag = self.scan_for_threats(px, r);
        if flag == ped_flag::GUN {
            self.react_to_gun_threat(px, r);
        } else if flag == ped_flag::DEADPEDS {
            self.react_to_dead_ped(px, r);
        } else if flag == ped_flag::EXPLOSION {
            self.react_to_explosion(px, r);
        } else {
            self.react_to_threatening_ped(px, r);
        }
    }

    fn react_to_gun_threat(&mut self, px: &mut Px, r: EntityRef) {
        let Some(threat) = self.ped(r).threat_entity.filter(|&t| self.is_ped(t)) else { return };
        let threat_dist_sqr = (self.epos(threat) - self.epos(r)).truncate().length_squared();
        let stats = &self.ents.ped_stats.0[self.ped(r).ped_stats];
        let (fear, lawfulness, temper) = (stats.fear, stats.lawfulness, stats.temper);
        if fear <= lawfulness {
            let threat_is_player = self.ped(threat).is_player();
            let pd = self.ped(r);
            let melee = pd.weapons[pd.current_weapon as usize].is_type_melee();
            if temper <= fear {
                if !threat_is_player || !self.run_to_report_crime(px, r, crime::POSSESSION_GUN) {
                    if threat_dist_sqr < 100.0 {
                        self.say(px, r, sound::PED_FLEE_SPRINT);
                        self.set_find_path_and_flee(px, r, threat, 10000, false);
                    } else {
                        let p = self.epos(threat);
                        self.set_find_path_and_flee_pos(r, Vec2::new(p.x, p.y), 5000, true);
                    }
                }
            } else if self.ped(r).objective != objective::NONE || melee {
                self.set_find_path_and_flee(px, r, threat, 5000, false);
                if threat_dist_sqr < 400.0 {
                    self.ped_mut(r).set_move_state(move_state::RUN);
                    self.say(px, r, sound::PED_FLEE_SPRINT);
                } else {
                    self.ped_mut(r).set_move_state(move_state::WALK);
                }
            } else if threat_is_player && self.wanted_info().current_cops != 0 {
                self.set_find_path_and_flee(px, r, threat, 5000, false);
                self.ped_mut(r).set_move_state(if threat_dist_sqr < 100.0 { move_state::RUN } else { move_state::WALK });
            } else {
                self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, threat);
            }
        } else {
            self.say(px, r, sound::PED_FLEE_SPRINT);
            if threat_dist_sqr < 100.0 {
                self.set_find_path_and_flee(px, r, threat, 10000, false);
                self.ped_mut(r).set_move_state(move_state::SPRINT);
            } else {
                self.set_find_path_and_flee(px, r, threat, 5000, false);
                self.ped_mut(r).set_move_state(move_state::RUN);
            }
        }
        self.set_look_flag_entity(r, threat, false);
        self.set_look_timer(r, 500);
    }

    fn react_to_dead_ped(&mut self, px: &mut Px, r: EntityRef) {
        let Some(dead) = self.ped(r).event_entity else { return };
        let dist_sqr = (self.epos(dead) - self.epos(r)).truncate().length_squared();
        let gang = self.ped(r).is_gang_member();
        if gang && self.ped(r).ped_type == self.ped(dead).ped_type {
            if dist_sqr < 25.0 {
                self.set_find_path_and_flee(px, r, dead, 2000, false);
                self.ped_mut(r).set_move_state(move_state::RUN);
            }
        } else if gang || dist_sqr > 25.0 {
            let mut investigate = true;
            if let Some(killer) = self.ped(dead).threat_entity.filter(|&k| self.is_ped(k)) {
                let (k, d) = (self.epos(killer), self.epos(dead));
                if (k - d).truncate().length_squared() < 100.0 {
                    investigate = false;
                }
            }
            if investigate {
                let p = self.epos(dead);
                self.set_investigate_event(px, r, Investigate { event: event_type::DEAD_PED, pos: Vec2::new(p.x, p.y), distance_to_count_done: 1.0, time: 20000, angle: 0.0 });
            }
        } else {
            self.set_find_path_and_flee(px, r, dead, 5000, false);
            self.ped_mut(r).set_move_state(move_state::RUN);
        }
    }

    fn react_to_explosion(&mut self, px: &mut Px, r: EntityRef) {
        let (event, me) = (self.ped(r).event_or_threat, self.epos(r));
        let away = event - Vec2::new(me.x, me.y);
        let dist_sqr = away.length_squared();
        if dist_sqr < 400.0 {
            self.say(px, r, sound::PED_FLEE_SPRINT);
            self.set_flee_pos(r, event, 2000);
            self.set_look_flag_direction(r, radian_angle_between_points(event.x, event.y, me.x, me.y), true);
            self.set_look_timer(r, 500);
        } else if dist_sqr < 1600.0 {
            if self.ped(r).flags.gonna_investigate_event {
                if self.ped(r).char_created_by != MISSION_CHAR && !self.ped(r).is_gang_member() {
                    self.set_investigate_event(px, r, Investigate { event: event_type::EXPLOSION, pos: event, distance_to_count_done: 6.0, time: 30000, angle: 0.0 });
                }
            } else {
                let mut heading = limit_radian_angle(radian_angle_between_points(away.x, away.y, 0.0, 0.0));
                if heading < 0.0 {
                    heading += TAU;
                }
                self.set_wander_path(px, r, (heading / 8.0) as i8);
            }
        }
    }

    /// No gun, corpse or blast: a nearby ped may be someone to fight or to run from.
    fn react_to_threatening_ped(&mut self, px: &mut Px, r: EntityRef) {
        let Some(threat) = self.ped(r).threat_entity.filter(|&t| self.is_ped(t)) else { return };
        let fear = self.ents.ped_stats.0[self.ped(r).ped_stats].fear as i32;
        let their_temper = self.ents.ped_stats.0[self.ped(threat).ped_stats].temper as i32;
        if fear <= 100 - their_temper && self.ped(threat).ped_type != ped_type::COP {
            let (ours, theirs) = (self.ped(r), self.ped(threat));
            let their_weapon_is_melee = theirs.weapons[ours.current_weapon as usize].is_type_melee();
            let our_weapon_is_melee = ours.weapons[ours.current_weapon as usize].is_type_melee();
            if their_weapon_is_melee || !our_weapon_is_melee {
                if theirs.is_player() && self.wanted_info().current_cops != 0 {
                    if matches!(self.ped(r).objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS) {
                        self.set_find_path_and_flee(px, r, threat, 10000, false);
                    }
                } else {
                    self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, threat);
                }
            }
        } else {
            self.set_find_path_and_flee(px, r, threat, 10000, true);
        }
    }

    /// `CPed::RunToReportCrime`: heads for the nearest free phone (which the original cancels on its first frame).
    pub fn run_to_report_crime(&mut self, px: &mut Px, r: EntityRef, crime: u8) -> bool {
        if self.ped(r).ped_state == state::SEEK_POS {
            return false;
        }
        let pos = self.epos(r);
        let Some((phone, stand_at)) = px.hooks.nearest_free_phone(pos) else { return false };
        if !px.hooks.phone_is_free(phone) {
            return false;
        }
        self.ped_mut(r).running_to_phone = true;
        self.set_seek_pos(px, r, stand_at, 1.0);
        let pd = self.ped_mut(r);
        pd.set_move_state(move_state::RUN);
        pd.flags.is_running = true;
        pd.phone_id = phone;
        pd.crime_to_report_on_phone = crime as u32;
        true
    }

    /// `CCivilianPed::ProcessControl`.
    pub fn civilian_process_control(&mut self, px: &mut Px, r: EntityRef) {
        let zone = self.ent(r).phys.as_ref().map_or(0, |p| p.zone_level);
        if zone > 0 && zone as i32 != self.ents.collision_in_memory {
            return;
        }
        self.process_control(px, r);
        if self.ent(r).base.flags.was_postponed || self.ped(r).dying_or_dead() {
            return;
        }
        self.update_current_weapon(r);
        match self.ped(r).ped_state {
            state::WANDER_RANGE | state::WANDER_PATH => {
                if self.ent(r).base.flags.is_visible {
                    self.scan_for_interesting_stuff(px, r);
                }
            }
            state::SEEK_ENTITY | state::SEEK_POS => self.civilian_seek(px, r),
            state::FACE_PHONE => {
                if self.face_phone(px, r) {
                    self.ped_mut(r).ped_state = state::MAKE_CALL;
                }
            }
            state::MAKE_CALL => {
                if self.make_phonecall(px, r) {
                    let dir = (px.rng.number() & 7) as i8;
                    self.set_wander_path(px, r, dir);
                }
            }
            state::MUG => self.mug(px, r),
            state::SOLICIT => self.solicit(px, r),
            state::UNKNOWN => self.idle_in_a_crowd(px, r),
            state::DRIVING => self.hooker_in_car(px, r),
            _ => {}
        }
        if self.ped(r).is_ped_in_control() {
            self.civilian_ai(px, r);
        }
        if self.ped(r).moved.length() > 0.0 {
            self.avoid(px, r);
        }
    }

    /// Seeking a position or an entity as a civilian: what to do on arrival.
    fn civilian_seek(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).ped_state == state::SEEK_ENTITY {
            let Some(t) = self.ped(r).seek_target else {
                self.restore_previous_state(px, r);
                return;
            };
            self.ped_mut(r).seek_pos = self.epos(t);
        }
        if !self.seek(px, r) {
            return;
        }
        let pd = self.ped(r);
        let objective = pd.objective;
        if matches!(objective, objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA) && pd.next_path_node.is_some() {
            self.ped_mut(r).next_path_node = None;
        } else if pd.running_to_phone {
            if !px.hooks.phone_is_free(pd.phone_id) {
                self.restore_previous_state(px, r);
                self.ped_mut(r).phone_id = -1;
            } else {
                px.hooks.phone_start_reporting(pd.phone_id);
                self.ped_mut(r).ped_state = state::FACE_PHONE;
            }
        } else if !matches!(objective, objective::KILL_CHAR_ANY_MEANS | objective::KILL_CHAR_ON_FOOT) {
            let target = pd.ped_in_objective.filter(|&t| self.try_ped(t).is_some());
            if objective == objective::FOLLOW_CHAR_IN_FORMATION {
                if let Some(t) = target
                    && pd.moved.length() == 0.0
                    && self.ped(t).move_state == move_state::STILL
                {
                    self.ped_mut(r).rotation_dest = self.ped(t).rotation_cur;
                }
            } else if objective == objective::GOTO_CHAR_ON_FOOT && target.is_some_and(|t| self.ped(t).move_state != move_state::STILL) {
                let s = self.ped(target.expect("target")).move_state;
                self.ped_mut(r).set_move_state(s);
            } else if matches!(objective, objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA) {
                self.set_idle(px, r);
            } else {
                self.restore_previous_state(px, r);
            }
        }
    }

    /// `PED_UNKNOWN`: stands around while at most four others of the same kind do the same, and pulls wanderers in.
    fn idle_in_a_crowd(&mut self, px: &mut Px, r: EntityRef) {
        self.idle(px, r);
        let pd = self.ped(r);
        let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
        let same = |w: &World, n: EntityRef, s: u8| w.try_ped(n).is_some_and(|d| d.ped_type == w.ped(r).ped_type && d.ped_state == s);
        let in_same_state = near.iter().filter(|&&n| same(self, n, state::UNKNOWN)).count();
        if in_same_state < 5 {
            let wanderers: Vec<EntityRef> = near.into_iter().filter(|&n| same(self, n, state::WANDER_PATH)).collect();
            for n in wanderers {
                self.ped_mut(n).ped_state = state::UNKNOWN;
            }
        }
    }

    /// A prostitute in the player's car: the fun costs money and the car has to shake.
    pub(super) fn hooker_in_car(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        if self.ped(r).ped_type != ped_type::PROSTITUTE || self.ents.player_info.hooker != Some(r) {
            return;
        }
        let Some(veh) = self.ped(r).my_vehicle else { return };
        if now > self.ents.player_info.next_sex_frequency_update_time {
            let driver = self.try_veh(veh).and_then(|v| v.driver).filter(|&d| self.ped(d).is_player() && self.ped(d).ped_state == state::DRIVING);
            if let Some(driver) = driver {
                let pos = self.epos(veh);
                let ground = self.process_vertical_line(pos, -100.0, Kinds { buildings: true, ..Kinds::NONE }, false, None);
                let still = self.ent(veh).phys.as_ref().is_some_and(|p| p.move_speed.length_squared() < 0.0001);
                let soft_ground = ground.is_some_and(|h| !matches!(h.point.surface_b, surface::DEFAULT | surface::TARMAC | surface::PAVEMENT));
                if still && soft_ground {
                    self.shake_car_for_hooker(px, r, veh, driver);
                } else {
                    let info = &mut self.ents.player_info;
                    info.next_sex_frequency_update_time = now + 3000;
                    info.sex_frequency = if info.sex_frequency >= 1000 || info.sex_frequency <= 250 { 1200 } else { 250 };
                }
            } else {
                self.hooker_leaves(px, r, veh);
            }
        }
        if now > self.ents.player_info.next_sex_money_update_time {
            let info = &mut self.ents.player_info;
            if info.money <= 1 {
                info.sex_frequency = 250;
            } else {
                info.money = (info.money - 1).max(0);
            }
            info.next_sex_money_update_time = now + 1000;
        }
    }

    fn hooker_leaves(&mut self, px: &mut Px, r: EntityRef, veh: EntityRef) {
        self.ped_mut(r).flags.wander_path_after_exiting_car = true;
        self.ents.player_info.hooker = None;
        self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
    }

    fn shake_car_for_hooker(&mut self, px: &mut Px, r: EntityRef, veh: EntityRef, driver: EntityRef) {
        let now = self.now();
        let model = self.try_veh(veh).map(|v| v.handling).unwrap_or(0);
        let handling = *self.ents.handling.get(model);
        let roof = self.try_veh(veh).is_some_and(|v| v.car_has_roof(&handling));
        if !roof {
            self.ped_mut(driver).health = 125.0;
            self.hooker_leaves(px, r, veh);
            return;
        }
        let (mass, pos, vpos) = (self.ent(r).phys.as_ref().map_or(70.0, |p| p.mass), self.epos(r), self.epos(veh));
        let jerk = px.rng.range_f32(-0.8, -1.2) * mass;
        let matrix = self.ent(veh).base.matrix;
        if let Some(p) = self.get_mut(veh).and_then(|e| e.phys.as_mut()) {
            p.apply_turn_force(&matrix, Vec3::new(0.0, 0.0, jerk), Vec3::new(pos.x - vpos.x, pos.y - vpos.y, 0.0));
        }
        px.hooks.play_one_shot(veh, sound::CAR_JERK, 0.0);
        let info = &mut self.ents.player_info;
        let frequency = info.sex_frequency;
        if info.money >= 10 && frequency > 250 {
            info.next_sex_frequency_update_time = now + frequency as u32;
            info.sex_frequency = 250.max(frequency - if frequency >= 350 { 30 } else { 10 });
            if info.sex_frequency == 250 {
                info.next_sex_frequency_update_time = now + 3000;
            }
            let d = self.ped_mut(driver);
            d.health = 125.0_f32.min(1.0 + d.health);
        } else {
            self.hooker_leaves(px, r, veh);
        }
    }
}
