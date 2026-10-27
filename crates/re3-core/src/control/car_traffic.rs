//! `CarCtrl.cpp`, part 2: how a car slows down for the cars and peds in front of it.
use super::car_ctrl::{GAME_SPEED_TO_CARAI_SPEED, GAME_SPEED_TO_METERS_PER_SECOND};
use crate::entity::{status, EntityRef};
use crate::general::radian_angle_between_points;
use crate::ped::ctx::Px;
use crate::ped::data::{move_state, state, RANDOM_CHAR};
use crate::vehicle::base::{driving_style, temp_action};
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, World};
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub const DISTANCE_TO_SCAN_FOR_DANGER: f32 = 11.0;
pub const SAFE_DISTANCE_TO_PED: f32 = 3.0;

/// `CCarCtrl::LimitRadianAngle`.
pub fn limit_radian_angle(mut a: f32) -> f32 {
    while a < -PI {
        a += TAU;
    }
    while a > PI {
        a -= TAU;
    }
    a
}

/// The square around a car that is scanned for danger.
#[derive(Clone, Copy)]
struct Scan {
    min: Vec2,
    max: Vec2,
}

impl World {
    fn danger_scan_box(&self, r: EntityRef) -> Scan {
        let p = self.epos(r);
        let d = DISTANCE_TO_SCAN_FOR_DANGER;
        Scan { min: Vec2::new(p.x - d, p.y - d), max: Vec2::new(p.x + d, p.y + d) }
    }

    /// `FindMaximumSpeedForThisCarInTraffic`: the speed the traffic around lets the car drive.
    pub fn find_maximum_speed_for_this_car_in_traffic(&mut self, px: &mut Px, r: EntityRef) -> f32 {
        let ap = self.try_veh(r).map(|v| v.auto_pilot).unwrap_or_default();
        if matches!(ap.driving_style, driving_style::AVOID_CARS | driving_style::PLOUGH_THROUGH) {
            return ap.cruise_speed as f32;
        }
        let s = self.danger_scan_box(r);
        let cruise = ap.cruise_speed as f32;
        let mut max_speed = cruise;
        let near = self.entities_in_sector_box(s.min.x, s.min.y, s.max.x, s.max.y, Kinds { vehicles: true, peds: true, ..Kinds::NONE });
        for e in near {
            if self.is_ped(e) {
                self.slow_car_down_for_ped(px, r, e, s, &mut max_speed, cruise);
            } else {
                self.slow_car_down_for_car(px, r, e, s, &mut max_speed, cruise);
            }
        }
        self.try_veh_mut(r).expect("vehicle").flags.warned_peds = true;
        if ap.driving_style == driving_style::STOP_FOR_CARS {
            return max_speed;
        }
        (max_speed + cruise) / 2.0
    }

    /// `ScanForPedDanger`: peds in the way get their warning, the slowed-down flag stays as it was.
    pub fn scan_for_ped_danger(&mut self, px: &mut Px, r: EntityRef) {
        let stored = self.try_veh(r).is_some_and(|v| v.auto_pilot.slowed_down_because_of_peds);
        let s = self.danger_scan_box(r);
        let cruise = self.try_veh(r).map_or(0.0, |v| v.auto_pilot.cruise_speed as f32);
        let mut max_speed = cruise;
        for e in self.entities_in_sector_box(s.min.x, s.min.y, s.max.x, s.max.y, Kinds { peds: true, ..Kinds::NONE }) {
            self.slow_car_down_for_ped(px, r, e, s, &mut max_speed, cruise);
        }
        let v = self.try_veh_mut(r).expect("vehicle");
        v.flags.warned_peds = true;
        v.auto_pilot.slowed_down_because_of_peds = stored;
    }

