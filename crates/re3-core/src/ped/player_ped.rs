//! `PlayerPed.cpp`, first part: making the player, stamina, weapon selection and the movement animations.
use super::callbacks;
use super::ctx::{cam_mode, PadState, Px};
use super::data::*;
use super::player::PlayerData;
use super::types::ped_type;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::modelinfo::indices_gta3::mi;
use crate::weapons::info::{flag as weapon_flag, weapon_type as wt, WeaponFire};
use crate::world::{Kinds, World, WorldError};
use glam::Vec3;

impl World {
    pub(super) fn player_data(&self, r: EntityRef) -> &PlayerData {
        self.ped(r).player.as_deref().expect("player data")
    }

    pub(super) fn player_data_mut(&mut self, r: EntityRef) -> &mut PlayerData {
        self.ped_mut(r).player.get_or_insert_with(Default::default)
    }

    /// `new CPlayerPed()` with the model's skeleton at hand.
    pub fn create_player(&mut self, px: &mut Px, clump: Option<super::clump::PedClump>) -> Option<EntityRef> {
        let r = self.create_ped(px, ped_type::PLAYER1, mi::PLAYER, clump)?;
        let pd = self.ped_mut(r);
        pd.class = PedClass::Player;
        pd.player = Some(Box::default());
        self.set_initial_state(px, r);
        let pd = self.ped_mut(r);
        pd.current_weapon = wt::UNARMED as u8;
        pd.point_gun_at = None;
        pd.ped_state = state::IDLE;
        let p = self.player_data_mut(r);
        p.selected_wep_slot = wt::UNARMED;
        p.speed_timer = 0;
        p.speed_timer_flag = false;
        p.current_stamina = 150.0;
        p.max_stamina = 150.0;
        p.stamina_progress = 0.0;
        p.evade_amount = 0;
        p.attack_button_counter = 0.0;
        p.have_target_selected = false;
        p.has_lock_on_target = false;
        p.can_be_damaged = true;
        p.walk_angle = 0.0;
        p.fps_move_heading = 0.0;
        p.targettable_objects = [None; 4];
        Some(r)
    }

    /// `CPlayerPed::SetupPlayerPed`: makes the player, puts them in the world and makes them the one in focus.
    pub fn setup_player_ped(&mut self, px: &mut Px, clump: Option<super::clump::PedClump>) -> Result<EntityRef, WorldError> {
        let r = self.create_player(px, clump).ok_or(WorldError::BadEntity)?;
        self.ents.player_ped = Some(r);
        self.get_mut(r).expect("player").base.set_orientation(0.0, 0.0, 0.0);
        self.add(r)?;
        self.ped_mut(r).wep_accuracy = 100;
        Ok(r)
    }

    /// `CPlayerPed::DeactivatePlayerPed`.
    pub fn deactivate_player_ped(&mut self) {
        if let Some(r) = self.ents.player_ped {
            self.remove(r);
        }
    }

    /// `CPlayerPed::ReactivatePlayerPed`.
    pub fn reactivate_player_ped(&mut self) -> Result<(), WorldError> {
        match self.ents.player_ped {
            Some(r) => self.add(r),
            None => Ok(()),
        }
    }

