//! `CarCtrl.cpp`, part 4: moving a car along the road by hand (`DragCarToPoint`) and steering it with physics.
// the original's Min(Max()) drops a NaN angle where `clamp` would keep it
#![allow(clippy::manual_clamp)]
use super::car_ctrl::{this_road_object_could_move, GAME_SPEED_TO_CARAI_SPEED, LANE_WIDTH, TIME_COPS_WAIT_TO_EXIT_AFTER_STOPPING};
use super::car_traffic::limit_radian_angle;
use crate::collision::{is_stored_poly_still_valid_vertical_line, ColPoint};
use crate::entity::EntityRef;
use crate::math::atan_of_xy;
use crate::modelinfo::indices_gta3::mi;
use crate::ped::ctx::Px;
use crate::vehicle::base::{car_mission as m, driving_style, temp_action, VehicleData};
use crate::world::{Kinds, World};
use glam::{Vec2, Vec3};

const DISTANCE_TO_SWITCH_FROM_BLOCK_TO_STOP: f32 = 5.0;
const DISTANCE_TO_SWITCH_FROM_STOP_TO_BLOCK: f32 = 10.0;
const MAX_SPEED_TO_ACCOUNT_IN_INTERCEPTING: f32 = 0.13;
const DISTANCE_TO_NEXT_NODE_TO_CONSIDER_SLOWING_DOWN: f32 = 40.0;
const MAX_ANGLE_TO_STEER_AT_HIGH_SPEED: f32 = 0.2;
const MIN_SPEED_TO_START_LIMITING_STEER: f32 = 0.45;
const DISTANCE_TO_NEXT_NODE_TO_SELECT_NEW: f32 = 5.0;
const DISTANCE_TO_FACING_NEXT_NODE_TO_SELECT_NEW: f32 = 8.0;
const DEFAULT_MAX_STEER_ANGLE: f32 = 0.5;
const MIN_LOWERING_SPEED_COEFFICIENT: f32 = 0.4;
const MAX_ANGLE_FOR_SPEED_LIMITING: f32 = 1.2;
const MIN_ANGLE_FOR_SPEED_LIMITING: f32 = 0.4;
const MIN_ANGLE_FOR_SPEED_LIMITING_BETWEEN_NODES: f32 = 0.1;
const MIN_ANGLE_TO_APPLY_HANDBRAKE: f32 = 0.7;
const MIN_SPEED_TO_APPLY_HANDBRAKE: f32 = 0.3;

/// What the steering decides for the pedals and the wheel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Steer {
    pub swerve: f32,
    pub accel: f32,
    pub brake: f32,
    pub handbrake: bool,
}

/// `FindSpeedMultiplier`: 1 for a small turn, falling to `coef` for a big one.
pub fn find_speed_multiplier(angle_change: f32, min_angle: f32, max_angle: f32, coef: f32) -> f32 {
    let angle = limit_radian_angle(angle_change).abs();
    let n = (angle - min_angle).max(0.0);
    let d = max_angle - min_angle;
    let mult = 1.0 - n / d * (1.0 - coef);
    if n > d { coef } else { mult }
}

fn norm2(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.y).normalize_or_zero()
}

impl World {
    /// `FindMaxSteerAngle`.
    pub fn find_max_steer_angle(&self, r: EntityRef) -> f32 {
        if self.ent(r).base.model_index == mi::ENFORCER { 0.7 } else { DEFAULT_MAX_STEER_ANGLE }
    }

    /// The ground height under one end of the car, from the stored triangle, or looking again, or what it was last time.
    fn ground_under(&mut self, r: EntityRef, at: Vec2, slot: usize, last: f32) -> f32 {
        let z = self.epos(r).z;
        let mut point = ColPoint::default();
        let mut poly = self.try_veh(r).expect("vehicle").coll_polys[slot];
        let road = Kinds { buildings: true, ..Kinds::NONE };
        let found;
        if is_stored_poly_still_valid_vertical_line(Vec3::new(at.x, at.y, z - 3.0), z - 3.0, &mut point, &mut poly) {
            found = Some(point.point.z);
        } else if let Some(h) = self.process_vertical_line(Vec3::new(at.x, at.y, z + 1.5), z - 2.0, road, false, Some(&mut poly)) {
            self.stand_on_road(r, h.entity, &mut poly);
            found = Some(h.point.point.z);
        } else if let Some(h) = self.process_vertical_line(Vec3::new(at.x, at.y, z + 3.0), z - 3.0, road, false, Some(&mut poly)) {
            self.stand_on_road(r, h.entity, &mut poly);
            found = Some(h.point.point.z);
        } else {
            found = None;
        }
        self.try_veh_mut(r).expect("vehicle").coll_polys[slot] = poly;
        found.unwrap_or(last)
    }

    fn stand_on_road(&mut self, r: EntityRef, road: EntityRef, poly: &mut crate::collision::StoredCollPoly) {
        let model = self.ent(road).base.model_index;
        self.try_veh_mut(r).expect("vehicle").cur_ground_entity = Some(road);
        if this_road_object_could_move(&self.ents.indices, model) {
            poly.valid = false;
        }
    }

