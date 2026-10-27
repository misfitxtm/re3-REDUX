//! `Darkel.cpp`: the kill frenzies, and the count of what the player has killed.
use crate::entity::EntityRef;
use crate::modelinfo::indices_gta3::mi::NUM_DEFAULT_MODELS;
use crate::ped::ctx::{Px, StatEvent};
use crate::ped::types::ped_type;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;

pub const FRENZY_ANY_PED: i32 = -1;
pub const FRENZY_ANY_CAR: i32 = -2;

/// The frenzy states.
pub mod frenzy {
    pub const NONE: u16 = 0;
    pub const ONGOING: u16 = 1;
    pub const PASSED: u16 = 2;
    pub const FAILED: u16 = 3;
}

/// `CDarkel`'s statics.
#[derive(Debug, Clone)]
pub struct Darkel {
    pub time_limit: i32,
    pub previous_time: i32,
    pub time_of_frenzy_start: i32,
    pub weapon_type: i32,
    pub ammo_interrupted_weapon: i32,
    pub kills_needed: i32,
    pub interrupted_weapon: i8,
    pub standard_sound_and_messages: bool,
    pub need_head_shot: bool,
    pub proper_kill_frenzy: bool,
    pub status: u16,
    pub registered_kills: Vec<u16>,
    pub model_to_kill: [i32; 4],
    pub start_message: Option<String>,
}

impl Default for Darkel {
    fn default() -> Self {
        Self {
            time_limit: 0,
            previous_time: 0,
            time_of_frenzy_start: 0,
            weapon_type: 0,
            ammo_interrupted_weapon: 0,
            kills_needed: 0,
            interrupted_weapon: 0,
            standard_sound_and_messages: false,
            need_head_shot: false,
            proper_kill_frenzy: false,
            status: frenzy::NONE,
            registered_kills: vec![0; NUM_DEFAULT_MODELS as usize],
            model_to_kill: [0; 4],
            start_message: None,
        }
    }
}

/// `CalcFade`: a message fades in and out over half a second at both ends of its time.
pub fn calc_fade(time: u32, start: u32, end: u32) -> u8 {
    if time >= start && time <= end {
        if time >= start + 500 {
            if time <= end - 500 { 255 } else { (255 * (end - time) / 500) as u8 }
        } else {
            (255 * (time - start) / 500) as u8
        }
    } else {
        0
    }
}

impl Darkel {
    /// `FrenzyOnGoing`.
    pub fn frenzy_on_going(&self) -> bool {
        self.status == frenzy::ONGOING
    }

    /// `QueryModelsKilledByPlayer`.
    pub fn query_models_killed_by_player(&self, model: i32) -> u16 {
        self.registered_kills.get(model as usize).copied().unwrap_or(0)
    }

    /// `ResetModelsKilledByPlayer`.
    pub fn reset_models_killed_by_player(&mut self) {
        self.registered_kills.fill(0);
    }

    fn count_model(&mut self, model: i32) {
        if let Some(k) = self.registered_kills.get_mut(model as usize) {
            *k = k.wrapping_add(1);
        }
    }

    fn matches_model(&self, wanted_any: i32, model: i32) -> bool {
        self.model_to_kill[0] == wanted_any || self.model_to_kill.contains(&model)
    }

    /// The weapon of the frenzy, with the drive-by Uzi counted as the Uzi.
    fn fixed_weapon(&self) -> usize {
        if self.weapon_type == wt::UZI_DRIVEBY as i32 { wt::UZI } else { self.weapon_type as usize }
    }
}

impl World {
    /// `CDarkel::RegisterCarBlownUpByPlayer`.
    pub fn register_car_blown_up_by_player(&mut self, px: &mut Px, vehicle: EntityRef) {
        let model = self.ent(vehicle).base.model_index as i32;
        let d = &mut self.ents.darkel;
        if d.frenzy_on_going() && d.matches_model(FRENZY_ANY_CAR, model) {
            d.kills_needed -= 1;
            px.hooks.front_end_sound_with(crate::audio::sound::RAMPAGE_CAR_BLOWN, 0);
        }
        d.count_model(model);
        px.hooks.stat(StatEvent::CarsExploded);
    }

