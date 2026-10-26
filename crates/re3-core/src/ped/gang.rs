//! `PedAI.cpp`: reacting to attackers and guns, gang help, ducking behind cars and warping near a target.
use super::ctx::Px;
use super::data::*;
use super::types::{ped_type, stat_flag};
use crate::anim::{assoc_group, tables_gta3::anim};
use crate::audio::sound;
use crate::control::pathfind::PathFind;
use crate::entity::{status, EntityRef};
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::base::{car_mission, driving_style, MISSION_VEHICLE};
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, LineOpts, World};
use glam::{Vec2, Vec3};
use std::f32::consts::TAU;

/// `GAME_SPEED_TO_CARAI_SPEED`.
const GAME_SPEED_TO_CARAI_SPEED: f32 = 60.0;

/// `SelectClosestNodeForSeek`: picks the path node next to the ped that gets it round an obstacle.
#[allow(clippy::too_many_arguments)]
fn select_closest_node_for_seek(paths: &PathFind, seek_pos: Vec3, next: &mut Option<usize>, node: usize, mut close_dist: Vec2, mut far_dist: Vec2, close_node: Option<usize>, close_node2: Option<usize>, mut run_count: i32) {
    let n = &paths.nodes[node];
    for i in 0..n.num_links as usize {
        let test = paths.connected_node(n.first_link as usize + i);
        if Some(test) == close_node || Some(test) == close_node2 {
            continue;
        }
        let diff3 = seek_pos - paths.nodes[test].pos;
        let pos_diff = Vec2::new(diff3.x, diff3.y);
        let dist = pos_diff.length_squared();
        if far_dist.length_squared() > dist {
            if close_dist.length_squared() <= dist {
                *next = close_node;
                close_dist = pos_diff;
            } else {
                *next = close_node2.or(Some(test));
                far_dist = pos_diff;
            }
        }
        run_count -= 1;
        if run_count > 0 {
            select_closest_node_for_seek(paths, seek_pos, next, test, close_dist, far_dist, close_node, close_node2.or(Some(test)), run_count);
        }
    }
}

impl World {
    /// `CPed::RegisterThreatWithGangPeds`.
    pub fn register_threat_with_gang_peds(&mut self, r: EntityRef, attacker: Option<EntityRef>) {
        let mut attacker_ped = None;
        if let Some(a) = attacker
            && !matches!(self.ped(r).objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS)
        {
            attacker_ped = if self.is_ped(a) {
                Some(a)
            } else {
                match self.try_veh(a).map(|v| v.driver) {
                    Some(Some(d)) => Some(d),
                    _ => return,
                }
            };
            if let Some(ap) = attacker_ped.filter(|&ap| self.ped(ap).is_player() || self.ped(ap).is_gang_member()) {
                let (our_type, flag) = (self.ped(r).ped_type, self.ents.ped_types.flag(self.ped(ap).ped_type));
                let near = self.ped(r).near_peds;
                for n in near.iter().take(self.ped(r).num_near_peds as usize).flatten().copied() {
                    if self.ped_pointer_valid(n) && n != r && self.ped(n).ped_type == our_type {
                        self.ped_mut(n).fear_flags |= flag;
                    }
                }
            }
        }
        let Some(ap) = attacker_ped.filter(|&ap| self.ped(ap).is_player() && (self.ped(ap).ped_state == state::CARJACK || self.ped(ap).in_vehicle)) else { return };
        let toyz = self.ped(ap).my_vehicle.is_some_and(|v| self.ent(v).base.model_index == mi::TOYZ);
        if toyz {
            return;
        }
        let pos = self.epos(r);
        let kinds = Kinds { vehicles: true, ..Kinds::NONE };
        let our_type = self.ped(r).ped_type;
        for near in self.find_objects_in_range(pos, ENTER_CAR_MAX_DIST, true, 6, kinds) {
            let Some(v) = self.try_veh(near) else { continue };
            if v.created_by == MISSION_VEHICLE {
                continue;
            }
            let Some(d) = v.driver.filter(|&d| d != r && self.ped(d).ped_type == our_type) else { continue };
            let _ = d;
            let model = self.ent(near).base.model_index;
            if v.is_vehicle_normal(model, self.ent(near).base.status) && v.is_car() {
                let cruise = self.ents.handling.get(v.handling).transmission.max_cruise_velocity;
                self.get_mut(near).expect("vehicle").base.status = status::PHYSICS;
                let v = self.try_veh_mut(near).expect("vehicle");
                v.auto_pilot.cruise_speed = (GAME_SPEED_TO_CARAI_SPEED * cruise * 0.8) as u8;
                v.auto_pilot.car_mission = car_mission::RAMPLAYER_FARAWAY;
                v.auto_pilot.temp_action = 0;
                v.auto_pilot.driving_style = driving_style::AVOID_CARS;
            }
        }
    }