    /// `DragCarToPoint`: puts the car on the road at `point`, tilted to the ground ahead and behind.
    pub fn drag_car_to_point(&mut self, r: EntityRef, point: Vec3) {
        let (pos, fwd) = {
            let e = self.ent(r);
            (e.base.position(), e.base.matrix.forward)
        };
        let behind = Vec2::new(pos.x, pos.y) - Vec2::new(fwd.x, fwd.y) * 1.5;
        let target = Vec2::new(point.x, point.y);
        let direction = behind - target;
        let mid = target + direction * 3.0 / direction.length();
        let (last_ahead, last_behind) = {
            let v = self.try_veh(r).expect("vehicle");
            (v.map_object_height_ahead, v.map_object_height_behind)
        };
        let ahead_z = self.ground_under(r, target, 0, last_ahead);
        self.try_veh_mut(r).expect("vehicle").map_object_height_ahead = ahead_z;
        let behind_z = self.ground_under(r, mid, 1, last_behind);
        self.try_veh_mut(r).expect("vehicle").map_object_height_behind = behind_z;
        let angle_z = ((ahead_z - behind_z) / 3.0).atan2(1.0);
        let (cos_z, sin_z) = (angle_z.cos(), angle_z.sin());
        let right = Vec3::new(target.y - mid.y, -(target.x - mid.x), 0.0) / 3.0;
        let forward = Vec3::new(-cos_z * right.y, cos_z * right.x, sin_z);
        let up = crate::math::cross(right, forward);
        let height = self.col_model_of(r).map_or(0.0, |c| VehicleData::height_above_road(&c));
        let e = self.get_mut(r).expect("vehicle");
        e.base.matrix.right = right;
        e.base.matrix.forward = forward;
        e.base.matrix.up = up;
        let mut p = (Vec3::new(mid.x, mid.y, behind_z) + Vec3::new(target.x, target.y, ahead_z)) / 2.0;
        p.z += height;
        e.base.matrix.pos = p;
    }

    /// `SteerAICarWithPhysics`: the temporary actions first, then the mission.
    pub fn steer_ai_car_with_physics(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let Some(v) = self.try_veh(r) else { return };
        let ta = v.auto_pilot.temp_action;
        let time_temp = v.auto_pilot.time_temp_action;
        let speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let fwd = self.ent(r).base.matrix.forward;
        let s;
        let mut clear = false;
        match ta {
            temp_action::WAIT => {
                s = Steer { swerve: 0.0, accel: 0.0, brake: 0.2, handbrake: false };
                if now > time_temp {
                    let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                    ap.temp_action = temp_action::NONE;
                    ap.anti_reverse_timer = now;
                    ap.time_temp_action = now;
                }
            }
            temp_action::REVERSE => {
                let mut o = self.steer_mission(px, r);
                o.handbrake = false;
                o.swerve = -o.swerve;
                if speed.dot(fwd) > 0.04 {
                    o.accel = 0.0;
                    o.brake = 0.5;
                } else {
                    o.accel = -0.5;
                    o.brake = 0.0;
                }
                s = o;
                clear = now > time_temp;
            }
            // the original swerves left for a right turn here (fixed in Vice City)
            temp_action::HANDBRAKETURNLEFT => {
                s = Steer { swerve: -1.0, accel: 0.0, brake: 0.0, handbrake: true };
                clear = now > time_temp;
            }
            temp_action::HANDBRAKETURNRIGHT => {
                s = Steer { swerve: 1.0, accel: 0.0, brake: 0.0, handbrake: true };
                clear = now > time_temp;
            }
            temp_action::HANDBRAKESTRAIGHT => {
                s = Steer { swerve: 0.0, accel: 0.0, brake: 0.0, handbrake: true };
                clear = now > time_temp;
            }
            temp_action::TURNLEFT => {
                s = Steer { swerve: 1.0, accel: 1.0, brake: 0.0, handbrake: false };
                clear = now > time_temp;
            }
            temp_action::TURNRIGHT => {
                s = Steer { swerve: -1.0, accel: 1.0, brake: 0.0, handbrake: false };
                clear = now > time_temp;
            }
            temp_action::GOFORWARD => {
                s = Steer { swerve: 0.0, accel: 0.5, brake: 0.0, handbrake: false };
                clear = now > time_temp;
            }
            temp_action::SWERVELEFT | temp_action::SWERVERIGHT => {
                let mut swerve = if ta == temp_action::SWERVERIGHT { 0.15 } else { -0.15 };
                // the second half of the swerve goes back
                if now > time_temp.wrapping_sub(1000) {
                    swerve = -swerve;
                }
                s = Steer { swerve, accel: 0.0, brake: 0.001, handbrake: false };
                clear = now > time_temp;
            }
            _ => s = self.steer_mission(px, r),
        }
        let v = self.try_veh_mut(r).expect("vehicle");
        if clear {
            v.auto_pilot.temp_action = temp_action::NONE;
        }
        v.steer_angle = s.swerve;
        v.flags.is_handbrake_on = s.handbrake;
        v.gas_pedal = s.accel;
        v.brake_pedal = s.brake;
    }

