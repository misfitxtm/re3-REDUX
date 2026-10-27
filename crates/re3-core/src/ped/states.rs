//! `Ped.cpp` state helpers: idle, wander path, look, say and the weapon model in the hand.
use super::ctx::Px;
use super::data::*;
use crate::control::pathfind::PathEnv;
use crate::entity::EntityRef;
use crate::world::{Kinds, LineOpts, World};
use glam::Vec3;

/// Path searches only ask `World` whether a line is clear.
struct WorldPathEnv<'a>(&'a mut World);

impl PathEnv for WorldPathEnv<'_> {
    fn is_line_of_sight_clear(&mut self, from: Vec3, to: Vec3) -> bool {
        let kinds = Kinds { buildings: true, ..Kinds::NONE };
        self.0.is_line_of_sight_clear(from, to, kinds, LineOpts::default())
    }

    // shortcut: wandering never asks, only spawning does; wire to the camera with the population port
    fn is_sphere_visible(&mut self, _pos: Vec3, _radius: f32, _cam: Option<&crate::math::Matrix>) -> bool {
        false
    }

    fn find_ground_z(&mut self, p: Vec3) -> (f32, bool) {
        self.0.find_ground_z_for_3d_coord(p)
    }
}

impl World {
    /// `ThePaths.FindNextNodeWandering(PATH_PED, ...)`: the next node and direction.
    pub fn find_next_ped_node(&mut self, pos: Vec3, last: Option<usize>, dir: u8) -> Option<(usize, u8)> {
        let paths = std::mem::take(&mut self.ents.paths);
        let hit = paths.find_next_node_wandering(&mut WorldPathEnv(self), crate::control::pathfind::PATH_PED, pos, last, dir);
        self.ents.paths = paths;
        hit
    }

    /// `CPed::SetIdle` (without VC_PED_PORTS, as the PC build).
    pub fn set_idle(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        let wait = px.rng.range_i32(2000, 4000) as u32;
        let pd = self.ped_mut(r);
        if !matches!(pd.ped_state, state::IDLE | state::MUG | state::FLEE_ENTITY) {
            pd.ped_state = state::IDLE;
            pd.set_move_state(move_state::STILL);
        }
        if pd.wait_state == wait_state::FALSE {
            pd.wait_timer = now + wait;
        }
    }

    /// `CPed::ClearLook`.
    pub fn clear_look(&mut self, px: &mut Px, r: EntityRef) {
        self.restore_previous_state(px, r);
        self.clear_look_flag(px, r);
    }

    /// `CPed::SetWanderPath`: true when the ped now follows a path.
    pub fn set_wander_path(&mut self, px: &mut Px, r: EntityRef, path_state_dest: i8) -> bool {
        if !self.ped(r).is_ped_in_control() {
            let pd = self.ped_mut(r);
            pd.path_dir = path_state_dest;
            pd.flags.start_wander_path_on_foot = true;
            return false;
        }
        if self.ped(r).flags.kinda_stay_in_same_place {
            self.set_idle(px, r);
            return false;
        }
        let mut dest = path_state_dest;
        self.ped_mut(r).path_dir = dest;
        if dest == 0 {
            dest = px.rng.range_i32(1, 7) as i8;
        }
        let pos = self.epos(r);
        let mut found = None;
        // try every direction once, starting at the requested one
        loop {
            let (last, dir) = (self.ped(r).last_path_node, self.ped(r).path_dir as u8);
            if let Some(hit) = self.find_next_ped_node(pos, last, dir) {
                found = Some(hit);
                break;
            }
            let pd = self.ped_mut(r);
            pd.path_dir = (pd.path_dir + 1) % 8;
            if pd.path_dir == dest {
                break;
            }
        }
        let Some((node, next_dir)) = found else {
            self.clear_all(px, r);
            self.set_idle(px, r);
            return false;
        };
        let pd = self.ped_mut(r);
        pd.next_path_node = Some(node);
        pd.path_dir = next_dir as i8;
        pd.ped_state = state::WANDER_PATH;
        pd.set_move_state(move_state::WALK);
        pd.flags.is_running = false;
        true
    }

    /// `CPed::Say`: queues a comment, subject to who is talking and how fast the camera moves.
    pub fn say(&mut self, px: &mut Px, r: EntityRef, mut audio: u16) {
        use crate::audio::sound as s;
        if self.ped(r).is_player() {
            match audio {
                s::PED_DEATH => audio = s::PED_DAMAGE,
                s::PED_DAMAGE | s::PED_HIT | s::PED_LAND => {}
                s::PED_BULLET_HIT | s::PED_CAR_JACKED | s::PED_DEFEND => audio = s::PED_HIT,
                _ => return,
            }
        } else {
            if px.hooks.camera_position().z + 3.0 < self.epos(r).z {
                return;
            }
            let speed = px.hooks.camera_average_speed();
            if speed > 1.65 {
                return;
            } else if speed > 1.25 {
                if !matches!(audio, s::PED_DEATH | s::PED_TAXI_WAIT | s::PED_EVADE) {
                    return;
                }
            } else if speed > 0.9 && !matches!(audio, s::PED_DEATH | s::PED_BURNING | s::PED_FLEE_SPRINT | s::PED_TAXI_WAIT | s::PED_EVADE | s::PED_ANNOYED_DRIVER) {
                return;
            }
        }
        let now = self.now();
        let pd = self.ped(r);
        if audio >= pd.queued_sound {
            return;
        }
        let (_, _, max_random, override_max) = super::process_misc::COMMENT_WAIT_TIME[(audio - s::PED_DEATH) as usize];
        if audio != pd.last_queued_sound || audio == s::PED_DEATH || override_max + pd.last_sound_start + px.rng.range_i32(0, max_random) as u32 <= now {
            self.ped_mut(r).queued_sound = audio;
        }
    }

    /// `CPed::AddWeaponModel`: only the id is kept, the atomic is the renderer's.
    pub fn add_weapon_model(&mut self, r: EntityRef, id: i32) {
        if id != -1 {
            self.ped_mut(r).wep_model_id = id;
        }
    }

    /// `CPed::RemoveWeaponModel`; the id argument is unused in the original too.
    pub fn remove_weapon_model(&mut self, r: EntityRef) {
        self.ped_mut(r).wep_model_id = -1;
    }
}
