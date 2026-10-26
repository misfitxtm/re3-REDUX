//! `PedFight.cpp`: aiming and shooting a weapon, and starting a fist fight.
use super::callbacks as cb;
use super::ctx::Px;
use super::data::*;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::{EntityRef, EntityType};
use crate::general::{limit_radian_angle, radian_angle_between_points};
use crate::weapons::info::{flag as wflag, weapon_type as wt, WeaponFire};
use crate::weapons::weapon::WeaponState;
use crate::world::{Kinds, LineOpts, World};
use glam::Vec3;
use std::f32::consts::PI;

/// Does the ped have a ped on the ground to stamp on (`CheckForPedsOnGroundToAttack`)?
fn on_ground_state(w: &mut World, px: &mut Px, attacker: EntityRef) -> (u8, Option<EntityRef>) {
    if !px.hooks.nasty_game() {
        return (on_ground::NO_PED, None);
    }
    let (pos, near) = (w.epos(attacker), {
        let pd = w.ped(attacker);
        pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect::<Vec<_>>()
    });
    let (mut on_floor, mut dead, mut below) = (None, None, None);
    let (mut found_floor, mut found_dead, mut found_below) = (false, false, false);
    for cur in near {
        let cp = w.epos(cur);
        let distance = (cp - pos).length();
        if distance >= 2.0 {
            continue;
        }
        let angle_to_face = limit_radian_angle(radian_angle_between_points(cp.x, cp.y, pos.x, pos.y));
        let cur_rot = limit_radian_angle(w.ped(attacker).rotation_cur);
        w.ped_mut(attacker).rotation_cur = cur_rot;
        let mut angle_diff = (angle_to_face - cur_rot).abs();
        if angle_diff > PI {
            angle_diff = 2.0 * PI - angle_diff;
        }
        let cp_state = w.ped(cur).ped_state;
        if w.ped(cur).on_ground_or_getting_up() {
            if distance < 2.0 && angle_diff < 65.0_f32.to_radians() {
                if cp_state == state::DEAD {
                    found_dead = true;
                    dead.get_or_insert(cur);
                } else if !w.is_ped_head_above_pos(cur, -0.6) {
                    found_floor = true;
                    on_floor.get_or_insert(cur);
                }
            }
        } else if (distance < 0.8 && angle_diff < 75.0_f32.to_radians())
            || (distance < 1.3 && angle_diff < 55.0_f32.to_radians())
            || (distance < 1.7 && angle_diff < 35.0_f32.to_radians())
            || (distance < 2.0 && angle_diff < 30.0_f32.to_radians())
        {
            found_below = true;
            below = Some(cur);
            break;
        } else if angle_diff < 75.0_f32.to_radians() {
            found_below = true;
            below.get_or_insert(cur);
        }
    }
    if found_floor {
        (on_ground::ON_THE_FLOOR, on_floor)
    } else if found_dead {
        (on_ground::DEAD_ON_THE_FLOOR, dead)
    } else if found_below {
        (on_ground::IN_FRONT_OF_ATTACKER, below)
    } else {
        (on_ground::NO_PED, None)
    }
}

impl World {
    /// `CheckForPedsOnGroundToAttack`: what lies in front of the ped and which ped it is.
    pub(super) fn on_ground_state_of(&mut self, px: &mut Px, r: EntityRef) -> (u8, Option<EntityRef>) {
        on_ground_state(self, px, r)
    }

    /// The weapon type and info of the ped's current weapon.
    pub(super) fn current_weapon_info(&self, r: EntityRef) -> (usize, crate::weapons::info::WeaponInfo) {
        let pd = self.ped(r);
        let ty = pd.weapons[pd.current_weapon as usize].ty;
        (ty, self.ents.weapon_infos.get(ty).clone())
    }

