//! `PedAI.cpp`: `CPed::ProcessObjective`, what a ped does every frame to reach its objective.
use super::ctx::Px;
use super::data::*;
use super::types::{ped_type, stat_flag};
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::{status, EntityRef};
use crate::general::radian_angle_between_points;
use crate::vehicle::automobile::node;
use crate::vehicle::base::{car_mission, driving_style, RANDOM_VEHICLE};
use crate::weapons::info::{weapon_type as wt, WeaponFire};
use crate::world::{Kinds, LineOpts, World};
use glam::{Vec2, Vec3};

/// Where control goes after one objective's code.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// `break`: on to the "objective done?" check.
    Tail,
    /// `return`: skip that check.
    Return,
}

/// The shot-test ray of the kill objectives starts a bit above and beside the ped's origin.
const HEAD_OFFSET: Vec3 = Vec3::new(0.5, 0.0, 0.6);

impl World {
    /// `CPed::ProcessObjective`.
    pub fn process_objective(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if pd.flags.clear_objective && (pd.is_ped_in_control() || pd.ped_state == state::DRIVING) {
            self.clear_objective(px, r);
            self.ped_mut(r).flags.clear_objective = false;
        }
        self.update_from_leader(px, r);

        let pd = self.ped(r);
        if pd.objective == objective::NONE || !(pd.is_ped_in_control() || pd.ped_state == state::DRIVING) {
            return;
        }
        let our_pos = if pd.in_vehicle {
            let Some(v) = pd.my_vehicle else {
                self.ped_mut(r).in_vehicle = false;
                return;
            };
            self.epos(v)
        } else {
            self.epos(r)
        };
        let mut dist = Vec3::ZERO;
        if let Some(t) = pd.ped_in_objective {
            let tp = self.ped(t);
            let target = match tp.my_vehicle.filter(|_| tp.in_vehicle_now() && tp.ped_state != state::DRAG_FROM_CAR) {
                Some(v) => self.epos(v),
                None => self.epos(t),
            };
            dist = target - our_pos;
        } else if let Some(c) = pd.car_in_objective {
            dist = self.epos(c) - our_pos;
        }

        if self.run_objective(px, r, our_pos, dist) == Flow::Return {
            return;
        }

        let now = self.now();
        let pd = self.ped(r);
        if pd.flags.objective_completed || (pd.objective_timer > 0 && now > pd.objective_timer) {
            self.restore_previous_objective(px, r);
            let pd = self.ped_mut(r);
            if pd.objective_timer > now || pd.objective_timer == 0 {
                pd.objective_timer = now.wrapping_sub(1);
            }
            let (created_by, in_vehicle) = (pd.char_created_by, pd.in_vehicle);
            if created_by != RANDOM_CHAR || in_vehicle {
                if self.ped(r).is_ped_in_control() {
                    self.restore_previous_state(px, r);
                }
            } else {
                let dir = (px.rng.number() & 7) as i8;
                self.set_wander_path(px, r, dir);
            }
            self.clear_aim_flag(r);
            self.clear_look_flag(px, r);
        }
    }

    /// The `switch (m_objective)` of `ProcessObjective`.
    fn run_objective(&mut self, px: &mut Px, r: EntityRef, our_pos: Vec3, dist: Vec3) -> Flow {
        let now = self.now();
        match self.ped(r).objective {
            objective::WAIT_ON_FOOT => {
                self.set_idle(px, r);
                let pd = self.ped_mut(r);
                pd.objective = objective::NONE;
                pd.set_move_state(move_state::STILL);
            }
            objective::FLEE_ON_FOOT_TILL_SAFE => {
                let pd = self.ped(r);
                if pd.in_vehicle_now() {
                    let veh = pd.my_vehicle.expect("in vehicle");
                    self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
                    self.ped_mut(r).flags.flee_after_exiting_car = true;
                } else if pd.ped_state != state::FLEE_POS {
                    let pos = self.epos(r);
                    self.set_flee_pos(r, Vec2::new(pos.x, pos.y), 10000);
                    let pd = self.ped_mut(r);
                    pd.flags.use_ped_node_seek = true;
                    pd.next_path_node = None;
                }
            }
            objective::GUARD_SPOT => return self.obj_guard_spot(px, r),
            objective::WAIT_IN_CAR | objective::WAIT_IN_CAR_THEN_GET_OUT => self.ped_mut(r).ped_state = state::DRIVING,
            objective::KILL_CHAR_ANY_MEANS => {
                if let Some(flow) = self.obj_kill_any_means(px, r, dist) {
                    return flow;
                }
                return self.obj_kill_on_foot(px, r, dist);
            }
            objective::KILL_CHAR_ON_FOOT => return self.obj_kill_on_foot(px, r, dist),
            objective::FLEE_CHAR_ON_FOOT_TILL_SAFE | objective::FLEE_CHAR_ON_FOOT_ALWAYS => {
                let pd = self.ped(r);
                if pd.in_vehicle_now() {
                    if pd.ped_state == state::DRIVING {
                        let veh = pd.my_vehicle.expect("in vehicle");
                        self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
                    }
                } else if pd.ped_state != state::FLEE_ENTITY {
                    let time = if pd.objective == objective::FLEE_CHAR_ON_FOOT_ALWAYS { 0 } else { 6000 };
                    if let Some(t) = pd.ped_in_objective {
                        self.set_find_path_and_flee(px, r, t, time, false);
                    }
                }
            }
            objective::GOTO_CHAR_ON_FOOT => self.obj_goto_char(px, r, dist),
            objective::FOLLOW_CHAR_IN_FORMATION => {
                if let Some(t) = self.ped(r).ped_in_objective {
                    let pos_to_go = self.get_formation_position(r);
                    let dist_with_target = (pos_to_go - our_pos).length();
                    self.set_seek_pos(px, r, pos_to_go, 1.0);
                    if dist_with_target <= 3.0 {
                        self.set_seek_pos(px, r, pos_to_go, 1.0);
                        let leader_move = self.ped(t).move_state;
                        if leader_move != move_state::STILL {
                            self.ped_mut(r).set_move_state(leader_move);
                        }
                    } else {
                        self.set_seek_pos(px, r, pos_to_go, 1.0);
                        self.ped_mut(r).set_move_state(move_state::RUN);
                    }
                } else {
                    self.set_objective(px, r, objective::NONE);
                }
            }
            objective::ENTER_CAR_AS_PASSENGER => {
                self.obj_enter_car_as_passenger(px, r);
                return self.obj_enter_car_as_driver(px, r, dist);
            }
            objective::ENTER_CAR_AS_DRIVER => return self.obj_enter_car_as_driver(px, r, dist),
            objective::DESTROY_CAR => self.obj_destroy_car(px, r, dist),
            objective::GOTO_AREA_ANY_MEANS => {
                if let Some(flow) = self.obj_goto_area_any_means(px, r) {
                    return flow;
                }
                self.obj_goto_area_on_foot(px, r);
            }
            objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA => self.obj_goto_area_on_foot(px, r),
            objective::GUARD_ATTACK => self.obj_guard_attack(px, r, dist),
            objective::FOLLOW_ROUTE => {
                if self.have_reached_next_point_on_route(r, 1.0) {
                    let next = self.get_next_point_on_route(r);
                    let pos = self.ents.ped_routes.point_position(next as i16);
                    self.ped_mut(r).next_route_point_pos = pos;
                } else {
                    let to = self.ped(r).next_route_point_pos;
                    self.set_seek_pos(px, r, to, 0.8);
                }
            }
            objective::SOLICIT_VEHICLE => {
                if let Some(car) = self.ped(r).car_in_objective {
                    if self.ped(r).objective_timer <= now {
                        if !self.ped(r).in_vehicle {
                            self.set_objective(px, r, objective::NONE);
                            let dir = (px.rng.number() & 7) as i8;
                            self.set_wander_path(px, r, dir);
                            self.ped_mut(r).objective_timer = now.wrapping_add(10000);
                            if self.ped(r).is_ped_in_control() {
                                self.ped_mut(r).my_vehicle = None;
                            }
                        }
                    } else if !matches!(self.ped(r).ped_state, state::FLEE_ENTITY | state::SOLICIT) {
                        self.set_seek_car(r, car, 0);
                    }
                } else {
                    self.give_up_car_objective(px, r);
                }
            }
            objective::HAIL_TAXI => {
                if self.ped_assoc(r, anim::STD_HAILTAXI).is_none() && now > self.ped(r).wait_timer {
                    self.say(px, r, sound::PED_TAXI_WAIT);
                    self.ped_blend_anim(r, assoc_group::STD, anim::STD_HAILTAXI, 4.0);
                    self.ped_mut(r).wait_timer = now.wrapping_add(2000);
                }
            }
            objective::CATCH_TRAIN => self.obj_catch_train(px, r),
            objective::BUY_ICE_CREAM => {
                if let Some(car) = self.ped(r).car_in_objective {
                    if !matches!(self.ped(r).ped_state, state::FLEE_ENTITY | state::BUY_ICECREAM) {
                        self.set_seek_car(r, car, 0);
                    }
                } else {
                    self.give_up_car_objective(px, r);
                }
            }
            objective::STEAL_ANY_CAR => self.obj_steal_any_car(px, r),
            objective::MUG_CHAR => return self.obj_mug_char(px, r, dist),
            objective::FLEE_CAR => {
                let pd = self.ped(r);
                if let Some(veh) = pd.my_vehicle.filter(|_| !pd.in_vehicle && pd.ped_state != state::FLEE_ENTITY) {
                    self.restore_previous_objective(px, r);
                    self.set_flee(px, r, veh, 6000);
                } else {
                    self.obj_leave_car(px, r);
                }
            }
            objective::LEAVE_CAR => self.obj_leave_car(px, r),
            _ => {}
        }
        Flow::Tail
    }

