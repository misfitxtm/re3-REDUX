//! `Ped.cpp`: turning to look, seeking a point or entity, fleeing, wandering paths and following a route.
use super::ctx::Px;
use super::data::*;
use crate::audio::sound;
use crate::general::{limit_radian_angle, node_heading_from_vector, radian_angle_between_points};
use crate::modelinfo::vehicle_type;
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, World};
use crate::entity::EntityRef;
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// `m_fRotationDest` brought within a half turn of `cur`.
fn wrap_towards(dest: f32, cur: f32) -> f32 {
    if cur - PI > dest {
        dest + TAU
    } else if PI + cur < dest {
        dest - TAU
    } else {
        dest
    }
}

impl World {
    /// `CPed::TurnBody`: true when the ped faces the look direction.
    pub fn turn_body(&mut self, r: EntityRef) -> bool {
        if let Some(t) = self.ped(r).look_target {
            let (target, pos) = (self.epos(t), self.epos(r));
            self.ped_mut(r).look_direction = radian_angle_between_points(target.x, target.y, pos.x, pos.y);
        }
        let pd = self.ped_mut(r);
        let mut limited = limit_radian_angle(pd.look_direction);
        let mut cur = pd.rotation_cur;
        if cur - PI > limited {
            limited += TAU;
        } else if PI + cur < limited {
            limited -= TAU;
        }
        let needed = cur - limited;
        pd.rotation_dest = limited;
        let mut done = true;
        if needed.abs() > 0.05 {
            done = false;
            cur -= needed * 0.2;
        }
        pd.rotation_cur = cur;
        pd.look_direction = limited;
        done
    }

    /// `CPed::SetSeek(pos, distanceToCountDone)`.
    pub fn set_seek_pos(&mut self, px: &mut Px, r: EntityRef, pos: Vec3, distance_to_count_done: f32) {
        let pd = self.ped(r);
        if !pd.is_ped_in_control() || (pd.ped_state == state::SEEK_POS && pd.seek_pos.x == pos.x && pd.seek_pos.y == pos.y) {
            return;
        }
        if matches!(pd.weapons[pd.current_weapon as usize].ty, wt::M16 | wt::AK47 | wt::SNIPERRIFLE | wt::ROCKETLAUNCHER | wt::SHOTGUN) {
            self.clear_point_gun_at(px, r);
        }
        if self.ped(r).ped_state != state::SEEK_POS {
            self.set_stored_state(r);
        }
        let pd = self.ped_mut(r);
        pd.ped_state = state::SEEK_POS;
        pd.distance_to_count_seek_done = distance_to_count_done;
        pd.seek_pos = pos;
    }

    /// `CPed::SetSeek(entity, distanceToCountDone)`.
    pub fn set_seek_entity(&mut self, r: EntityRef, seeking: Option<EntityRef>, distance_to_count_done: f32) {
        let pd = self.ped(r);
        if !pd.is_ped_in_control() || (pd.ped_state == state::SEEK_ENTITY && pd.seek_target == seeking) {
            return;
        }
        let Some(target) = seeking else { return };
        if self.ped(r).ped_state != state::SEEK_ENTITY {
            self.set_stored_state(r);
        }
        let pd = self.ped_mut(r);
        pd.ped_state = state::SEEK_ENTITY;
        pd.distance_to_count_seek_done = distance_to_count_done;
        pd.seek_target = Some(target);
        pd.set_move_state(move_state::STILL);
    }

    /// `CPed::ClearSeek`.
    pub fn clear_seek(&mut self, px: &mut Px, r: EntityRef) {
        self.set_idle(px, r);
        self.ped_mut(r).running_to_phone = false;
    }