    /// `CDarkel::RegisterKillByPlayer`.
    pub fn register_kill_by_player(&mut self, px: &mut Px, victim: EntityRef, weapon: usize, headshot: bool) {
        let model = self.ent(victim).base.model_index as i32;
        let d = &mut self.ents.darkel;
        let w = d.weapon_type;
        let counts = weapon as i32 == w
            || weapon == wt::EXPLOSION
            || (weapon == wt::UZI_DRIVEBY && w == wt::UZI as i32)
            || (weapon == wt::RAMMEDBYCAR && w == wt::RUNOVERBYCAR as i32)
            || (weapon == wt::RUNOVERBYCAR && w == wt::RAMMEDBYCAR as i32)
            || (weapon == wt::FLAMETHROWER && w == wt::MOLOTOV as i32);
        if d.frenzy_on_going() && counts && d.matches_model(FRENZY_ANY_PED, model) && (!d.need_head_shot || headshot) {
            d.kills_needed -= 1;
            px.hooks.front_end_sound_with(crate::audio::sound::RAMPAGE_KILL, 0);
        }
        d.count_model(model);
        let pd = self.ped(victim);
        let ty = if pd.flags.chris_criminal { ped_type::CRIMINAL } else { pd.ped_type };
        px.hooks.stat(StatEvent::PeopleKilledByPlayer);
        px.hooks.stat(StatEvent::PedKilledOfType(ty));
        if headshot {
            px.hooks.stat(StatEvent::HeadsPopped);
        }
        px.hooks.stat(StatEvent::KillsSinceLastCheckpoint);
    }

    /// `CDarkel::RegisterKillNotByPlayer`.
    pub fn register_kill_not_by_player(&mut self, px: &mut Px) {
        px.hooks.stat(StatEvent::PeopleKilledByOthers);
    }

    /// The end of a frenzy: the player gets the weapon and ammo from before back.
    fn restore_player_weapon_after_frenzy(&mut self, px: &mut Px) {
        let Some(player) = self.ents.player_ped else { return };
        let (fixed, interrupted, ammo) = {
            let d = &self.ents.darkel;
            (d.fixed_weapon(), d.interrupted_weapon, d.ammo_interrupted_weapon)
        };
        if fixed < wt::TOTAL {
            if let Some(p) = self.ped_mut(player).player.as_deref_mut() {
                p.selected_wep_slot = interrupted as usize;
            }
            self.ped_mut(player).weapons[fixed].ammo_total = ammo;
        }
        if self.find_player_vehicle().is_some() {
            self.remove_weapon_model(player);
            let slot = self.ped(player).player.as_ref().map_or(0, |p| p.selected_wep_slot);
            self.ped_mut(player).current_weapon = slot as u8;
            self.make_changes_for_new_weapon(px, player, slot);
        }
    }

    /// `CDarkel::ResetOnPlayerDeath`.
    pub fn reset_darkel_on_player_death(&mut self, px: &mut Px) {
        if self.ents.darkel.status != frenzy::ONGOING {
            return;
        }
        self.ents.population.all_random_peds_this_type = -1;
        self.ents.darkel.status = frenzy::FAILED;
        self.ents.darkel.time_of_frenzy_start = self.now() as i32;
        self.restore_player_weapon_after_frenzy(px);
    }