    /// `SteerAICarWithPhysics_OnlyMission`.
    pub fn steer_mission(&mut self, px: &mut Px, r: EntityRef) -> Steer {
        let ap = self.try_veh(r).map(|v| v.auto_pilot).unwrap_or_default();
        let still = Steer { swerve: 0.0, accel: 0.0, brake: 0.5, handbrake: true };
        let player_pos = self.player_info_pos();
        let player_speed = self.find_player_speed();
        match ap.car_mission {
            m::NONE | m::EMERGENCYVEHICLE_STOP | m::STOP_FOREVER => still,
            m::CRUISE | m::RAMPLAYER_FARAWAY | m::BLOCKPLAYER_FARAWAY | m::GOTOCOORDS | m::GOTOCOORDS_ACCURATE | m::RAMCAR_FARAWAY | m::BLOCKCAR_FARAWAY => self.steer_follow_path(px, r),
            m::RAMPLAYER_CLOSE => {
                let mut target = Vec2::new(player_pos.x, player_pos.y);
                let pv = self.find_player_vehicle();
                if let Some(pv) = pv {
                    let seed = self.ent(r).base.random_seed;
                    let (pfwd, pright) = (self.ent(pv).base.matrix.forward, self.ent(pv).base.matrix.right);
                    let my_fwd = self.ent(r).base.matrix.forward;
                    if seed & 1 != 0 && pfwd.dot(my_fwd) > 0.5 {
                        let target_width = self.col_model_of(pv).map_or(0.0, |c| c.bounding_box.max.x);
                        let own_width = self.col_model_of(r).map_or(0.0, |c| c.bounding_box.max.x);
                        let side = Vec2::new(pright.x, pright.y) * (target_width + own_width - 0.2);
                        if seed & 2 != 0 {
                            target += side;
                        } else {
                            target -= side;
                        }
                        let target_speed = player_speed.length();
                        let me = self.epos(r);
                        let dist = (Vec2::new(me.x, me.y) - target).length();
                        if 12.0 * target_speed + 2.0 > dist && ap.temp_action == temp_action::NONE {
                            let now = self.now();
                            let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                            a.temp_action = if seed & 2 != 0 { temp_action::TURNLEFT } else { temp_action::TURNRIGHT };
                            a.time_temp_action = now + 250;
                        }
                    } else {
                        target += Vec2::new(pright.x, pright.y) / 160.0 * ((seed & 0xFF) as f32 - 128.0);
                    }
                }
                self.steer_heading_for_target(px, r, pv, target.x, target.y)
            }
            m::BLOCKPLAYER_CLOSE => self.steer_trying_to_block(px, r, player_pos.x, player_pos.y, player_speed.x, player_speed.y),
            m::BLOCKPLAYER_HANDBRAKESTOP => self.steer_trying_to_block_stop(px, r, player_pos.x, player_pos.y, player_speed.x, player_speed.y),
            m::GOTOCOORDS_STRAIGHT | m::GOTO_COORDS_STRAIGHT_ACCURATE => self.steer_heading_for_target(px, r, None, ap.destination_coors.x, ap.destination_coors.y),
            m::RAMCAR_CLOSE => match ap.target_car {
                Some(t) => {
                    let p = self.epos(t);
                    self.steer_heading_for_target(px, r, Some(t), p.x, p.y)
                }
                None => Steer::default(),
            },
            m::BLOCKCAR_CLOSE | m::BLOCKCAR_HANDBRAKESTOP => {
                let Some(t) = ap.target_car else { return Steer::default() };
                let p = self.epos(t);
                let sp = self.ent(t).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
                if ap.car_mission == m::BLOCKCAR_CLOSE {
                    self.steer_trying_to_block(px, r, p.x, p.y, sp.x, sp.y)
                } else {
                    self.steer_trying_to_block_stop(px, r, p.x, p.y, sp.x, sp.y)
                }
            }
            _ => Steer::default(),
        }
    }

