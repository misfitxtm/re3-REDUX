//! `GameLogic.cpp`: what happens after the player is wasted, busted or has failed a critical mission.
use crate::entity::EntityRef;
use crate::ped::ctx::Px;
use crate::ped::data::state as ped_state;
use crate::player_info::wb_state;
use crate::world::World;
use glam::Vec3;

/// How long the screen has to fade out, and how long the player waits before being put back.
const FADE_OUT_AFTER: u32 = 0x800;
const RESURRECT_AFTER: u32 = 0x1000;
const LEVEL_GENERIC: i32 = 0;

/// What the game logic asks of the camera, the HUD, streaming and the statistics.
#[allow(unused_variables)]
pub trait GameLogicHooks {
    /// `CCutsceneMgr::IsCutsceneProcessing`.
    fn cutscene_processing(&mut self) -> bool {
        false
    }
    /// `TheCamera.SetFadeColour`.
    fn set_fade_colour(&mut self, r: u8, g: u8, b: u8) {}
    /// `TheCamera.Fade(duration, direction)`.
    fn fade(&mut self, duration: f32, fade_in: bool) {}
    /// `CEventList::Initialise`.
    fn initialise_event_list(&mut self) {}
    /// `CMessages::ClearMessages`.
    fn clear_messages(&mut self) {}
    /// `CGameLogic::SortOutStreamingAndMemory`: the models around the new place are loaded.
    fn sort_out_streaming_and_memory(&mut self, pos: Vec3) {}
    /// The camera forgets its shake and blur, and the pad stops shaking.
    fn reset_camera_and_pad(&mut self) {}
    /// `CPad::GetPad(PlayerInFocus)->DisablePlayerControls = PLAYERCONTROL_ENABLED`.
    fn enable_player_controls(&mut self) {}
    /// `TheCamera.SetCameraDirectlyInFrontForFollowPed_CamOnAString`.
    fn camera_in_front_of_player(&mut self) {}
    /// `CStats::DaysPassed += days`.
    fn add_days_passed(&mut self, days: i32) {}
    /// `CStats::CheckPointReachedUnsuccessfully`.
    fn checkpoint_reached_unsuccessfully(&mut self) {}
}

/// Nothing is shown or counted.
pub struct NoGameLogicHooks;
impl GameLogicHooks for NoGameLogicHooks {}

/// The three ways the player is put back, with what differs between them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Resurrection {
    Wasted,
    Busted,
    FailedCriticalMission,
}

impl World {
    /// `CGameLogic::PassTime`: the clock moves on by `minutes`, counting the days that went by.
    pub fn pass_time(&mut self, gh: &mut dyn GameLogicHooks, minutes: u32) {
        let (mut m, mut hours) = (minutes + u32::from(self.ents.clock.minutes), u32::from(self.ents.clock.hours));
        while m >= 60 {
            m -= 60;
            hours += 1;
        }
        if hours > 23 {
            let mut days = 0;
            while hours >= 24 {
                hours -= 24;
                days += 1;
            }
            gh.add_days_passed(days);
        }
        let now = self.now();
        self.ents.clock.set_game_clock(hours as u8, m as u8, now);
        self.ents.pickups.pass_time(minutes * 1000);
    }

    /// `CGameLogic::Update`.
    pub fn update_game_logic(&mut self, px: &mut Px, gh: &mut dyn GameLogicHooks) {
        if gh.cutscene_processing() {
            return;
        }
        let Some(player) = self.ents.player_ped else { return };
        let (state, since, before) = (self.ents.player_info.wb_state, self.now().wrapping_sub(self.ents.player_info.wb_time), self.ents.timer.previous_time_in_ms.wrapping_sub(self.ents.player_info.wb_time));
        let (kind, colour) = match state {
            wb_state::PLAYING => {
                if self.ped(player).ped_state == ped_state::DEAD {
                    self.clear_adrenaline(px);
                    self.kill_player(px);
                }
                if self.ped(player).ped_state == ped_state::ARRESTED {
                    self.clear_adrenaline(px);
                    self.arrest_player_info(px);
                }
                return;
            }
            wb_state::WASTED => (Resurrection::Wasted, 200),
            wb_state::BUSTED => (Resurrection::Busted, 0),
            wb_state::FAILED_CRITICAL_MISSION => (Resurrection::FailedCriticalMission, 0),
            _ => return,
        };
        if since > FADE_OUT_AFTER && before <= FADE_OUT_AFTER {
            gh.set_fade_colour(colour, colour, colour);
            gh.fade(2.0, false);
        }
        if since >= RESURRECT_AFTER {
            self.resurrect_player(px, gh, player, kind, colour);
        }
    }