    /// `CPed::Seek`: true once the ped is close enough to the seek position.
    pub fn seek(&mut self, px: &mut Px, r: EntityRef) -> bool {
        let now = self.now();
        let mut done_distance = self.ped(r).distance_to_count_seek_done;
        let mut next_move = move_state::NONE;
        let pd = self.ped(r);
        let blocked_states = matches!(pd.ped_state, state::EXIT_TRAIN | state::ENTER_TRAIN | state::SEEK_IN_BOAT);
        if pd.objective != objective::ENTER_CAR_AS_DRIVER
            && !blocked_states
            && pd.objective != objective::ENTER_CAR_AS_PASSENGER
            && pd.objective != objective::SOLICIT_VEHICLE
            && !pd.flags.duck_and_cover
        {
            let target_in_vehicle = pd.ped_in_objective.and_then(|p| self.try_ped(p)).is_some_and(|d| d.in_vehicle);
            let seed = self.ent(r).base.random_seed as u32;
            if !target_in_vehicle && self.ents.timer.frame_counter.wrapping_add(seed % 256).wrapping_add(17) & 7 == 0 {
                let kinds = Kinds { vehicles: true, ..Kinds::NONE };
                if let Some(obstacle) = self.test_sphere_against_world(pd.seek_pos, 0.4, None, kinds, false) {
                    // shortcut: the model's vehicle type stands in for the instance's `m_vehType`
                    let model = self.ent(obstacle).base.model_index as i32;
                    let is_car = self.ents.models.get(model).and_then(|m| m.vehicle()).is_none_or(|v| v.vehicle_type == vehicle_type::CAR);
                    if is_car {
                        done_distance = 2.5;
                    } else if let Some(col) = self.col_model_of(obstacle) {
                        done_distance = (col.bounding_box.max.y - col.bounding_box.min.y) * 0.55;
                    }
                }
            }
        }
        if self.ped(r).seek_target.is_none() && self.ped(r).ped_state == state::SEEK_ENTITY {
            self.clear_seek(px, r);
        }
        let pos = self.epos(r);
        let pd = self.ped(r);
        let to_seek = pd.seek_pos - pos;
        let seek_pos_dist = (to_seek.x * to_seek.x + to_seek.y * to_seek.y).sqrt();
        let leader_move = pd.ped_in_objective.and_then(|p| self.try_ped(p)).map_or(move_state::STILL, |d| d.move_state);
        if seek_pos_dist < 2.0 || pd.objective == objective::GOTO_AREA_ON_FOOT {
            if pd.objective == objective::FOLLOW_CHAR_IN_FORMATION {
                if leader_move != move_state::STILL {
                    next_move = leader_move;
                }
            } else {
                next_move = move_state::WALK;
            }
        } else if pd.objective != objective::FOLLOW_CHAR_IN_FORMATION {
            next_move = if matches!(pd.objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS | objective::RUN_TO_AREA) || pd.flags.is_running {
                move_state::RUN
            } else {
                move_state::WALK
            };
        } else if seek_pos_dist <= 2.0 {
            if leader_move != move_state::STILL {
                next_move = leader_move;
            }
        } else {
            next_move = move_state::RUN;
        }
        if pd.ped_state == state::SEEK_ENTITY && pd.seek_target.and_then(|t| self.try_ped(t)).is_some_and(|d| d.in_vehicle) {
            done_distance += 2.0;
        }

        if seek_pos_dist >= done_distance {
            let pd = self.ped(r);
            if pd.flags.is_running {
                next_move = move_state::RUN;
            }
            if now <= pd.ped_state_timer {
                if pd.action_x != 0.0 && pd.action_y != 0.0 {
                    let (ax, ay, running) = (pd.action_x, pd.action_y, pd.flags.is_running);
                    let dest = radian_angle_between_points(ax, ay, pos.x, pos.y);
                    let mut needed = (dest - pd.rotation_cur).abs();
                    if needed > PI {
                        needed = TAU - needed;
                    }
                    if needed > FRAC_PI_2 {
                        if seek_pos_dist >= 1.0 {
                            if seek_pos_dist < 2.0 {
                                next_move = if running { move_state::RUN } else { move_state::WALK };
                            }
                        } else {
                            next_move = move_state::STILL;
                        }
                    }
                    let pd = self.ped_mut(r);
                    pd.rotation_dest = dest;
                    if Vec2::new(pos.x - ax, pos.y - ay).length() < 0.5 {
                        pd.ped_state_timer = 0;
                        pd.action_x = 0.0;
                        pd.action_y = 0.0;
                    }
                }
            } else {
                let dest = radian_angle_between_points(pd.seek_pos.x, pd.seek_pos.y, pos.x, pos.y);
                let mut needed = (dest - pd.rotation_cur).abs();
                if needed > PI {
                    needed = TAU - needed;
                }
                if needed > FRAC_PI_2 {
                    if seek_pos_dist >= 1.0 && needed <= 135.0_f32.to_radians() {
                        if seek_pos_dist < 2.0 {
                            next_move = move_state::WALK;
                        }
                    } else {
                        next_move = move_state::STILL;
                    }
                }
                self.ped_mut(r).rotation_dest = dest;
            }
            let pd = self.ped(r);
            let fleeing = matches!(pd.ped_state, state::FLEE_POS | state::FLEE_ENTITY);
            if (fleeing && pd.move_state < next_move) || (!fleeing && pd.objective != objective::GOTO_CHAR_ON_FOOT && pd.wait_state == wait_state::FALSE) {
                self.ped_mut(r).set_move_state(next_move);
            }
            self.set_move_anim(px, r);
            return false;
        }

        let pd = self.ped_mut(r);
        if (pd.objective != objective::FOLLOW_CHAR_IN_FORMATION || leader_move == move_state::STILL) && pd.move_state != move_state::STILL {
            pd.ped_state_timer = 0;
            pd.action_x = 0.0;
            pd.action_y = 0.0;
        }
        if matches!(pd.objective, objective::GOTO_AREA_ON_FOOT | objective::RUN_TO_AREA | objective::GOTO_AREA_ANY_MEANS) {
            if pd.next_path_node.is_some() {
                pd.next_path_node = None;
            } else {
                pd.flags.script_objective_completed = true;
            }
            pd.flags.use_ped_node_seek = true;
        }
        if self.seek_following_path(r) {
            self.ped_mut(r).cur_path_node += 1;
        }
        true
    }