    /// `SteerAICarWithPhysicsFollowPath`.
    pub fn steer_follow_path(&mut self, px: &mut Px, r: EntityRef) -> Steer {
        let paths_links = |w: &World, i: u32| w.ents.paths.car_path_links[i as usize];
        let me = self.epos(r);
        let (matrix_fwd, speed) = (self.ent(r).base.matrix.forward, self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed));
        let forward = norm2(matrix_fwd);
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        let mut cur_link = paths_links(self, ap.current_path_node_info);
        let mut next_link = paths_links(self, ap.next_path_node_info);
        let mut cur_fwd = Vec2::new(cur_link.dir.x * ap.current_direction as f32, cur_link.dir.y * ap.current_direction as f32);
        let mut next_fwd = Vec2::new(next_link.dir.x * ap.next_direction as f32, next_link.dir.y * ap.next_direction as f32);
        let lane_pos = |l: &super::pathfind::CarPathLink, lane: i8, f: Vec2| Vec2::new(l.pos.x + ((lane as f32 + l.one_way_lane_offset()) * LANE_WIDTH) * f.y, l.pos.y - ((lane as f32 + l.one_way_lane_offset()) * LANE_WIDTH) * f.x);
        let on_cur = lane_pos(&cur_link, ap.current_lane, cur_fwd);
        let on_next = lane_pos(&next_link, ap.next_lane, next_fwd);
        let to_next_node = Vec2::new(me.x, me.y) - on_cur;
        let mut scalar_dist = to_next_node.length();
        let between = on_next - on_cur;
        let dp = between.dot(to_next_node);
        if scalar_dist < DISTANCE_TO_NEXT_NODE_TO_SELECT_NEW
            || (dp > 0.0 && scalar_dist < DISTANCE_TO_FACING_NEXT_NODE_TO_SELECT_NEW)
            || dp / (scalar_dist * between.length()) > 0.7
            || ap.next_path_node_info == ap.current_path_node_info
        {
            if self.pick_next_node_according_strategy(px, r) {
                let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                match a.car_mission {
                    m::GOTOCOORDS => {
                        a.car_mission = m::GOTOCOORDS_STRAIGHT;
                        return Steer::default();
                    }
                    m::GOTOCOORDS_ACCURATE => {
                        a.car_mission = m::GOTO_COORDS_STRAIGHT_ACCURATE;
                        return Steer::default();
                    }
                    _ => {}
                }
            }
            let ap2 = self.try_veh(r).expect("vehicle").auto_pilot;
            cur_link = paths_links(self, ap2.current_path_node_info);
            scalar_dist = (lane_pos(&cur_link, ap2.current_lane, cur_fwd) - Vec2::new(me.x, me.y)).length();
            next_link = paths_links(self, ap2.next_path_node_info);
            cur_fwd = Vec2::new(cur_link.dir.x * ap2.current_direction as f32, cur_link.dir.y * ap2.current_direction as f32);
            next_fwd = Vec2::new(next_link.dir.x * ap2.next_direction as f32, next_link.dir.y * ap2.next_direction as f32);
        }
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        let on_cur = lane_pos(&cur_link, ap.current_lane, cur_fwd);
        let mut projected = on_cur - cur_fwd * scalar_dist * 0.4;
        if scalar_dist > DISTANCE_TO_NEXT_NODE_TO_CONSIDER_SLOWING_DOWN {
            projected = on_cur;
        }
        let to_projected = projected - Vec2::new(me.x, me.y);
        let mut angle_current_link = atan_of_xy(to_projected.x, to_projected.y);
        let angle_forward = atan_of_xy(forward.x, forward.y);
        if ap.driving_style == driving_style::AVOID_CARS {
            angle_current_link = self.find_angle_to_weave_through_traffic(r, None, angle_current_link, angle_forward);
        }
        let mut steer_angle = limit_radian_angle(angle_current_link - angle_forward);
        let max_angle = self.find_max_steer_angle(r);
        steer_angle = steer_angle.max(-max_angle).min(max_angle);
        if speed.length() > MIN_SPEED_TO_START_LIMITING_STEER {
            steer_angle = steer_angle.max(-MAX_ANGLE_TO_STEER_AT_HIGH_SPEED).min(MAX_ANGLE_TO_STEER_AT_HIGH_SPEED);
        }
        let current_forward_speed = speed.dot(matrix_fwd) * GAME_SPEED_TO_CARAI_SPEED;
        let mut style_mult = match ap.driving_style {
            driving_style::STOP_FOR_CARS | driving_style::SLOW_DOWN_FOR_CARS => {
                let mut v = self.find_maximum_speed_for_this_car_in_traffic(px, r);
                // FIX_BUGS: a car that is not meant to move does not divide by zero
                if ap.cruise_speed != 0 {
                    v /= ap.cruise_speed as f32;
                }
                v
            }
            _ => 1.0,
        };
        if matches!(ap.driving_style, driving_style::STOP_FOR_CARS | driving_style::SLOW_DOWN_FOR_CARS) && self.should_car_stop_for_light(r, false) {
            self.car_has_reason_to_stop(r);
            style_mult = 0.0;
        }
        if self.should_car_stop_for_bridge(r) {
            self.car_has_reason_to_stop(r);
            style_mult = 0.0;
        }
        let trajectory = on_cur - Vec2::new(me.x, me.y);
        let angle_mult = find_speed_multiplier(atan_of_xy(trajectory.x, trajectory.y) - angle_forward, MIN_ANGLE_FOR_SPEED_LIMITING, MAX_ANGLE_FOR_SPEED_LIMITING, MIN_LOWERING_SPEED_COEFFICIENT);
        let wide_mult = find_speed_multiplier(atan_of_xy(cur_fwd.x, cur_fwd.y) - atan_of_xy(next_fwd.x, next_fwd.y), MIN_ANGLE_FOR_SPEED_LIMITING_BETWEEN_NODES, MAX_ANGLE_FOR_SPEED_LIMITING, MIN_LOWERING_SPEED_COEFFICIENT);
        let nodes_mult = if scalar_dist > DISTANCE_TO_NEXT_NODE_TO_CONSIDER_SLOWING_DOWN || ap.cruise_speed < 12 {
            1.0
        } else {
            1.0 - (1.0 - scalar_dist / DISTANCE_TO_NEXT_NODE_TO_CONSIDER_SLOWING_DOWN) * (1.0 - wide_mult)
        };
        let multiplier = style_mult.min(angle_mult.min(nodes_mult));
        let target_speed = ap.cruise_speed as f32 * multiplier;
        let diff = target_speed - current_forward_speed;
        let (brake, accel) = if target_speed < 0.05 && diff < 0.03 {
            (1.0, 0.0)
        } else if diff <= 0.0 {
            ((-diff * 0.05).min(0.5), 0.0)
        } else if current_forward_speed < 2.0 {
            (0.0, (diff * 0.25).min(1.0))
        } else {
            (0.0, (diff * 0.125).min(1.0))
        };
        Steer { swerve: steer_angle, accel, brake, handbrake: false }
    }

    /// `SteerAICarWithPhysicsHeadingForTarget`.
    pub fn steer_heading_for_target(&mut self, px: &mut Px, r: EntityRef, target: Option<EntityRef>, tx: f32, ty: f32) -> Steer {
        let _ = px;
        let me = self.epos(r);
        let speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let forward = norm2(self.ent(r).base.matrix.forward);
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        let mut angle_to_target = atan_of_xy(tx - me.x, ty - me.y);
        let angle_forward = atan_of_xy(forward.x, forward.y);
        if ap.driving_style == driving_style::AVOID_CARS {
            angle_to_target = self.find_angle_to_weave_through_traffic(r, target, angle_to_target, angle_forward);
        }
        let mut steer = limit_radian_angle(angle_to_target - angle_forward);
        let handbrake = speed.length() > MIN_SPEED_TO_APPLY_HANDBRAKE && steer.abs() > MIN_ANGLE_TO_APPLY_HANDBRAKE;
        let max_angle = self.find_max_steer_angle(r);
        steer = steer.max(-max_angle).min(max_angle);
        let mult = find_speed_multiplier(atan_of_xy(tx - me.x, ty - me.y) - angle_forward, MIN_ANGLE_FOR_SPEED_LIMITING, MAX_ANGLE_FOR_SPEED_LIMITING, MIN_LOWERING_SPEED_COEFFICIENT);
        let target_speed = ap.cruise_speed as f32 * mult;
        let current = speed.length() * GAME_SPEED_TO_CARAI_SPEED;
        let diff = target_speed - current;
        let (accel, brake) = if diff <= 0.0 {
            (0.0, (-diff * 0.05).min(0.5))
        } else if current < 25.0 {
            ((diff * 0.1).min(1.0), 0.0)
        } else {
            (1.0, 0.0)
        };
        Steer { swerve: steer, accel, brake, handbrake }
    }

    /// `SteerAICarWithPhysicsTryingToBlockTarget`.
    pub fn steer_trying_to_block(&mut self, px: &mut Px, r: EntityRef, tx: f32, ty: f32, tsx: f32, tsy: f32) -> Steer {
        let mut target = Vec2::new(tx, ty);
        let mut offset = Vec2::new(tsx, tsy);
        let len = offset.length();
        if len > MAX_SPEED_TO_ACCOUNT_IN_INTERCEPTING {
            offset *= MAX_SPEED_TO_ACCOUNT_IN_INTERCEPTING / len;
        }
        target += offset * GAME_SPEED_TO_CARAI_SPEED;
        self.try_veh_mut(r).expect("vehicle").auto_pilot.driving_style = driving_style::AVOID_CARS;
        let s = self.steer_heading_for_target(px, r, None, target.x, target.y);
        let me = self.epos(r);
        if (target - Vec2::new(me.x, me.y)).length_squared() < DISTANCE_TO_SWITCH_FROM_BLOCK_TO_STOP * DISTANCE_TO_SWITCH_FROM_BLOCK_TO_STOP {
            let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            a.car_mission = if a.car_mission == m::BLOCKCAR_CLOSE { m::BLOCKCAR_HANDBRAKESTOP } else { m::BLOCKPLAYER_HANDBRAKESTOP };
        }
        s
    }

    /// `SteerAICarWithPhysicsTryingToBlockTarget_Stop`.
    pub fn steer_trying_to_block_stop(&mut self, px: &mut Px, r: EntityRef, tx: f32, ty: f32, tsx: f32, tsy: f32) -> Steer {
        let s = Steer { swerve: 0.0, accel: 0.0, brake: 1.0, handbrake: true };
        let me = self.epos(r);
        let dist_sq = (Vec2::new(tx, ty) - Vec2::new(me.x, me.y)).length_squared();
        if dist_sq > DISTANCE_TO_SWITCH_FROM_STOP_TO_BLOCK * DISTANCE_TO_SWITCH_FROM_STOP_TO_BLOCK {
            let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            a.car_mission = if a.car_mission == m::BLOCKCAR_HANDBRAKESTOP { m::BLOCKCAR_CLOSE } else { m::BLOCKPLAYER_CLOSE };
            return s;
        }
        let (mission, law) = {
            let v = self.try_veh(r).expect("vehicle");
            (v.auto_pilot.car_mission, v.flags.is_law_enforcer)
        };
        let my_speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        if mission == m::BLOCKCAR_HANDBRAKESTOP {
            if Vec2::new(my_speed.x, my_speed.y).length_squared() < 0.01 * 0.01 && Vec2::new(tsx, tsy).length_squared() < 0.02 * 0.02 && law {
                self.tell_occupants_to_leave_car(px, r);
                let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                a.cruise_speed = 0;
                a.car_mission = m::NONE;
            }
        } else {
            let step_ms = self.ents.timer.time_step_in_milliseconds() as f32;
            let player_vehicle = self.find_player_vehicle();
            let slow = player_vehicle.is_some_and(|p| self.ent(p).phys.as_ref().is_some_and(|ph| ph.move_speed.length() < 0.05));
            let v = self.try_veh_mut(r).expect("vehicle");
            v.time_blocked = if slow { v.time_blocked.saturating_add(step_ms as u16) } else { 0 };
            let blocked = v.time_blocked as u32;
            let upside_down = player_vehicle.is_some_and(|p| VehicleData::is_upside_down(&self.ent(p).base.matrix));
            if (player_vehicle.is_none() || upside_down || (slow && blocked > TIME_COPS_WAIT_TO_EXIT_AFTER_STOPPING)) && law && dist_sq < DISTANCE_TO_SWITCH_FROM_STOP_TO_BLOCK * DISTANCE_TO_SWITCH_FROM_STOP_TO_BLOCK {
                self.tell_occupants_to_leave_car(px, r);
                let a = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                a.cruise_speed = 0;
                a.car_mission = m::NONE;
            }
        }
        s
    }

    /// `SteerAIBoatWithPhysics`.
    pub fn steer_ai_boat_with_physics(&mut self, r: EntityRef) {
        let ap = self.try_veh(r).map(|v| v.auto_pilot).unwrap_or_default();
        let me = self.epos(r);
        let speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let forward = norm2(self.ent(r).base.matrix.forward);
        let Some(v) = self.try_veh_mut(r) else { return };
        let crate::vehicle::base::VehicleSub::Boat(b) = &mut v.sub else { return };
        if ap.car_mission == m::GOTOCOORDS_ASTHECROWSWIMS {
            let to = Vec2::new(ap.destination_coors.x - me.x, ap.destination_coors.y - me.y);
            let angle_to = atan_of_xy(to.x, to.y);
            let angle_fwd = atan_of_xy(forward.x, forward.y);
            let diff = limit_radian_angle(angle_to - angle_fwd).max(-DEFAULT_MAX_STEER_ANGLE).min(DEFAULT_MAX_STEER_ANGLE);
            let current = Vec2::new(speed.x, speed.y).length();
            let speed_diff = ap.cruise_speed as f32 - current * 60.0;
            b.accelerate = if speed_diff > 0.0 {
                let remaining = speed_diff / ap.cruise_speed as f32;
                if remaining > 0.25 { 1.0 } else { 1.0 - (0.25 - remaining) * 4.0 }
            } else if speed_diff < -5.0 {
                -0.2
            } else {
                -0.1
            };
            b.brake = 0.0;
            b.steering_left_right = diff;
        } else if ap.car_mission == m::NONE {
            b.steering_left_right = 0.0;
            b.accelerate = 0.0;
            b.brake = 0.0;
        }
        let (steer, accel, brake) = (b.steering_left_right, b.accelerate, b.brake);
        v.steer_angle = steer;
        v.gas_pedal = accel;
        v.brake_pedal = brake;
        v.flags.is_handbrake_on = false;
    }
}