    /// A car objective whose car is gone: back to what the ped did before.
    fn give_up_car_objective(&mut self, px: &mut Px, r: EntityRef) {
        self.restore_previous_objective(px, r);
        self.restore_previous_state(px, r);
        if self.ped(r).is_ped_in_control() {
            self.ped_mut(r).my_vehicle = None;
        }
    }

    /// `SetFindPathAndFlee(entity, time, walk)` of `Ped.h`.
    pub fn set_find_path_and_flee(&mut self, px: &mut Px, r: EntityRef, from: EntityRef, time: i32, walk: bool) {
        self.set_flee(px, r, from, time);
        let pd = self.ped_mut(r);
        pd.flags.use_ped_node_seek = true;
        pd.next_path_node = None;
        if walk {
            pd.set_move_state(move_state::WALK);
        }
    }

    /// `SetFindPathAndFlee(from, time, walk)` of `Ped.h`.
    pub fn set_find_path_and_flee_pos(&mut self, r: EntityRef, from: Vec2, time: i32, walk: bool) {
        self.set_flee_pos(r, from, time);
        let pd = self.ped_mut(r);
        pd.flags.use_ped_node_seek = true;
        pd.next_path_node = None;
        if walk {
            pd.set_move_state(move_state::WALK);
        }
    }