    /// The player is put back at a hospital or police station after the screen has faded out.
    fn resurrect_player(&mut self, px: &mut Px, gh: &mut dyn GameLogicHooks, player: EntityRef, kind: Resurrection, colour: u8) {
        self.ents.player_info.wb_state = wb_state::PLAYING;
        match kind {
            Resurrection::Wasted => {
                if !std::mem::take(&mut self.ents.player_info.get_out_of_hospital_free) {
                    self.take_money_and_weapons(player, 1000);
                }
            }
            Resurrection::Busted => {
                let take = [100, 100, 200, 400, 600, 900, 1500][self.player_wanted().map_or(0, |w| w.level).clamp(0, 6) as usize];
                if !std::mem::take(&mut self.ents.player_info.get_out_of_jail_free) {
                    self.take_money_and_weapons(player, take);
                }
            }
            Resurrection::FailedCriticalMission => {}
        }
        self.player_leaves_car_for_good(player);
        gh.initialise_event_list();
        gh.clear_messages();
        self.ents.car_ctrl.clear_interesting_vehicle_list();
        let here = self.player_info_pos();
        self.clear_exciting_stuff_from_area(px, here, 4000.0, true);
        let (pos, angle) = if kind == Resurrection::Wasted { self.find_closest_hospital_restart_point(here) } else { self.find_closest_police_restart_point(here) };
        self.ents.restart.override_hospital_level = LEVEL_GENERIC;
        self.ents.restart.override_police_station_level = LEVEL_GENERIC;
        if kind != Resurrection::FailedCriticalMission {
            self.pass_time(gh, 720);
        }
        self.restore_player_stuff_during_resurrection(px, gh, player, pos, angle);
        if kind == Resurrection::Busted {
            self.clear_weapons(player);
        }
        let here = self.player_info_pos();
        gh.sort_out_streaming_and_memory(here);
        gh.reset_camera_and_pad();
        self.ents.car_ctrl.count_down_to_cars_at_start = 2;
        gh.enable_player_controls();
        let fade_in = match kind {
            Resurrection::Wasted => std::mem::replace(&mut self.ents.restart.fade_in_after_next_death, true),
            Resurrection::Busted => std::mem::replace(&mut self.ents.restart.fade_in_after_next_arrest, true),
            Resurrection::FailedCriticalMission => true,
        };
        if fade_in {
            gh.set_fade_colour(colour, colour, colour);
            gh.fade(4.0, true);
        }
    }

    fn take_money_and_weapons(&mut self, player: EntityRef, money: i32) {
        let info = &mut self.ents.player_info;
        info.money = (info.money - money).max(0);
        self.clear_weapons(player);
    }

    /// A player who died or was caught in a car leaves it: the car is abandoned, unless it is wrecked.
    fn player_leaves_car_for_good(&mut self, player: EntityRef) {
        let pd = self.ped(player);
        let (true, Some(car)) = (pd.in_vehicle, pd.my_vehicle) else { return };
        let Some(v) = self.try_veh_mut(car) else { return };
        if v.driver == Some(player) {
            v.driver = None;
            if self.ent(car).base.status != crate::entity::status::WRECKED {
                self.ent_mut(car).base.status = crate::entity::status::ABANDONED;
            }
        } else {
            v.remove_passenger(player);
        }
    }