impl World {
    /// `SlowCarOnRailsDownForTrafficAndLights`: eases the car's traffic speed towards what the lights and traffic allow.
    pub fn slow_car_on_rails_down_for_traffic_and_lights(&mut self, px: &mut Px, r: EntityRef) {
        let stop = self.should_car_stop_for_light(r, false) || self.should_car_stop_for_bridge(r);
        let max_speed = if stop {
            self.car_has_reason_to_stop(r);
            0.0
        } else {
            self.find_maximum_speed_for_this_car_in_traffic(px, r)
        };
        let cur = self.try_veh(r).expect("vehicle").auto_pilot.max_traffic_speed;
        let (now, step) = (self.now(), self.time_step());
        let to = if max_speed >= cur {
            if max_speed > cur { Some(max_speed.min(cur + 0.05 * step)) } else { None }
        } else if cur != 0.0 {
            Some(if cur < 0.1 { 0.0 } else { max_speed.max(cur - 0.5 * step) })
        } else {
            None
        };
        if let Some(speed) = to {
            let Self { ents, .. } = self;
            if let Some(crate::entity::Kind::Vehicle(v)) = ents.pools.get_mut(r).map(|e| &mut e.kind) {
                v.auto_pilot.modify_speed(speed, now, &ents.paths.car_path_links);
            }
        }
    }