    /// `CDarkel::StartFrenzy`: `proper_kill_frenzy` is whether the text is the standard "kill frenzy" page (PAGE_00).
    #[allow(clippy::too_many_arguments)] // mirrors the original's signature
    pub fn start_frenzy(&mut self, px: &mut Px, weapon: usize, time: i32, kills: u16, model0: i32, text: Option<String>, proper_kill_frenzy: bool, model2: i32, model3: i32, model4: i32, standard_sound: bool, need_head_shot: bool) {
        let now = self.now() as i32;
        {
            let d = &mut self.ents.darkel;
            d.weapon_type = weapon as i32;
            d.status = frenzy::ONGOING;
            d.kills_needed = kills as i32;
            d.model_to_kill = [model0, model2, model3, model4];
            d.start_message = if proper_kill_frenzy { None } else { text };
            d.proper_kill_frenzy = proper_kill_frenzy;
            d.standard_sound_and_messages = standard_sound;
            d.need_head_shot = need_head_shot;
            d.time_of_frenzy_start = now;
            d.time_limit = time;
            d.previous_time = time / 1000;
        }
        let fixed = self.ents.darkel.fixed_weapon();
        if let Some(player) = self.ents.player_ped.filter(|_| fixed < wt::TOTAL) {
            self.ents.darkel.interrupted_weapon = self.ped(player).current_weapon as i8;
            self.give_weapon(player, fixed, 0);
            self.ents.darkel.ammo_interrupted_weapon = self.ped(player).weapons[fixed].ammo_total;
            self.give_weapon(player, fixed, 30000);
            if let Some(p) = self.ped_mut(player).player.as_deref_mut() {
                p.selected_wep_slot = fixed;
            }
            self.make_changes_for_new_weapon(px, player, fixed);
            if self.find_player_vehicle().is_some() {
                self.ped_mut(player).current_weapon = fixed as u8;
                let clip = self.ents.weapon_infos.get(fixed).ammo;
                let w = &mut self.ped_mut(player).weapons[fixed];
                w.ammo_in_clip = w.ammo_total.min(clip);
                self.clear_weapon_target(px, player);
            }
        }
        if self.ents.darkel.standard_sound_and_messages {
            px.hooks.front_end_sound_with(crate::audio::sound::RAMPAGE_START, 0);
        }
    }

