//! The collision response in `CPed::ProcessControl`: what a ped does after it bumped into a building,
//! a vehicle or another ped last frame.
use super::ctx::Px;
use super::data::*;
use super::pending::Hit;
use super::types::ped_type;
use crate::anim::tables_gta3::anim;
use crate::audio::sound;
use crate::control::pathfind::PATH_PED;
use crate::entity::{status, EntityRef, EntityType, Kind};
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::math::heading;
use crate::modelinfo::indices_gta3::mi;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// `COLLRESPONSE_FENCEPART`.
const COLLRESPONSE_FENCEPART: u8 = 4;

/// A bare fist, bat, pistol or Uzi: the weapons a shaken fist goes with.
fn weak_weapon(t: usize) -> bool {
    matches!(t, wt::UNARMED | wt::BASEBALLBAT | wt::COLT45 | wt::UZI)
}

impl World {
    fn current_weapon_type(&self, r: EntityRef) -> usize {
        let pd = self.ped(r);
        pd.weapons[pd.current_weapon as usize].ty
    }

    /// The `switch (collidingEnt->GetType())` of `CPed::ProcessControl`.
    pub(super) fn respond_to_collision(&mut self, px: &mut Px, r: EntityRef, hit: EntityRef) {
        match self.ent(hit).base.ty {
            EntityType::Building | EntityType::Object => self.hit_building_or_object(px, r, hit),
            EntityType::Vehicle => self.hit_vehicle(px, r, hit),
            EntityType::Ped => self.hit_ped(px, r, hit),
            _ => {}
        }
    }

    fn hit_ped(&mut self, px: &mut Px, r: EntityRef, other: EntityRef) {
        self.collide_with_ped(px, r, other);
        if !self.ped(other).is_player() {
            return;
        }
        self.say(px, r, sound::PED_CHAT);
        let moving = self.ped(r).move_state > move_state::STILL;
        let (in_control, speed, looking, look_timer) = {
            let p = self.ped(other);
            (p.is_ped_in_control(), p.player.as_ref().map_or(0.0, |d| d.move_speed), p.flags.is_looking, p.look_timer)
        };
        if moving && in_control && speed < 1.0 && !looking && self.now() > look_timer {
            self.annoy_player_ped(other, false);
            self.set_look_flag_entity(other, r, true);
            self.set_look_timer(other, 1300);
            if weak_weapon(self.current_weapon_type(other)) {
                self.ped_mut(other).flags.shake_fist = true;
            }
        }
    }