    /// `UpdateCarOnRails`: a simple car follows its curve between two road links without any physics.
    pub fn update_car_on_rails(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let ta = self.try_veh(r).expect("vehicle").auto_pilot.temp_action;
        if ta == temp_action::WAIT {
            if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
                p.move_speed = Vec3::ZERO;
            }
            let Self { ents, .. } = self;
            if let Some(crate::entity::Kind::Vehicle(v)) = ents.pools.get_mut(r).map(|e| &mut e.kind) {
                v.auto_pilot.modify_speed(0.0, now, &ents.paths.car_path_links);
                // the original compares the clock with the action's number, not its time, so the wait ends at once
                if now > v.auto_pilot.temp_action as u32 {
                    v.auto_pilot.temp_action = temp_action::NONE;
                    v.auto_pilot.anti_reverse_timer = now;
                    v.auto_pilot.time_to_start_mission = now;
                }
            }
            return;
        }
        self.slow_car_on_rails_down_for_traffic_and_lights(px, r);
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        if ap.time_entered_curve.wrapping_add(ap.time_to_spend_on_current_curve as u32) <= now {
            self.pick_next_node_according_strategy(px, r);
        }
        if self.ent(r).base.status == crate::entity::status::PHYSICS {
            return;
        }
        let ap = self.try_veh(r).expect("vehicle").auto_pilot;
        let (cur, next) = (self.ents.paths.car_path_links[ap.current_path_node_info as usize], self.ents.paths.car_path_links[ap.next_path_node_info as usize]);
        let cur_fwd = Vec2::new(cur.dir.x * ap.current_direction as f32, cur.dir.y * ap.current_direction as f32);
        let next_fwd = Vec2::new(next.dir.x * ap.next_direction as f32, next.dir.y * ap.next_direction as f32);
        let on_cur = Vec3::new(cur.pos.x + ((ap.current_lane as f32 + cur.one_way_lane_offset()) * LANE_WIDTH) * cur_fwd.y, cur.pos.y - ((ap.current_lane as f32 + cur.one_way_lane_offset()) * LANE_WIDTH) * cur_fwd.x, 0.0);
        let on_next = Vec3::new(next.pos.x + ((ap.next_lane as f32 + next.one_way_lane_offset()) * LANE_WIDTH) * next_fwd.y, next.pos.y - ((ap.next_lane as f32 + next.one_way_lane_offset()) * LANE_WIDTH) * next_fwd.x, 0.0);
        let along = now.wrapping_sub(ap.time_entered_curve) as f32 / ap.time_to_spend_on_current_curve as f32;
        let (mut pos, dir) = super::curves::calc_curve_point(on_cur, on_next, Vec3::new(cur_fwd.x, cur_fwd.y, 0.0), Vec3::new(next_fwd.x, next_fwd.y, 0.0), along, ap.time_to_spend_on_current_curve);
        pos.z = 15.0;
        self.drag_car_to_point(r, pos);
        if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
            p.move_speed = dir / GAME_SPEED_TO_CARAI_SPEED;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{Box3, ColModel, Sphere};
    use crate::entity::physical::Physical;
    use crate::entity::{status, Ent, Entity, EntityType, Kind};
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::vehicle::base::{VehicleType, RANDOM_VEHICLE};