    /// `CPlayerPed::ClearWeaponTarget`.
    pub fn clear_weapon_target(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).ped_type == ped_type::PLAYER1 {
            self.set_weapon_lock_on_target(r, None);
            px.hooks.clear_player_weapon_mode();
            px.hooks.clear_cross_hair();
        }
        self.clear_point_gun_at(px, r);
    }

    /// `CPlayerPed::MakeObjectTargettable`.
    pub fn make_object_targettable(&mut self, r: EntityRef, object: EntityRef) {
        let slots = self.player_data(r).targettable_objects;
        // FIX_BUGS: an empty slot is free as well as one whose object is gone
        if let Some(i) = slots.iter().position(|s| s.is_none_or(|o| self.get(o).is_none())) {
            self.player_data_mut(r).targettable_objects[i] = Some(object);
        }
    }

    /// `CPlayerPed::AnnoyPlayerPed`.
    #[allow(clippy::if_same_then_else)] // the two temper steps mirror the original's cases
    pub fn annoy_player_ped(&mut self, r: EntityRef, annoyed_by_passing_entity: bool) {
        let stat = self.ped(r).ped_stats;
        let temper = &mut self.ents.ped_stats.0[stat].temper;
        if *temper < 52 {
            *temper += 1;
        } else if annoyed_by_passing_entity && *temper < 55 {
            *temper += 1;
        } else if annoyed_by_passing_entity {
            *temper = 46;
        }
    }

    /// `CPlayerPed::ClearAdrenaline`, for the player in focus.
    pub fn clear_adrenaline(&mut self, px: &mut Px) {
        let Some(r) = self.ents.player_ped else { return };
        let p = self.player_data_mut(r);
        if p.adrenaline_active && p.adrenaline_time != 0 {
            p.adrenaline_time = 0;
            px.hooks.set_time_scale(1.0);
        }
    }

    /// `CPlayerPed::UseSprintEnergy`.
    pub fn use_sprint_energy(&mut self, r: EntityRef) {
        let step = self.time_step();
        let infinite = self.ents.player_info.infinite_sprint;
        let p = self.player_data_mut(r);
        if p.current_stamina > -150.0 && !infinite && !p.adrenaline_active {
            p.current_stamina -= step;
            p.stamina_progress += step;
        }
        if p.stamina_progress >= 500.0 {
            p.stamina_progress = 0.0;
            if p.max_stamina < 1000.0 {
                p.max_stamina += 10.0;
            }
        }
    }

    /// `CPlayerPed::RestoreSprintEnergy`.
    pub fn restore_sprint_energy(&mut self, r: EntityRef, speed: f32) {
        let step = self.time_step();
        let p = self.player_data_mut(r);
        if p.current_stamina < p.max_stamina {
            p.current_stamina += speed * step * 0.5;
        }
    }

    /// `CPlayerPed::MakeChangesForNewWeapon`.
    pub fn make_changes_for_new_weapon(&mut self, px: &mut Px, r: EntityRef, weapon: usize) {
        if self.ped(r).ped_state == state::SNIPER_MODE {
            self.restore_previous_state(px, r);
            px.hooks.clear_player_weapon_mode();
        }
        self.set_current_weapon(r, weapon);
        let pd = self.ped(r);
        let ty = pd.weapons[pd.current_weapon as usize].ty;
        let info = self.ents.weapon_infos.get(ty);
        let (clip, can_aim) = (info.ammo, info.is_flag_set(weapon_flag::CANAIM));
        let pd = self.ped_mut(r);
        let w = &mut pd.weapons[pd.current_weapon as usize];
        w.ammo_in_clip = w.ammo_total.min(clip);
        if !can_aim {
            self.clear_weapon_target(px, r);
        }
        let sniper_anim = self.ents.weapon_infos.get(wt::SNIPERRIFLE).anim;
        if let Some(a) = self.ped_assoc_mut(r, sniper_anim) {
            a.flags |= aflags::RUNNING | aflags::FADE_OUT_WHEN_DONE;
        }
        px.hooks.clear_player_weapon_mode();
    }

    /// `CPlayerPed::ReApplyMoveAnims`: swaps the move animations over to the ones of the current group.
    pub fn re_apply_move_anims(&mut self, r: EntityRef) {
        let group = self.ped(r).anim_group as usize;
        for id in [anim::STD_WALK, anim::STD_RUN, anim::STD_RUNFAST, anim::STD_IDLE, anim::STD_STARTWALK] {
            let Some((name, blend_delta, blend_amount)) = self.ped_assoc(r, id).map(|a| (a.hierarchy.name.clone(), a.blend_delta, a.blend_amount)) else { continue };
            let differs = self.ents.anim.assoc(group, id as usize).is_some_and(|g| g.hierarchy.name != name);
            if !differs {
                continue;
            }
            // the new association goes to the front of the list, so the old one is looked up by id afterwards
            let Some(new_index) = self.ped_add_anim(r, group, id) else { continue };
            if let Some(a) = self.ped_assoc_at(r, new_index) {
                a.blend_delta = blend_delta;
                a.blend_amount = blend_amount;
            }
            if let Some(old) = self.ped_clump_mut(r).and_then(|c| c.anim.associations.iter_mut().skip(new_index + 1).find(|a| a.anim_id == id as i32)) {
                old.blend_delta = -1000.0;
                old.flags |= aflags::DELETE_FADED_OUT;
            }
        }
    }

    /// `CPlayerPed::SetInitialState`.
    pub fn set_initial_state(&mut self, px: &mut Px, r: EntityRef) {
        px.hooks.set_time_scale(1.0);
        let p = self.player_data_mut(r);
        p.adrenaline_active = false;
        p.adrenaline_time = 0;
        let pd = self.ped_mut(r);
        pd.seek_target = None;
        pd.seek_pos = Vec3::ZERO;
        pd.flee_from_pos_x = 0.0;
        pd.flee_from_pos_y = 0.0;
        pd.flee_from = None;
        pd.flee_timer = 0;
        pd.objective = objective::NONE;
        pd.prev_objective = objective::NONE;
        self.get_mut(r).expect("player").base.flags.uses_collision = true;
        self.clear_aim_flag(r);
        self.clear_look_flag(px, r);
        let pd = self.ped_mut(r);
        pd.flags.is_pointing_gun_at = false;
        pd.flags.render_ped_in_car = true;
        if let Some(fire) = pd.fire {
            self.extinguish_fire_index(px, fire);
        }
        if let Some(c) = self.ped_clump_mut(r) {
            c.anim.remove_associations(0);
        }
        let pd = self.ped_mut(r);
        pd.ped_state = state::IDLE;
        pd.set_move_state(move_state::STILL);
        pd.last_ped_state = state::NONE;
        pd.anim_group = assoc_group::PLAYER as u32;
        let p = self.player_data_mut(r);
        p.move_speed = 0.0;
        p.selected_wep_slot = wt::UNARMED;
        p.evade_amount = 0;
        p.evading_from = None;
        self.ped_mut(r).flags.is_ped_die_anim_playing = false;
        self.set_real_move_anim(px, r);
        self.player_data_mut(r).can_be_damaged = true;
        let stat = self.ped(r).ped_stats;
        self.ents.ped_stats.0[stat].temper = 50;
        self.player_data_mut(r).walk_angle = 0.0;
    }

    fn fade_out_anim(&mut self, r: EntityRef, id: u32, delta: f32) {
        if let Some(a) = self.ped_assoc_mut(r, id) {
            a.blend_delta = delta;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
    }

    /// The player stands, gets tired or not: starts the idle animation that fits.
    fn start_idle(&mut self, r: EntityRef, rate: f32, rng: &mut crate::general::Rng) -> u32 {
        let pos = self.epos(r);
        let tired = self.player_data(r).current_stamina < 0.0 && self.test_sphere_against_world(pos, 0.5, None, Kinds { buildings: true, ..Kinds::NONE }, false).is_none();
        let group = self.ped(r).anim_group as usize;
        let id = if tired {
            self.ped_blend_anim(r, assoc_group::STD, anim::STD_IDLE_TIRED, rate);
            anim::STD_IDLE_TIRED
        } else {
            self.ped_blend_anim(r, group, anim::STD_IDLE, rate);
            anim::STD_IDLE
        };
        let now = self.now();
        self.ped_mut(r).wait_timer = now + rng.range_i32(2500, 4000) as u32;
        id
    }

    /// `CPlayerPed::SetRealMoveAnim`: blends walk, run, sprint and idle by how fast the pad is pushed.
    pub fn set_real_move_anim(&mut self, px: &mut Px, r: EntityRef) {
        let has = |w: &World, id: u32| w.ped_assoc(r, id).is_some();
        if self.ped(r).flags.reset_walk_anims {
            for id in [anim::STD_WALK, anim::STD_RUN, anim::STD_RUNFAST] {
                if let Some(a) = self.ped_assoc_mut(r, id) {
                    a.set_current_time(0.0);
                }
            }
            self.ped_mut(r).flags.reset_walk_anims = false;
        }
        let mut idle = [anim::STD_IDLE, anim::STD_IDLE_TIRED, anim::STD_FIGHT_IDLE].into_iter().find(|&i| has(self, i));
        let running = |w: &World, id: u32| w.ped_assoc(r, id).is_some_and(|a| a.is_running());
        let group = self.ped(r).anim_group as usize;
        let move_speed = self.player_data(r).move_speed;
        if !(running(self, anim::STD_RUNSTOP1) || running(self, anim::STD_RUNSTOP2)) {
            let stop_going = |w: &World, id: u32| w.ped_assoc(r, id).is_some_and(|a| a.blend_delta >= 0.0);
            let has_sprint = has(self, anim::STD_RUNFAST);
            if stop_going(self, anim::STD_RUNSTOP1) || stop_going(self, anim::STD_RUNSTOP2) {
                let stop = if has(self, anim::STD_RUNSTOP1) { anim::STD_RUNSTOP1 } else { anim::STD_RUNSTOP2 };
                if let Some(a) = self.ped_assoc_mut(r, stop) {
                    a.flags |= aflags::DELETE_FADED_OUT;
                    a.blend_amount = 1.0;
                    a.blend_delta = -8.0;
                }
                self.restore_heading_rate(r);
                if idle.is_none() {
                    idle = Some(self.start_idle(r, 8.0, px.rng));
                }
                if let Some(a) = idle.and_then(|i| self.ped_assoc_mut(r, i)) {
                    a.blend_amount = 0.0;
                    a.blend_delta = 8.0;
                }
            } else if move_speed == 0.0 && !has_sprint {
                if idle.is_none() {
                    idle = Some(self.start_idle(r, 4.0, px.rng));
                }
                let pos = self.epos(r);
                let stamina = self.player_data(r).current_stamina;
                let tired_spot = |w: &mut World| stamina < 0.0 && w.test_sphere_against_world(pos, 0.5, None, Kinds { buildings: true, ..Kinds::NONE }, false).is_none();
                if stamina > 0.0 && idle == Some(anim::STD_IDLE_TIRED) {
                    self.ped_blend_anim(r, group, anim::STD_IDLE, 4.0);
                } else if self.ped(r).ped_state != state::FIGHT {
                    if idle != Some(anim::STD_IDLE_TIRED) && tired_spot(self) {
                        self.ped_blend_anim(r, assoc_group::STD, anim::STD_IDLE_TIRED, 4.0);
                    } else if idle != Some(anim::STD_IDLE) {
                        self.ped_blend_anim(r, group, anim::STD_IDLE, 4.0);
                    }
                }
                self.ped_mut(r).move_state = move_state::STILL;
            } else {
                self.move_anims_moving(r, idle, move_speed);
            }
        }
        self.apply_adrenaline_to_move_anims(px, r);
    }

    /// The moving branch of `SetRealMoveAnim`.
    fn move_anims_moving(&mut self, r: EntityRef, idle: Option<u32>, move_speed: f32) {
        let has = |w: &World, id: u32| w.ped_assoc(r, id).is_some();
        let group = self.ped(r).anim_group as usize;
        if idle.is_some() {
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_STARTWALK) {
                a.blend_amount = 1.0;
                a.blend_delta = 0.0;
            } else {
                self.ped_add_anim(r, group, anim::STD_STARTWALK);
            }
            for id in [anim::STD_WALK, anim::STD_RUN] {
                if let Some(a) = self.ped_assoc_mut(r, id) {
                    a.set_current_time(0.0);
                }
            }
            for id in [idle.unwrap_or(anim::STD_IDLE), anim::STD_IDLE_TIRED, anim::STD_FIGHT_IDLE, anim::STD_RUNFAST] {
                self.ped_remove_assoc(r, id);
            }
            self.ped_mut(r).move_state = move_state::WALK;
        }
        for stop in [anim::STD_RUNSTOP1, anim::STD_RUNSTOP2] {
            if has(self, stop) {
                self.ped_remove_assoc(r, stop);
                self.restore_heading_rate(r);
            }
        }
        for id in [anim::STD_WALK, anim::STD_RUN] {
            if !has(self, id) {
                self.ped_add_anim(r, group, id);
                if let Some(a) = self.ped_assoc_mut(r, id) {
                    a.blend_amount = 0.0;
                }
            }
        }
        if self.ped_assoc(r, anim::STD_STARTWALK).is_some_and(|a| !a.is_running()) {
            self.ped_remove_assoc(r, anim::STD_STARTWALK);
            for id in [anim::STD_WALK, anim::STD_RUN] {
                if let Some(a) = self.ped_assoc_mut(r, id) {
                    a.flags |= aflags::RUNNING;
                }
            }
        }
        let stamina = self.player_data(r).current_stamina;
        if self.ped(r).move_state == move_state::SPRINT {
            let sprint_fading = self.ped_assoc(r, anim::STD_RUNFAST).is_none_or(|a| a.blend_delta < 0.0);
            if stamina < 0.0 && (stamina <= -150.0 || sprint_fading) || has(self, anim::STD_STARTWALK) {
                self.ped_mut(r).move_state = move_state::STILL;
            }
        }
        let sprint = self.ped_assoc(r, anim::STD_RUNFAST).map(|a| (a.blend_amount, a.blend_delta, a.current_time / a.hierarchy.total_length));
        let move_state_now = self.ped(r).move_state;
        if let Some((amount, delta, progress)) = sprint.filter(|_| move_state_now != move_state::SPRINT || move_speed < 0.4) {
            if amount == 0.0 {
                self.fade_out_anim(r, anim::STD_RUNFAST, -1000.0);
            } else if delta >= 0.0 || amount >= 0.8 {
                if move_speed < 0.4 {
                    let stop = if (progress as f64) < 0.5 { anim::STD_RUNSTOP1 } else { anim::STD_RUNSTOP2 };
                    self.ped_add_anim(r, assoc_group::STD, stop);
                    if let Some(a) = self.ped_assoc_mut(r, stop) {
                        a.blend_amount = 1.0;
                        a.set_delete_callback(callbacks::RESTORE_HEADING_RATE);
                    }
                    self.ped_mut(r).heading_rate = 0.0;
                    self.fade_out_anim(r, anim::STD_RUNFAST, -1000.0);
                    for id in [anim::STD_WALK, anim::STD_RUN] {
                        if let Some(a) = self.ped_assoc_mut(r, id) {
                            a.flags &= !aflags::RUNNING;
                            a.blend_amount = 0.0;
                            a.blend_delta = 0.0;
                        }
                    }
                } else if delta >= 0.0 {
                    // tired of sprinting
                    self.fade_out_anim(r, anim::STD_RUNFAST, -1.0);
                    if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUN) {
                        a.blend_delta = 1.0;
                    }
                }
            } else if move_speed < 1.0 {
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUNFAST) {
                    a.blend_delta = -8.0;
                }
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUN) {
                    a.blend_delta = 8.0;
                }
            }
        } else if has(self, anim::STD_STARTWALK) {
            // walk start and walk or run never play together
            for id in [anim::STD_WALK, anim::STD_RUN] {
                if let Some(a) = self.ped_assoc_mut(r, id) {
                    a.flags &= !aflags::RUNNING;
                    a.blend_amount = 0.0;
                }
            }
        } else if move_state_now == move_state::SPRINT {
            if let Some(sprint_delta) = sprint.map(|s| s.1) {
                if sprint_delta < 0.0 {
                    if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUNFAST) {
                        a.blend_delta = 2.0;
                    }
                    if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUN) {
                        a.blend_delta = -2.0;
                    }
                }
            } else {
                // from run to sprint
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_WALK) {
                    a.blend_amount = 0.0;
                }
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUN) {
                    a.blend_amount = 1.0;
                }
                self.ped_blend_anim(r, group, anim::STD_RUNFAST, 2.0);
            }
            self.use_sprint_energy(r);
        } else {
            let (walk, run, state) = if move_speed < 1.0 {
                (1.0, 0.0, move_state::WALK)
            } else if move_speed < 2.0 {
                (2.0 - move_speed, move_speed - 1.0, move_state::RUN)
            } else {
                (0.0, 1.0, move_state::RUN)
            };
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_WALK) {
                a.blend_amount = walk;
            }
            if let Some(a) = self.ped_assoc_mut(r, anim::STD_RUN) {
                a.blend_amount = run;
            }
            self.ped_mut(r).move_state = state;
        }
    }

    /// The tail of `SetRealMoveAnim`: slow motion makes the player's own animations run faster.
    fn apply_adrenaline_to_move_anims(&mut self, px: &mut Px, r: EntityRef) {
        if !self.player_data(r).adrenaline_active {
            return;
        }
        let over = self.now() > self.player_data(r).adrenaline_time;
        if over {
            self.player_data_mut(r).adrenaline_active = false;
            px.hooks.set_time_scale(1.0);
        } else {
            px.hooks.set_time_scale(1.0 / 3.0);
        }
        let speed = if over { 1.0 } else { 2.0 };
        for id in [anim::STD_STARTWALK, anim::STD_WALK, anim::STD_RUN, anim::STD_RUNFAST] {
            if let Some(a) = self.ped_assoc_mut(r, id) {
                a.speed = speed;
            }
        }
    }

    /// `CPlayerPed::DoWeaponSmoothSpray`.
    pub fn do_weapon_smooth_spray(&self, r: EntityRef, free_cam_mouse: bool) -> bool {
        let pd = self.ped(r);
        if pd.ped_state != state::ATTACK || pd.point_gun_at.is_some() {
            return false;
        }
        let ty = pd.weapons[pd.current_weapon as usize].ty;
        if free_cam_mouse && matches!(ty, wt::COLT45 | wt::UZI) {
            return false;
        }
        matches!(ty, wt::FLAMETHROWER | wt::COLT45 | wt::UZI | wt::SHOTGUN | wt::AK47 | wt::M16 | wt::HELICANNON)
    }

    /// `CPlayerPed::DoesTargetHaveToBeBroken`.
    pub fn does_target_have_to_be_broken(&self, r: EntityRef, target: Vec3) -> bool {
        let pd = self.ped(r);
        let ty = pd.weapons[pd.current_weapon as usize].ty;
        let mut dist = target - self.epos(r);
        if dist.length() > self.ents.weapon_infos.get(ty).range {
            return true;
        }
        if ty != wt::SHOTGUN && ty != wt::AK47 {
            return false;
        }
        dist = crate::math::normalise(dist);
        dist.dot(self.ent(r).base.matrix.forward) < 0.4
    }

    /// `CPlayerPed::RunningLand`: a running player cancels the landing animation.
    pub fn running_land(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        let landing = self.ped_assoc(r, anim::STD_FALL_LAND).is_some_and(|a| a.current_time == 0.0);
        if !(landing && self.player_data(r).move_speed > 1.5 && (pad.walk_left_right != 0.0 || pad.walk_up_down != 0.0)) {
            return;
        }
        self.fade_out_anim(r, anim::STD_FALL_LAND, -1000.0);
        if let Some(i) = self.ped_add_anim(r, assoc_group::STD, anim::STD_JUMP_LAND)
            && let Some(a) = self.ped_assoc_at(r, i)
        {
            a.set_finish_callback(callbacks::FINISH_JUMP);
        }
        if self.ped(r).ped_state == state::JUMP {
            self.restore_previous_state(px, r);
        }
    }

    /// `CPlayerPed::IsThisPedAttackingPlayer`.
    pub fn is_this_ped_attacking_player(&self, player: EntityRef, suspect: EntityRef) -> bool {
        let s = self.ped(suspect);
        s.point_gun_at == Some(player) || (matches!(s.objective, objective::KILL_CHAR_ON_FOOT | objective::KILL_CHAR_ANY_MEANS) && s.ped_in_objective == Some(player))
    }

    /// `CPlayerPed::ProcessAnimGroups`: the animation group follows the weapon in hand (and the strafe direction).
    pub fn process_anim_groups(&mut self, px: &mut Px, r: EntityRef) {
        let cam = px.hooks.camera();
        let pd = self.ped(r);
        let ty = pd.weapons[pd.current_weapon as usize].ty;
        let walk_angle = self.player_data(r).walk_angle;
        let rocket = ty == wt::ROCKETLAUNCHER;
        let group = if (walk_angle <= -50.0_f32.to_radians() || walk_angle >= 50.0_f32.to_radians()) && cam.using_3rd_person_mouse_cam && self.ped(r).can_strafe_or_mouse_control() {
            if (-130.0_f32.to_radians()..=130.0_f32.to_radians()).contains(&walk_angle) {
                match (walk_angle > 0.0, rocket) {
                    (true, true) => assoc_group::ROCKETLEFT,
                    (true, false) => assoc_group::PLAYERLEFT,
                    (false, true) => assoc_group::ROCKETRIGHT,
                    (false, false) => assoc_group::PLAYERRIGHT,
                }
            } else if rocket {
                assoc_group::ROCKETBACK
            } else {
                assoc_group::PLAYERBACK
            }
        } else if rocket {
            assoc_group::PLAYERROCKET
        } else if ty == wt::BASEBALLBAT {
            assoc_group::PLAYERBBBAT
        } else if ty != wt::COLT45 && ty != wt::UZI {
            if self.ped(r).weapons[self.ped(r).current_weapon as usize].is_type_2_handed() { assoc_group::PLAYER2ARMED } else { assoc_group::PLAYER }
        } else {
            assoc_group::PLAYER1ARMED
        };
        if self.ped(r).anim_group != group as u32 {
            self.ped_mut(r).anim_group = group as u32;
            self.re_apply_move_anims(r);
        }
    }

    /// `CPlayerPed::ProcessWeaponSwitch`.
    #[allow(clippy::collapsible_if)] // mirrors the original's nesting
    pub fn process_weapon_switch(&mut self, px: &mut Px, r: EntityRef, pad: &PadState) {
        let cam = px.hooks.camera();
        let mode = cam.player_weapon_mode;
        let cur = self.ped(r).current_weapon as usize;
        let has_ammo = |w: &World, slot: usize| w.ped(r).has_weapon(slot) && w.ped(r).weapons[slot].has_ammo_to_be_used();
        let no_target = self.ped(r).point_gun_at.is_none();
        if px.hooks.frenzy_on_going() {
            // no weapon changes during a kill frenzy
        } else if pad.cycle_weapon_right_just_down && no_target {
            if !matches!(mode, cam_mode::M16_1STPERSON | cam_mode::M16_1STPERSON_RUNABOUT | cam_mode::SNIPER | cam_mode::SNIPER_RUNABOUT | cam_mode::ROCKETLAUNCHER | cam_mode::ROCKETLAUNCHER_RUNABOUT) {
                let found = (cur + 1..wt::TOTAL_INVENTORY).find(|&s| has_ammo(self, s));
                self.player_data_mut(r).selected_wep_slot = found.unwrap_or(wt::UNARMED);
            }
        } else if pad.cycle_weapon_left_just_down && no_target {
            if !matches!(mode, cam_mode::M16_1STPERSON | cam_mode::SNIPER | cam_mode::ROCKETLAUNCHER) {
                let mut slot = cur as i32 - 1;
                loop {
                    if slot < wt::UNARMED as i32 {
                        slot = wt::DETONATOR as i32;
                    }
                    if has_ammo(self, slot as usize) {
                        break;
                    }
                    slot -= 1;
                }
                self.player_data_mut(r).selected_wep_slot = slot as usize;
            }
        } else if self.ents.weapon_infos.get(cur).fire != WeaponFire::Melee {
            if self.ped(r).weapons[cur].ammo_total <= 0 {
                if matches!(mode, cam_mode::M16_1STPERSON | cam_mode::SNIPER | cam_mode::ROCKETLAUNCHER) {
                    return;
                }
                let found = (0..cur).rev().find(|&s| s == wt::BASEBALLBAT && self.ped(r).has_weapon(wt::BASEBALLBAT) || self.ped(r).weapons[s].ammo_total > 0 && s != wt::MOLOTOV && s != wt::GRENADE);
                self.player_data_mut(r).selected_wep_slot = found.unwrap_or(wt::UNARMED);
            }
        }
        let selected = self.player_data(r).selected_wep_slot;
        if selected != cur && !matches!(self.ped(r).ped_state, state::ATTACK | state::AIM_GUN | state::FIGHT) {
            self.make_changes_for_new_weapon(px, r, selected);
        }
    }

    /// `m_pPointGunAt` is a ped; a sound for a shot that can't zoom.
    pub(super) fn no_zoom_shot_sound(&self, r: EntityRef) -> Option<u16> {
        let pd = self.ped(r);
        match pd.weapons[pd.current_weapon as usize].ty {
            wt::SNIPERRIFLE => Some(sound::WEAPON_SNIPER_SHOT_NO_ZOOM),
            wt::ROCKETLAUNCHER => Some(sound::WEAPON_ROCKET_SHOT_NO_ZOOM),
            _ => None,
        }
    }
}
