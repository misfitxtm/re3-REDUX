//! `PedFight.cpp`: fist fights (defending, the combo state machine, strikes) and investigating events.
use super::callbacks as cb;
use super::ctx::Px;
use super::data::*;
use super::pending::Hit;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::weapons::info::weapon_type as wt;
use crate::world::World;
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

use super::types::stat_flag;

/// `hitSoundsByFightMoves`: our move (punch to bodyblow... 12 rows) against what the victim is doing (10 columns).
const NO_SND: u16 = sound::NO_SOUND;
const HIT_SOUNDS: [[u16; 10]; 12] = {
    use sound::*;
    const S33: u16 = FIGHT_PUNCH_33;
    const S34: u16 = FIGHT_KICK_34;
    const S35: u16 = FIGHT_HEADBUTT_35;
    const S36: u16 = FIGHT_PUNCH_36;
    const S37: u16 = FIGHT_PUNCH_37;
    const S38: u16 = FIGHT_CLOSE_PUNCH_38;
    const S39: u16 = FIGHT_PUNCH_39;
    const S40: u16 = FIGHT_PUNCH_OR_KICK_BELOW_40;
    const S41: u16 = FIGHT_PUNCH_41;
    const S42: u16 = FIGHT_PUNCH_FROM_BEHIND_42;
    const S43: u16 = FIGHT_KNEE_OR_KICK_43;
    const S44: u16 = FIGHT_KICK_44;
    [
        [S39, S42, S43, S43, S39, S39, S39, S39, S39, S42],
        [NO_SND; 10],
        [NO_SND; 10],
        [S39, S39, S39, S39, S33, S43, S39, S39, S39, S39],
        [S39, S39, S39, S39, S35, S39, S38, S38, S39, S39],
        [S39, S39, S39, S39, S33, S39, S41, S36, S39, S39],
        [S39, S39, S39, S39, S37, S40, S38, S38, S39, S39],
        [S39, S39, S39, S39, S34, S43, S44, S37, S39, S39],
        [S39, S39, S39, S39, S34, S43, S44, S37, S39, S39],
        [S39, S39, S39, S39, S34, S43, S44, S37, S39, S40],
        [S39, S39, S39, S39, S33, S39, S41, S37, S39, S40],
        [S39, S39, S39, S39, S39, S39, S39, S39, S33, S33],
    ]
};

/// `CPed::GetLocalDirection` for a ped heading `rotation`.
pub fn local_direction(rotation: f32, offset: Vec2) -> i32 {
    let mut angle = (-offset.x).atan2(offset.y) - rotation + 45.0_f32.to_radians();
    while angle < 0.0 {
        angle += TAU;
    }
    let mut direction = (angle.to_degrees() / 90.0) as i32;
    while direction > 3 {
        direction -= 4;
    }
    direction
}

impl World {
    fn fight_move_row(&self, id: u32) -> crate::ped::data::FightMove {
        self.ents.fight_moves.get(id as usize).copied().unwrap_or_default()
    }

    /// `CPed::GetLocalDirection`: 0 front, 1 left, 2 back, 3 right.
    pub fn get_local_direction(&self, r: EntityRef, offset: Vec2) -> i32 {
        local_direction(self.ped(r).rotation_cur, offset)
    }