    /// `CPed::InformMyGangOfAttack`: gang members that follow this ped go for the attacker.
    pub fn inform_my_gang_of_attack(&mut self, px: &mut Px, r: EntityRef, attacker: EntityRef) {
        if matches!(self.ped(r).objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS) {
            return;
        }
        let attacker_ped = if self.is_ped(attacker) {
            attacker
        } else {
            match self.try_veh(attacker).map(|v| v.driver) {
                Some(Some(d)) => d,
                _ => return,
            }
        };
        if self.ped(attacker_ped).ped_type == ped_type::COP {
            return;
        }
        let pd = self.ped(r);
        let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
        for n in near {
            if n == r {
                continue;
            }
            let np = self.ped(n);
            let stats = &self.ents.ped_stats.0[np.ped_stats];
            if np.leader == Some(r) && stats.fear < stats.temper {
                let now = self.now();
                self.set_objective_entity(px, n, objective::KILL_CHAR_ON_FOOT, attacker_ped);
                self.ped_mut(n).set_objective_timer(30000, now);
            }
        }
    }

    /// `CPed::SelectGunIfArmed`: used when the current gun runs dry.
    pub fn select_gun_if_armed(&mut self, r: EntityRef) -> bool {
        let max = self.ped(r).max_weapon_type_allowed as usize;
        for i in 0..max.min(wt::TOTAL_INVENTORY) {
            let w = &self.ped(r).weapons[i];
            if w.ammo_total > 0 && matches!(w.ty, wt::BASEBALLBAT | wt::COLT45 | wt::UZI | wt::SHOTGUN | wt::M16 | wt::SNIPERRIFLE | wt::ROCKETLAUNCHER) {
                self.set_current_weapon(r, i);
                return true;
            }
        }
        self.set_current_weapon(r, wt::UNARMED);
        false
    }