    /// `SlowCarDownForPedsSectorList` for one ped.
    fn slow_car_down_for_ped(&mut self, px: &mut Px, r: EntityRef, ped: EntityRef, s: Scan, speed: &mut f32, cur_speed: f32) {
        let Some(col) = self.col_model_of(r) else { return };
        let front_offset = col.bounding_box.max.y;
        let front_safe = front_offset + SAFE_DISTANCE_TO_PED;
        let Some(pe) = self.get(ped) else { return };
        if !pe.base.flags.uses_collision {
            return;
        }
        let ped_pos = pe.base.position();
        if ped_pos.x < s.min.x || ped_pos.x > s.max.x || ped_pos.y < s.min.y || ped_pos.y > s.max.y {
            return;
        }
        let (me, fwd, right, move_speed) = {
            let e = self.ent(r);
            (e.base.position(), e.base.matrix.forward, e.base.matrix.right, e.phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed))
        };
        if (ped_pos.z - me.z).abs() >= 4.0 {
            return;
        }
        let to_ped = ped_pos - me;
        let dot_direction = fwd.dot(to_ped);
        let dot_velocity = fwd.dot(move_speed);
        // already run over: nothing to do
        if dot_direction <= front_offset {
            return;
        }
        let distance_until_hit = dot_direction - front_offset;
        let towards_per_second = GAME_SPEED_TO_METERS_PER_SECOND * dot_velocity;
        if 4.0 * towards_per_second <= distance_until_hit {
            return;
        }
        let sideways = right.dot(to_ped).abs();
        let mut side_length = col.bounding_box.max.x;
        let (is_bike, model, veh_status) = (self.try_veh(r).is_some_and(|v| v.is_bike()), self.ent(r).base.model_index, self.ent(r).base.status);
        if is_bike {
            side_length *= 1.6;
        }
        if side_length + 0.5 < sideways {
            return;
        }
        let ped_state = self.ped(ped).ped_state;
        if ped_state != state::STEP_AWAY && ped_state != state::DIVE_AWAY {
            if distance_until_hit < towards_per_second {
                // very close: time to get out of the way
                if model == self.ents.indices.rc_bandit() {
                    if dot_velocity * GAME_SPEED_TO_METERS_PER_SECOND / 2.0 > distance_until_hit {
                        self.set_evasive_step(px, ped, r, 0);
                    }
                } else if dot_velocity > 0.3 {
                    if side_length + 0.1 < sideways {
                        self.set_evasive_step(px, ped, r, 0);
                    } else {
                        self.set_evasive_dive(px, ped, r, false);
                    }
                } else if dot_velocity > 0.1 {
                    if side_length - 0.5 < sideways {
                        self.set_evasive_step(px, ped, r, 0);
                    } else {
                        self.set_evasive_dive(px, ped, r, false);
                    }
                }
            } else {
                // far enough, but the ped should still notice
                let pd = self.ped(ped);
                if veh_status == status::PLAYER && pd.ped_state != state::FLEE_ENTITY && pd.char_created_by == RANDOM_CHAR {
                    let angle = limit_radian_angle(radian_angle_between_points(me.x, me.y, ped_pos.x, ped_pos.y));
                    let rate = limit_radian_angle(pd.heading_rate);
                    self.ped_mut(ped).heading_rate = rate;
                    let mut visibility = (angle - rate).abs();
                    if visibility > PI {
                        visibility = TAU - visibility;
                    }
                    let horn = self.try_veh(r).is_some_and(|v| v.car_horn_timer != 0);
                    if visibility < FRAC_PI_2 || horn {
                        self.set_flee(px, ped, r, 2000);
                        let pd = self.ped_mut(ped);
                        pd.flags.use_ped_node_seek = false;
                        pd.set_move_state(move_state::RUN);
                    }
                } else {
                    let now = self.now();
                    let pd = self.ped(ped);
                    if pd.is_player() && dot_direction < front_safe && pd.is_ped_in_control() && pd.player.as_ref().is_some_and(|p| p.move_speed < 1.0) && !pd.flags.is_looking && now > pd.look_timer {
                        self.annoy_player_ped(ped, false);
                        self.set_look_flag_entity(ped, r, true);
                        self.set_look_timer(ped, 1500);
                        let w = self.ped(ped).current_weapon as usize;
                        if matches!(w, wt::UNARMED | wt::BASEBALLBAT | wt::COLT45 | wt::UZI) {
                            self.ped_mut(ped).flags.shake_fist = true;
                        }
                    }
                }
            }
        }
        // the car part of it
        if distance_until_hit < 10.0 {
            let style = self.try_veh(r).map_or(0, |v| v.auto_pilot.driving_style);
            if matches!(style, driving_style::STOP_FOR_CARS | driving_style::SLOW_DOWN_FOR_CARS) {
                *speed = speed.min((distance_until_hit - 1.0).abs() * 0.1 * cur_speed);
                let now = self.now();
                let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                ap.slowed_down_because_of_peds = true;
                if distance_until_hit < 2.0 {
                    ap.temp_action = temp_action::WAIT;
                    ap.time_temp_action = now + 3000;
                }
            }
        }
    }

    /// `SlowCarDownForCarsSectorList` for one vehicle.
    fn slow_car_down_for_car(&mut self, px: &mut Px, r: EntityRef, other: EntityRef, s: Scan, speed: &mut f32, cur_speed: f32) {
        if other == r || !self.get(other).is_some_and(|e| e.base.flags.uses_collision) {
            return;
        }
        let c = self.bound_centre(other);
        if c.x < s.min.x || c.x > s.max.x || c.y < s.min.y || c.y > s.max.y {
            return;
        }
        if (c.z - self.epos(r).z).abs() < 5.0 {
            self.slow_car_down_for_other_car(px, other, r, speed, cur_speed);
        }
    }

    /// `SlowCarDownForOtherCar`: the car in front sets how fast this one may go.
    pub fn slow_car_down_for_other_car(&mut self, px: &mut Px, other: EntityRef, r: EntityRef, speed: &mut f32, cur_speed: f32) {
        let _ = px;
        let (me, my_fwd) = (self.epos(r), self.ent(r).base.matrix.forward);
        let mut fwd_a = my_fwd;
        let n = Vec2::new(fwd_a.x, fwd_a.y).normalize_or_zero();
        (fwd_a.x, fwd_a.y) = (n.x, n.y);
        let other_pos = self.epos(other);
        if Vec2::new(other_pos.x - me.x, other_pos.y - me.y).dot(Vec2::new(fwd_a.x, fwd_a.y)) < 0.0 {
            return;
        }
        let other_fwd = self.ent(other).base.matrix.forward;
        let nb = Vec2::new(other_fwd.x, other_fwd.y).normalize_or_zero();
        let fwd_b = Vec3::new(nb.x, nb.y, 0.0);
        fwd_a.z = 0.0;
        let other_speed = self.ent(other).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let (sx, sy) = (GAME_SPEED_TO_CARAI_SPEED * other_speed.x, GAME_SPEED_TO_CARAI_SPEED * other_speed.y);
        let (proj_x, proj_y) = (sx - fwd_a.x * cur_speed, sy - fwd_a.y * cur_speed);
        let a = self.test_collision_between_2_moving_rects(other, r, proj_x, proj_y, fwd_a, fwd_b, 0);
        let b = self.test_collision_between_2_moving_rects(r, other, -proj_x, -proj_y, fwd_b, fwd_a, 1);
        let mut min_prox = a.min(b);
        if (0.0..1.0).contains(&min_prox) {
            min_prox = ((min_prox - 0.2) * 1.25).max(0.0);
            self.try_veh_mut(r).expect("vehicle").auto_pilot.slowed_down_because_of_cars = true;
            *speed = speed.min(min_prox * cur_speed);
        }
        let now = self.now();
        let (Some(mine), Some(theirs)) = (self.try_veh(r), self.try_veh(other)) else { return };
        if (0.0..0.5).contains(&min_prox) && now.wrapping_sub(mine.auto_pilot.time_to_start_mission) > 15000 && now.wrapping_sub(theirs.auto_pilot.time_to_start_mission) > 15000 {
            // cars that stand for 15 seconds face to face: one of them starts to avoid
            let ofwd = self.ent(other).base.matrix.forward;
            if Some(other) != self.find_player_vehicle() && Vec2::new(my_fwd.x, my_fwd.y).dot(Vec2::new(ofwd.x, ofwd.y)) < -0.5 && r.handle < other.handle {
                *speed = (cur_speed / 5.0).max(*speed);
                if self.ent(r).base.status == status::SIMPLE {
                    self.get_mut(r).expect("vehicle").base.status = status::PHYSICS;
                    self.switch_vehicle_to_real_physics(r);
                }
                let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
                ap.driving_style = driving_style::AVOID_CARS;
                ap.time_temp_action = now + 1000;
            }
        }
    }

    /// `TestCollisionBetween2MovingRects`: how far along the relative movement (0..1) car A's corners first touch car B's rectangle; 1 for never.
    #[allow(clippy::too_many_arguments)] // the original's signature
    pub fn test_collision_between_2_moving_rects(&self, a: EntityRef, b: EntityRef, proj_x: f32, proj_y: f32, fwd_a: Vec3, fwd_b: Vec3, _id: u8) -> f32 {
        let (Some(ca), Some(cb)) = (self.col_model_of(a), self.col_model_of(b)) else { return 1.0 };
        let (pa, pb) = (self.epos(a), self.epos(b));
        let b_to_a = Vec2::new(pa.x - pb.x, pa.y - pb.y);
        let (len_b, width_b, back_len_b) = (cb.bounding_box.max.y, cb.bounding_box.max.x, -cb.bounding_box.min.y);
        let (len_a, width_a, back_len_a) = (ca.bounding_box.max.y, ca.bounding_box.max.x, -ca.bounding_box.min.y);
        let mut proximity = 1.0f32;
        let full_width_b = 2.0 * width_b;
        let full_len_b = len_b + back_len_b;
        for i in 0..4 {
            let (tx, ty) = match i {
                0 => (b_to_a.x + width_a * fwd_b.y + len_a * fwd_b.x, b_to_a.y + len_a * fwd_b.y - width_a * fwd_b.x),
                1 => (b_to_a.x + -width_a * fwd_b.x + len_a * fwd_b.x, b_to_a.y + len_a * fwd_b.y + width_a * fwd_b.x),
                2 => (b_to_a.x + width_a * fwd_b.y - back_len_a * fwd_b.x, b_to_a.y - back_len_a * fwd_b.y - width_a * fwd_b.x),
                _ => (b_to_a.x - width_a * fwd_b.y - back_len_a * fwd_b.x, b_to_a.y - back_len_a * fwd_b.y + width_a * fwd_b.x),
            };
            // width
            let (mut base_w, mut full_w) = (0.0f32, 1.0f32);
            let w_dist = tx * fwd_a.y - ty * fwd_a.x;
            let w_proj = proj_x * fwd_a.y - proj_y * fwd_a.x;
            if w_dist > width_b {
                if w_proj < 0.0 {
                    let p = -(w_dist - width_b) / w_proj;
                    if p < 1.0 {
                        base_w = p;
                        full_w = (p - full_width_b / w_proj).min(1.0);
                    } else {
                        base_w = 1.0;
                    }
                } else {
                    base_w = 1.0;
                    full_w = 1.0;
                }
            } else if w_dist < -width_b {
                if w_proj > 0.0 {
                    let p = -(w_dist + width_b) / w_proj;
                    if p < 1.0 {
                        base_w = p;
                        full_w = (p + full_width_b / w_proj).min(1.0);
                    } else {
                        base_w = 1.0;
                    }
                } else {
                    base_w = 1.0;
                    full_w = 1.0;
                }
            } else if w_proj > 0.0 {
                full_w = (width_b - w_dist) / w_proj;
            } else if w_proj < 0.0 {
                full_w = -(width_b + w_dist) / w_proj;
            }
            // length
            let (mut base_l, mut full_l) = (0.0f32, 1.0f32);
            let l_dist = tx * fwd_a.x + ty * fwd_a.y;
            let l_proj = proj_x * fwd_a.x + proj_y * fwd_a.y;
            if l_dist > len_b {
                if l_proj < 0.0 {
                    let p = -(l_dist - len_b) / l_proj;
                    if p < 1.0 {
                        base_l = p;
                        full_l = (p - full_len_b / l_proj).min(1.0);
                    } else {
                        base_l = 1.0;
                    }
                } else {
                    base_l = 1.0;
                    full_l = 1.0;
                }
            } else if l_dist < -back_len_b {
                if l_proj > 0.0 {
                    let p = -(l_dist + back_len_b) / l_proj;
                    if p < 1.0 {
                        base_l = p;
                        full_l = (p + full_len_b / l_proj).min(1.0);
                    } else {
                        base_l = 1.0;
                    }
                } else {
                    base_l = 1.0;
                    full_l = 1.0;
                }
            } else if l_proj > 0.0 {
                full_l = (len_b - l_dist) / l_proj;
            } else if l_proj < 0.0 {
                full_l = -(back_len_b + l_dist) / l_proj;
            }
            let base = base_w.max(base_l);
            if base < full_w && base < full_l {
                proximity = proximity.min(base);
            }
        }
        proximity
    }
}