    /// `CDarkel::Update`: the time runs, the frenzy fails or is passed.
    pub fn update_darkel(&mut self, px: &mut Px) {
        if self.ents.darkel.status != frenzy::ONGOING {
            return;
        }
        let now = self.now() as i32;
        let d = &self.ents.darkel;
        let frame_time = d.time_limit - (now - d.time_of_frenzy_start);
        if frame_time > 0 || d.time_limit < 0 {
            px.hooks.front_end_sound_with(crate::audio::sound::RAMPAGE_ONGOING, frame_time);
            let prev = frame_time / 1000;
            if prev != self.ents.darkel.previous_time {
                if self.ents.darkel.previous_time < 12 {
                    px.hooks.front_end_sound_with(crate::audio::sound::CLOCK_TICK, prev);
                }
                self.ents.darkel.previous_time = prev;
            }
        } else {
            self.ents.population.all_random_peds_this_type = -1;
            self.ents.darkel.status = frenzy::FAILED;
            self.ents.darkel.time_of_frenzy_start = now;
            self.restore_player_weapon_after_frenzy(px);
            if self.ents.darkel.standard_sound_and_messages {
                px.hooks.front_end_sound_with(crate::audio::sound::RAMPAGE_FAILED, 0);
            }
        }
        if self.ents.darkel.kills_needed <= 0 {
            self.ents.population.all_random_peds_this_type = -1;
            self.ents.darkel.status = frenzy::PASSED;
            if self.ents.darkel.proper_kill_frenzy {
                px.hooks.stat(StatEvent::KillFrenzyPassed);
            }
            self.ents.darkel.time_of_frenzy_start = now;
            let t = self.now();
            if let Some(w) = self.player_wanted_mut() {
                w.set_wanted_level(0, t);
            }
            self.restore_player_weapon_after_frenzy(px);
            if self.ents.darkel.standard_sound_and_messages {
                px.hooks.front_end_sound_with(crate::audio::sound::RAMPAGE_PASSED, 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::PedHooks;

    #[derive(Default)]
    struct Rec {
        sounds: Vec<(u16, i32)>,
        stats: Vec<StatEvent>,
    }
    impl PedHooks for Rec {
        fn front_end_sound_with(&mut self, sound: u16, param: i32) {
            self.sounds.push((sound, param));
        }
        fn stat(&mut self, stat: StatEvent) {
            self.stats.push(stat);
        }
    }

    fn world() -> (World, EntityRef, EntityRef) {
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let (mut rng, mut hooks) = (Rng::default(), crate::ped::ctx::NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let player = w.create_ped(&mut px, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(player).player = Some(Default::default());
        w.ents.player_ped = Some(player);
        let victim = w.create_ped(&mut px, ped_type::CIVMALE, 50, None).unwrap();
        (w, player, victim)
    }

    #[test]
    fn fades_ramp_up_hold_and_ramp_down() {
        assert_eq!((calc_fade(0, 0, 8000), calc_fade(250, 0, 8000), calc_fade(500, 0, 8000), calc_fade(4000, 0, 8000)), (0, 127, 255, 255));
        assert_eq!((calc_fade(7750, 0, 8000), calc_fade(8000, 0, 8000), calc_fade(9000, 0, 8000)), (127, 0, 0));
    }

    #[test]
    fn kills_count_towards_the_frenzy_only_with_the_right_weapon_and_model() {
        let (mut w, player, victim) = world();
        let mut rng = Rng::default();
        let mut hooks = Rec::default();
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.ents.timer.time_in_ms = 1000;
        w.start_frenzy(&mut px, wt::UZI, 60_000, 3, FRENZY_ANY_PED, None, true, 0, 0, 0, true, false);
        assert_eq!(w.ents.darkel.status, frenzy::ONGOING);
        assert_eq!(w.ped(player).player.as_ref().unwrap().selected_wep_slot, wt::UZI);
        w.register_kill_by_player(&mut px, victim, wt::COLT45, false);
        assert_eq!(w.ents.darkel.kills_needed, 3);
        w.register_kill_by_player(&mut px, victim, wt::UZI_DRIVEBY, false);
        w.register_kill_by_player(&mut px, victim, wt::EXPLOSION, false);
        assert_eq!(w.ents.darkel.kills_needed, 1);
        assert_eq!(w.ents.darkel.query_models_killed_by_player(50), 3);
        assert!(hooks.stats.contains(&StatEvent::PedKilledOfType(ped_type::CIVMALE)));
    }

    #[test]
    fn a_frenzy_ends_by_time_or_by_kills_and_gives_the_old_weapon_back() {
        let (mut w, player, _) = world();
        let mut rng = Rng::default();
        let mut hooks = Rec::default();
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.ents.timer.time_in_ms = 1000;
        w.start_frenzy(&mut px, wt::UZI, 5_000, 1, FRENZY_ANY_PED, None, false, 0, 0, 0, true, false);
        w.ents.timer.time_in_ms = 7_000;
        w.update_darkel(&mut px);
        assert_eq!(w.ents.darkel.status, frenzy::FAILED);
        w.start_frenzy(&mut px, wt::UZI, -1, 1, FRENZY_ANY_PED, None, true, 0, 0, 0, false, false);
        w.ents.darkel.kills_needed = 0;
        w.update_darkel(&mut px);
        assert_eq!(w.ents.darkel.status, frenzy::PASSED);
        // ammo of the interrupted weapon came back
        assert_eq!(w.ped(player).weapons[wt::UZI].ammo_total, w.ents.darkel.ammo_interrupted_weapon);
        assert!(hooks.stats.contains(&StatEvent::KillFrenzyPassed));
        assert!(hooks.sounds.iter().any(|(s, _)| *s == crate::audio::sound::RAMPAGE_FAILED));
    }

    #[test]
    fn dying_during_a_frenzy_fails_it() {
        let (mut w, _, _) = world();
        let (mut rng, mut hooks) = (Rng::default(), Rec::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.reset_darkel_on_player_death(&mut px);
        assert_eq!(w.ents.darkel.status, frenzy::NONE);
        w.start_frenzy(&mut px, wt::COLT45, 10_000, 5, FRENZY_ANY_PED, None, false, 0, 0, 0, false, false);
        w.ents.population.all_random_peds_this_type = 5;
        w.reset_darkel_on_player_death(&mut px);
        assert_eq!((w.ents.darkel.status, w.ents.population.all_random_peds_this_type), (frenzy::FAILED, -1));
    }
}
