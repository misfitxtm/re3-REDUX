//! `Ped.cpp`: jumping over obstacles.
use super::callbacks;
use super::ctx::Px;
use super::data::*;
use crate::anim::{assoc_group, flags as aflags, tables_gta3::anim};
use crate::entity::EntityRef;
use crate::surface;
use crate::world::{Kinds, LineOpts, World};
use glam::Vec3;

impl World {
    /// `CPed::CanPedJumpThis` (the PC build without VC_PED_PORTS): nothing solid one step ahead.
    pub fn can_ped_jump_this(&mut self, r: EntityRef) -> bool {
        let (pos, rot) = (self.epos(r), self.ped(r).rotation_cur);
        let forward_pos = Vec3::new(-rot.sin() + pos.x, rot.cos() + pos.y, pos.z);
        let kinds = Kinds { buildings: true, objects: true, ..Kinds::NONE };
        self.is_line_of_sight_clear(pos, forward_pos, kinds, LineOpts::default())
    }

    /// `CPed::SetJump`.
    pub fn set_jump(&mut self, r: EntityRef) {
        let e = self.ent(r);
        let (forward, normal, touched) = (e.base.matrix.forward, e.phys.as_ref().map_or(Vec3::ZERO, |p| p.damage_normal), e.phys.as_ref().map_or(0, |p| p.surface_touched));
        let pd = self.ped(r);
        if pd.in_vehicle || pd.ped_state == state::JUMP || self.ped_assoc(r, anim::STD_JUMP_LAUNCH).is_some() {
            return;
        }
        if touched == surface::STEEP_CLIFF && forward.dot(normal) < 0.0 {
            return;
        }
        self.set_stored_state(r);
        self.ped_mut(r).ped_state = state::JUMP;
        if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, anim::STD_JUMP_LAUNCH, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
            a.set_finish_callback(callbacks::FINISH_LAUNCH);
        }
        let pd = self.ped_mut(r);
        pd.rotation_dest = pd.rotation_cur;
    }

    /// `CPed::FinishLaunchCB`: the take-off of a jump.
    pub fn finish_launch_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: i32) {
        if self.ped(r).ped_state != state::JUMP {
            return;
        }
        let (forward, pos) = (self.ent(r).base.matrix.forward, self.epos(r));
        let sphere_z = self.col_model_of(r).and_then(|c| c.spheres.first().map(|s| s.center.z)).unwrap_or(0.0);
        let kinds = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
        let mut probe = pos + forward * 0.15;
        probe.z += sphere_z + 0.25;
        let mut obstacle = self.test_sphere_against_world(probe, 0.25, None, kinds, false);
        if obstacle.is_none() {
            probe += forward * 0.15;
            probe.z += 0.15;
            obstacle = self.test_sphere_against_world(probe, 0.25, None, kinds, false);
        }
        if obstacle.is_some() {
            if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(anim_id)) {
                a.flags |= aflags::DELETE_FADED_OUT;
            }
            if let Some(a) = self.ped_blend_anim(r, assoc_group::STD, anim::STD_HANDSCOWER, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                a.flags &= !aflags::FADE_OUT_WHEN_DONE;
                a.set_finish_callback(callbacks::FINISH_HIT_HEAD);
            }
            self.ped_mut(r).flags.is_landing = true;
            return;
        }
        let mut velocity_from_anim = 0.1;
        if let Some(a) = self.ped_assoc(r, anim::STD_RUNFAST) {
            velocity_from_anim = 0.05 * a.blend_amount + 0.17;
        } else if let Some(a) = self.ped_assoc(r, anim::STD_RUN) {
            velocity_from_anim = 0.07 * a.blend_amount + 0.1;
        }
        let boost = if self.ped(r).is_player() { 8.5 } else { 4.5 };
        let e = self.get_mut(r).expect("ped");
        let phys = e.phys.as_mut().expect("physical");
        phys.apply_move_force(Vec3::new(0.0, 0.0, boost));
        let speed_sq = phys.move_speed.x * phys.move_speed.x + phys.move_speed.y * phys.move_speed.y;
        if velocity_from_anim * velocity_from_anim > speed_sq {
            let rot = if px.hooks.third_person_mouse_cam() {
                let cur = self.ped(r).rotation_cur;
                self.work_out_heading_for_moving_first_person(px, r, cur)
            } else {
                self.ped(r).rotation_cur
            };
            let phys = self.get_mut(r).expect("ped").phys.as_mut().expect("physical");
            phys.move_speed.x = -velocity_from_anim * rot.sin();
            phys.move_speed.y = velocity_from_anim * rot.cos();
        }
        let pd = self.ped_mut(r);
        pd.flags.is_standing = false;
        pd.flags.is_in_the_air = true;
        if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(anim_id)) {
            a.blend_delta = -1000.0;
        }
        self.ped_add_anim(r, assoc_group::STD, anim::STD_JUMP_GLIDE);

        if self.ped(r).flags.do_bloody_footprints {
            let right = self.ent(r).base.matrix.right;
            for node in [node::FOOTL, node::FOOTR] {
                let mut blood = self.ped_node_position(r, node);
                blood.z -= 0.1;
                blood += forward * 0.2;
                px.hooks.add_blood_footprint(blood, forward, right);
            }
            let pd = self.ped_mut(r);
            if pd.bloody_footprint_count_or_death_time <= 40 {
                pd.bloody_footprint_count_or_death_time = 0;
                pd.flags.do_bloody_footprints = false;
            } else {
                pd.bloody_footprint_count_or_death_time -= 40;
            }
        }
    }

    /// `CPed::FinishJumpCB`.
    pub fn finish_jump_cb(&mut self, r: EntityRef, anim_id: i32) {
        let pd = self.ped_mut(r);
        pd.flags.reset_walk_anims = true;
        pd.flags.is_landing = false;
        if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(anim_id)) {
            a.blend_delta = -1000.0;
        }
    }

    /// `CPed::FinishHitHeadCB`.
    pub fn finish_hit_head_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: Option<i32>) {
        if let Some(a) = anim_id.and_then(|id| self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(id))) {
            a.blend_delta = -4.0;
            a.flags |= aflags::DELETE_FADED_OUT;
        }
        if self.ped(r).ped_state == state::JUMP {
            self.restore_previous_state(px, r);
        }
        self.ped_mut(r).flags.is_landing = false;
    }
}