const VEHICLE_HEIGHT_DIFF_TO_CONSIDER_WEAVING: f32 = 4.0;
const PED_HEIGHT_DIFF_TO_CONSIDER_WEAVING: f32 = 4.0;
const OBJECT_HEIGHT_DIFF_TO_CONSIDER_WEAVING: f32 = 8.0;
const WIDTH_COEF_TO_WEAVE_SAFELY: f32 = 1.2;
const OBJECT_WIDTH_TO_WEAVE: f32 = 0.3;
const PED_WIDTH_TO_WEAVE: f32 = 0.8;

/// The two bounds of the angles a car must avoid: the left edge, then the right edge.
struct Weave {
    left: f32,
    right: f32,
}

impl Weave {
    /// The common tail of `WeaveForOtherCar`, `WeaveForPed` and `WeaveForObject`: widen the bounds around an obstacle.
    fn around(&mut self, angle_to_obstacle: f32, length_to_evade: f32) {
        let angle_to_weave = length_to_evade / 2.0;
        if limit_radian_angle(angle_to_obstacle - self.left).abs() < angle_to_weave {
            self.left = angle_to_obstacle - angle_to_weave;
            while self.left < -PI {
                self.left += TAU;
            }
        }
        if limit_radian_angle(angle_to_obstacle - self.right).abs() < angle_to_weave {
            self.right = angle_to_obstacle + angle_to_weave;
            while self.right > PI {
                self.right -= TAU;
            }
        }
    }
}