    /// `CPed::SetFlee(from, time)`.
    pub fn set_flee_pos(&mut self, r: EntityRef, from: Vec2, time: i32) {
        let now = self.now();
        let pd = self.ped(r);
        if now < pd.ped_state_timer || !pd.is_ped_in_control() || pd.flags.kinda_stay_in_same_place {
            return;
        }
        if pd.ped_state != state::FLEE_ENTITY {
            self.set_stored_state(r);
            let pd = self.ped_mut(r);
            pd.ped_state = state::FLEE_POS;
            pd.set_move_state(move_state::RUN);
            pd.flee_from_pos_x = from.x;
            pd.flee_from_pos_y = from.y;
        }
        let pos = self.epos(r);
        let pd = self.ped_mut(r);
        pd.flags.use_ped_node_seek = true;
        pd.next_path_node = None;
        pd.flee_timer = now.wrapping_add(time as u32);
        let angle = radian_angle_between_points(pos.x, pos.y, from.x, from.y);
        pd.rotation_dest = wrap_towards(limit_radian_angle(angle), pd.rotation_cur);
    }

    /// `CPed::SetFlee(entity, time)`.
    pub fn set_flee(&mut self, _px: &mut Px, r: EntityRef, flee_from: EntityRef, time: i32) {
        let pd = self.ped(r);
        if !pd.is_ped_in_control() || pd.flags.kinda_stay_in_same_place {
            return;
        }
        self.set_stored_state(r);
        let (now, pos, from) = (self.now(), self.epos(r), self.epos(flee_from));
        let pd = self.ped_mut(r);
        pd.ped_state = state::FLEE_ENTITY;
        pd.flags.use_ped_node_seek = true;
        pd.set_move_state(move_state::RUN);
        pd.flee_from = Some(flee_from);
        pd.flee_timer = if time <= 0 { 0 } else { now.wrapping_add(time as u32) };
        let angle = radian_angle_between_points(pos.x, pos.y, from.x, from.y);
        pd.rotation_dest = wrap_towards(limit_radian_angle(angle), pd.rotation_cur);
    }