    /// `CPed::ReactToPointGun`.
    pub fn react_to_point_gun(&mut self, px: &mut Px, r: EntityRef, ped_with_gun: EntityRef) {
        let pd = self.ped(r);
        if pd.is_player() || !pd.is_ped_in_control() || pd.char_created_by == MISSION_CHAR || pd.leader == Some(ped_with_gun) {
            return;
        }
        let (pos, gun_pos) = (self.epos(r), self.epos(ped_with_gun));
        let far = (pos.x - gun_pos.x).powi(2) + (pos.y - gun_pos.y).powi(2) > 225.0;
        if matches!(pd.wait_state, wait_state::PLAYANIM_HANDSUP | wait_state::PLAYANIM_HANDSCOWER) || far {
            return;
        }
        if let Some(l) = pd.leader {
            if self.ents.player_ped == Some(l) {
                return;
            }
            self.clear_leader(px, r);
        }
        let pd = self.ped(r);
        let melee = pd.weapons[pd.current_weapon as usize].is_type_melee();
        let stats_flags = self.ents.ped_stats.0[pd.ped_stats].flags;
        if stats_flags & stat_flag::GUN_PANIC != 0 && (pd.ped_state != state::ATTACK || melee) && pd.ped_state != state::FLEE_ENTITY && pd.ped_state != state::AIM_GUN {
            let wait = px.rng.range_i32(3000, 6000);
            self.set_wait_state(px, r, wait_state::PLAYANIM_HANDSCOWER, Some(wait));
            self.say(px, r, sound::PED_HANDS_COWER);
            let pd = self.ped_mut(r);
            pd.look_target = Some(ped_with_gun);
            pd.set_move_state(move_state::NONE);
        } else if pd.ped_type != self.ped(ped_with_gun).ped_type {
            let ours = pd.ped_type;
            if self.ped(r).is_gang_member() || matches!(ours, ped_type::EMERGENCY | ped_type::FIREMAN) {
                self.register_threat_with_gang_peds(r, Some(ped_with_gun));
            }
            if ours == ped_type::COP && self.ped(ped_with_gun).is_player() {
                self.set_wanted_level_no_drop(2);
                let pd = self.ped(r);
                if pd.flags.crouch_when_shooting || pd.flags.kinda_stay_in_same_place {
                    let time = px.rng.range_i32(1000, 3000) as u32;
                    self.set_duck(r, time);
                    return;
                }
            }
            let pd = self.ped(r);
            let gun_is_player = self.ped(ped_with_gun).is_player();
            let melee = pd.weapons[pd.current_weapon as usize].is_type_melee();
            if ours != ped_type::COP
                && (pd.ped_state != state::ATTACK || melee)
                && (pd.ped_state != state::FLEE_ENTITY || (gun_is_player && pd.flee_from != Some(ped_with_gun)))
                && pd.ped_state != state::AIM_GUN
                && pd.objective != objective::KILL_CHAR_ON_FOOT
            {
                let wait = px.rng.range_i32(3000, 6000);
                self.set_wait_state(px, r, wait_state::PLAYANIM_HANDSUP, Some(wait));
                self.say(px, r, sound::PED_HANDS_UP);
                let pd = self.ped_mut(r);
                pd.look_target = Some(ped_with_gun);
                pd.set_move_state(move_state::NONE);
                if pd.ped_state == state::FLEE_ENTITY {
                    pd.flee_from = Some(ped_with_gun);
                }
                if self.ents.player_ped == Some(ped_with_gun) && self.ped(r).flags.rich_from_mugging {
                    self.drop_mugging_money(px, r);
                }
            }
        }
    }

    /// A mugged ped that is held up by the player drops its money around itself.
    fn drop_mugging_money(&mut self, px: &mut Px, r: EntityRef) {
        let money = px.rng.range_i32(100, 300);
        let pickup_count = money / 40 + 1;
        let money_per_pickup = money / pickup_count;
        let pos = self.epos(r);
        for _ in 0..pickup_count {
            let a = (px.rng.number() % 256) as f32 / 256.0 * TAU;
            let x = 1.5 * a.sin() + pos.x;
            let a = (px.rng.number() % 256) as f32 / 256.0 * TAU;
            let y = 1.5 * a.cos() + pos.y;
            let (z, found) = self.find_ground_z_for_3d_coord(Vec3::new(x, y, pos.z));
            if found {
                let amount = money_per_pickup + (px.rng.number() & 7) as i32;
                px.hooks.generate_money_pickup(Vec3::new(x, y, z + 0.5), amount);
            }
        }
        self.ped_mut(r).flags.rich_from_mugging = false;
    }