    /// `CPed::SetPointGunAt`.
    pub fn set_point_gun_at(&mut self, px: &mut Px, r: EntityRef, to: impl Into<Option<EntityRef>>) {
        let to = to.into();
        if let Some(t) = to {
            self.set_look_flag_entity(r, t, true);
            self.set_aim_flag_entity(r, t);
        }
        let pd = self.ped(r);
        if pd.ped_state == state::AIM_GUN || pd.flags.is_ducking || pd.wait_state == wait_state::PLAYANIM_DUCK {
            return;
        }
        if pd.ped_state != state::ATTACK {
            self.set_stored_state(r);
        }
        let (_, info) = self.current_weapon_info(r);
        let pd = self.ped_mut(r);
        pd.ped_state = state::AIM_GUN;
        pd.flags.is_pointing_gun_at = true;
        pd.set_move_state(move_state::NONE);
        let crouch = self.ped(r).flags.crouch_when_shooting;
        let wanted = if crouch { info.anim2 } else { info.anim };
        if self.ped_assoc(r, wanted).is_none_or(|a| a.blend_delta < 0.0) {
            let i = if crouch { self.ped_blend_anim(r, assoc_group::STD, info.anim2, 4.0) } else { self.ped_add_anim(r, assoc_group::STD, info.anim) };
            if let Some(a) = i.and_then(|i| self.ped_assoc_at(r, i)) {
                a.blend_amount = 0.0;
                a.blend_delta = 8.0;
            }
        }
        if to.is_some() {
            self.say(px, r, sound::PED_ATTACK);
        }
    }

    /// `CPed::PointGunAt`: holds the aim pose.
    pub fn point_gun_at(&mut self, r: EntityRef) {
        let (_, info) = self.current_weapon_info(r);
        let mut id = info.anim;
        if self.ped_assoc(r, id).is_none_or(|a| a.blend_delta < 0.0) {
            id = info.anim2;
        }
        if let Some(a) = self.ped_assoc_mut(r, id)
            && a.current_time > info.anim_loop_start
        {
            a.set_current_time(info.anim_loop_start);
            a.flags &= !aflags::RUNNING;
            if info.is_flag_set(wflag::CANAIM_WITHARM) {
                self.ped_mut(r).ik.flags |= super::ik::ik_flag::AIMS_WITH_ARM;
            } else {
                self.ped_mut(r).ik.flags &= !super::ik::ik_flag::AIMS_WITH_ARM;
            }
        }
    }

    /// `CPed::ClearPointGunAt`.
    pub fn clear_point_gun_at(&mut self, px: &mut Px, r: EntityRef) {
        self.clear_look_flag(px, r);
        self.clear_aim_flag(r);
        self.ped_mut(r).flags.is_pointing_gun_at = false;
        if self.ped(r).ped_state == state::AIM_GUN {
            self.restore_previous_state(px, r);
            let (_, info) = self.current_weapon_info(r);
            let mut id = info.anim;
            if self.ped_assoc(r, id).is_none_or(|a| a.blend_delta < 0.0) {
                id = info.anim2;
            }
            if let Some(a) = self.ped_assoc_mut(r, id) {
                a.flags |= aflags::DELETE_FADED_OUT;
                a.blend_delta = -4.0;
            }
        }
    }

    /// `CWeapon::HitsGround` for the ped's current weapon: a building just below the line to the target.
    fn weapon_hits_ground(&mut self, r: EntityRef, aiming_to: Option<EntityRef>) -> bool {
        let Some(seek) = self.ped(r).seek_target else { return false };
        let (_, info) = self.current_weapon_info(r);
        let source = self.ent(r).base.matrix.transform_point(info.fire_offset + Vec3::new(0.0, 0.0, 0.6));
        let mut target = self.epos(aiming_to.unwrap_or(seek));
        target.z += 0.6;
        let kinds = Kinds { buildings: true, ..Kinds::NONE };
        let hit = self.process_line_of_sight(source, target, kinds, LineOpts::default());
        let building = hit.filter(|h| self.get(h.entity).is_some_and(|e| e.base.ty == EntityType::Building));
        crate::weapons::weapon::Weapon::hits_ground(source.z, building.map(|h| h.point.point.z))
    }