    fn obj_guard_spot(&mut self, px: &mut Px, r: EntityRef) -> Flow {
        let now = self.now();
        let pd = self.ped(r);
        let (spot, done_dist, target) = (pd.seek_pos_ex, pd.distance_to_count_seek_done_ex, pd.ped_in_objective);
        let dist = (spot - self.epos(r)).length();
        if let Some(t) = target {
            self.set_look_flag_entity(r, t, true);
            self.ped_mut(r).look_target = Some(t);
            self.turn_body(r);
        }
        if 2.0 * done_dist >= dist {
            if target.is_some() {
                if dist <= done_dist {
                    self.set_idle(px, r);
                } else {
                    self.set_seek_pos(px, r, spot, done_dist);
                }
            } else if now > self.ped(r).look_timer {
                self.scan_for_threats(px, r);
                let time = px.rng.range_i32(500, 1500);
                self.set_look_timer(r, time);
                // the original would also act on a gun threat that has no entity, by reading null
                if let Some(threat) = self.ped(r).threat_entity.filter(|&t| self.is_ped(t)) {
                    self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, threat);
                }
            }
        } else {
            self.set_seek_pos(px, r, spot, done_dist);
        }
        Flow::Tail
    }

    /// The first half of `OBJECTIVE_KILL_CHAR_ANY_MEANS`: None runs on into the on-foot code.
    fn obj_kill_any_means(&mut self, px: &mut Px, r: EntityRef, dist: Vec3) -> Option<Flow> {
        let pd = self.ped(r);
        let Some(target) = pd.ped_in_objective else {
            self.clear_look_flag(px, r);
            self.ped_mut(r).flags.objective_completed = true;
            return None;
        };
        let wanted = self.wanted_info();
        let target_is_player = self.ped(target).is_player();
        if target_is_player && pd.char_created_by != MISSION_CHAR && pd.ped_type != ped_type::COP && wanted.current_cops != 0 && !pd.flags.kinda_stay_in_same_place {
            self.set_objective(px, r, objective::FLEE_ON_FOOT_TILL_SAFE);
            return Some(Flow::Tail);
        }
        if pd.in_vehicle_now() {
            self.kill_any_means_in_car(px, r, target, dist, wanted.level);
            return Some(Flow::Tail);
        }
        if dist.length() > 30.0 && !pd.flags.kinda_stay_in_same_place {
            self.kill_any_means_find_car(px, r, target);
            return Some(Flow::Tail);
        }
        None
    }

    fn kill_any_means_in_car(&mut self, px: &mut Px, r: EntityRef, target: EntityRef, dist: Vec3, wanted_level: i32) {
        let veh = self.ped(r).my_vehicle.expect("in vehicle");
        let speed_sq = self.ent(veh).phys.as_ref().map_or(0.0, |p| p.move_speed.length_squared());
        if dist.length() >= 20.0 || speed_sq >= 0.02 * 0.02 {
            let v = self.try_veh(veh).expect("vehicle");
            if v.driver == Some(r) && v.getting_in_flags == 0 {
                let cop = self.ped(r).ped_type == ped_type::COP;
                let cruise = self.ents.handling.get(v.handling).transmission.max_cruise_velocity;
                let mission = if cop { px.hooks.police_car_mission() } else { car_mission::RAMPLAYER_FARAWAY };
                self.get_mut(veh).expect("vehicle").base.status = status::PHYSICS;
                let v = self.try_veh_mut(veh).expect("vehicle");
                v.auto_pilot.prev_route_node = 0;
                v.auto_pilot.cruise_speed = if cop { ((wanted_level as f32 * 0.1 + 0.6) * (GAME_SPEED_TO_CARAI_SPEED * cruise)) as u8 } else { (GAME_SPEED_TO_CARAI_SPEED * cruise * 0.8) as u8 };
                v.auto_pilot.car_mission = mission;
                v.auto_pilot.driving_style = driving_style::AVOID_CARS;
            }
        } else {
            let target_in_vehicle = self.ped(target).in_vehicle;
            let can_exit = !target_in_vehicle || self.ped(target).my_vehicle.and_then(|v| self.vehicle_can_exit(v)).unwrap_or(true);
            if can_exit {
                let v = self.try_veh_mut(veh).expect("vehicle");
                v.auto_pilot.cruise_speed = 0;
                v.auto_pilot.car_mission = car_mission::NONE;
                self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            }
        }
    }

    /// `CVehicle::CanPedExitCar` for the vehicle `v`.
    fn vehicle_can_exit(&self, v: EntityRef) -> Option<bool> {
        let e = self.get(v)?;
        Some(crate::vehicle::base::VehicleData::can_ped_exit_car(&e.base.matrix, e.phys.as_ref()?))
    }

    /// A far away target: get into a car, a nearby one or a new one.
    fn kill_any_means_find_car(&mut self, px: &mut Px, r: EntityRef, target: EntityRef) {
        if let Some(veh) = self.ped(r).my_vehicle {
            self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
            self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, veh);
            return;
        }
        let (pos, target_veh) = (self.epos(r), self.ped(target).my_vehicle);
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        let mut closest = 60.0;
        let mut found = None;
        let is_player = self.ped(r).is_player();
        for near in self.find_objects_in_range(pos, 25.0, true, 6, kinds) {
            let d = (self.epos(near) - pos).length();
            let can_open = self.try_veh(near).is_some_and(|v| v.can_ped_open_locks(is_player));
            if d < closest && target_veh != Some(near) && can_open {
                found = Some(near);
                closest = d;
            }
        }
        self.ped_mut(r).my_vehicle = found;
        if let Some(veh) = found {
            self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
            self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, veh);
        } else if !px.hooks.is_on_screen(r) {
            self.spawn_getaway_car(px, r);
        }
    }

    /// A ped off screen that needs a car and has none close by gets one on the nearest road.
    fn spawn_getaway_car(&mut self, px: &mut Px, r: EntityRef) {
        let pos = self.epos(r);
        let node = self.ents.paths.find_node_closest_to_coors(pos, crate::control::pathfind::PATH_CAR, 20.0, false, false);
        if node < 0 {
            return;
        }
        let node_pos = self.ents.paths.nodes[node as usize].pos;
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        if !self.find_objects_kinda_colliding(node_pos, 10.0, true, 2, kinds).is_empty() {
            return;
        }
        let Some(model) = px.hooks.choose_car_model(pos) else { return };
        let Some(car) = self.new_car(model, RANDOM_VEHICLE, px.rng) else { return };
        let e = self.get_mut(car).expect("car");
        e.base.set_position(Vec3::new(node_pos.x, node_pos.y, node_pos.z + 4.0));
        e.base.set_heading(200.0_f32.to_radians());
        e.base.status = status::ABANDONED;
        if let crate::entity::Kind::Vehicle(v) = &mut e.kind {
            v.door_lock = crate::vehicle::base::CarLock::Unlocked;
            v.auto_pilot.cruise_speed = 0;
        }
        self.add(car).ok();
        self.ped_mut(r).my_vehicle = Some(car);
        self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, car);
    }

    /// `OBJECTIVE_KILL_CHAR_ON_FOOT` (and the tail of the any-means variant).
    fn obj_kill_on_foot(&mut self, px: &mut Px, r: EntityRef, dist: Vec3) -> Flow {
        let now = self.now();
        let mut kill_player_in_no_police_zone = false;
        let pd = self.ped(r);
        if pd.objective == objective::KILL_CHAR_ON_FOOT && pd.in_vehicle_now() {
            let veh = pd.my_vehicle.expect("in vehicle");
            self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            return Flow::Tail;
        }
        let target = pd.ped_in_objective.filter(|&t| !self.ped(t).dying_or_dead());
        let Some(target) = target else {
            self.clear_look_flag(px, r);
            self.ped_mut(r).flags.objective_completed = true;
            self.set_move_anim(px, r);
            return Flow::Tail;
        };
        let target_is_player = self.ped(target).is_player();
        if target_is_player && px.hooks.no_police_zone() {
            kill_player_in_no_police_zone = true;
        }
        let melee = |w: &World, e: EntityRef| {
            let p = w.ped(e);
            p.weapons[p.current_weapon as usize].is_type_melee()
        };
        let pd = self.ped(r);
        if !pd.flags.not_allowed_to_duck || kill_player_in_no_police_zone {
            if pd.ped_type == ped_type::COP && !melee(self, target) && !melee(self, r) {
                self.ped_mut(r).flags.not_allowed_to_duck = true;
            }
        } else if !self.ped(target).in_vehicle {
            if melee(self, target) || melee(self, r) {
                let pd = self.ped_mut(r);
                pd.flags.not_allowed_to_duck = false;
                pd.flags.crouch_when_shooting = false;
            } else if self.duck_and_cover(px, r) {
                return Flow::Tail;
            }
        } else {
            let pd = self.ped_mut(r);
            pd.flags.not_allowed_to_duck = false;
            pd.flags.crouch_when_shooting = false;
        }
        let pd = self.ped(r);
        if pd.leave_car_timer > now && !pd.flags.kinda_stay_in_same_place {
            self.ped_mut(r).set_move_state(move_state::STILL);
            return Flow::Tail;
        }
        if target_is_player {
            let wanted = self.wanted_info();
            let tp = self.ped(target);
            if (self.ped(r).ped_type == ped_type::COP && wanted.ignored_by_cops) || wanted.ignored_by_everyone || self.ped_in_water(target) || tp.ped_state == state::ARRESTED {
                if self.ped(r).ped_state != state::ARREST_PLAYER {
                    self.set_idle(px, r);
                }
                return Flow::Tail;
            }
        }
        let weapon_type = {
            let pd = self.ped(r);
            pd.weapons[pd.current_weapon as usize].ty
        };
        let info = self.ents.weapon_infos.get(weapon_type).clone();
        let mut wep_range = info.range;
        let mut max_dist_to_keep;
        if weapon_type != wt::UNARMED {
            max_dist_to_keep = wep_range / 3.0;
        } else {
            if self.ped(r).ped_state == state::FIGHT {
                let stats_flags = self.ents.ped_stats.0[self.ped(r).ped_stats].flags;
                if !self.ped(r).is_player() && stats_flags & stat_flag::CAN_KICK == 0 {
                    wep_range = 2.0;
                }
            } else {
                wep_range = 1.3;
            }
            max_dist_to_keep = wep_range;
        }
        if self.ped(target).get_up_timer > now && max_dist_to_keep < 2.5 {
            max_dist_to_keep = 2.5;
        }
        let wanted = self.wanted_info();
        let pd = self.ped(r);
        if target_is_player && pd.ped_type != ped_type::COP && pd.char_created_by != MISSION_CHAR && wanted.current_cops != 0 {
            self.set_objective(px, r, objective::FLEE_ON_FOOT_TILL_SAFE);
            return Flow::Tail;
        }
        if self.ped(target).health <= 0.0 {
            let pd = self.ped_mut(r);
            pd.flags.objective_completed = true;
            pd.flags.script_objective_completed = true;
            self.set_move_anim(px, r);
            return Flow::Tail;
        }
        let dist_with_target = dist.length();
        let tp = self.ped(target);
        if tp.in_vehicle && tp.ped_state != state::DRAG_FROM_CAR {
            return self.kill_target_in_vehicle(px, r, target, dist_with_target, wep_range, kill_player_in_no_police_zone, &info);
        }
        if self.ped(r).move_state == move_state::STILL && self.ped(r).is_ped_in_control() {
            self.set_look_flag_entity(r, target, false);
            self.turn_body(r);
        }
        let tp = self.ped(target);
        if self.ped(r).ped_type == ped_type::COP && dist_with_target < 1.5 && tp.is_player() && (tp.get_up_timer > now || tp.ped_state == state::DRAG_FROM_CAR) {
            self.set_arrest_player(px, r, target);
            return Flow::Return;
        }
        let pd = self.ped(r);
        let tp = self.ped(target);
        if !pd.flags.kinda_stay_in_same_place && !pd.flags.stop_and_shoot && pd.ped_state != state::ATTACK && !kill_player_in_no_police_zone {
            let cannot_see = dist_with_target > pd.distance_to_count_seek_done && !self.can_see_entity(r, target, CAN_SEE_ENTITY_ANGLE_THRESHOLD);
            if dist_with_target > wep_range || tp.get_up_timer > now || tp.ped_state == state::ARRESTED || (tp.entering_car() && dist_with_target < 3.0) || cannot_see {
                if tp.entering_car() {
                    max_dist_to_keep = 2.0;
                }
                if self.ped(r).flags.use_ped_node_seek {
                    let target_pos = self.epos(target);
                    self.ped_mut(r).seek_pos = target_pos;
                    if self.ped(r).next_path_node.is_none() {
                        let mut best = Vec3::ZERO;
                        self.find_best_coords_from_nodes(r, &mut best);
                    }
                    if let Some(n) = self.ped(r).next_path_node {
                        let p = self.ents.paths.nodes[n].pos;
                        self.ped_mut(r).seek_pos = p;
                    }
                    let (seek, done) = (self.ped(r).seek_pos, self.ped(r).distance_to_count_seek_done);
                    self.set_seek_pos(px, r, seek, done);
                } else {
                    self.set_seek_entity(r, Some(target), max_dist_to_keep);
                }
                self.ped_mut(r).flags.crouch_when_shooting = false;
                if self.ped(target).current_phys_surface.is_some() && dist_with_target < 5.0 {
                    if wep_range <= 5.0 {
                        let player_sped = self.ped(target).player.as_ref().is_some_and(|p| p.speed_timer_flag);
                        let pd = self.ped(r);
                        if self.ped(target).is_player() && player_sped && (pd.is_gang_member() || pd.ped_type == ped_type::COP) && weapon_type == wt::UNARMED {
                            self.give_weapon(r, wt::COLT45, 1000);
                            self.set_current_weapon(r, wt::COLT45);
                        }
                    } else {
                        self.ped_mut(r).flags.stop_and_shoot = true;
                    }
                    self.ped_mut(r).set_move_state(move_state::STILL);
                    self.set_move_anim(px, r);
                    return Flow::Tail;
                }
                self.ped_mut(r).flags.stop_and_shoot = false;
                self.set_move_anim(px, r);
                return Flow::Tail;
            }
        }

        let pd = self.ped(r);
        let (target_state, target_surface) = (self.ped(target).ped_state, self.ped(target).current_phys_surface);
        if pd.attack_timer < now && dist_with_target < wep_range && target_state != state::GETUP && target_state != state::DRAG_FROM_CAR {
            if pd.flags.is_ducking {
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_DUCK_DOWN) {
                    a.blend_delta = -2.0;
                    return Flow::Tail;
                }
                self.ped_mut(r).flags.is_ducking = false;
            } else if wep_range <= 5.0 {
                self.ped_mut(r).set_move_state(move_state::STILL);
                self.set_attack(px, r, target);
                let (tp, pos) = (self.epos(target), self.epos(r));
                self.ped_mut(r).rotation_dest = radian_angle_between_points(tp.x, tp.y, pos.x, pos.y);
                let shoot = px.rng.range_f32(0.0, 500.0) as u32;
                let attack = px.rng.range_f32(0.0, 1500.0) as u32;
                self.set_shoot_timer(r, shoot);
                self.set_attack_timer(r, attack);
                self.ped_mut(r).flags.obstacle_showed_up_during_kill_objective = false;
            } else {
                self.kill_with_gun(px, r, target, dist_with_target, max_dist_to_keep, &info);
            }
        } else {
            if target_surface.is_none() {
                self.ped_mut(r).flags.stop_and_shoot = false;
            }
            let pd = self.ped(r);
            if pd.ped_state != state::ATTACK && pd.ped_state != state::FIGHT {
                if pd.flags.not_allowed_to_duck && pd.flags.kinda_stay_in_same_place {
                    let duck = self.ped_assoc(r, anim::STD_DUCK_DOWN).map(|a| a.blend_delta);
                    if !self.ped(r).flags.is_ducking {
                        if duck.is_none_or(|d| d < 0.0) {
                            self.ped_blend_anim(r, assoc_group::STD, anim::STD_DUCK_DOWN, 4.0);
                            self.ped_mut(r).flags.is_ducking = true;
                        }
                        return Flow::Tail;
                    }
                    if duck.is_none_or(|d| d < 0.0) {
                        self.ped_mut(r).flags.is_ducking = false;
                    } else {
                        return Flow::Tail;
                    }
                }
                let pd = self.ped(r);
                if pd.flags.obstacle_showed_up_during_kill_objective {
                    let blocker = pd.flee_from;
                    let blocked_by_object = blocker.is_some_and(|b| matches!(self.get(b).map(|e| e.base.ty), Some(crate::entity::EntityType::Object)));
                    let blocked_by_vehicle = blocker.is_some_and(|b| self.is_vehicle_ref(b));
                    max_dist_to_keep = if pd.ped_type == ped_type::COP {
                        if weapon_type > wt::COLT45 || blocked_by_object {
                            6.0
                        } else if blocked_by_vehicle {
                            4.0
                        } else {
                            2.0
                        }
                    } else {
                        2.0
                    };
                }
                if dist_with_target <= max_dist_to_keep {
                    let pd = self.ped_mut(r);
                    pd.set_move_state(move_state::STILL);
                    pd.flags.is_pointing_gun_at = true;
                    if pd.ped_state != state::AIM_GUN && !pd.flags.duck_and_cover {
                        pd.attack_timer = now;
                        self.set_idle(px, r);
                    }
                } else if !matches!(pd.ped_state, state::SEEK_ENTITY | state::SEEK_POS) && !pd.flags.stop_and_shoot && !kill_player_in_no_police_zone && !pd.flags.kinda_stay_in_same_place {
                    self.say(px, r, sound::PED_ATTACK);
                    self.set_seek_entity(r, Some(target), max_dist_to_keep);
                    self.ped_mut(r).flags.is_running = true;
                }
            }
        }

        if dist_with_target < 2.5 && wep_range > 5.0 && info.fire != WeaponFire::Melee {
            self.set_attack(px, r, target);
            if self.ped(r).attack_timer < now {
                let time = px.rng.range_f32(500.0, 1000.0) as u32;
                self.set_attack_timer(r, time);
                self.set_shoot_timer(r, time.wrapping_sub(500));
            }
            self.ped_mut(r).set_move_state(move_state::STILL);
        }
        if now > self.ped(r).ped_state_timer {
            let (tp, pos) = (self.epos(target), self.epos(r));
            self.ped_mut(r).rotation_dest = radian_angle_between_points(tp.x, tp.y, pos.x, pos.y);
        }
        self.set_move_anim(px, r);
        Flow::Tail
    }

    /// Is the ped standing in water (`bIsInWater`)?
    fn ped_in_water(&self, r: EntityRef) -> bool {
        self.get(r).and_then(|e| e.phys.as_ref()).is_some_and(|p| p.flags.is_in_water)
    }

    /// Shoot at a ped from a distance when the line is clear, else wait for the obstacle to go.
    fn kill_with_gun(&mut self, px: &mut Px, r: EntityRef, target: EntityRef, dist_with_target: f32, max_dist_to_keep: f32, info: &crate::weapons::info::WeaponInfo) {
        let m = self.ent(r).base.matrix;
        let our_head = m.transform_point(HEAD_OFFSET);
        let mut target_pos = if self.is_ped(target) { self.ped_node_position(target, node_mid()) } else { self.epos(target) };
        target_pos -= our_head;
        target_pos *= info.range / target_pos.length();
        target_pos += our_head;
        let hit = self.line_of_sight_with_dead_peds(our_head, target_pos);
        if hit == Some(target) {
            self.set_attack(px, r, target);
            self.ped_mut(r).point_gun_at = Some(target);
            let shoot = px.rng.range_f32(500.0, 2000.0) as u32;
            self.set_shoot_timer(r, shoot);
            let time = if dist_with_target <= max_dist_to_keep { px.rng.range_f32(100.0, 500.0) } else { px.rng.range_f32(1500.0, 3000.0) } as u32;
            self.set_attack_timer(r, time);
            self.ped_mut(r).flags.obstacle_showed_up_during_kill_objective = false;
            return;
        }
        if let Some(blocker) = hit {
            let (time, obstacle) = if self.is_ped(blocker) {
                (px.rng.range_f32(500.0, 1000.0), false)
            } else if matches!(self.get(blocker).map(|e| e.base.ty), Some(crate::entity::EntityType::Object)) {
                (px.rng.range_f32(200.0, 400.0), true)
            } else if self.is_vehicle_ref(blocker) {
                (px.rng.range_f32(400.0, 600.0), true)
            } else {
                (px.rng.range_f32(700.0, 1200.0), true)
            };
            self.set_attack_timer(r, time as u32);
            let pd = self.ped_mut(r);
            pd.flags.obstacle_showed_up_during_kill_objective = obstacle;
            pd.flee_from = Some(blocker);
        }
        self.set_point_gun_at(px, r, target);
    }

    /// `ProcessLineOfSight` with dead peds counted, as the kill objectives do.
    fn line_of_sight_with_dead_peds(&mut self, from: Vec3, to: Vec3) -> Option<EntityRef> {
        let kinds = Kinds { dummies: false, ..Kinds::ALL };
        self.ents.include_dead_peds = true;
        let hit = self.process_line_of_sight(from, to, kinds, LineOpts { ignore_see_through: true, ..Default::default() });
        self.ents.include_dead_peds = false;
        hit.map(|h| h.entity)
    }

    /// The target sits in a vehicle: shoot at it or walk to its door.
    #[allow(clippy::too_many_arguments)]
    fn kill_target_in_vehicle(&mut self, px: &mut Px, r: EntityRef, target: EntityRef, dist_with_target: f32, wep_range: f32, no_police_zone: bool, info: &crate::weapons::info::WeaponInfo) -> Flow {
        let now = self.now();
        let veh = self.ped(target).my_vehicle.expect("in vehicle");
        let veh_status = self.ent(veh).base.status;
        if veh_status == status::PLAYER_DISABLED || (self.ped(target).is_player() && px.hooks.player_controls_disabled()) {
            self.set_idle(px, r);
            return Flow::Return;
        }
        self.set_look_flag_entity(r, veh, false);
        let pd = self.ped(r);
        if pd.ped_state != state::CARJACK && self.ped(target).ped_state != state::ARRESTED {
            if pd.attack_timer < now && info.fire != WeaponFire::Melee && dist_with_target < wep_range && dist_with_target > 3.0 {
                let our_head = self.ent(r).base.matrix.transform_point(HEAD_OFFSET);
                let mut max_shot = self.epos(veh) - our_head;
                max_shot *= info.range / max_shot.length();
                max_shot += our_head;
                if self.line_of_sight_with_dead_peds(our_head, max_shot) == Some(veh) {
                    self.set_attack(px, r, veh);
                    self.ped_mut(r).point_gun_at = Some(veh);
                    let shoot = px.rng.range_i32(500, 2000) as u32;
                    self.set_shoot_timer(r, shoot);
                    if dist_with_target <= self.ped(r).distance_to_count_seek_done {
                        let t = px.rng.range_i32(200, 500) as u32;
                        self.set_attack_timer(r, t);
                        self.ped_mut(r).set_move_state(move_state::STILL);
                    } else {
                        let t = px.rng.range_i32(2000, 5000) as u32;
                        self.set_attack_timer(r, t);
                    }
                }
            } else if pd.ped_state != state::ATTACK && !pd.flags.kinda_stay_in_same_place && !no_police_zone {
                let is_bus = self.try_veh(veh).is_some_and(|v| v.flags.is_bus);
                if self.ped(r).ped_type == ped_type::COP || is_bus {
                    self.go_to_nearest_door(px, r, veh);
                } else {
                    let v = self.try_veh(veh).expect("vehicle");
                    let door = if Some(target) == v.driver || v.flags.is_bus {
                        node::DOOR_LF
                    } else if Some(target) == v.passengers[0] {
                        node::DOOR_RF
                    } else if Some(target) == v.passengers[1] {
                        node::DOOR_LR
                    } else if Some(target) == v.passengers[2] {
                        node::DOOR_RR
                    } else {
                        0
                    };
                    self.ped_mut(r).veh_door = door as u16;
                    self.set_seek_car(r, veh, door as u32);
                    self.ped_mut(r).set_move_state(move_state::RUN);
                }
            }
        }
        self.set_move_anim(px, r);
        Flow::Tail
    }

    fn obj_goto_char(&mut self, px: &mut Px, r: EntityRef, dist: Vec3) {
        let now = self.now();
        let Some(target) = self.ped(r).ped_in_objective else {
            self.set_objective(px, r, objective::NONE);
            return;
        };
        let safe_distance = if self.ped(target).in_vehicle { 3.0 } else { 2.0 };
        let dist_with_target = dist.length();
        if self.ped(r).ped_state_timer >= now {
            return;
        }
        if dist_with_target <= safe_distance {
            self.ped_mut(r).flags.script_objective_completed = true;
            if self.ped(r).ped_state != state::ATTACK {
                self.set_idle(px, r);
                self.ped_mut(r).look_target = Some(target);
                self.turn_body(r);
            }
            let target_move = self.ped(target).move_state;
            self.ped_mut(r).set_move_state(if dist_with_target > 2.0 { target_move } else { move_state::STILL });
            return;
        }
        self.set_seek_entity(r, Some(target), safe_distance);
        let leader = self.ped(r).leader;
        let leader_move = leader.map(|l| self.ped(l).move_state);
        let new_move = if dist_with_target >= 5.0 {
            if leader_move == Some(move_state::SPRINT) { move_state::SPRINT } else { move_state::RUN }
        } else if let Some(lm) = leader_move.filter(|&m| m != move_state::STILL && m != move_state::NONE) {
            if self.ped(leader.expect("leader")).is_player() {
                let speed = self.ped(leader.expect("leader")).player.as_ref().map_or(0.0, |p| p.move_speed);
                if dist_with_target >= 3.0 && speed >= 1.3 { move_state::RUN } else { move_state::WALK }
            } else {
                lm
            }
        } else if dist_with_target <= 3.0 {
            move_state::WALK
        } else {
            move_state::RUN
        };
        self.ped_mut(r).set_move_state(new_move);
    }

    fn obj_enter_car_as_passenger(&mut self, px: &mut Px, r: EntityRef) -> Option<Flow> {
        let now = self.now();
        let car = self.ped(r).car_in_objective?;
        let (full, flags, passengers) = self.try_veh(car).map(|v| (v.num_passengers >= v.num_max_passengers, v.getting_in_flags, v.passengers)).unwrap_or((false, 0, [None; 8]));
        if !self.ped(r).in_vehicle && full {
            self.restore_previous_objective(px, r);
            self.restore_previous_state(px, r);
            if self.ped(r).is_ped_in_control() {
                self.ped_mut(r).my_vehicle = None;
            }
            return Some(Flow::Tail);
        }
        let taxi_light = self.try_veh(car).is_some_and(|v| matches!(&v.sub, crate::vehicle::base::VehicleSub::Car(c) if c.flags.taxi_light));
        if self.ped(r).prev_objective == objective::HAIL_TAXI && !taxi_light {
            self.restore_previous_objective(px, r);
            self.clear_objective(px, r);
            let dir = (px.rng.number() & 7) as i8;
            self.set_wander_path(px, r, dir);
            self.ped_mut(r).flags.is_running = false;
            return Some(Flow::Tail);
        }
        let timer = self.ped(r).objective_timer;
        if timer != 0 && timer < now {
            if !self.ped(r).entering_car() {
                let flag = |n: usize| flags & crate::vehicle::automobile::car_door_flag(n) != 0;
                let max = self.try_veh(car).map_or(0, |v| v.num_max_passengers as usize);
                let mut door = None;
                if passengers[0].is_some() || flag(node::DOOR_RF) {
                    if passengers[1].is_some() || flag(node::DOOR_LR) {
                        if !(passengers[2].is_some() || flag(node::DOOR_RR)) {
                            door = Some(node::DOOR_RR);
                        }
                    } else {
                        door = Some(node::DOOR_LR);
                    }
                } else {
                    door = Some(node::DOOR_RF);
                }
                for seat in passengers.iter().take(max).skip(2) {
                    if seat.is_none() && !flag(node::DOOR_RF) {
                        door = Some(node::DOOR_RF);
                    }
                }
                if let Some(d) = door {
                    self.ped_mut(r).veh_door = d as u16;
                    let p = self.get_position_to_open_car_door(car, d as u32);
                    self.get_mut(r).expect("ped").base.set_position(p);
                    self.set_enter_car(px, r, car, d as u32);
                }
            }
            self.ped_mut(r).objective_timer = 0;
        }
        None
    }

    fn obj_enter_car_as_driver(&mut self, px: &mut Px, r: EntityRef, dist: Vec3) -> Flow {
        let now = self.now();
        let pd = self.ped(r);
        let Some(car) = pd.car_in_objective.filter(|_| !pd.in_vehicle) else {
            let pd = self.ped_mut(r);
            pd.flags.objective_completed = true;
            pd.flags.script_objective_completed = true;
            self.restore_previous_state(px, r);
            return Flow::Tail;
        };
        if pd.leave_car_timer > now {
            self.ped_mut(r).set_move_state(move_state::STILL);
            return Flow::Tail;
        }
        if !pd.is_ped_in_control() {
            return Flow::Tail;
        }
        let pd = self.ped(r);
        let (v_driver, v_health) = self.try_veh(car).map_or((None, 0.0), |v| (v.driver, v.health));
        if pd.prev_objective == objective::KILL_CHAR_ANY_MEANS {
            if dist.length() < 20.0 {
                self.restore_previous_objective(px, r);
                self.restore_previous_state(px, r);
                return Flow::Return;
            }
            if pd.objective == objective::ENTER_CAR_AS_DRIVER
                && let Some(d) = v_driver
                && self.ped(d).objective == objective::KILL_CHAR_ANY_MEANS
                && Some(d) != pd.ped_in_objective
            {
                self.set_objective_entity(px, r, objective::ENTER_CAR_AS_PASSENGER, car);
            }
        } else if pd.objective != objective::ENTER_CAR_AS_PASSENGER
            && let Some(d) = v_driver
            && self.ped(d).ped_type == pd.ped_type
        {
            self.set_objective_entity(px, r, objective::ENTER_CAR_AS_PASSENGER, car);
        }
        let upside_down = self.get(car).is_some_and(|e| crate::vehicle::base::VehicleData::is_upside_down(&e.base.matrix));
        let is_bike = self.try_veh(car).is_some_and(|v| v.is_bike());
        if upside_down && !is_bike {
            self.restore_previous_objective(px, r);
            self.restore_previous_state(px, r);
            return Flow::Return;
        }
        let is_boat = self.try_veh(car).is_some_and(|v| v.is_boat());
        if !is_boat || self.ped(r).ped_state == state::SEEK_IN_BOAT {
            if self.ped(r).ped_state != state::SEEK_CAR {
                self.set_seek_car(r, car, 0);
            }
        } else {
            self.set_seek_boat_position(r, car);
        }
        let pd = self.ped_mut(r);
        if pd.move_state == move_state::STILL && !pd.flags.veh_enter_door_is_blocked {
            pd.set_move_state(move_state::RUN);
        }
        if v_health > 0.0 {
            let d = (self.epos(car) - self.epos(r)).length();
            if !self.ped(r).in_vehicle && ENTER_CAR_RANGE_MULTIPLIER * ENTER_CAR_MAX_DIST < d {
                if v_driver.is_none() && !px.hooks.is_on_screen(car) && !px.hooks.is_on_screen(r) {
                    self.warp_ped_to_near_entity_off_screen(px, r, car);
                }
                let pd = self.ped(r);
                if pd.char_created_by != MISSION_CHAR || pd.prev_objective == objective::KILL_CHAR_ANY_MEANS {
                    self.restore_previous_objective(px, r);
                    self.restore_previous_state(px, r);
                    if self.ped(r).is_ped_in_control() {
                        self.ped_mut(r).my_vehicle = None;
                    }
                } else {
                    self.set_idle(px, r);
                    self.ped_mut(r).set_move_state(move_state::STILL);
                }
            }
        } else if !self.ped(r).in_vehicle {
            self.restore_previous_objective(px, r);
            self.restore_previous_state(px, r);
            if self.ped(r).is_ped_in_control() {
                self.ped_mut(r).my_vehicle = None;
            }
        }
        Flow::Tail
    }

    fn obj_destroy_car(&mut self, px: &mut Px, r: EntityRef, dist: Vec3) {
        let now = self.now();
        let Some(car) = self.ped(r).car_in_objective else {
            self.clear_look_flag(px, r);
            self.ped_mut(r).flags.objective_completed = true;
            return;
        };
        let pd = self.ped(r);
        let weapon_type = pd.weapons[pd.current_weapon as usize].ty;
        let info = self.ents.weapon_infos.get(weapon_type).clone();
        let wep_range = info.range;
        let dist_with_target = dist.length();
        let pd = self.ped_mut(r);
        pd.look_target = Some(car);
        pd.seek_target = Some(car);
        self.turn_body(r);
        if self.try_veh(car).map_or(0.0, |v| v.health) <= 0.0 {
            self.clear_look_flag(px, r);
            self.ped_mut(r).flags.script_objective_completed = true;
            return;
        }
        let pd = self.ped(r);
        if pd.attack_timer < now && info.fire != WeaponFire::Melee && dist_with_target < wep_range {
            let our_head = self.ent(r).base.matrix.transform_point(HEAD_OFFSET);
            let mut max_shot = self.epos(car) - our_head;
            max_shot *= info.range / max_shot.length();
            max_shot += our_head;
            if self.line_of_sight_with_dead_peds(our_head, max_shot) == Some(car) {
                self.set_attack(px, r, car);
                self.ped_mut(r).point_gun_at = Some(car);
                let shoot = px.rng.range_i32(500, 2000) as u32;
                self.set_shoot_timer(r, shoot);
                if dist_with_target > 10.0 && !self.ped(r).flags.kinda_stay_in_same_place {
                    let t = px.rng.range_i32(2000, 5000) as u32;
                    self.set_attack_timer(r, t);
                } else {
                    let t = px.rng.range_i32(50, 300) as u32;
                    self.set_attack_timer(r, t);
                    self.ped_mut(r).set_move_state(move_state::STILL);
                }
            }
        } else if pd.ped_state != state::ATTACK && !pd.flags.kinda_stay_in_same_place {
            let safe_distance = if wep_range <= 5.0 { 3.0 } else { wep_range * 0.25 };
            self.set_seek_entity(r, Some(car), safe_distance);
            self.ped_mut(r).set_move_state(move_state::RUN);
        }
        self.set_look_flag_entity(r, car, false);
        self.turn_body(r);
    }

    /// The first half of `OBJECTIVE_GOTO_AREA_ANY_MEANS`: None runs on into the on-foot code.
    fn obj_goto_area_any_means(&mut self, px: &mut Px, r: EntityRef) -> Option<Flow> {
        let dest = self.ped(r).next_route_point_pos;
        let mut dist = dest - self.epos(r);
        dist.z = 0.0;
        if self.ped(r).in_vehicle_now() {
            let veh = self.ped(r).my_vehicle.expect("in vehicle");
            px.hooks.car_goto_coors(veh, dest);
            px.hooks.register_vehicle_of_interest(veh);
            if dist.length_squared() < 20.0 * 20.0 {
                self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
                let cur = self.ped(r).objective;
                self.ped_mut(r).force_stored_objective(cur);
                self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            }
            return Some(Flow::Tail);
        }
        if dist.length() > 30.0 {
            if let Some(veh) = self.ped(r).my_vehicle {
                self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
            } else {
                let pos = self.epos(r);
                let target_veh = self.ped(r).ped_in_objective.and_then(|p| self.try_ped(p)).and_then(|p| p.my_vehicle);
                let kinds = Kinds { vehicles: true, ..Kinds::NONE };
                let mut closest = 60.0 * 60.0;
                let mut found = None;
                for near in self.find_objects_in_range(pos, 25.0, true, 6, kinds) {
                    let d = (self.epos(near) - pos).length_squared();
                    if d < closest && target_veh != Some(near) {
                        found = Some(near);
                        closest = d;
                    }
                }
                self.ped_mut(r).my_vehicle = found;
                if let Some(veh) = found {
                    self.try_veh_mut(veh).expect("vehicle").auto_pilot.cruise_speed = 0;
                    self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, veh);
                }
            }
            return Some(Flow::Tail);
        }
        None
    }

    fn obj_goto_area_on_foot(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        if matches!(pd.objective, objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA) && pd.in_vehicle_now() {
            let veh = pd.my_vehicle.expect("in vehicle");
            self.set_objective_entity(px, r, objective::LEAVE_CAR, veh);
            return;
        }
        let mut dist = pd.next_route_point_pos - self.epos(r);
        dist.z = 0.0;
        if pd.distance_to_count_seek_done * pd.distance_to_count_seek_done >= dist.length_squared() {
            let pd = self.ped_mut(r);
            pd.flags.objective_completed = true;
            pd.flags.script_objective_completed = true;
            pd.set_move_state(move_state::STILL);
        } else if now > pd.ped_state_timer || pd.ped_state != state::SEEK_POS {
            if pd.flags.use_ped_node_seek {
                let dest = pd.next_route_point_pos;
                self.ped_mut(r).seek_pos = dest;
                if self.ped(r).next_path_node.is_none() {
                    let mut best = Vec3::ZERO;
                    self.find_best_coords_from_nodes(r, &mut best);
                }
                if let Some(n) = self.ped(r).next_path_node {
                    let p = self.ents.paths.nodes[n].pos;
                    self.ped_mut(r).seek_pos = p;
                }
            }
            let pd = self.ped(r);
            let (seek, done) = (pd.seek_pos, pd.distance_to_count_seek_done);
            self.set_seek_pos(px, r, seek, done);
        }
    }

    fn obj_guard_attack(&mut self, px: &mut Px, r: EntityRef, dist: Vec3) {
        let now = self.now();
        let Some(target) = self.ped(r).ped_in_objective else {
            self.restore_previous_objective(px, r);
            return;
        };
        self.set_look_flag_entity(r, target, true);
        let attack_timer = self.ped(r).attack_timer;
        let pd = self.ped_mut(r);
        pd.look_target = Some(target);
        pd.look_timer = attack_timer;
        self.turn_body(r);
        let dist_with_target = dist.length();
        if dist_with_target >= 20.0 {
            self.restore_previous_objective(px, r);
        } else if attack_timer < now {
            if self.ped(r).ped_state != state::SEEK_ENTITY && dist_with_target >= 2.0 {
                self.set_seek_entity(r, Some(target), 1.0);
            } else {
                self.set_attack(px, r, target);
                let t = px.rng.range_f32(500.0, 1500.0) as u32;
                self.set_shoot_timer(r, t);
            }
            self.set_attack_timer(r, 1000);
        }
    }

    fn obj_catch_train(&mut self, px: &mut Px, r: EntityRef) {
        if let Some(car) = self.ped(r).car_in_objective {
            self.set_objective_entity(px, r, objective::ENTER_CAR_AS_PASSENGER, car);
            return;
        }
        let pos = self.epos(r);
        let target_veh = self.ped(r).ped_in_objective.and_then(|p| self.try_ped(p)).and_then(|p| p.my_vehicle);
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        let mut closest = CHECK_NEARBY_THINGS_MAX_DIST;
        let mut train = None;
        for near in self.find_objects_in_range(pos, 10.0, true, 6, kinds) {
            if self.try_veh(near).is_some_and(|v| v.is_train()) {
                let d = (pos - self.epos(near)).length();
                if d < closest && target_veh != Some(near) {
                    train = Some(near);
                    closest = d;
                }
            }
        }
        if let Some(t) = train {
            self.ped_mut(r).car_in_objective = Some(t);
        }
    }

    fn obj_steal_any_car(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        if self.ped(r).in_vehicle {
            self.ped_mut(r).flags.script_objective_completed = true;
            self.restore_previous_objective(px, r);
            return;
        }
        if self.ped(r).car_jack_timer >= now {
            return;
        }
        let (pos, is_player) = (self.epos(r), self.ped(r).is_player());
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        let mut closest = ENTER_CAR_MAX_DIST;
        let mut steal = None;
        // NB: the original should search ENTER_CAR_MAX_DIST here, it is fixed in Vice City
        for near in self.find_objects_in_range(pos, CHECK_NEARBY_THINGS_MAX_DIST, true, 6, kinds) {
            let Some(v) = self.try_veh(near) else { continue };
            let speed = self.ent(near).phys.as_ref().map_or(0.0, |p| p.move_speed.length());
            if v.created_by != crate::vehicle::base::MISSION_VEHICLE && speed <= 0.1 && v.can_ped_open_locks(is_player) {
                let d = (pos - self.epos(near)).length();
                if d < closest {
                    steal = Some(near);
                    closest = d;
                }
            }
        }
        if let Some(car) = steal {
            self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, car);
            self.ped_mut(r).car_jack_timer = now.wrapping_add(5000);
        } else {
            self.restore_previous_objective(px, r);
            self.restore_previous_state(px, r);
        }
    }

    fn obj_mug_char(&mut self, px: &mut Px, r: EntityRef, dist: Vec3) -> Flow {
        let Some(victim) = self.ped(r).ped_in_objective else {
            self.clear_objective(px, r);
            return Flow::Tail;
        };
        let vp = self.ped(victim);
        if vp.is_player() || vp.in_vehicle || vp.health <= 0.0 || vp.move_state > move_state::WALK {
            self.clear_objective(px, r);
            return Flow::Return;
        }
        if vp.objective == objective::KILL_CHAR_ON_FOOT && vp.ped_in_objective == Some(r) {
            self.set_objective_entity(px, r, objective::FLEE_CHAR_ON_FOOT_TILL_SAFE, victim);
            self.ped_mut(r).set_move_state(move_state::SPRINT);
            return Flow::Return;
        }
        if (vp.ped_state == state::FLEE_ENTITY && vp.flee_from == Some(r)) || (vp.objective == objective::FLEE_CHAR_ON_FOOT_TILL_SAFE && vp.ped_in_objective == Some(r)) {
            self.clear_objective(px, r);
            self.set_find_path_and_flee(px, r, victim, 15000, true);
            return Flow::Return;
        }
        let dist_sq = dist.length_squared();
        if dist_sq > 10.0 * 10.0 {
            self.clear_objective(px, r);
            let dir = (px.rng.number() & 7) as i8;
            self.set_wander_path(px, r, dir);
            return Flow::Tail;
        }
        if dist_sq > 1.4 * 1.4 {
            self.set_seek_entity(r, Some(victim), 1.0);
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_WALK) {
                a.speed = 1.3;
            }
            return Flow::Tail;
        }
        let (tp, pos) = (self.epos(victim), self.epos(r));
        self.ped_mut(r).rotation_dest = radian_angle_between_points(tp.x, tp.y, pos.x, pos.y);
        let reload = self.ped_assoc(r, anim::STD_AK_RELOAD).map(|a| (a.is_running(), a.current_time / a.hierarchy.total_length));
        if reload.is_some() || !self.ped(victim).is_ped_shootable() {
            if reload.is_some_and(|(running, progress)| !running || progress > 0.8) {
                if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, anim::STD_PARTIAL_PUNCH, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.flags |= aflags::DELETE_FADED_OUT | aflags::FADE_OUT_WHEN_DONE;
                }
                let dir = self.get_local_direction(victim, Vec2::new(dist.x, dist.y));
                self.start_fight_defend(px, victim, dir, hit_level::HIGH, 5);
                self.react_to_attack(px, victim, r);
                self.say(px, victim, sound::PED_ROBBED);
                self.say(px, r, sound::PED_MUGGING);
                self.ped_mut(r).flags.rich_from_mugging = true;
                // ClearObjective clears the victim in Vice City, so it is kept here
                self.clear_objective(px, r);
                let vp = self.ped(victim);
                if vp.objective != objective::KILL_CHAR_ON_FOOT || vp.ped_in_objective != Some(r) {
                    self.set_find_path_and_flee(px, r, victim, 15000, true);
                } else {
                    self.set_objective_entity(px, r, objective::FLEE_CHAR_ON_FOOT_TILL_SAFE, victim);
                    self.ped_mut(r).set_move_state(move_state::SPRINT);
                }
                self.ped_mut(r).last_ped_state = state::WANDER_PATH;
            }
        } else {
            let pd = self.ped(r);
            let weapon = pd.weapons[pd.current_weapon as usize].ty;
            if weapon != wt::UNARMED && weapon != wt::BASEBALLBAT {
                self.set_current_weapon(r, wt::UNARMED);
            }
            if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, anim::STD_AK_RELOAD, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                a.flags |= aflags::DELETE_FADED_OUT | aflags::FADE_OUT_WHEN_DONE;
            }
        }
        Flow::Tail
    }

    fn obj_leave_car(&mut self, px: &mut Px, r: EntityRef) {
        if self.now() <= self.ped(r).leave_car_timer {
            return;
        }
        let pd = self.ped(r);
        if !pd.in_vehicle_now() {
            self.restore_previous_objective(px, r);
            return;
        }
        let veh = pd.my_vehicle.expect("in vehicle");
        let slow = self.ent(veh).phys.as_ref().is_some_and(|p| p.move_speed.x * p.move_speed.x + p.move_speed.y * p.move_speed.y < 0.005 * 0.005);
        if !matches!(pd.ped_state, state::EXIT_CAR | state::DRAG_FROM_CAR | state::EXIT_TRAIN) && (pd.ped_type != ped_type::COP || slow) {
            if self.try_veh(veh).is_some_and(|v| v.is_train()) {
                self.set_exit_train(px, r, veh);
            } else {
                self.set_exit_car(px, r, veh, 0);
            }
        }
    }
}

/// `PED_MID`.
fn node_mid() -> usize {
    node_ids::MID
}

use super::data::node as node_ids;

/// `GAME_SPEED_TO_CARAI_SPEED`.
const GAME_SPEED_TO_CARAI_SPEED: f32 = 60.0;
/// `nEnterCarRangeMultiplier`, set by the script.
const ENTER_CAR_RANGE_MULTIPLIER: f32 = 1.0;