    /// `CPed::ReactToAttack`.
    pub fn react_to_attack(&mut self, px: &mut Px, r: EntityRef, attacker: EntityRef) {
        if self.ped(r).is_player() && self.is_ped(attacker) {
            self.inform_my_gang_of_attack(px, r, attacker);
            self.set_look_flag_entity(r, attacker, true);
            self.set_look_timer(r, 700);
            return;
        }
        let pd = self.ped(r);
        if !(pd.is_ped_in_control() && (pd.char_created_by != MISSION_CHAR || pd.flags.responds_to_threats)) {
            return;
        }
        let our_leader = pd.leader;
        if our_leader == Some(attacker) || our_leader.is_some_and(|l| self.ents.player_ped == Some(l)) || !self.is_ped(attacker) {
            return;
        }
        let melee = |w: &World, e: EntityRef| {
            let p = w.ped(e);
            p.weapons[p.current_weapon as usize].is_type_melee()
        };
        if pd.flags.not_allowed_to_duck {
            if !melee(self, attacker) {
                let now = self.now();
                self.ped_mut(r).duck_and_cover_timer = now;
                return;
            }
        } else if pd.flags.crouch_when_shooting || pd.flags.kinda_stay_in_same_place {
            let time = px.rng.range_i32(1000, 3000) as u32;
            self.set_duck(r, time);
            return;
        }
        let ours = &self.ents.ped_stats.0[self.ped(r).ped_stats];
        let theirs = &self.ents.ped_stats.0[self.ped(attacker).ped_stats];
        let (our_fear, their_temper) = (i32::from(ours.fear), i32::from(theirs.temper));
        let same_stats = self.ped(r).ped_stats == self.ped(attacker).ped_stats;
        if our_fear <= 100 - their_temper {
            if !same_stats {
                let ours = self.ped(r).ped_type;
                if self.ped(r).is_gang_member() || matches!(ours, ped_type::EMERGENCY | ped_type::FIREMAN) {
                    self.register_threat_with_gang_peds(r, Some(attacker));
                }
                if !melee(self, attacker) && melee(self, r) {
                    self.set_objective_entity(px, r, objective::FLEE_CHAR_ON_FOOT_TILL_SAFE, attacker);
                    self.ped_mut(r).set_move_state(move_state::RUN);
                } else {
                    self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, attacker);
                    let now = self.now();
                    self.ped_mut(r).set_objective_timer(20000, now);
                }
            }
        } else {
            self.set_objective_entity(px, r, objective::FLEE_CHAR_ON_FOOT_TILL_SAFE, attacker);
            self.ped_mut(r).set_move_state(move_state::RUN);
            if melee(self, attacker) {
                self.say(px, r, sound::PED_FLEE_RUN);
            }
        }
    }

    /// `CPed::SetDuck`.
    pub fn set_duck(&mut self, r: EntityRef, time: u32) {
        let now = self.now();
        let pd = self.ped(r);
        if pd.flags.is_ducking || now <= pd.duck_timer {
            return;
        }
        let crouch = pd.flags.crouch_when_shooting && matches!(pd.ped_state, state::ATTACK | state::AIM_GUN);
        let which = if crouch { anim::STD_DUCK_LOW } else { anim::STD_DUCK_DOWN };
        if self.ped_assoc(r, which).is_none_or(|a| a.blend_delta < 0.0) {
            self.ped_blend_anim(r, assoc_group::STD, which, 4.0);
            let pd = self.ped_mut(r);
            pd.flags.is_ducking = true;
            pd.duck_timer = now.wrapping_add(time);
        }
    }

    /// `CPed::Duck`.
    pub fn duck(&mut self, r: EntityRef) {
        if self.now() > self.ped(r).duck_timer {
            self.clear_duck(r);
        }
    }

    /// `CPed::ClearDuck`.
    pub fn clear_duck(&mut self, r: EntityRef) {
        if self.ped_assoc(r, anim::STD_DUCK_DOWN).is_none() && self.ped_assoc(r, anim::STD_DUCK_LOW).is_none() {
            self.ped_mut(r).flags.is_ducking = false;
            return;
        }
        let pd = self.ped(r);
        if !pd.flags.crouch_when_shooting || !matches!(pd.ped_state, state::ATTACK | state::AIM_GUN) {
            return;
        }
        if self.ped_assoc(r, anim::STD_RBLOCK_SHOOT).is_none_or(|a| a.blend_delta < 0.0) {
            self.ped_blend_anim(r, assoc_group::STD, anim::STD_RBLOCK_SHOOT, 4.0);
        }
    }

    /// `CPed::DuckAndCover`: true while the ped is still on its way to cover.
    pub fn duck_and_cover(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let now = self.now();
        let pd = self.ped(r);
        let Some(target) = pd.ped_in_objective.filter(|_| now > pd.duck_and_cover_timer) else { return false };
        if pd.flags.kinda_stay_in_same_place {
            if now <= pd.leave_car_timer {
                if pd.look_target != Some(target) {
                    self.ped_mut(r).look_target = Some(target);
                }
                if !self.ped(r).flags.is_aiming_gun {
                    self.set_aim_flag_entity(r, target);
                }
            } else {
                let wait = px.rng.range_i32(20000, 30000) as u32;
                let cover = self.ped(r).seek_target;
                let pd = self.ped_mut(r);
                pd.flags.crouch_when_shooting = false;
                pd.flags.kinda_stay_in_same_place = false;
                pd.flags.is_ducking = false;
                pd.flags.duck_and_cover = false;
                pd.heading_rate = 10.0;
                pd.duck_and_cover_timer = now.wrapping_add(wait);
                if let Some(v) = cover.and_then(|c| self.try_veh_mut(c)) {
                    v.num_peds_use_it_as_cover -= 1;
                }
            }
            return false;
        }
        let mut just_ducked = false;
        let pd = self.ped_mut(r);
        pd.flags.is_ducking = false;
        pd.flags.crouch_when_shooting = false;
        if now > self.ped(r).leave_car_timer {
            let pos = self.epos(r);
            let kinds = Kinds { vehicles: true, ..Kinds::NONE };
            let mut max_dist = 225.0;
            let mut found = None;
            for v in self.find_objects_in_range(pos, CHECK_NEARBY_THINGS_MAX_DIST, true, 6, kinds) {
                let d = self.try_veh(v).expect("vehicle");
                let speed = self.ent(v).phys.as_ref().map_or(0.0, |p| p.move_speed.length());
                if speed <= 0.02 && !d.flags.is_bus && !d.flags.is_van && !d.flags.is_big && d.num_peds_use_it_as_cover < 3 {
                    let dist = (pos - self.epos(v)).length_squared();
                    if dist < max_dist {
                        max_dist = dist;
                        found = Some(v);
                    }
                }
            }
            if let Some(veh) = found {
                let covers = self.try_veh(veh).expect("vehicle").num_peds_use_it_as_cover;
                let (right_side, left_side) = match covers {
                    2 => (Vec3::new(1.5, -0.5, 0.0), Vec3::new(-1.5, -0.5, 0.0)),
                    1 => (Vec3::new(1.5, 0.5, 0.0), Vec3::new(-1.5, 0.5, 0.0)),
                    _ => (Vec3::new(1.5, 0.0, 0.0), Vec3::new(-1.5, 0.0, 0.0)),
                };
                let m = self.ent(veh).base.matrix;
                let on = |s: Vec3| m.right * s.x + m.forward * s.y + m.up * s.z + m.pos;
                let (right_pos, left_pos) = (on(right_side), on(left_side));
                let target_pos = self.epos(target);
                let duck_pos = if (target_pos - right_pos).length_squared() <= (target_pos - left_pos).length_squared() { left_pos } else { right_pos };
                let solid = Kinds { buildings: true, vehicles: true, peds: true, ..Kinds::NONE };
                let los = Kinds { buildings: true, objects: true, ..Kinds::NONE };
                if self.test_sphere_against_world(duck_pos, 0.5, None, solid, false).is_some() && self.is_line_of_sight_clear(pos, duck_pos, los, LineOpts::default()) {
                    self.set_seek_pos(px, r, duck_pos, 1.0);
                    let pd = self.ped_mut(r);
                    pd.heading_rate = 15.0;
                    pd.flags.is_running = true;
                    pd.flags.duck_and_cover = true;
                    just_ducked = true;
                    pd.leave_car_timer = now.wrapping_add(500);
                    if self.try_veh(veh).is_some_and(|v| v.flags.is_law_enforcer) {
                        self.ped_mut(r).car_in_objective = Some(veh);
                    }
                    self.ped_mut(r).seek_target = Some(veh);
                    self.clear_point_gun_at(px, r);
                } else {
                    let wait = px.rng.range_i32(10000, 15000) as u32;
                    let pd = self.ped_mut(r);
                    pd.duck_and_cover_timer = now.wrapping_add(wait);
                    pd.flags.duck_and_cover = false;
                }
            } else {
                self.ped_mut(r).flags.duck_and_cover = false;
            }
        }
        if !just_ducked && !self.ped(r).flags.duck_and_cover {
            return false;
        }
        if !self.seek(px, r) {
            return true;
        }
        let pd = self.ped_mut(r);
        pd.flags.kinda_stay_in_same_place = true;
        pd.flags.duck_and_cover = false;
        pd.seek_pos = Vec3::ZERO;
        if let Some(v) = self.ped(r).seek_target.and_then(|c| self.try_veh_mut(c)) {
            v.num_peds_use_it_as_cover += 1;
        }
        self.set_idle(px, r);
        self.ped_mut(r).set_move_state(move_state::STILL);
        self.set_move_anim(px, r);
        if self.ped(r).look_target != Some(target) {
            self.ped_mut(r).look_target = Some(target);
        }
        let wait = px.rng.range_i32(3000, 6000) as u32;
        self.ped_mut(r).leave_car_timer = now.wrapping_add(wait);
        false
    }

    /// `CPed::FindBestCoordsFromNodes`: a path node to head for when a wall blocks the way to the seek position.
    pub fn find_best_coords_from_nodes(&mut self, r: EntityRef, best_coords: &mut Vec3) -> bool {
        let pd = self.ped(r);
        if pd.next_path_node.is_some() || !pd.flags.use_ped_node_seek {
            return false;
        }
        let our_pos = self.epos(r);
        let seek_pos = pd.seek_pos;
        let closest = self.ents.paths.find_node_closest_to_coors(our_pos, crate::control::pathfind::PATH_PED, 999_999.9, false, false);
        let mut seek_obj_pos = seek_pos;
        seek_obj_pos.z += 1.0;
        let walls = Kinds { buildings: true, objects: true, ..Kinds::NONE };
        if self.is_line_of_sight_clear(our_pos, seek_obj_pos, walls, LineOpts::default()) || closest < 0 {
            return false;
        }
        self.ped_mut(r).next_path_node = None;
        let seek_pos_dist = Vec2::new(seek_pos.x - our_pos.x, seek_pos.y - our_pos.y);
        let closest_node = closest as usize;
        let node_pos = self.ents.paths.nodes[closest_node].pos;
        let close_dist = Vec2::new(seek_pos.x - node_pos.x, seek_pos.y - node_pos.y);
        let mut next = None;
        select_closest_node_for_seek(&self.ents.paths, seek_pos, &mut next, closest_node, close_dist, seek_pos_dist, Some(closest_node), None, 3);
        self.ped_mut(r).next_path_node = next;
        // the search decided that going to the next node is more sensible than seeking the object
        if let Some(n) = next {
            let p = self.ents.paths.nodes[n].pos;
            let to_next = p - our_pos;
            if to_next.x * to_next.x + to_next.y * to_next.y < seek_pos_dist.length_squared() {
                *best_coords = p;
                return true;
            }
            self.ped_mut(r).next_path_node = None;
        }
        false
    }

    /// `CPed::SetSeekBoatPosition`.
    pub fn set_seek_boat_position(&mut self, r: EntityRef, boat: EntityRef) {
        let pd = self.ped(r);
        if pd.ped_state == state::SEEK_IN_BOAT || self.try_veh(boat).is_some_and(|v| v.driver.is_some()) || !pd.is_ped_in_control() {
            return;
        }
        self.set_stored_state(r);
        let pd = self.ped_mut(r);
        pd.car_in_objective = Some(boat);
        pd.my_vehicle = Some(boat);
        pd.distance_to_count_seek_done = 0.5;
        pd.ped_state = state::SEEK_IN_BOAT;
    }

    /// `CPed::SeekBoatPosition`.
    pub fn seek_boat_position(&mut self, px: &mut Px, r: EntityRef) {
        let Some(boat) = self.ped(r).car_in_objective.filter(|&b| self.try_veh(b).is_some_and(|v| v.driver.is_none())) else {
            self.restore_previous_state(px, r);
            return;
        };
        let model = self.ent(boat).base.model_index as i32;
        let front = self.ents.models.get(model).and_then(|m| m.vehicle()).map_or(Vec3::ZERO, |v| v.positions[2]);
        let enter_offset = Vec3::new(0.0, front.y, front.z);
        let m = self.ent(boat).base.matrix;
        let enter_pos = m.right * enter_offset.x + m.forward * enter_offset.y + m.up * enter_offset.z + m.pos;
        let pd = self.ped_mut(r);
        pd.set_move_state(move_state::WALK);
        pd.seek_pos = enter_pos;
        if self.seek(px, r) {
            // arrived at the boat
            self.ped_mut(r).veh_door = 0;
            self.set_enter_car(px, r, boat, 0);
        }
    }

    /// `CPed::WarpPedToNearLeaderOffScreen`.
    pub fn warp_ped_to_near_leader_off_screen(&mut self, px: &mut Px, r: EntityRef) -> bool {
        match self.ped(r).leader {
            Some(l) => self.warp_ped_to_near_entity_off_screen(px, r, l),
            None => false,
        }
    }

    /// `CPed::WarpPedToNearEntityOffScreen`: steps towards the entity until a spot out of the camera's sight is found.
    pub fn warp_ped_to_near_entity_off_screen(&mut self, px: &mut Px, r: EntityRef, warp_to: EntityRef) -> bool {
        let now = self.now();
        if px.hooks.is_on_screen(r) || self.ped(r).leave_car_timer > now {
            return false;
        }
        let warp_to_pos = self.epos(warp_to);
        let our_pos = self.epos(r);
        let dist_vec = warp_to_pos - our_pos;
        let half_of_dist = dist_vec.length() * 0.5;
        let half_normalized = dist_vec / half_of_dist;
        let mut appropriate = our_pos;
        let mut z_corrected = appropriate;
        let try_count = half_of_dist.min(10.0) as i32;
        let mut teleported = false;
        let seen = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
        for _ in 0..try_count {
            appropriate += half_normalized;
            self.find_z_coor_for_ped(&mut z_corrected);
            if (z_corrected.z - warp_to_pos.z).abs() >= 3.0 && (z_corrected.z - appropriate.z).abs() >= 3.0 {
                continue;
            }
            appropriate.z = z_corrected.z;
            if !px.hooks.is_sphere_visible(appropriate, 0.6)
                && self.is_line_of_sight_clear(appropriate, warp_to_pos, seen, LineOpts::default())
                && self.test_sphere_against_world(appropriate, 0.6, Some(r), seen, false).is_none()
            {
                teleported = true;
                self.teleport(r, appropriate);
            }
        }
        self.ped_mut(r).leave_car_timer = now.wrapping_add(3000);
        teleported
    }
}