impl World {
    /// `FindAngleToWeaveThroughTraffic`: the heading that gets the car past what is in the way of `angle_to_target`.
    pub fn find_angle_to_weave_through_traffic(&mut self, r: EntityRef, target: Option<EntityRef>, angle_to_target: f32, angle_forward: f32) -> f32 {
        let speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let distance = (2.0f32).min(Vec2::new(speed.x, speed.y).length() * 2.5 + 1.0) * 12.0;
        let me = self.epos(r);
        let (min, max) = (Vec2::new(me.x - distance, me.y - distance), Vec2::new(me.x + distance, me.y + distance));
        let mut w = Weave { left: angle_to_target, right: angle_to_target };
        // the original repeats the scan until nothing changes, but every entity is only looked at once
        let near = self.entities_in_sector_box(min.x, min.y, max.x, max.y, Kinds { vehicles: true, peds: true, objects: true, ..Kinds::NONE });
        for e in near {
            let Some(en) = self.get(e) else { continue };
            if !en.base.flags.uses_collision {
                continue;
            }
            match en.base.ty {
                crate::entity::EntityType::Vehicle => {
                    if Some(e) == target {
                        continue;
                    }
                    let c = self.bound_centre(e);
                    if c.x < min.x || c.x > max.x || c.y < min.y || c.y > max.y || (self.epos(e).z - me.z).abs() >= VEHICLE_HEIGHT_DIFF_TO_CONSIDER_WEAVING {
                        continue;
                    }
                    if e != r {
                        self.weave_for_other_car(e, r, &mut w);
                    }
                }
                crate::entity::EntityType::Ped => {
                    if Some(e) == target {
                        continue;
                    }
                    let p = self.epos(e);
                    if p.x < min.x || p.x > max.x || p.y < min.y || p.y > max.y || (p.z - me.z).abs() >= PED_HEIGHT_DIFF_TO_CONSIDER_WEAVING {
                        continue;
                    }
                    if self.ped(e).current_phys_surface != Some(r) {
                        self.weave_for_ped(e, r, &mut w);
                    }
                }
                crate::entity::EntityType::Object => {
                    let p = self.epos(e);
                    if p.x < min.x || p.x > max.x || p.y < min.y || p.y > max.y || (p.z - me.z).abs() >= OBJECT_HEIGHT_DIFF_TO_CONSIDER_WEAVING {
                        continue;
                    }
                    if self.ent(e).base.matrix.up.z > 0.9 {
                        self.weave_for_object(e, r, &mut w);
                    }
                }
                _ => {}
            }
        }
        let diff_actual_to_target = limit_radian_angle(angle_forward - angle_to_target);
        let bisect = limit_radian_angle(angle_to_target + diff_actual_to_target / 2.0);
        let diff_left = limit_radian_angle(w.left - bisect).abs();
        let diff_right = limit_radian_angle(w.right - bisect).abs();
        if diff_left > FRAC_PI_2 && diff_right > FRAC_PI_2 {
            return bisect;
        }
        if (diff_left - diff_right).abs() < 0.08 {
            return w.right;
        }
        if diff_left < diff_right { w.left } else { w.right }
    }