    /// `CPed::StartFightDefend`.
    pub fn start_fight_defend(&mut self, px: &mut Px, r: EntityRef, direction: i32, hit_level: u8, unk: u8) {
        let st = self.ped(r).ped_state;
        if st == state::DEAD {
            if !px.hooks.nasty_game() {
                return;
            }
            if hit_level == self::hit_level::GROUND {
                let frontal = self.ped_first_assoc(r, aflags::FRONTAL).is_some();
                let id = if frontal { anim::STD_HIT_FLOOR_FRONT } else { self.fight_move_row(fight_move::HITONFLOOR).anim_id };
                if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, id, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.set_current_time(0.0);
                    a.flags |= aflags::RUNNING;
                    a.flags &= !aflags::FADE_OUT_WHEN_DONE;
                }
            }
            let head = self.ped_node_position(r, node::HEAD);
            let forward = self.ent(r).base.matrix.forward;
            for _ in 0..4 {
                px.hooks.add_blood(head - forward * 0.2, Vec3::new(0.0, 0.0, 0.1));
            }
        } else if st == state::FALL {
            if hit_level == self::hit_level::GROUND && !self.is_ped_head_above_pos(r, -0.3) {
                let frontal = self.ped_first_assoc(r, aflags::FRONTAL).is_some();
                let id = if frontal { anim::STD_HIT_FLOOR_FRONT } else { anim::STD_HIT_FLOOR };
                if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, id, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.flags &= !aflags::FADE_OUT_WHEN_DONE;
                    a.flags |= aflags::DELETE_FADED_OUT;
                }
            }
        } else if self.ped(r).is_ped_in_control() {
            self.defend_in_control(px, r, direction, hit_level, unk);
        }
    }

    /// The last branch of `StartFightDefend`: a ped that can react to being hit.
    fn defend_in_control(&mut self, px: &mut Px, r: EntityRef, direction: i32, hit_level: u8, unk: u8) {
        let pd = self.ped(r);
        let player = pd.is_player();
        let speed = pd.player.as_ref().map_or(0.0, |p| p.move_speed);
        if (player && pd.ped_state != state::FIGHT && speed > 1.0) || (!player && pd.objective == objective::FLEE_CHAR_ON_FOOT_TILL_SAFE) {
            // (without VC_PED_PORTS) running victims are hit by the gun animations or knocked over
            let hit_by_gun_anim = (hit_level != self::hit_level::HIGH && hit_level != self::hit_level::LOW) || ((player || px.rng.number() & 3 != 0) && px.rng.number() & 7 != 0);
            if hit_by_gun_anim {
                if player || px.rng.number() & 3 != 0 {
                    let shot_anim = match direction {
                        1 => anim::STD_HITBYGUN_LEFT,
                        2 => anim::STD_HITBYGUN_BACK,
                        3 => anim::STD_HITBYGUN_RIGHT,
                        _ => anim::STD_HITBYGUN_FRONT,
                    };
                    if self.ped_assoc(r, shot_anim).is_none_or(|a| a.blend_delta < 0.0) {
                        self.ped_blend_anim(r, assoc_group::STD, shot_anim, 8.0);
                    }
                    if let Some(a) = self.ped_assoc_mut(r, shot_anim) {
                        a.set_current_time(0.0);
                        a.flags |= aflags::RUNNING | aflags::FADE_OUT_WHEN_DONE;
                    }
                } else {
                    let time = px.rng.range_i32(1000, 3000);
                    self.set_wait_state(px, r, wait_state::PLAYANIM_DUCK, Some(time));
                }
            } else {
                let fall = match direction {
                    1 => anim::STD_HIGHIMPACT_LEFT,
                    2 => anim::STD_HIGHIMPACT_BACK,
                    3 => anim::STD_HIGHIMPACT_RIGHT,
                    _ => anim::STD_KO_SHOT_STOMACH,
                };
                self.set_fall(px, r, 500, fall, false);
            }
            self.say(px, r, sound::PED_DEFEND);
            return;
        }
        self.say(px, r, sound::PED_DEFEND);
        let new_move = match hit_level {
            self::hit_level::GROUND => fight_move::HITONFLOOR,
            self::hit_level::LOW => {
                if direction == 2 {
                    self.set_fall(px, r, 1000, anim::STD_HIGHIMPACT_BACK, false);
                    return;
                }
                fight_move::HITBODY
            }
            self::hit_level::HIGH => match direction {
                1 => fight_move::HITLEFT,
                2 => fight_move::HITBACK,
                3 => fight_move::HITRIGHT,
                _ if unk <= 5 => fight_move::HITHEAD,
                _ => fight_move::HITBIGSTEP,
            },
            _ => match direction {
                1 => fight_move::HITLEFT,
                2 => fight_move::HITBACK,
                3 => fight_move::HITRIGHT,
                _ if unk <= 5 => fight_move::HITCHEST,
                _ => fight_move::HITBIGSTEP,
            },
        };
        self.ped_mut(r).cur_fight_move = new_move;
        if self.ped(r).ped_state == state::GETUP && !self.is_ped_head_above_pos(r, 0.0) {
            self.ped_mut(r).cur_fight_move = fight_move::HITONFLOOR;
        }
        let move_anim = self.fight_move_row(self.ped(r).cur_fight_move).anim_id;
        let pd = self.ped(r);
        if pd.ped_state == state::FIGHT {
            let player = pd.is_player();
            if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, move_anim, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                a.set_current_time(0.0);
                a.set_finish_callback(cb::FINISH_FIGHT_MOVE);
                if player {
                    a.speed = 1.3;
                }
            }
            let pd = self.ped_mut(r);
            pd.take_a_step_after_attack = false;
            pd.fight_button_pressure = 0;
        } else if pd.is_player() && pd.current_weapon as usize != wt::UNARMED {
            if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, move_anim, 4.0).and_then(|i| self.ped_assoc_at(r, i)) {
                a.set_current_time(0.0);
                a.speed = 1.3;
            }
        } else {
            if !matches!(pd.ped_state, state::AIM_GUN | state::ATTACK) {
                self.set_stored_state(r);
            }
            if self.ped(r).wait_state != wait_state::FALSE {
                self.ped_mut(r).wait_state = wait_state::FALSE;
                self.restore_heading_rate(r);
            }
            let pd = self.ped_mut(r);
            pd.ped_state = state::FIGHT;
            pd.fight_button_pressure = 0;
            if let Some(c) = self.ped_clump_mut(r) {
                c.anim.remove_associations(aflags::REPEAT);
            }
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_STARTWALK) {
                a.flags |= aflags::DELETE_FADED_OUT;
                a.blend_delta = -1000.0;
            }
            let stop = [anim::STD_RUNSTOP1, anim::STD_RUNSTOP2].into_iter().find(|&a| self.ped_assoc(r, a).is_some());
            if let Some(id) = stop {
                if let Some(a) = self.ped_assoc_mut(r, id) {
                    a.flags |= aflags::DELETE_FADED_OUT;
                    a.blend_delta = -1000.0;
                }
                self.restore_heading_rate(r);
            }
            let pd = self.ped_mut(r);
            pd.set_move_state(move_state::NONE);
            pd.stored_move_state = move_state::NONE as i32;
            if let Some(a) = self.ped_add_anim(r, assoc_group::STD, anim::STD_FIGHT_IDLE).and_then(|i| self.ped_assoc_at(r, i)) {
                a.blend_amount = 1.0;
            }
            if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, move_anim, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                a.set_finish_callback(cb::FINISH_FIGHT_MOVE);
            }
            let pd = self.ped_mut(r);
            pd.fight_state = fight_state::NO_MOVE;
            pd.take_a_step_after_attack = false;
            pd.flags.is_attacking = true;
        }
    }

    /// `CPed::Fight`: picks and runs the next move of a fist fight.
    pub fn fight(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let cur_move = self.ped(r).cur_fight_move;
        let current = match cur_move {
            fight_move::NULL => return,
            fight_move::IDLE2NORM => {
                self.ped_mut(r).cur_fight_move = fight_move::NULL;
                self.restore_previous_state(px, r);
                None
            }
            fight_move::IDLE => None,
            _ => Some(self.fight_move_row(cur_move).anim_id),
        }
        .filter(|&id| self.ped_assoc(r, id).is_some());
        let cur_move = self.ped(r).cur_fight_move;
        let pd = self.ped(r);
        let (attacking, player, fight_state_now, weapon_ty, pressure, step_after) =
            (pd.flags.is_attacking, pd.is_player(), pd.fight_state, pd.weapons[pd.current_weapon as usize].ty, pd.fight_button_pressure, pd.take_a_step_after_attack);
        if !attacking && player {
            if let Some(a) = current.and_then(|id| self.ped_assoc_mut(r, id)) {
                a.blend_delta = -1000.0;
                a.flags |= aflags::DELETE_FADED_OUT;
                a.flags &= !aflags::RUNNING;
            }
            let mode = if step_after { ENDFIGHT_WITH_A_STEP } else { ENDFIGHT_FAST };
            self.end_fight(px, r, mode);
        } else if let (Some(id), true) = (current, fight_state_now > fight_state::MOVE_FINISHED) {
            self.fight_continue_move(px, r, id, cur_move);
        } else if weapon_ty != wt::UNARMED {
            self.end_fight(px, r, ENDFIGHT_FAST);
        } else if pressure != 0 {
            self.fight_pick_next_move(px, r);
        } else if step_after && cur_move != fight_move::SHUFFLE_F && self.ground_state_for_step(px, r) == on_ground::IN_FRONT_OF_ATTACKER {
            self.ped_mut(r).cur_fight_move = fight_move::SHUFFLE_F;
            let anim_id = self.fight_move_row(fight_move::SHUFFLE_F).anim_id;
            let existing = self.ped_assoc(r, anim_id).is_some();
            if existing {
                if let Some(a) = self.ped_assoc_mut(r, anim_id) {
                    a.set_current_time(0.0);
                    a.blend_delta = 4.0;
                    a.flags |= aflags::RUNNING;
                    a.set_finish_callback(cb::FINISH_FIGHT_MOVE);
                }
            } else if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, anim_id, 32.0).and_then(|i| self.ped_assoc_at(r, i)) {
                a.set_finish_callback(cb::FINISH_FIGHT_MOVE);
            }
            let pd = self.ped_mut(r);
            pd.fight_state = fight_state::NO_MOVE;
            pd.fight_button_pressure = 0;
            pd.take_a_step_after_attack = false;
        } else if step_after {
            self.end_fight(px, r, ENDFIGHT_FAST);
        } else if cur_move == fight_move::IDLE {
            if now > self.ped(r).wait_timer {
                self.end_fight(px, r, ENDFIGHT_NORMAL);
            }
        } else {
            let pd = self.ped_mut(r);
            pd.cur_fight_move = fight_move::IDLE;
            pd.wait_timer = now.wrapping_add(if pd.is_player() { 500 } else { 2000 });
        }
    }

    /// `CheckForPedsOnGroundToAttack` for the step after an attack (the state only).
    fn ground_state_for_step(&mut self, px: &mut Px, r: EntityRef) -> u8 {
        // FIX_BUGS compares with PED_IN_FRONT_OF_ATTACKER
        self.on_ground_state_of(px, r).0
    }

    /// A move that is playing: strike when its hit window opens, speed it up or chain the next combo move.
    fn fight_continue_move(&mut self, px: &mut Px, r: EntityRef, assoc: u32, cur_move: u32) {
        let (anim_time, player) = (self.ped_assoc(r, assoc).map_or(0.0, |a| a.current_time), self.ped(r).is_player());
        let m = self.fight_move_row(cur_move);
        let fight_state_now = self.ped(r).fight_state;
        if m.hit_level != hit_level::NULL && anim_time > m.start_fire_time && anim_time <= m.end_fire_time && fight_state_now >= fight_state::NO_MOVE {
            let node = match cur_move {
                fight_move::STDPUNCH | fight_move::PUNCHHOOK | fight_move::BODYBLOW => Some(node::HANDR),
                fight_move::KNEE => Some(node::LOWERLEGR),
                fight_move::HEADBUTT => Some(node::HEAD),
                fight_move::PUNCHJAB => Some(node::HANDL),
                fight_move::KICK | fight_move::LONGKICK | fight_move::ROUNDHOUSE | fight_move::GROUNDKICK => Some(node::FOOTR),
                _ => None,
            };
            let forward = self.ent(r).base.matrix.forward;
            let mut touching = node.map_or(self.ped_node_position_of_origin(r), |n| self.ped_node_position(r, n));
            if cur_move == fight_move::PUNCHJAB {
                touching += forward * 0.1;
            } else if cur_move == fight_move::PUNCHHOOK {
                touching += forward * 0.22;
            }
            self.fight_strike(px, r, touching);
            self.ped_mut(r).fight_button_pressure = 0;
            return;
        }
        if m.hit_level != hit_level::NULL {
            if anim_time > m.end_fire_time
                && let Some(a) = self.ped_assoc_mut(r, assoc)
            {
                a.speed = if player { 1.0 } else { 0.8 };
            }
            let pd = self.ped(r);
            let in_combo = pd.player.as_ref().is_some_and(|p| p.combo_move != 0);
            if player && !in_combo && m.combo_follow_on_time > 0.0 && pd.fight_button_pressure != 0 && anim_time > m.combo_follow_on_time {
                // the next move in the table: we are in a combo
                let next = cur_move + 1;
                self.ped_mut(r).cur_fight_move = next;
                let id = self.fight_move_row(next).anim_id;
                if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, id, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.set_finish_callback(cb::FINISH_FIGHT_MOVE);
                    let t = 0.1 * a.hierarchy.total_length;
                    a.set_current_time(t);
                }
                let pd = self.ped_mut(r);
                pd.fight_button_pressure = 0;
                pd.player.get_or_insert_with(Default::default).combo_move = 1;
            }
        } else if m.start_fire_time > 0.0
            && cur_move != fight_move::SHUFFLE_F
            && anim_time > m.start_fire_time
            && let Some(a) = self.ped_assoc_mut(r, assoc)
        {
            a.speed = if player { 1.3 } else { 0.8 };
        }
    }

    /// The origin of the ped, for moves that touch with no node.
    fn ped_node_position_of_origin(&self, _r: EntityRef) -> Vec3 {
        Vec3::ZERO
    }

    /// The part of `Fight` that chooses the next move once the pressed button is noticed.
    fn fight_pick_next_move(&mut self, px: &mut Px, r: EntityRef) {
        let mut can_affect_multiple = true;
        let mut next_angle = self.ped(r).rotation_cur;
        let (mut kick_ground, mut angle_for_ground_kick) = (false, 0.0);
        let mut ped_on_ground: Option<EntityRef> = None;
        let mut go_forward = false;
        self.say(px, r, sound::PED_ATTACK);
        let pd = self.ped(r);
        let player = pd.is_player();
        let pressure = pd.fight_button_pressure;
        let (can_roundhouse, punch_only, can_kick, mut next_fight_move, has_shopping_bags, can_knee_head);
        if player {
            can_roundhouse = false;
            punch_only = false;
            can_kick = true;
            next_fight_move = if pressure > 190 { fight_move::HEADBUTT } else { fight_move::KNEE };
            has_shopping_bags = false;
            can_knee_head = true;
            self.ped_mut(r).player.get_or_insert_with(Default::default).combo_move = 0;
        } else {
            next_fight_move = if pressure > 120 { fight_move::HEADBUTT } else { fight_move::KNEE };
            let f = self.ents.ped_stats.0[pd.ped_stats].flags;
            punch_only = f & stat_flag::PUNCH_ONLY != 0;
            can_roundhouse = f & stat_flag::CAN_ROUNDHOUSE != 0;
            can_knee_head = f & stat_flag::CAN_KNEE_HEAD != 0;
            can_kick = f & stat_flag::CAN_KICK != 0;
            has_shopping_bags = f & stat_flag::SHOPPING_BAGS != 0;
        }
        let nasty = px.hooks.nasty_game();
        let pos = self.epos(r);
        if player || self.ped(r).ped_in_objective.is_none() {
            let pd = self.ped(r);
            let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
            for near_ped in near {
                let np = self.epos(near_ped);
                let dist = (np - pos).length();
                if dist < 3.0 {
                    next_angle = limit_radian_angle(radian_angle_between_points(np.x, np.y, pos.x, pos.y));
                    let cur = limit_radian_angle(self.ped(r).rotation_cur);
                    self.ped_mut(r).rotation_cur = cur;
                    let mut needed_turn = (next_angle - cur).abs();
                    if needed_turn > PI {
                        needed_turn = TAU - needed_turn;
                    }
                    if !self.ped(near_ped).on_ground_or_getting_up() {
                        if dist < 0.8 && needed_turn < 75.0_f32.to_radians() && can_knee_head {
                            can_affect_multiple = false;
                        } else if dist >= 1.3 || needed_turn >= 55.0_f32.to_radians() || has_shopping_bags {
                            if dist < 1.7 && needed_turn < 35.0_f32.to_radians() && (can_kick || has_shopping_bags) {
                                next_fight_move = fight_move::KICK;
                                if has_shopping_bags || (can_roundhouse && px.rng.number() & 1 != 0) {
                                    next_fight_move = fight_move::ROUNDHOUSE;
                                }
                                can_affect_multiple = false;
                            } else if dist < 2.0 && needed_turn < 30.0_f32.to_radians() && can_kick {
                                can_affect_multiple = false;
                                next_fight_move = fight_move::LONGKICK;
                            } else if needed_turn < 30.0_f32.to_radians() {
                                go_forward = true;
                            }
                        } else {
                            // makes it a jab or a hook
                            next_fight_move += 2;
                            if punch_only {
                                next_fight_move = fight_move::PUNCHJAB;
                            }
                            can_affect_multiple = false;
                        }
                    } else if !nasty || dist >= 1.3 || needed_turn >= 55.0_f32.to_radians() || punch_only {
                        if dist > 0.8 && dist < 3.0 && needed_turn < 30.0_f32.to_radians() {
                            go_forward = true;
                        }
                    } else if self.ped(near_ped).ped_state != state::DEAD || ped_on_ground.is_some() {
                        if !self.is_ped_head_above_pos(near_ped, -0.3) {
                            can_affect_multiple = false;
                            next_fight_move = fight_move::GROUNDKICK;
                        }
                    } else {
                        ped_on_ground = Some(near_ped);
                        kick_ground = true;
                        angle_for_ground_kick = next_angle;
                    }
                }
                if !can_affect_multiple {
                    self.ped_mut(r).rotation_dest = next_angle;
                    if player {
                        let pd = self.ped_mut(r);
                        pd.rotation_cur = pd.rotation_dest;
                        pd.look_timer = 0;
                        self.set_look_flag_entity(r, near_ped, true);
                        self.set_look_timer(r, 1500);
                    }
                    break;
                }
            }
        } else if let Some(target) = self.ped(r).ped_in_objective {
            // a scripted fight with one particular ped
            can_affect_multiple = false;
            let tp = self.epos(target);
            let dist = (tp - pos).length();
            if has_shopping_bags {
                next_fight_move = if dist >= 1.7 { fight_move::SHUFFLE_F } else { fight_move::ROUNDHOUSE };
            } else if punch_only {
                next_fight_move = if dist >= 1.3 { fight_move::SHUFFLE_F } else { fight_move::PUNCHJAB };
            } else if dist >= 3.0 {
                next_fight_move = fight_move::STDPUNCH;
            } else {
                next_angle = limit_radian_angle(radian_angle_between_points(tp.x, tp.y, pos.x, pos.y));
                let pd = self.ped_mut(r);
                pd.rotation_dest = next_angle;
                pd.rotation_cur = next_angle;
                if !self.ped(target).on_ground_or_getting_up() {
                    if dist >= 0.8 || !can_knee_head {
                        if dist >= 1.3 {
                            if dist < 1.7 && can_kick {
                                next_fight_move = fight_move::KICK;
                                if can_roundhouse && px.rng.number() & 1 != 0 {
                                    next_fight_move = fight_move::ROUNDHOUSE;
                                }
                            } else if dist < 2.0 && can_kick {
                                next_fight_move += 5;
                            } else {
                                next_fight_move = fight_move::SHUFFLE_F;
                            }
                        } else {
                            next_fight_move += 2;
                        }
                    }
                } else if !nasty || dist >= 1.3 || self.ped(target).is_player() || (self.ped(target).ped_state != state::DEAD && self.is_ped_head_above_pos(target, -0.3)) {
                    next_fight_move = fight_move::IDLE;
                } else {
                    next_fight_move = fight_move::GROUNDKICK;
                }
            }
        }
        if can_affect_multiple {
            if let (true, true, Some(g)) = (kick_ground, player, ped_on_ground) {
                next_fight_move = fight_move::GROUNDKICK;
                let pd = self.ped_mut(r);
                pd.rotation_dest = angle_for_ground_kick;
                pd.rotation_cur = angle_for_ground_kick;
                pd.look_timer = 0;
                self.set_look_flag_entity(r, g, true);
                self.set_look_timer(r, 1500);
            } else if go_forward {
                next_fight_move = fight_move::SHUFFLE_F;
            } else {
                next_fight_move = fight_move::STDPUNCH;
            }
        }
        if next_fight_move != fight_move::IDLE {
            self.ped_mut(r).cur_fight_move = next_fight_move;
            let id = self.fight_move_row(next_fight_move).anim_id;
            let finished = self.ped(r).fight_state == fight_state::MOVE_FINISHED;
            if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, id, 4.0).and_then(|i| self.ped_assoc_at(r, i)) {
                a.set_finish_callback(cb::FINISH_FIGHT_MOVE);
                if finished && a.current_time != 0.0 {
                    a.set_current_time(0.0);
                    a.flags |= aflags::RUNNING;
                }
            }
            self.ped_mut(r).fight_button_pressure = 0;
        }
        self.ped_mut(r).fight_state = fight_state::NO_MOVE;
    }

    /// `CPed::EndFight`.
    pub fn end_fight(&mut self, px: &mut Px, r: EntityRef, end_type: u8) {
        if self.ped(r).ped_state != state::FIGHT {
            return;
        }
        self.ped_mut(r).cur_fight_move = fight_move::NULL;
        self.restore_previous_state(px, r);
        if let Some(a) = self.ped_assoc_mut(r, anim::STD_FIGHT_IDLE) {
            a.flags |= aflags::DELETE_FADED_OUT;
        }
        let group = self.ped(r).anim_group as usize;
        match end_type {
            ENDFIGHT_NORMAL => {
                self.ped_blend_anim(r, group, anim::STD_IDLE, 8.0);
                self.ped_blend_anim(r, assoc_group::STD, anim::STD_FIGHT_2IDLE, 8.0);
            }
            ENDFIGHT_WITH_A_STEP => {
                self.ped_blend_anim(r, group, anim::STD_IDLE, 1.0);
                self.ped_blend_anim(r, assoc_group::STD, anim::STD_STARTWALK, 8.0);
            }
            ENDFIGHT_FAST => {
                self.ped_blend_anim(r, group, anim::STD_IDLE, 8.0);
                if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, anim::STD_FIGHT_2IDLE, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.speed = 2.0;
                }
            }
            _ => {}
        }
        self.ped_mut(r).wait_timer = 0;
    }

    /// `CPed::PlayHitSound`.
    pub fn play_hit_sound(&mut self, px: &mut Px, r: EntityRef, hit_to: EntityRef) {
        let cur = self.ped(r).cur_fight_move;
        if cur == fight_move::NULL || cur >= fight_move::HITFRONT {
            return;
        }
        let row = (cur - fight_move::STDPUNCH) as usize;
        let victim_move = self.ped(hit_to).cur_fight_move;
        let column = if victim_move > fight_move::GROUNDKICK && victim_move < fight_move::IDLE2NORM {
            (victim_move - fight_move::HITFRONT) as usize
        } else if self.ped(hit_to).ped_state == state::DEAD || self.ped(hit_to).use_ground_col_model() {
            (fight_move::HITONFLOOR - fight_move::HITFRONT) as usize
        } else {
            0
        };
        let sound_id = HIT_SOUNDS[row][column];
        if sound_id != NO_SND {
            px.hooks.play_one_shot(r, sound_id, 0.0);
        }
    }

    /// `CPed::FightStrike`: tests the struck spot against the peds close by and hurts the first one hit.
    pub fn fight_strike(&mut self, px: &mut Px, r: EntityRef, touched_node_pos: Vec3) -> bool {
        let state_now = self.ped(r).fight_state;
        if state_now == fight_state::JUST_ATTACKED {
            return false;
        }
        let cur_move = self.ped(r).cur_fight_move;
        let m = self.fight_move_row(cur_move);
        let unarmed = {
            let pd = self.ped(r);
            pd.weapons[pd.current_weapon as usize].ty == wt::UNARMED
        };
        let pd = self.ped(r);
        let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
        let mut found: Option<(EntityRef, u8, Vec3)> = None;
        'peds: for near_ped in near {
            let radius = self.bound_radius_of(near_ped);
            let max_to_be_beaten = if unarmed { radius + m.strike_radius } else { radius + m.strike_radius + 0.1 };
            let uses_collision = self.ent(near_ped).base.flags.uses_collision;
            if !(uses_collision || self.ped(near_ped).ped_state == state::DEAD) {
                continue;
            }
            let centre = self.bound_centre(near_ped);
            if max_to_be_beaten * max_to_be_beaten <= (centre - touched_node_pos).length_squared() {
                continue;
            }
            let ground = self.ped(near_ped).on_ground() || !self.is_ped_head_above_pos(near_ped, -0.3);
            let Some(col) = self.ped_hit_col_model(r, ground) else { continue };
            for s in &col.spheres {
                let attack_distance = self.epos(near_ped) + s.center - touched_node_pos;
                let max_to_beat = s.radius + m.strike_radius;
                if max_to_beat * max_to_beat > attack_distance.length_squared() {
                    found = Some((near_ped, s.piece, attack_distance));
                    break 'peds;
                }
            }
        }
        if let Some((victim, piece, attack_distance)) = found {
            self.strike_victim(px, r, victim, piece, attack_distance, touched_node_pos, m);
        }
        if self.ped(r).fight_state == fight_state::NO_MOVE {
            self.ped_mut(r).fight_state = fight_state::STATE_1;
        }
        self.ped_mut(r).hit_last_pos = touched_node_pos;
        false
    }

    /// What happens to the ped a strike landed on.
    #[allow(clippy::too_many_arguments)]
    fn strike_victim(&mut self, px: &mut Px, r: EntityRef, victim: EntityRef, piece: u8, attack_distance: Vec3, touched: Vec3, m: crate::ped::data::FightMove) {
        if self.ped(victim).is_player() && self.ped(victim).ped_state == state::GETUP {
            return;
        }
        let old_health = self.ped(victim).health;
        let blood_pos = attack_distance * 0.5 + touched;
        let mut damage_mult = i32::from(m.damage) * ((px.rng.number() & 1) as i32 + 2) + 1;
        let (pos, vpos) = (self.epos(r), self.epos(victim));
        let mut direction = self.get_local_direction(victim, Vec2::new(pos.x - vpos.x, pos.y - vpos.y));
        if self.ped(r).is_player() {
            if self.ped(r).player.as_ref().is_some_and(|p| p.adrenaline_active) {
                damage_mult = 20;
            }
        } else {
            damage_mult = (damage_mult as f32 * self.ents.ped_stats.0[self.ped(r).ped_stats].attack_strength) as i32;
        }
        // a kick sometimes comes from the side
        if self.ped(r).cur_fight_move == fight_move::KICK && px.rng.number() & 1 != 0 {
            direction += 1;
            if direction > 3 {
                direction -= 4;
            }
        }
        self.react_to_attack(px, victim, r);
        let pd = self.ped(r);
        let armed = pd.weapons[pd.current_weapon as usize].ty != wt::UNARMED;
        // mostly unused: above 5 the walk-hit animation plays
        let unk = if armed && !self.ped(victim).is_player() { 101 } else { damage_mult.clamp(0, 255) as u8 };
        self.start_fight_defend(px, victim, direction, m.hit_level, unk);
        self.play_hit_sound(px, r, victim);
        self.ped_mut(r).fight_state = fight_state::JUST_ATTACKED;
        if let Some(a) = self.ped_assoc_mut(r, m.anim_id) {
            a.speed = 0.6;
        }
        if !self.ped(victim).dying_or_dead() {
            self.inflict_damage(px, victim, Hit { by: Some(r), weapon: wt::UNARMED, damage: damage_mult as f32 * 3.0, piece, direction: direction as u8 });
        }
        if px.hooks.nasty_game() && m.hit_level > hit_level::MEDIUM && self.ped(victim).ped_state == state::DIE && px.hooks.is_on_screen(victim) {
            let dir = attack_distance / (10.0 * attack_distance.length());
            for _ in 0..4 {
                px.hooks.add_blood(blood_pos, dir);
            }
        }
        if !self.ped(victim).on_ground() {
            let vp = self.ped(victim);
            let cur_health = vp.health;
            let one_hit = self.ents.ped_stats.0[vp.ped_stats].flags & stat_flag::ONE_HIT_KNOCKDOWN != 0;
            let player_armed = armed && self.ped(r).is_player();
            if cur_health > 0.0 && ((cur_health < 40.0 && old_health > 40.0 && !vp.is_player()) || (cur_health < 20.0 && old_health > 20.0) || player_armed || one_hit) {
                self.set_fall(px, victim, 0, anim::STD_HIGHIMPACT_FRONT + direction as u32, false);
                if self.ped(victim).ped_state == state::FALL {
                    self.ped_mut(victim).flags.is_standing = false;
                }
            }
        }
        if self.ped(victim).ped_state == state::DIE || !self.ped(victim).flags.is_standing {
            let mut away = (self.epos(victim) - self.epos(r)).normalize_or_zero();
            away.z = 1.0;
            self.ped_mut(victim).flags.is_standing = false;
            let move_mult = if self.ped(r).cur_fight_move == fight_move::GROUNDKICK {
                (damage_mult as f32 * 0.6).min(4.0)
            } else if self.ped(victim).ped_state != state::DIE || damage_mult >= 20 {
                damage_mult as f32
            } else {
                (damage_mult as f32 * 2.0).min(14.0)
            };
            if let Some(p) = self.get_mut(victim).and_then(|e| e.phys.as_mut()) {
                p.apply_move_force(away * (move_mult * 0.6));
            }
        }
        let event = if self.ped(victim).ped_type == super::types::ped_type::COP { event_type::ASSAULT_POLICE } else { event_type::ASSAULT };
        px.hooks.register_event(event, victim, 2000);
    }

    /// `CPed::FinishFightMoveCB`.
    pub fn finish_fight_move_cb(&mut self, _px: &mut Px, r: EntityRef, anim_id: i32) {
        let cur = self.ped(r).cur_fight_move;
        if self.fight_move_row(cur).anim_id as i32 == anim_id {
            self.ped_mut(r).fight_state = fight_state::MOVE_FINISHED;
            if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(anim_id)) {
                a.blend_delta = -1000.0;
            }
        }
    }

    /// `CPed::SetInvestigateEvent`.
    pub fn set_investigate_event(&mut self, _px: &mut Px, r: EntityRef, what: super::pending::Investigate) {
        let pd = self.ped(r);
        if !pd.is_ped_in_control() || pd.char_created_by == MISSION_CHAR {
            return;
        }
        self.set_stored_state(r);
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.flags.find_new_node_after_state_restore = false;
        pd.ped_state = state::INVESTIGATE;
        pd.chat_timer = now.wrapping_add(what.time as u16 as u32);
        pd.event_type = what.event;
        pd.event_or_threat = what.pos;
        pd.distance_to_count_seek_done = what.distance_to_count_done;
        pd.angle_to_event = what.angle;
        if what.event >= event_type::ICECREAM {
            pd.look_timer = 0;
        } else {
            self.ped_blend_anim(r, assoc_group::STD, anim::STD_HANDSCOWER, 4.0);
        }
    }

    /// `CPed::InvestigateEvent`.
    pub fn investigate_event(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        if self.ped(r).wait_state == wait_state::TURN180 {
            return;
        }
        let pd = self.ped(r);
        if now > pd.chat_timer {
            if pd.chat_timer != 0 {
                if pd.event_type < event_type::ASSAULT_NASTYWEAPON {
                    self.set_wait_state(px, r, wait_state::TURN180, None);
                }
                self.ped_mut(r).chat_timer = 0;
            } else {
                self.clear_investigate_event(px, r);
            }
            return;
        }
        let pos = self.epos(r);
        let vec_dist = pd.event_or_threat - Vec2::new(pos.x, pos.y);
        let dist_sq = vec_dist.length_squared();
        let done = pd.distance_to_count_seek_done;
        if done * done >= dist_sq {
            self.ped_mut(r).rotation_dest = radian_angle_between_points(vec_dist.x, vec_dist.y, 0.0, 0.0);
            self.ped_mut(r).set_move_state(move_state::STILL);
            self.investigate_at_event(px, r);
        } else {
            let z = pos.z;
            let pd = self.ped_mut(r);
            pd.seek_pos = Vec3::new(pd.event_or_threat.x, pd.event_or_threat.y, z);
            self.seek(px, r);
            let pd = self.ped(r);
            if pd.event_type < event_type::ICECREAM {
                let far = 5.0 + pd.distance_to_count_seek_done;
                if far * far < dist_sq {
                    self.ped_mut(r).set_move_state(move_state::RUN);
                    return;
                }
            }
            if pd.event_type <= event_type::EXPLOSION || pd.event_type >= event_type::SHOPSTALL {
                self.ped_mut(r).set_move_state(move_state::WALK);
                return;
            }
            if dist_sq > 1.2 * 1.2 {
                self.ped_mut(r).set_move_state(move_state::WALK);
                return;
            }
            let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
            let spot = pd.event_or_threat;
            for n in near {
                let np = self.epos(n);
                if (spot - Vec2::new(np.x, np.y)).length_squared() < 0.4 * 0.4 {
                    self.ped_mut(r).set_move_state(move_state::STILL);
                    return;
                }
            }
            self.ped_mut(r).set_move_state(move_state::WALK);
        }
    }

    /// The ped has reached the event: look at it the way the event asks.
    fn investigate_at_event(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        let group = pd.anim_group as usize;
        match pd.event_type {
            event_type::DEAD_PED | event_type::HIT_AND_RUN | event_type::HIT_AND_RUN_COP => {
                if now > pd.look_timer {
                    if let Some(a) = self.ped_assoc_mut(r, anim::STD_ROADCROSS) {
                        a.blend_delta = -8.0;
                        a.flags |= aflags::DELETE_FADED_OUT;
                        if let Some(e) = self.ped(r).event_entity {
                            self.set_look_flag_entity(r, e, true);
                        }
                        let t = px.rng.range_i32(1500, 4000);
                        self.set_look_timer(r, t);
                    } else if px.rng.number() & 3 != 0 {
                        self.clear_look_flag(px, r);
                        self.ped_blend_anim(r, assoc_group::STD, anim::STD_ROADCROSS, 4.0);
                        let t = px.rng.range_i32(1000, 2500);
                        self.set_look_timer(r, t);
                        self.say(px, r, sound::PED_CHAT_EVENT);
                    } else {
                        self.clear_investigate_event(px, r);
                    }
                }
            }
            event_type::FIRE | event_type::EXPLOSION => {
                if pd.flags.has_a_camera && now > pd.look_timer {
                    let idle = [anim::STD_IDLE_CAM, anim::STD_IDLE].into_iter().find(|&a| self.ped_assoc(r, a).is_some());
                    if idle == Some(anim::STD_IDLE_CAM) {
                        self.ped_blend_anim(r, group, anim::STD_IDLE, 4.0);
                        let t = px.rng.range_i32(1000, 2500);
                        self.set_look_timer(r, t);
                    } else if px.rng.number() & 3 != 0 {
                        self.ped_blend_anim(r, assoc_group::STD, anim::STD_IDLE_CAM, 4.0);
                        let t = px.rng.range_i32(2500, 5000);
                        self.set_look_timer(r, t);
                        self.say(px, r, sound::PED_CHAT_EVENT);
                    } else {
                        self.ped_mut(r).chat_timer = 0;
                    }
                } else if now > pd.look_timer {
                    let found = [anim::STD_IDLE, anim::STD_IDLE_HBHB, anim::STD_XPRESS_SCRATCH].into_iter().find(|&a| self.ped_assoc(r, a).is_some());
                    match found {
                        Some(anim::STD_IDLE) => {
                            let a = if px.rng.number() & 1 != 0 { anim::STD_IDLE_HBHB } else { anim::STD_XPRESS_SCRATCH };
                            self.ped_blend_anim(r, assoc_group::STD, a, 4.0);
                            let t = px.rng.range_i32(1500, 4000);
                            self.set_look_timer(r, t);
                        }
                        Some(anim::STD_IDLE_HBHB) => {
                            if let Some(a) = self.ped_assoc_mut(r, anim::STD_IDLE_HBHB) {
                                a.blend_delta = -8.0;
                                a.flags |= aflags::DELETE_FADED_OUT;
                            }
                            let (a, g) = if px.rng.number() & 1 != 0 { (anim::STD_IDLE, group) } else { (anim::STD_XPRESS_SCRATCH, assoc_group::STD) };
                            self.ped_blend_anim(r, g, a, 4.0);
                            let t = px.rng.range_i32(1000, 2500);
                            self.set_look_timer(r, t);
                        }
                        _ => {
                            let (a, g) = if px.rng.number() & 1 != 0 { (anim::STD_IDLE, group) } else { (anim::STD_IDLE_HBHB, assoc_group::STD) };
                            self.ped_blend_anim(r, g, a, 4.0);
                            let t = px.rng.range_i32(1000, 2500);
                            self.set_look_timer(r, t);
                        }
                    }
                    self.say(px, r, sound::PED_CHAT_EVENT);
                }
            }
            event_type::ICECREAM | event_type::SHOPSTALL => {
                let angle = pd.angle_to_event;
                self.ped_mut(r).rotation_dest = angle;
                if now > self.ped(r).look_timer {
                    if self.ped(r).look_timer != 0 {
                        if let Some(a) = self.ped_assoc_mut(r, anim::STD_ROADCROSS) {
                            a.blend_delta = -8.0;
                            a.flags |= aflags::DELETE_FADED_OUT;
                            let play = if self.ped(r).event_type == event_type::ICECREAM { anim::STD_CHAT } else { anim::STD_XPRESS_SCRATCH };
                            self.ped_blend_anim(r, assoc_group::STD, play, 4.0);
                            let t = px.rng.range_i32(2000, 5000);
                            self.set_look_timer(r, t);
                        } else if let Some(a) = self.ped_assoc_mut(r, anim::STD_CHAT) {
                            a.blend_delta = -8.0;
                            a.flags |= aflags::DELETE_FADED_OUT;
                            self.clear_investigate_event(px, r);
                        } else {
                            if let Some(a) = self.ped_assoc_mut(r, anim::STD_XPRESS_SCRATCH) {
                                a.blend_delta = -8.0;
                                a.flags |= aflags::DELETE_FADED_OUT;
                            }
                            self.clear_investigate_event(px, r);
                        }
                    } else {
                        self.ped_blend_anim(r, assoc_group::STD, anim::STD_ROADCROSS, 4.0);
                        let t = px.rng.range_i32(1000, 2500);
                        self.set_look_timer(r, t);
                    }
                }
            }
            _ => {}
        }
    }

    /// `CPed::ClearInvestigateEvent`.
    pub fn clear_investigate_event(&mut self, px: &mut Px, r: EntityRef) {
        let found = [anim::STD_ROADCROSS, anim::STD_XPRESS_SCRATCH, anim::STD_IDLE_HBHB, anim::STD_CHAT].into_iter().find(|&a| self.ped_assoc(r, a).is_some());
        if let Some(a) = found.and_then(|id| self.ped_assoc_mut(r, id)) {
            a.blend_delta = -8.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
        let now = self.now();
        let pd = self.ped_mut(r);
        if pd.event_type > event_type::EXPLOSION {
            pd.chat_timer = now.wrapping_add(15000);
        }
        pd.flags.gonna_investigate_event = false;
        pd.event_entity = None;
        self.clear_look_flag(px, r);
        self.restore_previous_state(px, r);
        let pd = self.ped_mut(r);
        if matches!(pd.move_state, move_state::NONE | move_state::STILL) {
            pd.set_move_state(move_state::WALK);
        }
    }
}
