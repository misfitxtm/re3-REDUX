//! `Ped.cpp`: `CPed::SetDirectionToWalkAroundObject`, how a walking ped steers round something in the way
//! (the original algorithm; `NEW_WALK_AROUND_ALGORITHM` is off).
use super::ctx::Px;
use super::data::*;
use crate::entity::{EntityRef, EntityType};
use crate::general::limit_radian_angle;
use crate::math::{heading, Matrix};
use crate::modelinfo::indices_gta3::slot;
use crate::vehicle::automobile::node;
use crate::world::{Kinds, World};
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI};

/// What the sphere probe found at a corner of the object: 0 nothing, 1 building, 2 vehicle, 3 anything else.
fn corner_class(w: &World, hit: Option<EntityRef>) -> u8 {
    match hit.and_then(|h| w.get(h)).map(|e| e.base.ty) {
        None => 0,
        Some(EntityType::Building) => 1,
        Some(EntityType::Vehicle) => 2,
        Some(_) => 3,
    }
}

impl World {
    /// `CPed::SetDirectionToWalkAroundObject`.
    pub fn set_direction_to_walk_around_object(&mut self, px: &mut Px, r: EntityRef, obj: EntityRef) {
        let now = self.now();
        let mut dist_limit_for_timer = 8.0_f32;
        let Some(col) = self.col_model_of(obj) else { return };
        let (col_min, col_max) = (col.bounding_box.min, col.bounding_box.max);
        let mut col_centre = (col_min + col_max) / 2.0;
        let mut obj_mat = self.ent(obj).base.matrix;
        let mut dir_to_set = heading(obj_mat.forward);
        let (mut going_to_enter_car_and_van, mut going_to_enter_car, mut obj_upside_down) = (false, false, false);
        let mut check_interval_in_dist = (col_max.y - col_min.y) * 0.1;
        if matches!(self.ped(r).move_state, move_state::NONE | move_state::STILL) {
            return;
        }
        let model = self.ent(obj).base.model_index;
        if self.ped(r).char_created_by != MISSION_CHAR && model == self.ents.indices.gta3_slot(slot::PHONEBOOTH1) {
            let running = matches!(self.ped(r).move_state, move_state::RUN | move_state::SPRINT);
            self.set_find_path_and_flee(px, r, obj, 5000, !running);
            return;
        }
        let mut adj_min = Vec2::new(col_min.x - 0.35, col_min.y - 0.35);
        let mut adj_max = Vec2::new(col_max.x + 0.35, col_max.y + 0.35);
        check_interval_in_dist = check_interval_in_dist.max(0.5);
        check_interval_in_dist = check_interval_in_dist.min((col_max.z - col_min.z) / 2.0);
        check_interval_in_dist = check_interval_in_dist.min((adj_max.x - adj_min.x) / 2.0);
        if obj_mat.up.z < 0.0 {
            obj_upside_down = true;
        }
        let is_light = [slot::TRAFFICLIGHTS, slot::SINGLESTREETLIGHTS1, slot::SINGLESTREETLIGHTS2].into_iter().any(|s| model == self.ents.indices.gta3_slot(s));
        let obj_matrix = self.ent(obj).base.matrix;
        if !is_light {
            col_centre = obj_matrix.transform_point(col_centre);
        } else {
            check_interval_in_dist = 0.4;
            if obj_mat.up.z <= 0.57 {
                // a street light that fell over: the dimensions are those of a lying pole
                adj_min.x = 1.2 * adj_min.x.min(adj_min.y);
                adj_max.x = 1.2 * adj_max.x.max(adj_max.y);
                adj_min.y = 1.2 * col_min.z;
                adj_max.y = 1.2 * col_max.z;
                dir_to_set = heading(obj_mat.up);
                obj_mat = Matrix::IDENTITY;
                obj_mat.rotate_z(dir_to_set);
                obj_mat.pos += self.epos(obj);
                col_centre = self.epos(obj);
            } else {
                col_centre.x = adj_max.x - 0.25;
                col_centre = obj_matrix.transform_point(col_centre);
                dist_limit_for_timer = 0.75;
            }
            obj_upside_down = false;
        }
        let old_rot_dest = self.ped(r).rotation_dest;
        let pos = self.epos(r);
        let angle_to_face_obj_centre = heading(col_centre - pos);
        let angle_diff = limit_radian_angle(dir_to_set - angle_to_face_obj_centre);
        let obj_top_right_heading = (adj_max.x - adj_min.x).atan2(adj_max.y - adj_min.y);
        let check_interval_in_time = if self.ped(r).is_player() {
            let speed = self.ents.player_ped.and_then(|p| self.try_ped(p)).and_then(|p| p.player.as_ref()).map_or(0.0, |p| p.move_speed);
            if speed <= 0.0 { 0.0 } else { 2.0 / speed }
        } else {
            match self.ped(r).move_state {
                move_state::WALK => 2.0,
                move_state::RUN | move_state::SPRINT => 0.5,
                _ => 0.0,
            }
        };
        let mut check_interval_in_time = check_interval_in_time;
        let vehicle_door = self.ped(r).veh_door as usize;
        if self.ped(r).seek_target == Some(obj) && self.try_veh(obj).is_some() && matches!(self.ped(r).objective, objective::ENTER_CAR_AS_DRIVER | objective::ENTER_CAR_AS_PASSENGER | objective::SOLICIT_VEHICLE) {
            going_to_enter_car = true;
            if self.ped(r).is_player() {
                check_interval_in_time = 0.0;
            }
            if self.try_veh(obj).is_some_and(|v| v.flags.is_van) {
                going_to_enter_car_and_van = true;
            }
        }
        // what stands at (top-left, top-right, bottom-right, bottom-left) of the object
        let (mut top_left, mut top_right, mut bottom_right, mut bottom_left) = (0u8, 0u8, 0u8, 0u8);
        if now > self.ped(r).colliding_thing_timer || self.ped(r).colliding_entity_while_fleeing != Some(obj) {
            let mut colliding_thing_changed = true;
            if self.try_veh(obj).is_none() || obj_upside_down {
                colliding_thing_changed = false;
            } else {
                let interval = 0.7 * check_interval_in_dist;
                let kinds = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
                let probe = |w: &mut World, x: f32, y: f32| {
                    let mut p = obj_mat.transform_point(Vec3::new(x, y, 0.0));
                    p.z += 0.6;
                    let hit = w.test_sphere_against_world(p, check_interval_in_dist, Some(obj), kinds, false);
                    corner_class(w, hit)
                };
                top_left = probe(self, adj_min.x + interval, adj_max.y - interval);
                top_right = probe(self, adj_max.x - interval, adj_max.y - interval);
                bottom_right = probe(self, adj_max.x - interval, adj_min.y + interval);
                bottom_left = probe(self, adj_min.x + interval, adj_min.y + interval);
            }
            if top_left != 0 && top_right != 0 && bottom_right != 0 && bottom_left != 0 {
                colliding_thing_changed = false;
                (top_left, bottom_left, top_right, bottom_right) = (0, 0, 0, 0);
            }
            let rot_dest = self.ped(r).rotation_dest;
            let side = limit_radian_angle(rot_dest - angle_to_face_obj_centre);
            let left_door = going_to_enter_car && matches!(vehicle_door, node::DOOR_LF | node::DOOR_LR);
            let right_door = going_to_enter_car && matches!(vehicle_door, node::DOOR_RF | node::DOOR_RR);
            // (unless a branch below sets it, the old value stays)
            let mut walk_around_type = self.ped(r).walk_around_type;
            if !colliding_thing_changed {
                walk_around_type = 0;
            } else {
                // a "blocked" corner is a building, or something else with the opposite corners free
                let blocked = |c: u8, other_a: u8, other_b: u8| c == 1 || (c != 0 && other_a == 0 && other_b == 0);
                if angle_diff.abs() >= obj_top_right_heading {
                    if PI - obj_top_right_heading >= angle_diff.abs() {
                        if (angle_diff <= 0.0 || obj_upside_down) && (angle_diff < 0.0 || !obj_upside_down) {
                            if right_door {
                                walk_around_type = 0;
                            } else if side >= 0.0 {
                                if blocked(bottom_right, top_left, top_right) || blocked(bottom_left, top_left, top_right) {
                                    walk_around_type = 1;
                                }
                            } else if blocked(top_right, bottom_right, bottom_left) || blocked(top_left, bottom_right, bottom_left) {
                                walk_around_type = 4;
                            }
                        } else if left_door {
                            walk_around_type = 0;
                        } else if side <= 0.0 {
                            if blocked(bottom_left, top_left, top_right) || blocked(bottom_right, top_left, top_right) {
                                walk_around_type = 2;
                            }
                        } else if blocked(top_left, bottom_right, bottom_left) || blocked(top_right, bottom_right, bottom_left) {
                            walk_around_type = 3;
                        }
                    } else if left_door || side < 0.0 {
                        if blocked(top_left, top_right, bottom_right) {
                            walk_around_type = 3;
                        }
                    } else if blocked(top_right, top_left, bottom_left) {
                        walk_around_type = 4;
                    }
                } else if left_door || side > 0.0 {
                    if blocked(bottom_left, top_right, bottom_right) {
                        walk_around_type = 2;
                    }
                } else if blocked(bottom_right, top_left, bottom_left) {
                    walk_around_type = 1;
                } else {
                    walk_around_type = 0;
                }
            }
            self.ped_mut(r).walk_around_type = walk_around_type;
        }
        let rand = px.rng.number() as u32;
        let pd = self.ped_mut(r);
        pd.colliding_entity_while_fleeing = Some(obj);
        pd.colliding_thing_timer = now.wrapping_add(512).wrapping_add(rand);

        let pd = self.ped(r);
        let (walk_type, rot_dest, door) = (pd.walk_around_type, pd.rotation_dest, pd.veh_door as usize);
        let side = limit_radian_angle(rot_dest - angle_to_face_obj_centre);
        let (new_dest, mut local);
        let face = |a: f32| limit_radian_angle(a);
        if angle_diff.abs() < obj_top_right_heading {
            if going_to_enter_car {
                if going_to_enter_car_and_van && matches!(door, node::DOOR_LR | node::DOOR_RR) {
                    return;
                }
                if door != node::DOOR_LF && door != node::DOOR_LR && (bottom_right == 0 || bottom_left != 0) {
                    new_dest = face(dir_to_set - FRAC_PI_2);
                    local = Vec3::new(adj_max.x, adj_min.y, 0.0);
                } else {
                    new_dest = face(FRAC_PI_2 + dir_to_set);
                    local = Vec3::new(adj_min.x, adj_min.y, 0.0);
                }
            } else if walk_type != 1 && walk_type != 4 && (walk_type != 0 || side <= 0.0) {
                new_dest = face(dir_to_set - FRAC_PI_2);
                local = Vec3::new(adj_max.x, adj_min.y, 0.0);
            } else {
                new_dest = face(FRAC_PI_2 + dir_to_set);
                local = Vec3::new(adj_min.x, adj_min.y, 0.0);
            }
        } else if PI - obj_top_right_heading >= angle_diff.abs() {
            if angle_diff <= 0.0 {
                if !going_to_enter_car || !going_to_enter_car_and_van || (door != node::DOOR_LR && door != node::DOOR_RR) {
                    if going_to_enter_car && (door == node::DOOR_RF || (door == node::DOOR_RR && !going_to_enter_car_and_van)) {
                        return;
                    }
                    if walk_type == 4 || walk_type == 3 || (walk_type == 0 && side > 0.0) {
                        new_dest = face(PI + dir_to_set);
                        local = Vec3::new(adj_max.x, adj_min.y, 0.0);
                    } else {
                        new_dest = dir_to_set;
                        local = Vec3::new(adj_max.x, adj_max.y, 0.0);
                    }
                } else {
                    new_dest = face(PI + dir_to_set);
                    local = Vec3::new(adj_max.x, adj_min.y, 0.0);
                }
            } else if going_to_enter_car && going_to_enter_car_and_van && matches!(door, node::DOOR_LR | node::DOOR_RR) {
                new_dest = face(PI + dir_to_set);
                local = Vec3::new(adj_min.x, adj_min.y, 0.0);
            } else {
                if going_to_enter_car && (door == node::DOOR_LF || (door == node::DOOR_LR && !going_to_enter_car_and_van)) {
                    return;
                }
                if walk_type == 1 || walk_type == 2 || (walk_type == 0 && side > 0.0) {
                    new_dest = dir_to_set;
                    local = Vec3::new(adj_min.x, adj_max.y, 0.0);
                } else {
                    new_dest = face(PI + dir_to_set);
                    local = Vec3::new(adj_min.x, adj_min.y, 0.0);
                }
            }
        } else if going_to_enter_car && (!going_to_enter_car_and_van || (door != node::DOOR_LR && door != node::DOOR_RR)) {
            if door != node::DOOR_LF && door != node::DOOR_LR && (top_right == 0 || top_left != 0) {
                new_dest = face(dir_to_set - FRAC_PI_2);
                local = Vec3::new(adj_max.x, adj_max.y, 0.0);
            } else {
                new_dest = face(FRAC_PI_2 + dir_to_set);
                local = Vec3::new(adj_min.x, adj_max.y, 0.0);
            }
        } else if walk_type == 2 || walk_type == 3 || (walk_type == 0 && side > 0.0) {
            new_dest = face(dir_to_set - FRAC_PI_2);
            local = Vec3::new(adj_max.x, adj_max.y, 0.0);
        } else {
            new_dest = face(FRAC_PI_2 + dir_to_set);
            local = Vec3::new(adj_min.x, adj_max.y, 0.0);
        }
        let _ = new_dest;
        if obj_upside_down {
            local.x = -local.x;
        }
        local = obj_mat.transform_point(local);
        let pd = self.ped_mut(r);
        pd.action_x = local.x;
        pd.action_y = local.y;
        local -= pos;
        pd.rotation_dest = limit_radian_angle(heading(local));
        if pd.rotation_dest != pd.rotation_cur && pd.flags.hit_something_last_frame {
            // the aim jumps back to where it was, or turns away from the object
            pd.rotation_dest = if pd.rotation_dest == old_rot_dest { old_rot_dest } else { limit_radian_angle(PI + dir_to_set) };
        }
        let mut dist = Vec2::new(local.x, local.y).length();
        if dist < 0.5 {
            dist = 0.5;
        }
        if dist > dist_limit_for_timer {
            dist = dist_limit_for_timer;
        }
        pd.ped_state_timer = now.wrapping_add((280.0 * dist * check_interval_in_time) as u32);
    }
}