    fn weave_for_other_car(&mut self, other: EntityRef, r: EntityRef, w: &mut Weave) {
        let ap = self.try_veh(r).map(|v| v.auto_pilot).unwrap_or_default();
        use crate::vehicle::base::car_mission as m;
        if (ap.car_mission == m::RAMPLAYER_CLOSE && Some(other) == self.find_player_vehicle()) || (ap.car_mission == m::RAMCAR_CLOSE && Some(other) == ap.target_car) {
            return;
        }
        let (me, other_pos) = (self.epos(r), self.epos(other));
        let diff = Vec2::new(other_pos.x - me.x, other_pos.y - me.y);
        let angle_between = crate::math::atan_of_xy(diff.x, diff.y);
        let distance = diff.length();
        if distance < 1.0 {
            return;
        }
        let (my_speed, other_speed) = (self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed), self.ent(other).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed));
        let (Some(my_col), Some(other_col)) = (self.col_model_of(r), self.col_model_of(other)) else { return };
        let rel = my_speed - other_speed;
        if Vec2::new(rel.x, rel.y).dot(diff) * 110.0 - other_col.bounding_sphere.radius - my_col.bounding_sphere.radius < distance {
            return;
        }
        let fwd = self.ent(r).base.matrix.forward;
        let f = Vec2::new(fwd.x, fwd.y).normalize_or_zero();
        let forward_angle = crate::math::atan_of_xy(f.x, f.y);
        let angle_diff = angle_between - forward_angle;
        let len_projection = (other_col.bounding_box.max.y * angle_diff.sin()).abs();
        let width_projection = (other_col.bounding_box.max.x * angle_diff.cos()).abs();
        let length_to_evade = (2.0 * (len_projection + width_projection) + WIDTH_COEF_TO_WEAVE_SAFELY * 2.0 * my_col.bounding_box.max.x) / distance;
        w.around(angle_between, length_to_evade);
    }

    fn weave_for_ped(&mut self, ped: EntityRef, r: EntityRef, w: &mut Weave) {
        use crate::vehicle::base::car_mission as m;
        let mission = self.try_veh(r).map_or(0, |v| v.auto_pilot.car_mission);
        if mission == m::RAMPLAYER_CLOSE && Some(ped) == self.ents.player_ped {
            return;
        }
        let (me, p) = (self.epos(r), self.epos(ped));
        let diff = Vec2::new(p.x - me.x, p.y - me.y);
        let angle = crate::math::atan_of_xy(diff.x, diff.y);
        let width = self.col_model_of(r).map_or(0.0, |c| c.bounding_box.max.x);
        w.around(angle, (WIDTH_COEF_TO_WEAVE_SAFELY * 2.0 * width + PED_WIDTH_TO_WEAVE) / diff.length());
    }

    fn weave_for_object(&mut self, obj: EntityRef, r: EntityRef, w: &mut Weave) {
        use crate::modelinfo::indices_gta3::slot as s;
        let model = self.ent(obj).base.model_index;
        let crate::modelinfo::Indices::Gta3(i) = &self.ents.indices else { return };
        let (right_coef, forward_coef) = if model == i.get(s::TRAFFICLIGHTS) {
            (2.957, 0.147)
        } else if model == i.get(s::SINGLESTREETLIGHTS1) {
            (0.744, 0.0)
        } else if model == i.get(s::SINGLESTREETLIGHTS2) {
            (0.043, 0.0)
        } else if model == i.get(s::SINGLESTREETLIGHTS3) {
            (1.143, 0.145)
        } else if model == i.get(s::DOUBLESTREETLIGHTS) {
            (0.0, -0.048)
        } else if i.is_tree_model(model) || model == i.get(s::STREETLAMP1) || model == i.get(s::STREETLAMP2) {
            (0.0, 0.0)
        } else {
            return;
        };
        let m = self.ent(obj).base.matrix;
        let me = self.epos(r);
        let p = m.pos + right_coef * m.right + forward_coef * m.forward;
        let diff = Vec2::new(p.x - me.x, p.y - me.y);
        let angle = crate::math::atan_of_xy(diff.x, diff.y);
        let width = self.col_model_of(r).map_or(0.0, |c| c.bounding_box.max.x);
        w.around(angle, (WIDTH_COEF_TO_WEAVE_SAFELY * 2.0 * width + OBJECT_WIDTH_TO_WEAVE) / diff.length());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{Box3, ColModel, Sphere};
    use crate::entity::physical::Physical;
    use crate::entity::{Ent, Entity, EntityType, Kind};
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::vehicle::base::{VehicleData, VehicleType, RANDOM_VEHICLE};

    fn box_world() -> World {
        let mut models = crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3);
        let mi = models.add_simple_model(10).unwrap();
        let b = Box3 { min: Vec3::new(-1.0, -2.0, -1.0), max: Vec3::new(1.0, 2.0, 1.0), surface: 0, piece: 0 };
        mi.col_model = Some(std::sync::Arc::new(ColModel { bounding_sphere: Sphere::new(2.5, Vec3::ZERO, 0, 0), bounding_box: b, boxes: vec![b], ..Default::default() }));
        World::new(models, crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3))
    }

    fn car(w: &mut World, pos: Vec3, cruise: u8) -> EntityRef {
        let mut rng = Rng::default();
        let mut v = VehicleData::new(RANDOM_VEHICLE, 0, VehicleType::Car, &mut rng);
        v.auto_pilot.cruise_speed = cruise;
        let mut base = Entity::new(EntityType::Vehicle, &mut rng);
        base.model_index = 10;
        base.flags.uses_collision = true;
        base.set_position(pos);
        base.status = status::SIMPLE;
        let r = w.insert(Ent::new(base, Some(Physical::default()), Kind::Vehicle(Box::new(v)))).unwrap();
        w.add(r).unwrap();
        r
    }

    #[test]
    fn rects_meet_when_the_cars_close_in_and_never_when_they_part() {
        let mut w = box_world();
        let b = car(&mut w, Vec3::ZERO, 10);
        let a = car(&mut w, Vec3::new(0.0, 10.0, 0.0), 10);
        let closing = w.test_collision_between_2_moving_rects(a, b, 0.0, -10.0, Vec3::Y, Vec3::Y, 0);
        assert!(closing > 0.0 && closing < 1.0, "{closing}");
        let parting = w.test_collision_between_2_moving_rects(a, b, 0.0, 10.0, Vec3::Y, Vec3::Y, 0);
        assert_eq!(parting, 1.0);
    }

    #[test]
    fn an_empty_road_lets_the_car_cruise_and_a_car_ahead_slows_it() {
        let mut w = box_world();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let me = car(&mut w, Vec3::ZERO, 10);
        assert_eq!(w.find_maximum_speed_for_this_car_in_traffic(&mut px, me), 10.0);
        assert!(w.try_veh(me).unwrap().flags.warned_peds);
        car(&mut w, Vec3::new(0.0, 6.0, 0.0), 10);
        let slowed = w.find_maximum_speed_for_this_car_in_traffic(&mut px, me);
        assert!(slowed < 10.0, "{slowed}");
        assert!(w.try_veh(me).unwrap().auto_pilot.slowed_down_because_of_cars);
        // a driver who avoids cars does not care
        w.try_veh_mut(me).unwrap().auto_pilot.driving_style = driving_style::AVOID_CARS;
        assert_eq!(w.find_maximum_speed_for_this_car_in_traffic(&mut px, me), 10.0);
    }

    #[test]
    fn weaving_steers_round_a_ped_in_the_way_and_leaves_a_clear_road_alone() {
        use crate::ped::types::ped_type;
        let mut w = box_world();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let me = car(&mut w, Vec3::ZERO, 10);
        let straight = crate::math::atan_of_xy(0.0, 1.0);
        assert_eq!(w.find_angle_to_weave_through_traffic(me, None, straight, straight), straight);
        w.add_ped(&mut px, ped_type::CIVMALE, 10, Vec3::new(0.0, 8.0, 0.0)).unwrap();
        let woven = w.find_angle_to_weave_through_traffic(me, None, straight, straight);
        assert!((woven - straight).abs() > 0.05, "{woven} vs {straight}");
        // the ped being the target itself is no obstacle
        let ped = w.ents.pools.peds.iter().next().map(|(i, _)| EntityRef { pool: crate::entity::PoolKind::Ped, handle: w.ents.pools.peds.handle_of(i) });
        assert_eq!(w.find_angle_to_weave_through_traffic(me, ped, straight, straight), straight);
    }
}