    fn world_with_car(mission: u8) -> (World, EntityRef) {
        let mut models = crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3);
        let mi = models.add_simple_model(10).unwrap();
        let b = Box3 { min: Vec3::new(-1.0, -2.0, -1.0), max: Vec3::new(1.0, 2.0, 1.0), surface: 0, piece: 0 };
        mi.col_model = Some(std::sync::Arc::new(ColModel { bounding_sphere: Sphere::new(2.5, Vec3::ZERO, 0, 0), bounding_box: b, boxes: vec![b], ..Default::default() }));
        let mut w = World::new(models, crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let mut rng = Rng::default();
        let mut v = VehicleData::new(RANDOM_VEHICLE, 0, VehicleType::Car, &mut rng);
        v.auto_pilot.car_mission = mission;
        v.auto_pilot.cruise_speed = 10;
        let mut base = Entity::new(EntityType::Vehicle, &mut rng);
        base.model_index = 10;
        base.flags.uses_collision = true;
        base.status = status::PHYSICS;
        let r = w.insert(Ent::new(base, Some(Physical::default()), Kind::Vehicle(Box::new(v)))).unwrap();
        w.add(r).unwrap();
        (w, r)
    }

    #[test]
    fn the_speed_multiplier_falls_with_the_turn() {
        assert_eq!(find_speed_multiplier(0.0, 0.4, 1.2, 0.4), 1.0);
        assert_eq!(find_speed_multiplier(3.0, 0.4, 1.2, 0.4), 0.4);
        assert!((find_speed_multiplier(0.8, 0.4, 1.2, 0.4) - 0.7).abs() < 1e-5);
        // the sign of the turn does not matter
        assert_eq!(find_speed_multiplier(-0.8, 0.4, 1.2, 0.4), find_speed_multiplier(0.8, 0.4, 1.2, 0.4));
    }