    /// `CGameLogic::RestorePlayerStuffDuringResurrection`.
    pub fn restore_player_stuff_during_resurrection(&mut self, px: &mut Px, gh: &mut dyn GameLogicHooks, player: EntityRef, mut pos: Vec3, angle: f32) {
        {
            let pd = self.ped_mut(player);
            (pd.health, pd.armour, pd.bloody_footprint_count_or_death_time, pd.flags.do_bloody_footprints) = (100.0, 0.0, 0, false);
        }
        self.ent_mut(player).base.flags.is_visible = true;
        self.clear_adrenaline(px);
        if let Some(d) = self.ped_mut(player).player.as_deref_mut() {
            d.current_stamina = d.max_stamina;
        }
        if let Some(f) = self.ped(player).fire {
            self.extinguish_fire_index(px, f);
        }
        let pd = self.ped_mut(player);
        (pd.in_vehicle, pd.my_vehicle, pd.vehicle_anim) = (false, None, None);
        self.reset_player_wanted(px);
        self.restart_non_partial_anims(player);
        self.make_player_safe(px, false);
        self.ent_mut(player).base.flags.remove_from_world = false;
        self.clear_weapon_target(px, player);
        self.set_initial_state(px, player);
        self.ents.car_ctrl.clear_interesting_vehicle_list();
        pos.z += 1.0;
        self.teleport(player, pos);
        if let Some(p) = self.ent_mut(player).phys.as_mut() {
            p.move_speed = Vec3::ZERO;
        }
        let rotation = angle.to_radians();
        let pd = self.ped_mut(player);
        (pd.rotation_cur, pd.rotation_dest) = (rotation, rotation);
        self.ent_mut(player).base.set_heading(rotation);
        px.hooks.clear_space_for_mission_entity(pos, player);
        self.clear_exciting_stuff_from_area(px, pos, 4000.0, true);
        self.restore_heading_rate(player);
        gh.camera_in_front_of_player();
        self.ents.garages.player_arrested_or_died();
        gh.checkpoint_reached_unsuccessfully();
        self.remove(player);
        let _ = self.add(player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::ped::types::ped_type;
    use crate::world::tests::world_with_cars;

    #[derive(Default)]
    struct Spy {
        days: i32,
        fades: Vec<(f32, bool)>,
        colours: Vec<u8>,
        streamed: Vec<Vec3>,
        checkpoints: u32,
    }
    impl GameLogicHooks for Spy {
        fn add_days_passed(&mut self, days: i32) {
            self.days += days;
        }
        fn fade(&mut self, duration: f32, fade_in: bool) {
            self.fades.push((duration, fade_in));
        }
        fn set_fade_colour(&mut self, r: u8, _: u8, _: u8) {
            self.colours.push(r);
        }
        fn sort_out_streaming_and_memory(&mut self, pos: Vec3) {
            self.streamed.push(pos);
        }
        fn checkpoint_reached_unsuccessfully(&mut self) {
            self.checkpoints += 1;
        }
    }

    fn world_with_player() -> World {
        let mut w = world_with_cars();
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w
    }

    fn update(w: &mut World, spy: &mut Spy) {
        w.update_game_logic(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, spy);
    }

    #[test]
    fn time_passes_across_midnight_and_counts_the_days() {
        let mut w = world_with_cars();
        w.ents.clock.set_game_clock(23, 30, 0);
        let mut spy = Spy::default();
        w.pass_time(&mut spy, 720);
        assert_eq!((w.ents.clock.hours, w.ents.clock.minutes, spy.days), (11, 30, 1));
    }

    #[test]
    fn a_wasted_player_is_put_back_after_the_fade_and_loses_money() {
        let mut w = world_with_player();
        let p = w.ents.player_ped.unwrap();
        w.ents.player_info.money = 5000;
        w.ents.player_info.wb_state = wb_state::WASTED;
        w.ped_mut(p).health = 0.0;
        w.ents.timer.time_in_ms = 10_000;
        w.ents.player_info.wb_time = 10_000 - 0x805;
        w.ents.timer.previous_time_in_ms = 10_000 - 20;
        let mut spy = Spy::default();
        // this frame is the one that crosses 0x800 ms: the screen starts to fade out, the player stays down
        update(&mut w, &mut spy);
        assert_eq!((spy.fades.clone(), spy.colours.clone(), w.ents.player_info.wb_state), (vec![(2.0, false)], vec![200], wb_state::WASTED));
        w.ents.player_info.wb_time = 10_000 - 0x1000;
        update(&mut w, &mut spy);
        let info = &w.ents.player_info;
        assert_eq!((info.wb_state, info.money), (wb_state::PLAYING, 4000));
        assert_eq!((w.ped(p).health, spy.checkpoints, spy.streamed.len(), w.ents.restart.fade_in_after_next_death), (100.0, 1, 1, true));
    }

    #[test]
    fn a_free_release_from_jail_keeps_the_money_once() {
        let mut w = world_with_player();
        w.ents.player_info.money = 5000;
        w.ents.player_info.get_out_of_jail_free = true;
        for expected in [5000, 4900] {
            w.ents.player_info.wb_state = wb_state::BUSTED;
            w.ents.timer.time_in_ms = 100_000;
            w.ents.player_info.wb_time = 100_000 - 0x1000;
            w.ents.timer.previous_time_in_ms = 100_000 - 20;
            update(&mut w, &mut Spy::default());
            assert_eq!(w.ents.player_info.money, expected);
        }
    }
}