    /// `CPed::ClearFlee`.
    pub fn clear_flee(&mut self, px: &mut Px, r: EntityRef) {
        self.restore_previous_state(px, r);
        let pd = self.ped_mut(r);
        pd.flags.use_ped_node_seek = false;
        pd.chat_timer = 0;
        pd.flee_timer = 0;
    }

    /// `CPed::Flee`.
    pub fn flee(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let (pos, flee_pos) = (self.epos(r), self.ents.flee_position);
        let pd = self.ped(r);
        if now > pd.flee_timer && pd.flee_timer != 0 {
            let may_finish = !(pd.ped_state == state::FLEE_ENTITY && (Vec2::new(pos.x, pos.y) - flee_pos).length_squared() < 30.0 * 30.0);
            if may_finish {
                let old_move = pd.move_state;
                self.clear_flee(px, r);
                let pd = self.ped(r);
                if matches!(pd.objective, objective::FLEE_CHAR_ON_FOOT_TILL_SAFE | objective::FLEE_CHAR_ON_FOOT_ALWAYS) {
                    self.restore_previous_objective(px, r);
                }
                if matches!(self.ped(r).ped_state, state::IDLE | state::WANDER_PATH) && px.rng.number() & 1 != 0 {
                    let wait = if old_move <= move_state::WALK { wait_state::CROSS_ROAD_LOOK } else { wait_state::FINISH_FLEE };
                    self.set_wait_state(px, r, wait, None);
                }
                return;
            }
            self.ped_mut(r).flee_timer = now.wrapping_add(5000);
        }

        if self.ped(r).flags.use_ped_node_seek {
            self.flee_along_nodes(px, r);
            return;
        }

        let pd = self.ped(r);
        if matches!(pd.ped_state, state::FLEE_ENTITY | state::ON_FIRE) && pd.ped_state_timer < now {
            let angle = radian_angle_between_points(pos.x, pos.y, flee_pos.x, flee_pos.y);
            let pd = self.ped_mut(r);
            pd.rotation_dest = wrap_towards(limit_radian_angle(angle), pd.rotation_cur);
        }

        if now & 0x20 != 0 {
            let forward_pos = pos + self.ent(r).base.matrix.forward * 4.0;
            let kinds = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
            // the original also compares forwardPos.z with itself, so only a missing floor counts
            if self.process_vertical_line(forward_pos, forward_pos.z - 100.0, kinds, false, None).is_none() {
                let pd = self.ped_mut(r);
                pd.rotation_dest += 112.5_f32.to_radians();
                pd.ped_state_timer = now.wrapping_add(2000);
            }
        }

        let pd = self.ped(r);
        if now >= pd.colliding_thing_timer {
            return;
        }
        let Some(colliding) = pd.colliding_entity_while_fleeing else { return };
        let damage_normal = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.damage_normal);
        let priority = f64::from(pd.colliding_thing_timer.wrapping_sub(now)) * 2.0 / 2500.0;
        let (cur, dest) = (pd.rotation_cur, pd.rotation_dest);
        let new_dest = if priority <= 1.5 {
            let to = self.epos(colliding);
            let to_entity = f64::from(limit_radian_angle(radian_angle_between_points(pos.x, pos.y, to.x, to.y)));
            let mut to_thing = f64::from(limit_radian_angle(radian_angle_between_points(damage_normal.x, damage_normal.y, 0.0, 0.0)));
            if to_entity - f64::from(PI) > to_thing {
                to_thing += f64::from(TAU);
            } else if f64::from(PI) + to_entity < to_thing {
                to_thing -= f64::from(TAU);
            }
            if priority <= 1.0 {
                let mut both = ((to_thing + to_entity) * 0.5) as f32;
                if dest - PI > both {
                    both += TAU;
                } else if PI + dest < both {
                    both -= TAU;
                }
                ((1.0 - priority) * f64::from(dest) + priority * f64::from(both)) as f32
            } else {
                let adjusted = (priority - 1.0) * 2.0;
                (to_entity * (1.0 - adjusted) + adjusted * to_thing) as f32
            }
        } else {
            limit_radian_angle(radian_angle_between_points(damage_normal.x, damage_normal.y, 0.0, 0.0))
        };
        let cur = limit_radian_angle(cur);
        let pd = self.ped_mut(r);
        pd.rotation_cur = cur;
        pd.rotation_dest = wrap_towards(new_dest, cur);
    }

    /// The path-node half of `CPed::Flee`.
    fn flee_along_nodes(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let (pos, flee_pos) = (self.epos(r), self.ents.flee_position);
        let away = |dir: Vec2| node_heading_from_vector(pos.x - dir.x, pos.y - dir.y);
        let mut real_last = None;
        let mut cur_dir = 9;
        let pd = self.ped(r);
        if pd.ped_state_timer < now && pd.colliding_thing_timer < now {
            if pd.next_path_node.is_some() && now > pd.chat_timer {
                cur_dir = away(flee_pos);
                let pd = self.ped_mut(r);
                if (pd.path_dir as i32) < cur_dir {
                    pd.path_dir += 8;
                }
                let diff = pd.path_dir as i32 - cur_dir;
                if diff > 2 && diff < 6 {
                    real_last = None;
                    pd.last_path_node = pd.next_path_node;
                    pd.next_path_node = None;
                }
            }
            if let Some(n) = self.ped(r).next_path_node {
                let node_pos = self.ents.paths.nodes[n].pos;
                let pd = self.ped_mut(r);
                pd.seek_pos = node_pos;
                if pd.move_state == move_state::RUN {
                    pd.flags.is_running = true;
                }
                let keep_move = pd.move_state;
                if self.seek(px, r) {
                    let pd = self.ped_mut(r);
                    real_last = pd.last_path_node;
                    pd.last_path_node = pd.next_path_node;
                    pd.next_path_node = None;
                }
                let pd = self.ped_mut(r);
                pd.flags.is_running = false;
                pd.set_move_state(keep_move);
            }
        }
        if self.ped(r).next_path_node.is_none() {
            if cur_dir == 9 {
                cur_dir = away(flee_pos);
            }
            let last = self.ped(r).last_path_node;
            let hit = self.find_next_ped_node(pos, last, cur_dir as u8);
            let next_direction = hit.map_or(0, |h| h.1);
            self.ped_mut(r).next_path_node = hit.map(|h| h.0);
            if cur_dir < next_direction as i32 {
                cur_dir += 8;
            }
            let pd = self.ped_mut(r);
            if pd.next_path_node.is_some() && pd.next_path_node != real_last && pd.next_path_node != pd.last_path_node && cur_dir - next_direction as i32 != 4 {
                pd.path_dir = next_direction as i8;
                pd.chat_timer = now.wrapping_add(2000);
            } else {
                pd.flags.use_ped_node_seek = false;
                pd.set_move_state(move_state::RUN);
                self.flee(px, r);
            }
        }
    }

    /// `CPed::WanderRange` (unused by the game): bounds are `[min x, min y, max x, max y]`.
    pub fn wander_range(&mut self, px: &mut Px, r: EntityRef) {
        if !self.seek(px, r) {
            return;
        }
        self.idle(px, r);
        let seed = self.ent(r).base.random_seed as u32;
        if seed.wrapping_add(self.ents.timer.frame_counter.wrapping_mul(3)) % 1000 > 997
            && let Some([min_x, min_y, max_x, max_y]) = self.ped(r).wander_range_bounds
        {
            let (dist_x, dist_y) = ((max_x - min_x).abs() as i32, (max_y - min_y).abs() as i32);
            let x = (px.rng.number() as i32 % dist_x) as f32 + min_x;
            let y = (px.rng.number() as i32 % dist_y) as f32 + min_y;
            let z = self.epos(r).z;
            self.set_seek_pos(px, r, Vec3::new(x, y, z), 2.5);
        }
    }

    /// `CPed::WanderPath`.
    pub fn wander_path(&mut self, px: &mut Px, r: EntityRef) {
        let Some(next) = self.ped(r).next_path_node else {
            self.set_idle(px, r);
            return;
        };
        let pd = self.ped_mut(r);
        if pd.wait_state == wait_state::FALSE && matches!(pd.move_state, move_state::STILL | move_state::NONE) {
            pd.set_move_state(move_state::WALK);
        }
        let mut seek_pos = self.ents.paths.nodes[next].pos;
        seek_pos.z += 1.0;
        self.ped_mut(r).seek_pos = seek_pos;
        if !self.seek(px, r) {
            return;
        }
        let previous_last = self.ped(r).last_path_node;
        let seed = self.ent(r).base.random_seed as u32;
        let rand_val = seed.wrapping_add(self.ents.timer.frame_counter.wrapping_mul(3)) % 100;
        let dir = self.ped(r).path_dir;
        let dir_we_wouldnt_prefer = if dir as u8 <= 3 { dir as u8 + 4 } else { dir as u8 - 4 };
        let mut node_we_wouldnt_prefer = None;
        let mut dir_to_set = 9u8;
        let mut dir_we_wouldnt_prefer2 = 9u8;
        let pd = self.ped_mut(r);
        if rand_val <= 90 {
            if rand_val > 80 {
                pd.path_dir += 2;
                pd.path_dir %= 8;
            }
        } else {
            pd.path_dir -= 2;
            if pd.path_dir < 0 {
                pd.path_dir += 8;
            }
        }
        pd.last_path_node = pd.next_path_node;
        let pos = self.epos(r);
        let (last, dir) = (self.ped(r).last_path_node, self.ped(r).path_dir as u8);
        let hit = self.find_next_ped_node(pos, last, dir);
        if let Some((_, d)) = hit {
            dir_to_set = d;
        }
        self.ped_mut(r).next_path_node = hit.map(|h| h.0);
        let mut try_count = 0;
        while self.ped(r).next_path_node.is_none() {
            try_count += 1;
            let pd = self.ped_mut(r);
            pd.path_dir = (pd.path_dir + 1) % 8;
            if try_count > 7 {
                let Some(n) = node_we_wouldnt_prefer else {
                    self.clear_all(px, r);
                    self.set_idle(px, r);
                    return;
                };
                self.ped_mut(r).next_path_node = Some(n);
                dir_to_set = dir_we_wouldnt_prefer2;
            } else {
                let (last, dir) = (self.ped(r).last_path_node, self.ped(r).path_dir as u8);
                if let Some((n, d)) = self.find_next_ped_node(pos, last, dir) {
                    dir_to_set = d;
                    if d == dir_we_wouldnt_prefer {
                        node_we_wouldnt_prefer = Some(n);
                        dir_we_wouldnt_prefer2 = d;
                    } else {
                        self.ped_mut(r).next_path_node = Some(n);
                    }
                }
            }
        }
        let pd = self.ped_mut(r);
        pd.path_dir = dir_to_set as i8;
        let (last, next) = (pd.last_path_node, pd.next_path_node);
        if last == next {
            pd.next_path_node = previous_last;
            self.set_wait_state(px, r, wait_state::DOUBLEBACK, None);
            self.say(px, r, sound::PED_WAIT_DOUBLEBACK);
        } else if let (Some(l), Some(n)) = (last, next)
            && self.ents.paths.test_for_ped_traffic_light(l, n)
        {
            self.set_wait_state(px, r, wait_state::TRAFFIC_LIGHTS, None);
        } else if let (Some(l), Some(n)) = (last, next)
            && self.ents.paths.test_crosses_road(l, n)
        {
            self.set_wait_state(px, r, wait_state::CROSS_ROAD, None);
        } else if next == previous_last {
            self.set_wait_state(px, r, wait_state::DOUBLEBACK, None);
            self.say(px, r, sound::PED_WAIT_DOUBLEBACK);
        }
    }

    /// `CPed::Avoid`: steer around a ped this one dislikes.
    pub fn avoid(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        let stats = &self.ents.ped_stats.0[pd.ped_stats];
        if stats.temper > stats.fear && stats.temper > 50 {
            return;
        }
        if now <= pd.ped_state_timer || matches!(pd.move_state, move_state::NONE | move_state::STILL) {
            return;
        }
        let Some(nearest) = pd.near_peds[0] else { return };
        let Some(nd) = self.try_ped(nearest) else { return };
        if nd.dead() || Some(nearest) == pd.seek_target || Some(nearest) == pd.ped_in_objective {
            return;
        }
        if self.ents.ped_types.avoid(pd.ped_type) & self.ents.ped_types.flag(nd.ped_type) == 0 {
            return;
        }
        let pos = self.epos(r);
        let forward = Vec2::new(-pd.rotation_cur.sin(), pd.rotation_cur.cos());
        let test = Vec2::new(pos.x, pos.y) + forward * 1.25;
        let dist = (Vec2::new(self.epos(nearest).x, self.epos(nearest).y) - test).length();
        if dist <= 1.0 && self.our_ped_can_see_this_one(r, nearest) {
            let seed = self.ent(r).base.random_seed as u32;
            let jitter = seed.wrapping_add(self.ents.timer.frame_counter.wrapping_mul(3)) % 1000 / 5;
            let pd = self.ped_mut(r);
            pd.ped_state_timer = now.wrapping_add(500).wrapping_add(jitter);
            pd.rotation_dest += 45.0_f32.to_radians();
            if !pd.flags.is_looking {
                self.set_look_flag_entity(r, nearest, false);
                let time = px.rng.range_i32(500, 800);
                self.set_look_timer(r, time);
            }
        }
    }

    /// `CPed::SeekFollowingPath`.
    pub fn seek_following_path(&self, r: EntityRef) -> bool {
        let pd = self.ped(r);
        pd.cur_path_node as i32 <= pd.num_path_nodes as i32 && pd.num_path_nodes != 0
    }

    /// `CPed::SetFollowPath`: the player only.
    pub fn set_follow_path(&mut self, r: EntityRef, dest: Vec3) -> bool {
        if self.ped(r).ped_state == state::FOLLOW_PATH || self.ents.player_ped != Some(r) {
            return false;
        }
        let pos = self.epos(r);
        if (dest - pos).length() <= 2.0 {
            return false;
        }
        let route = self.calc_ped_route(0, pos, dest, 7).unwrap_or_default();
        let pd = self.ped_mut(r);
        for (i, p) in route.iter().enumerate() {
            pd.path_node_states[i] = Vec2::new(p.x, p.y);
        }
        pd.cur_path_node = 0;
        pd.num_path_nodes = route.len() as u16;
        if pd.num_path_nodes < 1 {
            return false;
        }
        self.set_stored_state(r);
        let pd = self.ped_mut(r);
        pd.ped_state = state::FOLLOW_PATH;
        pd.set_move_state(move_state::WALK);
        true
    }

    /// `CPed::FollowPath`.
    pub fn follow_path(&mut self, px: &mut Px, r: EntityRef) {
        let z = self.epos(r).z;
        let pd = self.ped_mut(r);
        let p = pd.path_node_states[pd.cur_path_node as usize];
        pd.seek_pos = Vec3::new(p.x, p.y, z);
        if self.seek(px, r) {
            let pd = self.ped_mut(r);
            pd.cur_path_node += 1;
            if pd.cur_path_node as u16 == pd.num_path_nodes {
                self.restore_previous_state(px, r);
            }
        }
    }
}