    #[test]
    fn a_car_without_a_mission_holds_still_and_one_that_waits_brakes_gently_then_goes_on() {
        let (mut w, r) = world_with_car(m::NONE);
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.steer_ai_car_with_physics(&mut px, r);
        let v = w.try_veh(r).unwrap();
        assert!(v.flags.is_handbrake_on && v.brake_pedal == 0.5 && v.gas_pedal == 0.0);
        w.try_veh_mut(r).unwrap().auto_pilot.temp_action = temp_action::WAIT;
        w.try_veh_mut(r).unwrap().auto_pilot.time_temp_action = 1000;
        w.ents.timer.time_in_ms = 500;
        w.steer_ai_car_with_physics(&mut px, r);
        let v = w.try_veh(r).unwrap();
        assert!(!v.flags.is_handbrake_on && v.brake_pedal == 0.2 && v.auto_pilot.temp_action == temp_action::WAIT);
        w.ents.timer.time_in_ms = 1500;
        w.steer_ai_car_with_physics(&mut px, r);
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.temp_action, temp_action::NONE);
    }

    #[test]
    fn heading_for_a_target_steers_towards_it_and_speeds_up() {
        let (mut w, r) = world_with_car(m::GOTOCOORDS_STRAIGHT);
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let ahead = w.steer_heading_for_target(&mut px, r, None, 0.0, 100.0);
        assert!(ahead.swerve.abs() < 1e-4 && ahead.accel > 0.0 && ahead.brake == 0.0 && !ahead.handbrake);
        let right = w.steer_heading_for_target(&mut px, r, None, 50.0, 50.0);
        let left = w.steer_heading_for_target(&mut px, r, None, -50.0, 50.0);
        assert!(right.swerve != 0.0 && right.swerve == -left.swerve, "{right:?} {left:?}");
        // a target behind the car needs a hard turn at speed: the handbrake comes on
        w.get_mut(r).unwrap().phys.as_mut().unwrap().move_speed = Vec3::new(0.0, 0.4, 0.0);
        assert!(w.steer_heading_for_target(&mut px, r, None, 0.0, -100.0).handbrake);
    }

    #[test]
    fn blocking_a_car_that_is_close_switches_to_the_stop() {
        let (mut w, r) = world_with_car(m::BLOCKCAR_CLOSE);
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.steer_trying_to_block(&mut px, r, 0.0, 3.0, 0.0, 0.0);
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.car_mission, m::BLOCKCAR_HANDBRAKESTOP);
        let s = w.steer_trying_to_block_stop(&mut px, r, 0.0, 30.0, 0.0, 0.0);
        assert!(s.handbrake && s.brake == 1.0);
        // the target drove off: back to blocking
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.car_mission, m::BLOCKCAR_CLOSE);
    }

    #[test]
    fn dragging_a_car_to_a_point_with_no_ground_keeps_it_level_between_the_ends() {
        let (mut w, r) = world_with_car(m::CRUISE);
        w.drag_car_to_point(r, Vec3::new(0.0, 10.0, 0.0));
        let e = w.ent(r);
        // heading along +y, upright, and half way between the car's back and the point
        assert!(e.base.matrix.forward.y > 0.99 && e.base.matrix.up.z > 0.99);
        assert!(e.base.matrix.pos.y > 0.0 && e.base.matrix.pos.y < 10.0);
    }

    #[test]
    fn a_car_on_rails_is_put_on_the_road_and_given_the_speed_of_its_curve() {
        let (mut w, r) = world_with_car(m::CRUISE);
        w.ents.paths = crate::control::car_nodes::test_road_line();
        w.get_mut(r).unwrap().base.status = status::SIMPLE;
        {
            let ap = &mut w.try_veh_mut(r).unwrap().auto_pilot;
            (ap.current_route_node, ap.next_route_node, ap.current_path_node_info, ap.next_path_node_info) = (0, 1, 0, 1);
            (ap.current_direction, ap.next_direction) = (1, 1);
            ap.max_traffic_speed = 10.0;
            ap.time_to_spend_on_current_curve = 4000;
            ap.time_entered_curve = 1000;
        }
        w.ents.timer.time_in_ms = 3000;
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.update_car_on_rails(&mut px, r);
        let e = w.ent(r);
        assert!(e.phys.as_ref().unwrap().move_speed.y > 0.0, "{:?}", e.phys.as_ref().unwrap().move_speed);
        assert!(e.base.matrix.pos.y.is_finite() && e.base.matrix.forward.y > 0.9);
    }

    #[test]
    fn a_car_waiting_stands_still_and_resumes() {
        let (mut w, r) = world_with_car(m::CRUISE);
        w.ents.paths = crate::control::car_nodes::test_road_line();
        {
            let ap = &mut w.try_veh_mut(r).unwrap().auto_pilot;
            (ap.next_route_node, ap.next_path_node_info, ap.temp_action) = (1, 1, temp_action::WAIT);
        }
        w.get_mut(r).unwrap().phys.as_mut().unwrap().move_speed = Vec3::Y;
        w.ents.timer.time_in_ms = 5000;
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.update_car_on_rails(&mut px, r);
        assert_eq!(w.ent(r).phys.as_ref().unwrap().move_speed, Vec3::ZERO);
        assert_eq!(w.try_veh(r).unwrap().auto_pilot.temp_action, temp_action::NONE);
    }

    #[test]
    fn a_car_put_on_the_road_system_faces_along_one_of_its_links() {
        let (mut w, r) = world_with_car(m::CRUISE);
        w.ents.paths = crate::control::car_nodes::test_road_line();
        w.ents.paths.num_path_nodes = 3;
        w.ents.paths.num_car_path_nodes = 3;
        w.get_mut(r).unwrap().base.set_position(Vec3::new(0.0, 60.0, 0.0));
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.join_car_with_road_system(&mut px, r);
        let ap = w.try_veh(r).unwrap().auto_pilot;
        // heading along +y the car goes from a lower node to a higher one
        assert!(ap.current_route_node < ap.next_route_node, "{ap:?}");
        assert_eq!((ap.current_lane, ap.next_lane), (0, 0));
        assert_eq!(ap.path_find_nodes_count, 0);
    }
}