    /// `CPed::SetAttack`.
    pub fn set_attack(&mut self, px: &mut Px, r: EntityRef, victim: impl Into<Option<EntityRef>>) {
        let victim = victim.into();
        let now = self.now();
        let victim_ped = victim.filter(|&v| self.is_ped(v));
        if let Some(a) = self.ped_assoc_mut(r, anim::STD_IDLE_BIGGUN) {
            a.blend_delta = -1000.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
        let pd = self.ped(r);
        if pd.attack_timer > now || pd.wait_state == wait_state::SURPRISE {
            return;
        }
        if self.ped_assoc(r, anim::STD_HGUN_RELOAD).is_some() {
            self.ped_mut(r).flags.is_attacking = false;
            return;
        }
        if self.ped_assoc(r, anim::STD_AK_RELOAD).is_some() {
            let pd = self.ped_mut(r);
            let keep = pd.is_player() && pd.ped_state == state::ATTACK && !pd.player.as_ref().is_some_and(|p| p.have_target_selected);
            pd.flags.is_attacking = keep;
            return;
        }
        let (weapon_type, info) = self.current_weapon_info(r);
        if info.fire == WeaponFire::InstantHit && !self.ped(r).is_player() && self.weapon_hits_ground(r, victim) {
            return;
        }
        if weapon_type == wt::UNARMED {
            let pd = self.ped(r);
            let shopping = self.ents.ped_stats.0[pd.ped_stats].flags & super::types::stat_flag::SHOPPING_BAGS != 0;
            if pd.is_player() || (pd.ped_state != state::FIGHT && pd.move_state != move_state::NONE && pd.move_state != move_state::STILL && !shopping) {
                if pd.ped_state != state::ATTACK {
                    let pd = self.ped_mut(r);
                    pd.ped_state = state::ATTACK;
                    pd.flags.is_attacking = false;
                    self.start_attack_anim(r, info.anim, 8.0);
                }
            } else {
                let pressure = (px.rng.number() % 256) as u8;
                self.start_fight_attack(px, r, pressure);
            }
            return;
        }
        self.ped_mut(r).seek_target = victim;
        if info.is_flag_set(wflag::CANAIM) {
            let m = self.ent(r).base.matrix;
            let aim_pos = m.right * 0.1 + m.forward * 0.2 + m.pos;
            if self.test_sphere_against_world(aim_pos, 0.2, None, Kinds { buildings: true, objects: true, ..Kinds::NONE }, false).is_some() {
                return;
            }
            self.ped_mut(r).look_target = victim;
            if let Some(v) = victim {
                self.set_aim_flag_entity(r, v);
            } else {
                let rot = self.ped(r).rotation_cur;
                self.set_aim_flag_angle(r, rot);
                if self.ents.player_ped == Some(r) && px.hooks.player_weapon_first_person() {
                    let pitch = px.hooks.quick_aim_pitch();
                    self.ped_mut(r).player.get_or_insert_with(Default::default).fps_move_heading = pitch;
                }
            }
        } else if weapon_type == wt::FLAMETHROWER && px.hooks.player_weapon_first_person() && self.ents.player_ped == Some(r) {
            // FIX_BUGS: aim the flamethrower with the PC controls
            let rot = self.ped(r).rotation_cur;
            self.set_aim_flag_angle(r, rot);
            let pitch = px.hooks.quick_aim_pitch();
            self.ped_mut(r).player.get_or_insert_with(Default::default).fps_move_heading = pitch;
        }
        if self.ped(r).ped_state == state::ATTACK {
            self.ped_mut(r).flags.is_attacking = true;
            return;
        }
        let pd = self.ped(r);
        if pd.is_player() || victim_ped.is_none() || victim_ped.is_some_and(|v| self.ped(v).is_ped_in_control()) {
            if pd.is_player() {
                px.hooks.reset_average_weapon();
            }
            let first_person = px.hooks.player_weapon_first_person();
            let point_blank = if (info.fire == WeaponFire::InstantHit || weapon_type == wt::FLAMETHROWER) && !first_person { self.check_for_point_blank_peds(r, victim_ped) } else { point_blank::NONE };
            if point_blank != point_blank::NONE {
                self.clear_aim_flag(r);
                // this condition is pointless
                if point_blank == point_blank::FOR_WANTED_PED || victim_ped.is_none() {
                    self.start_fight_attack(px, r, 200);
                }
            } else {
                if !info.is_flag_set(wflag::CANAIM) {
                    self.ped_mut(r).seek_target = None;
                }
                if self.ped(r).ped_state != state::AIM_GUN {
                    self.set_stored_state(r);
                }
                let pd = self.ped_mut(r);
                pd.ped_state = state::ATTACK;
                pd.set_move_state(move_state::NONE);
                if self.ped(r).flags.crouch_when_shooting {
                    self.start_attack_anim(r, anim::STD_RBLOCK_SHOOT, 4.0);
                } else {
                    let delta = if info.fire == WeaponFire::Melee { 1000.0 } else { 8.0 };
                    let (ground, _) = on_ground_state(self, px, r);
                    let id = if weapon_type != wt::BASEBALLBAT || ground < on_ground::ON_THE_FLOOR { info.anim } else { info.anim2 };
                    self.start_attack_anim(r, id, delta);
                }
            }
            return;
        }
        if weapon_type == wt::BASEBALLBAT && victim_ped.is_some_and(|v| self.ped(v).ped_state == state::GETUP) {
            self.set_wait_state(px, r, wait_state::SURPRISE, None);
        }
        if let Some(v) = victim {
            self.set_look_flag_entity(r, v, false);
        }
        self.set_look_timer(r, 100);
    }

    /// Blends an attack animation in and runs it from its start with the finish callback.
    fn start_attack_anim(&mut self, r: EntityRef, id: u32, delta: f32) {
        let i = self.ped_blend_anim(r, assoc_group::STD, id, delta);
        if let Some(a) = i.and_then(|i| self.ped_assoc_at(r, i)) {
            a.flags |= aflags::RUNNING;
            if a.current_time == a.hierarchy.total_length {
                a.set_current_time(0.0);
            }
            a.set_finish_callback(cb::FINISHED_ATTACK);
        }
    }

    /// `CPed::ClearAttack`.
    pub fn clear_attack(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if pd.ped_state != state::ATTACK || pd.flags.is_ducking || pd.wait_state == wait_state::PLAYANIM_DUCK {
            return;
        }
        if pd.flags.is_pointing_gun_at {
            match pd.look_target {
                Some(t) => self.set_point_gun_at(px, r, Some(t)),
                None => self.clear_point_gun_at(px, r),
            }
        } else if pd.objective != objective::NONE {
            self.set_idle(px, r);
        } else {
            self.restore_previous_state(px, r);
        }
    }

    /// `CPed::ClearAttackByRemovingAnim`.
    pub fn clear_attack_by_removing_anim(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        if pd.ped_state != state::ATTACK || pd.flags.is_ducking {
            return;
        }
        let (_, info) = self.current_weapon_info(r);
        let mut id = Some(info.anim).filter(|&a| self.ped_assoc(r, a).is_some());
        if id.is_none() {
            id = Some(info.anim2).filter(|&a| self.ped_assoc(r, a).is_some());
            if id.is_none() && info.is_flag_set(wflag::THROW) {
                id = Some(anim::STD_THROW_UNDER).filter(|&a| self.ped_assoc(r, a).is_some());
            }
            if id.is_none() {
                self.clear_attack(px, r);
                return;
            }
        }
        if let Some(a) = id.and_then(|id| self.ped_assoc_mut(r, id)) {
            a.blend_delta = -8.0;
            a.flags &= !aflags::RUNNING;
            a.flags |= aflags::DELETE_FADED_OUT;
            a.set_delete_callback(cb::FINISHED_ATTACK);
        }
    }

    /// `CPed::FinishedAttackCB`; the animation is `None` when the original is called without one.
    pub fn finished_attack_cb(&mut self, px: &mut Px, r: EntityRef, attack_anim: Option<i32>) {
        if let Some(id) = attack_anim {
            let kind = id as u32;
            match kind {
                anim::STD_START_THROW => {
                    // (the original tests the same thing twice)
                    let pd = self.ped(r);
                    let selected = pd.player.as_ref().is_some_and(|p| p.have_target_selected);
                    let next = if (!pd.is_player() || selected) && pd.is_player() { anim::STD_THROW_UNDER } else { anim::STD_WEAPON_THROW };
                    if let Some(a) = self.ped_assoc_mut(r, kind) {
                        a.blend_delta = -1000.0;
                    }
                    if let Some(a) = self.ped_add_anim(r, assoc_group::STD, next).and_then(|i| self.ped_assoc_at(r, i)) {
                        a.set_finish_callback(cb::FINISHED_ATTACK);
                    }
                    return;
                }
                anim::STD_PARTIAL_PUNCH => {
                    if let Some(a) = self.ped_assoc_mut(r, kind) {
                        a.blend_delta = -8.0;
                        a.flags |= aflags::DELETE_FADED_OUT;
                    }
                    self.clear_attack(px, r);
                    return;
                }
                anim::STD_WEAPON_THROW | anim::STD_THROW_UNDER => {
                    let (ty, info) = self.current_weapon_info(r);
                    let pd = self.ped(r);
                    if pd.weapons[pd.current_weapon as usize].ammo_total > 0 {
                        let _ = ty;
                        self.add_weapon_model(r, info.model_id);
                    }
                }
                _ => {}
            }
        }
        if !self.ped(r).flags.is_attacking {
            self.clear_attack(px, r);
        }
    }

    /// `CPed::CheckForPointBlankPeds`.
    pub fn check_for_point_blank_peds(&mut self, r: EntityRef, ped_to_verify: Option<EntityRef>) -> u8 {
        let pd = self.ped(r);
        let pb_distance = if pd.weapons[pd.current_weapon as usize].is_type_2_handed() { 1.6 } else { 1.1 };
        let near: Vec<EntityRef> = pd.near_peds.iter().take(pd.num_near_peds as usize).flatten().copied().collect();
        let pos = self.epos(r);
        for n in near {
            if ped_to_verify.is_some_and(|v| v != n) {
                continue;
            }
            let np = self.epos(n);
            if (np - pos).length() >= pb_distance {
                continue;
            }
            let needed = limit_radian_angle(radian_angle_between_points(np.x, np.y, pos.x, pos.y));
            let cur = limit_radian_angle(self.ped(r).rotation_cur);
            self.ped_mut(r).rotation_cur = cur;
            let mut turn = (needed - cur).abs();
            if turn > PI {
                turn = 2.0 * PI - turn;
            }
            if self.ped(n).on_ground_or_getting_up() || self.ped(n).ped_state == state::DIVE_AWAY {
                return point_blank::NONE;
            }
            if turn < CAN_SEE_ENTITY_ANGLE_THRESHOLD {
                return if ped_to_verify == Some(n) { point_blank::FOR_WANTED_PED } else { point_blank::FOR_SOMEONE_ELSE };
            }
        }
        point_blank::NONE
    }

    /// `CPed::Attack`: advances a shooting or swinging attack each frame.
    pub fn attack(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let (weapon_type, info) = self.current_weapon_info(r);
        let mut attack_should_continue = self.ped(r).flags.is_attacking;
        let mut delay_between_anim_and_fire = info.anim_frame_fire;
        let weapon_anim = info.anim;
        let reload_anim = if weapon_anim == anim::STD_WEAPON_HGUN_BODY {
            Some(anim::STD_HGUN_RELOAD)
        } else if weapon_anim == anim::STD_WEAPON_AK_BODY {
            Some(anim::STD_AK_RELOAD)
        } else {
            None
        };
        let reloading_anim = reload_anim.is_some_and(|a| self.ped_assoc(r, a).is_some());
        if self.ped(r).flags.is_ducking {
            return;
        }
        if reloading_anim {
            let pd = self.ped(r);
            if !pd.is_player() || pd.player.as_ref().is_some_and(|p| p.have_target_selected) {
                self.clear_attack(px, r);
            }
            return;
        }
        if now < self.ped(r).shoot_timer {
            attack_should_continue = true;
        }
        let mut anim_id = Some(info.anim).filter(|&a| self.ped_assoc(r, a).is_some());
        if anim_id.is_none() {
            anim_id = Some(info.anim2).filter(|&a| self.ped_assoc(r, a).is_some());
            delay_between_anim_and_fire = info.anim2_frame_fire;
            // the long throw of a grenade or molotov
            if anim_id.is_none() && info.is_flag_set(wflag::THROW) {
                anim_id = Some(anim::STD_THROW_UNDER).filter(|&a| self.ped_assoc(r, a).is_some());
                delay_between_anim_and_fire = 0.2;
            }
            if anim_id.is_none() {
                if attack_should_continue {
                    let pd = self.ped(r);
                    let selected = pd.player.as_ref().is_some_and(|p| p.have_target_selected);
                    if info.fire != WeaponFire::Projectile || !pd.is_player() || selected {
                        let nasty = px.hooks.nasty_game();
                        let melee_ground = nasty && info.fire == WeaponFire::Melee && on_ground_state(self, px, r).0 >= on_ground::ON_THE_FLOOR;
                        let id = if !nasty || info.fire != WeaponFire::Melee || !melee_ground { info.anim } else { info.anim2 };
                        self.start_attack_anim(r, id, 8.0);
                        if self.ped(r).is_player() {
                            let p = self.ped_mut(r).player.get_or_insert_with(Default::default);
                            p.attack_button_counter = 0.0;
                            p.have_target_selected = false;
                        }
                    }
                } else {
                    self.finished_attack_cb(px, r, None);
                }
                return;
            }
        }
        let id = anim_id.expect("checked");
        let snapshot = |w: &World| w.ped_assoc(r, id).map(|a| (a.current_time, a.time_step, a.is_running(), a.speed, a.anim_id as u32));
        let Some((time, step, running, speed, assoc_id)) = snapshot(self) else { return };
        if time > info.anim_loop_start && time - step <= info.anim_loop_start {
            if info.is_flag_set(wflag::CANAIM_WITHARM) {
                self.ped_mut(r).ik.flags |= super::ik::ik_flag::AIMS_WITH_ARM;
            } else {
                self.ped_mut(r).ik.flags &= !super::ik::ik_flag::AIMS_WITH_ARM;
            }
        }
        if time <= delay_between_anim_and_fire || time - step > delay_between_anim_and_fire || !running {
            if speed < 1.0
                && let Some(a) = self.ped_assoc_mut(r, id)
            {
                a.speed = 1.0;
            }
        } else {
            let mut fire_pos = info.fire_offset;
            let m = self.ent(r).base.matrix;
            if weapon_type == wt::BASEBALLBAT {
                if assoc_id == info.anim2 {
                    fire_pos.z = 0.7 * info.radius - 1.0;
                }
                fire_pos = m.transform_point(fire_pos);
            } else if weapon_type != wt::UNARMED {
                let node = if assoc_id == anim::STD_KICKGROUND { node::FOOTR } else { node::HANDR };
                let node_m = self.ped_node_position(r, node);
                fire_pos = node_m + (m.right * fire_pos.x + m.forward * fire_pos.y + m.up * fire_pos.z);
            } else {
                fire_pos = m.transform_point(fire_pos);
            }
            self.fire_weapon(px, r, fire_pos);
            if matches!(weapon_type, wt::MOLOTOV | wt::GRENADE) {
                self.remove_weapon_model(r);
            }
            let w = {
                let pd = self.ped(r);
                pd.weapons[pd.current_weapon as usize].clone()
            };
            if w.ammo_total == 0 && info.fire != WeaponFire::Melee && self.ents.player_ped != Some(r) {
                self.select_gun_if_armed(r);
            }
            if w.state != WeaponState::MeleeMadeContact {
                // reloading just began: start the reload animation
                if w.state == WeaponState::Reloading
                    && let Some(rel) = reload_anim
                {
                    self.ped_blend_anim(r, assoc_group::STD, rel, 8.0);
                    self.clear_look_flag(px, r);
                    self.clear_aim_flag(r);
                    let pd = self.ped_mut(r);
                    pd.flags.is_attacking = false;
                    pd.flags.is_pointing_gun_at = false;
                    pd.shoot_timer = now;
                    return;
                }
            } else {
                if assoc_id == anim::STD_WEAPON_BAT_V || assoc_id == anim::STD_WEAPON_BAT_H {
                    px.hooks.play_one_shot(r, sound::WEAPON_BAT_ATTACK, 1.0);
                } else if assoc_id == anim::STD_PARTIAL_PUNCH {
                    px.hooks.play_one_shot(r, sound::FIGHT_PUNCH_39, 0.0);
                }
                let keep = self.ped(r).flags.is_attacking || now < self.ped(r).shoot_timer;
                if let Some(a) = self.ped_assoc_mut(r, id) {
                    a.speed = 0.5;
                    if keep {
                        a.callback = crate::anim::CallbackKind::None;
                    }
                }
            }
            attack_should_continue = false;
        }
        if weapon_type == wt::SHOTGUN {
            let (time, step, running) = self.ped_assoc(r, id).map_or((0.0, 0.0, false), |a| (a.current_time, a.time_step, a.is_running()));
            if time > 1.0 && time - step <= 1.0 && running {
                let m = self.ent(r).base.matrix;
                let hand = self.ped_node_position(r, node::HANDR);
                let f = info.fire_offset;
                let fire_pos = hand + (m.right * f.x + m.forward * f.y + m.up * f.z);
                let shell_pos = Vec3::new(fire_pos.x - 0.6 * m.forward.x, fire_pos.y - 0.6 * m.forward.y, fire_pos.z - 0.15 * m.up.z);
                let rot = glam::Vec2::new(m.right.x, m.right.y).normalize_or_zero();
                px.hooks.add_gunshell(shell_pos, rot, 0.025);
            }
        }
        let mut anim_loop_end = info.anim_loop_end;
        if info.fire == WeaponFire::Melee && assoc_id == info.anim2 {
            anim_loop_end = 3.4 / 6.0;
        }
        let (time, step, running) = self.ped_assoc(r, id).map_or((0.0, 0.0, false), |a| (a.current_time, a.time_step, a.is_running()));
        // end of the loop: either start it again or finish the attack
        if time > anim_loop_end || (!running && info.fire != WeaponFire::Projectile) {
            let w_state = {
                let pd = self.ped(r);
                pd.weapons[pd.current_weapon as usize].state
            };
            if time - 2.0 * step <= anim_loop_end && (self.ped(r).flags.is_attacking || now < self.ped(r).shoot_timer) && w_state != WeaponState::Reloading {
                let melee_ground = info.fire == WeaponFire::Melee && on_ground_state(self, px, r).0 >= on_ground::ON_THE_FLOOR;
                if info.fire != WeaponFire::Melee || !melee_ground {
                    if assoc_id != info.anim2 || assoc_id == anim::STD_RBLOCK_SHOOT {
                        if let Some(a) = self.ped_assoc_mut(r, id) {
                            a.start(info.anim_loop_start);
                        }
                    } else {
                        self.ped_blend_anim(r, assoc_group::STD, info.anim, 8.0);
                    }
                } else if assoc_id == info.anim2 {
                    if let Some(a) = self.ped_assoc_mut(r, id) {
                        a.set_current_time(0.1);
                    }
                } else {
                    self.ped_blend_anim(r, assoc_group::STD, info.anim2, 8.0);
                }
            } else {
                self.clear_aim_flag(r);
                // the echo of the bullets at the end of the attack (not played while reloading)
                if time - step <= info.anim_loop_end {
                    let echo = match weapon_type {
                        wt::UZI => Some(sound::WEAPON_UZI_BULLET_ECHO),
                        wt::AK47 => Some(sound::WEAPON_AK47_BULLET_ECHO),
                        wt::M16 => Some(sound::WEAPON_M16_BULLET_ECHO),
                        _ => None,
                    };
                    if let Some(s) = echo {
                        px.hooks.play_one_shot(r, s, 0.0);
                    }
                }
                // removing this makes the flamethrower reload
                if weapon_type == wt::FLAMETHROWER
                    && let Some(a) = self.ped_assoc_mut(r, id)
                    && a.is_running()
                {
                    a.flags |= aflags::DELETE_FADED_OUT;
                    a.flags &= !aflags::RUNNING;
                    a.blend_delta = -4.0;
                }
            }
        }
        if self.ped_assoc(r, id).is_some_and(|a| a.current_time > delay_between_anim_and_fire) {
            attack_should_continue = false;
        }
        self.ped_mut(r).flags.is_attacking = attack_should_continue;
    }

    /// `CPed::StartFightAttack`.
    pub fn start_fight_attack(&mut self, px: &mut Px, r: EntityRef, button_pressure: u8) {
        let now = self.now();
        let pd = self.ped(r);
        if !pd.is_ped_in_control() || pd.attack_timer > now {
            return;
        }
        if pd.ped_state == state::FIGHT {
            self.ped_mut(r).fight_button_pressure = button_pressure;
            return;
        }
        if pd.ped_state != state::AIM_GUN {
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
        let run_stop = self.ped_assoc(r, anim::STD_RUNSTOP1).map(|_| anim::STD_RUNSTOP1).or_else(|| self.ped_assoc(r, anim::STD_RUNSTOP2).map(|_| anim::STD_RUNSTOP2));
        if let Some(id) = run_stop {
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
        let (ground_state, on_ground_ped) = if self.ped(r).is_player() { on_ground_state(self, px, r) } else { (on_ground::NO_PED, None) };
        let shopping = self.ents.ped_stats.0[self.ped(r).ped_stats].flags & super::types::stat_flag::SHOPPING_BAGS != 0;
        let fight_move = if self.ped(r).is_player() && ground_state > on_ground::IN_FRONT_OF_ATTACKER {
            fight_move::GROUNDKICK
        } else if shopping {
            fight_move::ROUNDHOUSE
        } else {
            fight_move::STDPUNCH
        };
        self.ped_mut(r).cur_fight_move = fight_move;
        if let (true, Some(g)) = (self.ped(r).is_player(), on_ground_ped) {
            let (gp, pos) = (self.epos(g), self.epos(r));
            let dest = limit_radian_angle(radian_angle_between_points(gp.x, gp.y, pos.x, pos.y));
            let pd = self.ped_mut(r);
            pd.rotation_dest = dest;
            pd.rotation_cur = dest;
            pd.look_timer = 0;
            self.set_look_flag_entity(r, g, true);
            self.set_look_timer(r, 1500);
        }
        let move_anim = self.ents.fight_moves.get(fight_move as usize).map_or(anim::STD_NUM, |m| m.anim_id);
        if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, move_anim, 4.0).and_then(|i| self.ped_assoc_at(r, i)) {
            a.set_finish_callback(cb::FINISH_FIGHT_MOVE);
        }
        let pd = self.ped_mut(r);
        pd.fight_state = fight_state::NO_MOVE;
        pd.take_a_step_after_attack = false;
        pd.flags.is_attacking = true;
        if pd.is_player() {
            pd.player.get_or_insert_with(Default::default).combo_move = 0;
        }
    }
}
