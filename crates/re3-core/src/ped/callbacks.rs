//! Animation callbacks that belong to peds: the token stored on the association, and what it does.
use super::ctx::Px;
use super::data::*;
use crate::anim::{tables_gta3::anim, Event};
use crate::entity::EntityRef;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;

/// `FinishFuckUCB`.
pub const FINISH_FUCK_U: u64 = 1;
/// `PedGetupCB`.
pub const PED_GETUP: u64 = 2;
/// `PedLandCB`.
pub const PED_LAND: u64 = 3;
/// `PedEvadeCB`.
pub const PED_EVADE: u64 = 4;
/// `FinishDieAnimCB`.
pub const FINISH_DIE_ANIM: u64 = 5;
/// `FinishedWaitCB`.
pub const FINISHED_WAIT: u64 = 6;
/// `RestoreHeadingRateCB`.
pub const RESTORE_HEADING_RATE: u64 = 7;
/// `FinishLaunchCB`.
pub const FINISH_LAUNCH: u64 = 8;
/// `FinishJumpCB`.
pub const FINISH_JUMP: u64 = 9;
/// `FinishHitHeadCB`.
pub const FINISH_HIT_HEAD: u64 = 10;
/// `PedAnimAlignCB`.
pub const PED_ANIM_ALIGN: u64 = 11;
/// `PedAnimDoorOpenCB`.
pub const PED_ANIM_DOOR_OPEN: u64 = 12;
/// `PedAnimPullPedOutCB`.
pub const PED_ANIM_PULL_PED_OUT: u64 = 13;
/// `PedAnimGetInCB`.
pub const PED_ANIM_GET_IN: u64 = 14;
/// `PedAnimDoorCloseCB`.
pub const PED_ANIM_DOOR_CLOSE: u64 = 15;
/// `PedAnimStepOutCarCB`.
pub const PED_ANIM_STEP_OUT_CAR: u64 = 16;
/// `PedSetOutCarCB`.
pub const PED_SET_OUT_CAR: u64 = 17;
/// `PedSetDraggedOutCarCB`.
pub const PED_SET_DRAGGED_OUT_CAR: u64 = 18;
/// `PedSetInCarCB`.
pub const PED_SET_IN_CAR: u64 = 19;
/// `PedSetDraggedOutCarPositionCB`.
pub const PED_SET_DRAGGED_OUT_CAR_POSITION: u64 = 20;
/// `PedSetQuickDraggedOutCarPositionCB`.
pub const PED_SET_QUICK_DRAGGED_OUT_CAR_POSITION: u64 = 21;
/// `PedSetOutTrainCB`.
pub const PED_SET_OUT_TRAIN: u64 = 22;
/// `PedSetInTrainCB`.
pub const PED_SET_IN_TRAIN: u64 = 23;
/// `PedAnimDoorCloseRollingCB`.
pub const PED_ANIM_DOOR_CLOSE_ROLLING: u64 = 24;
/// `FinishedAttackCB`.
pub const FINISHED_ATTACK: u64 = 25;
/// `FinishFightMoveCB`.
pub const FINISH_FIGHT_MOVE: u64 = 26;

impl World {
    /// Runs the callback the animation system reported for `r`.
    pub fn ped_anim_callback(&mut self, px: &mut Px, r: EntityRef, event: &Event) {
        match event.token {
            FINISH_FUCK_U => {
                let pd = self.ped(r);
                let unarmed = pd.weapons[pd.current_weapon as usize].ty == wt::UNARMED;
                if event.anim_id == anim::STD_PARTIAL_FUCKU as i32 && unarmed {
                    self.remove_weapon_model(r);
                }
            }
            PED_GETUP => self.ped_getup_cb(px, r, event.anim_id),
            PED_LAND => self.ped_land_cb(px, r, event.anim_id),
            PED_EVADE => self.ped_evade_cb(px, r, Some(event.anim_id)),
            FINISH_DIE_ANIM => self.finish_die_anim_cb(r),
            FINISHED_WAIT => self.finished_wait_cb(px, r),
            RESTORE_HEADING_RATE => self.restore_heading_rate(r),
            FINISH_LAUNCH => self.finish_launch_cb(px, r, event.anim_id),
            FINISH_JUMP => self.finish_jump_cb(r, event.anim_id),
            FINISH_HIT_HEAD => self.finish_hit_head_cb(px, r, Some(event.anim_id)),
            PED_ANIM_ALIGN => self.ped_anim_align_cb(px, r, Some(event.anim_id)),
            PED_ANIM_DOOR_OPEN => self.ped_anim_door_open_cb(px, r, Some(event.anim_id)),
            PED_ANIM_PULL_PED_OUT => self.ped_anim_pull_ped_out_cb(px, r, Some(event.anim_id)),
            PED_ANIM_GET_IN => self.ped_anim_get_in_cb(px, r, Some(event.anim_id)),
            PED_ANIM_DOOR_CLOSE => self.ped_anim_door_close_cb(px, r, Some(event.anim_id)),
            PED_ANIM_STEP_OUT_CAR => self.ped_anim_step_out_car_cb(px, r, Some(event.anim_id)),
            PED_SET_OUT_CAR => self.ped_set_out_car_cb(px, r, Some(event.anim_id)),
            PED_SET_DRAGGED_OUT_CAR => self.ped_set_dragged_out_car_cb(px, r, Some(event.anim_id)),
            PED_SET_IN_CAR => self.ped_set_in_car_cb(px, r, Some(event.anim_id)),
            PED_SET_DRAGGED_OUT_CAR_POSITION => self.ped_set_dragged_out_car_position_cb(px, r),
            PED_SET_QUICK_DRAGGED_OUT_CAR_POSITION => self.ped_set_quick_dragged_out_car_position_cb(px, r),
            PED_SET_OUT_TRAIN => self.ped_set_out_train_cb(px, r),
            PED_SET_IN_TRAIN => self.ped_set_in_train_cb(px, r),
            FINISHED_ATTACK => self.finished_attack_cb(px, r, Some(event.anim_id)),
            FINISH_FIGHT_MOVE => self.finish_fight_move_cb(px, r, event.anim_id),
            PED_ANIM_DOOR_CLOSE_ROLLING => self.ped_anim_door_close_rolling_cb(r, Some(event.anim_id)),
            _ => {}
        }
    }

    /// `CPed::PedGetupCB`.
    fn ped_getup_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: i32) {
        if self.ped(r).ped_state == state::GETUP {
            self.ped_set_blend_deltas(r, crate::anim::flags::PARTIAL, -1000.0);
        }
        self.ped_mut(r).flags.fallen_down = false;
        if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(anim_id)) {
            a.blend_delta = -1000.0;
        }
        if self.ped(r).ped_state == state::GETUP {
            self.restore_previous_state(px, r);
        }
        let pd = self.ped_mut(r);
        pd.set_move_state(if pd.ped_state != state::FLEE_POS && pd.ped_state != state::FLEE_ENTITY { move_state::STILL } else { move_state::RUN });
        self.set_move_anim(px, r);
        self.ped_mut(r).flags.get_up_anim_started = false;
    }

    /// `CPed::PedLandCB`.
    fn ped_land_cb(&mut self, px: &mut Px, r: EntityRef, anim_id: i32) {
        if let Some(a) = self.ped_clump_mut(r).and_then(|c| c.anim.association_mut(anim_id)) {
            a.blend_delta = -1000.0;
        }
        self.ped_mut(r).flags.is_landing = false;
        if self.ped(r).ped_state == state::JUMP {
            self.restore_previous_state(px, r);
        }
    }
}