    fn hit_building_or_object(&mut self, px: &mut Px, r: EntityRef, hit: EntityRef) {
        let special = match &self.ent(hit).kind {
            Kind::Object(o) => Some(o.special_collision_response),
            _ => None,
        };
        let small = self.col_model_of(hit).is_some_and(|c| c.bounding_box.max.x < 3.0 && c.bounding_box.max.y < 3.0);
        let is_player = self.ped(r).is_player();
        if (special.is_some_and(|s| s != COLLRESPONSE_FENCEPART) || small) && !is_player {
            self.set_direction_to_walk_around_object(px, r, hit);
            return;
        }
        if is_player {
            self.ped_mut(r).flags.hit_something_last_frame = true;
            return;
        }
        let now = self.now();
        let pos = self.epos(r);
        let normal = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.damage_normal);
        let angle_when_hit = radian_angle_between_points(pos.x, pos.y, normal.x + pos.x, normal.y + pos.y);
        let mut needed_turn = (self.ped(r).rotation_cur - angle_when_hit).abs();
        if needed_turn > PI {
            needed_turn = TAU - needed_turn;
        }
        let old_dest_rot = limit_radian_angle(self.ped(r).rotation_dest);
        let objective = self.ped(r).objective;
        let target = self.ped(r).ped_in_objective;
        if let Some(target) = target.filter(|_| matches!(objective, objective::GOTO_CHAR_ON_FOOT | objective::KILL_CHAR_ON_FOOT)) {
            let target_is_player = self.try_ped(target).is_some_and(|d| d.is_player());
            if target_is_player && (needed_turn < 20.0_f32.to_radians() || self.ped(r).panic_counter > 10) {
                if self.can_ped_jump_this(r) {
                    self.set_jump(r);
                } else if objective == objective::KILL_CHAR_ON_FOOT {
                    self.set_wait_state(px, r, wait_state::LOOK_ABOUT, None);
                } else {
                    self.set_wait_state(px, r, wait_state::PLAYANIM_TAXI, None);
                    self.ped_mut(r).heading_rate = 0.0;
                    self.set_look_flag_entity(r, target, true);
                    self.set_look_timer(r, 3000);
                    self.say(px, r, sound::PED_TAXI_CALL);
                }
            } else {
                self.ped_mut(r).look_target = Some(target);
                self.turn_body(r);
            }
        } else {
            if self.ped(r).ped_type != ped_type::COP && needed_turn < 15.0_f32.to_radians() && self.ped(r).wait_state == wait_state::FALSE {
                let running = matches!(self.ped(r).stored_move_state, s if s == move_state::RUN as i32 || s == move_state::SPRINT as i32);
                if running && normal.z < 0.3 {
                    let run = self.ped_assoc(r, anim::STD_RUN).or_else(|| self.ped_assoc(r, anim::STD_RUNFAST));
                    if run.is_some_and(|a| a.blend_amount > 0.9 && a.is_running()) {
                        self.set_wait_state(px, r, wait_state::HITWALL, None);
                    }
                }
            }
            if self.ped(r).ped_state == state::FLEE_POS {
                let flee_pos = Vec2::new(self.epos(hit).x, self.epos(hit).y);
                let old_flee_timer = self.ped(r).flee_timer;
                self.set_flee_pos(r, flee_pos, 5000);
                if old_flee_timer != self.ped(r).flee_timer {
                    self.ped_mut(r).ped_state_timer = now + 500;
                }
            } else if self.ped(r).ped_state == state::FLEE_ENTITY && (needed_turn < 25.0_f32.to_radians() || self.ped(r).panic_counter > 10) {
                let pd = self.ped_mut(r);
                pd.colliding_thing_timer = now + 2500;
                pd.colliding_entity_while_fleeing = Some(hit);
                let current_dir = ((PI + pd.rotation_cur) / 45.0_f32.to_radians()).floor() as u8;
                let last = pd.last_path_node;
                let next = self.find_next_ped_node(pos, last, current_dir);
                self.ped_mut(r).next_path_node = next.map(|n| n.0);
            } else if needed_turn < 60.0_f32.to_radians() && !self.turn_away_from_hit(px, r, normal, old_dest_rot) {
                return;
            }
        }
        let pd = self.ped_mut(r);
        if pd.ped_state != state::WANDER_PATH && pd.ped_state != state::FLEE_ENTITY {
            pd.next_path_node = None;
        }
        pd.flags.hit_something_last_frame = true;
    }

    /// The `neededTurn < DEGTORAD(60.0f)` branch: picks a new heading away from what was hit.
    /// False if the original leaves the whole response at once.
    fn turn_away_from_hit(&mut self, px: &mut Px, r: EntityRef, normal: Vec3, old_dest_rot: f32) -> bool {
        let now = self.now();
        let pos = self.epos(r);
        let mut pos_to_head = normal * 4.0;
        pos_to_head.z = 0.0;
        pos_to_head += pos;
        let closest = self.ents.paths.find_node_closest_to_coors(pos_to_head, PATH_PED, 999999.9, false, false);
        // shortcut: no path node anywhere means the original reads node -1; the turn is left alone
        let Some(node) = usize::try_from(closest).ok().and_then(|i| self.ents.paths.nodes.get(i).map(|n| (i, n.pos))) else { return true };
        let (node_index, node_pos) = node;
        let objective = self.ped(r).objective;
        let angle_to_face;
        if objective != objective::KILL_CHAR_ON_FOOT && objective != objective::KILL_CHAR_ANY_MEANS {
            match self.ped(r).ped_state {
                state::SEEK_POS | state::SEEK_CAR => {
                    angle_to_face = radian_angle_between_points(node_pos.x, node_pos.y, pos.x, pos.y);
                    let to_node = Vec2::new(node_pos.x - pos.x, node_pos.y - pos.y);
                    let seek = self.ped(r).seek_pos;
                    if to_node.dot(Vec2::new(seek.x - pos.x, seek.y - pos.y)) < 0.0 {
                        let pd = self.ped_mut(r);
                        pd.rotation_cur = pd.rotation_dest;
                        return false;
                    }
                }
                state::WANDER_PATH => {
                    self.ped_mut(r).next_path_node = Some(node_index);
                    angle_to_face = radian_angle_between_points(node_pos.x, node_pos.y, pos.x, pos.y);
                }
                _ => {
                    if node_pos.x == 0.0 || node_pos.y == 0.0 {
                        pos_to_head = 3.0 * normal + pos;
                        pos_to_head.x += (px.rng.number() % 512) as f32 / 250.0 - 1.0;
                        pos_to_head.y += (px.rng.number() % 512) as f32 / 250.0 - 1.0;
                    } else {
                        pos_to_head.x = node_pos.x;
                        pos_to_head.y = node_pos.y;
                    }
                    angle_to_face = radian_angle_between_points(pos_to_head.x, pos_to_head.y, pos.x, pos.y);
                    if self.ped(r).ped_state != state::FOLLOW_PATH {
                        self.ped_mut(r).ped_state_timer = now + 500;
                    }
                }
            }
        } else {
            let mut away = radian_angle_between_points(normal.x, normal.y, 0.0, 0.0);
            let cur = self.ped(r).rotation_cur;
            if away < cur {
                away += TAU;
            }
            let needed = away - cur;
            let rate = self.ents.ped_stats.0[self.ped(r).ped_stats].heading_change_rate.to_radians() * 2.0;
            let pd = self.ped_mut(r);
            if needed <= PI {
                angle_to_face = 0.5 * needed + cur;
                pd.rotation_cur += rate;
            } else {
                angle_to_face = cur - (TAU - needed) * 0.5;
                pd.rotation_cur -= rate;
            }
            pd.ped_state_timer = now + 200;
            if pd.ped_type == ped_type::COP
                && let Some(criminal) = pd.ped_in_objective
            {
                let c = self.epos(criminal);
                let to_criminal = limit_radian_angle(radian_angle_between_points(c.x, c.y, pos.x, pos.y));
                let face = limit_radian_angle(angle_to_face);
                let turn = if to_criminal < face { to_criminal + TAU } else { to_criminal } - face;
                if turn > 150.0_f32.to_radians() && turn < 210.0_f32.to_radians() {
                    self.ped_mut(r).cop.get_or_insert_with(Default::default).stop_and_shoot_disabled_zone = true;
                }
            }
        }
        let pd = self.ped_mut(r);
        pd.rotation_dest = limit_radian_angle(angle_to_face);
        if pd.rotation_cur - PI > pd.rotation_dest {
            pd.rotation_dest += TAU;
        } else if PI + pd.rotation_cur < pd.rotation_dest {
            pd.rotation_dest -= TAU;
        }
        if old_dest_rot == pd.rotation_dest && now > pd.ped_state_timer {
            pd.ped_state_timer = now + 200;
            pd.rotation_dest += FRAC_PI_2;
        }
        true
    }

    fn hit_vehicle(&mut self, px: &mut Px, r: EntityRef, veh: EntityRef) {
        let now = self.now();
        let (veh_speed, veh_forward, veh_right) = {
            let e = self.ent(veh);
            (e.phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed), e.base.matrix.forward, e.base.matrix.right)
        };
        let veh_speed_sqr = veh_speed.length_squared();
        if self.ped(r).my_vehicle == Some(veh) {
            self.ped_mut(r).flags.collided_with_my_vehicle = true;
        }
        let is_player = self.ped(r).is_player();
        let damage_impulse = self.ent(r).phys.as_ref().map_or(0.0, |p| p.damage_impulse);
        if veh_speed_sqr <= 1.0 / 400.0 {
            let objective = self.ped(r).objective;
            let player_busy = is_player && !matches!(objective, objective::GOTO_AREA_ON_FOOT | objective::ENTER_CAR_AS_DRIVER | objective::RUN_TO_AREA);
            if !self.ped(r).is_ped_in_control() || player_busy {
                if is_player && !self.ped(r).flags.is_in_the_air {
                    let pd = self.ped(r);
                    let idle_player = pd.is_ped_in_control() && pd.player.as_ref().is_some_and(|d| d.move_speed == 0.0) && !pd.flags.is_looking && now > pd.look_timer;
                    if idle_player && self.try_veh(veh).is_some_and(|v| v.driver.is_some()) {
                        self.annoy_player_ped(r, false);
                        self.set_look_flag_entity(r, veh, true);
                        self.set_look_timer(r, 1300);
                        if weak_weapon(self.current_weapon_type(r)) {
                            self.ped_mut(r).flags.shake_fist = true;
                        }
                    } else {
                        self.set_look_flag_entity(r, veh, true);
                        self.set_look_timer(r, 500);
                    }
                }
            } else if !self.ped(r).flags.veh_enter_door_is_blocked {
                let mission_char = self.ped(r).char_created_by == MISSION_CHAR;
                if self.ent(veh).base.status != status::PLAYER || mission_char {
                    self.set_direction_to_walk_around_object(px, r, veh);
                } else if now >= self.ents.player_info.last_bump_player_car_timer || self.ped(r).ped_state_timer >= now {
                    self.set_direction_to_walk_around_object(px, r, veh);
                    self.ents.player_info.last_bump_player_car_timer = self.ped(r).ped_state_timer;
                } else if self.ped(r).flee_from != Some(veh) {
                    self.set_flee(px, r, veh, 4000);
                    let pd = self.ped_mut(r);
                    pd.flags.use_ped_node_seek = false;
                    pd.set_move_state(move_state::WALK);
                }
            }
            return;
        }
        px.hooks.play_one_shot(veh, sound::CAR_PED_COLLISION, damage_impulse);
        if !is_player {
            self.kill_ped_with_car(px, r, veh, damage_impulse);
            return;
        }
        let Some(col) = self.col_model_of(veh) else { return };
        let (col_min, col_max) = (col.bounding_box.min, col.bounding_box.max);
        let pos = self.epos(r);
        let centre_dist = self.ent(veh).base.matrix.transform_point((col_min + col_max) * 0.5) - pos;
        let angle_diff = limit_radian_angle(heading(veh_forward) - heading(centre_dist));
        let top_right = (col_max.x - col_min.x).atan2(col_max.y - col_min.y);
        let veh_dist = (pos - self.epos(veh)).normalize_or_zero();
        let forward = self.ent(r).base.matrix.forward;
        let right_dot;
        if angle_diff.abs() >= top_right && angle_diff.abs() < PI - top_right {
            let side = if angle_diff <= 0.0 { 1.0 } else { -1.0 };
            right_dot = (side * veh_right).dot(veh_speed);
            if veh_speed_sqr > 1.0 / 100.0 && right_dot < 0.1 && side * veh_right.dot(forward) < 0.0 && veh_dist.dot(veh_speed) > 0.0 {
                self.set_evasive_step(px, r, veh, 1);
            }
        } else {
            right_dot = veh_dist.dot(veh_speed);
        }
        if right_dot <= 0.1 {
            if self.ped(r).ped_state != state::FIGHT {
                self.set_look_flag_entity(r, veh, true);
                self.set_look_timer(r, 700);
            }
            return;
        }
        self.ped_mut(r).flags.is_standing = false;
        let dir = self.get_local_direction(r, Vec2::new(-veh_speed.x, -veh_speed.y));
        self.set_fall(px, r, 1000, (dir + anim::STD_HIGHIMPACT_FRONT as i32) as u32, false);
        let driver_is_player = self.try_veh(veh).and_then(|v| v.driver).is_some_and(|d| self.try_ped(d).is_some_and(|p| p.is_player()));
        let damage = if driver_is_player {
            right_dot * 1000.0
        } else if self.ent(veh).base.model_index == mi::TRAIN {
            50.0
        } else {
            20.0
        };
        self.inflict_damage(px, r, Hit { by: Some(veh), weapon: wt::RAMMEDBYCAR, damage, piece: piece::TORSO, direction: dir as u8 });
        self.say(px, r, sound::PED_DAMAGE);
    }
}
