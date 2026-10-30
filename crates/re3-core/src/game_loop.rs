//! `CGame::Process`: the order the game's systems run in each frame.
use crate::control::game_logic::GameLogicHooks;
use crate::ped::ctx::Px;
use crate::script::ScriptHooks;
use crate::vehicle::env::CarEnv;
use crate::world::WorldHooks;
use crate::world::World;

/// The systems of the frame that are not part of the game logic here (drawing, audio, input, streaming and the cosmetic ones), in the order the original runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    UpdatePads,
    CutsceneUpdate,
    FrontEnd,
    Streaming,
    Weather,
    Collision,
    Helis,
    Skidmarks,
    Antennas,
    Glass,
    EventList,
    Particles,
    Clouds,
    MovingThings,
    WaterCannons,
    UserDisplay,
    Replay,
    AccidentManager,
    Rubbish,
    SpecialFx,
    TimeCycle,
    Camera,
    CullZones,
    Coronas,
    Shadows,
}

/// What the host supplies for the frame: the systems that are not ported, and the questions about them.
#[allow(unused_variables)]
pub trait FrameHooks: ScriptHooks + GameLogicHooks + WorldHooks {
    /// A system of the frame that is not part of this crate runs now.
    fn step(&mut self, step: Step) {}
    /// `CTimer::GetIsPaused`: nothing but the pads and the menu run.
    fn paused(&mut self) -> bool {
        false
    }
    /// `CCutsceneMgr::IsRunning`.
    fn cutscene_running(&mut self) -> bool {
        false
    }
    /// `CReplay::IsPlayingBack`.
    fn replay_playing(&mut self) -> bool {
        false
    }
    /// The debug key that skips the clock forward, and `gbFastTime`.
    fn clock_flags(&mut self) -> (bool, bool) {
        (false, false)
    }
    /// The story flag that opens the bridge (`CTheScripts::...CommercialPassed`).
    fn commercial_passed(&mut self) -> bool {
        false
    }
}

impl World {
    /// `CGame::Process`. `px` and `hooks` must be different objects; `env` is what the cars and the world consult this frame.
    pub fn process_frame(&mut self, px: &mut Px, env: &mut CarEnv, hooks: &mut dyn FrameHooks) {
        hooks.step(Step::UpdatePads);
        hooks.step(Step::CutsceneUpdate);
        hooks.step(Step::FrontEnd);
        hooks.step(Step::Streaming);
        if hooks.paused() {
            return;
        }
        let (player, now) = (self.player_info_pos(), self.now());
        self.ents.zones.update(player);
        let (skip, fast) = hooks.clock_flags();
        if self.ents.clock.update(now, skip, fast) {
            hooks.add_days_passed(1);
        }
        hooks.step(Step::Weather);
        self.process_scripts(px, hooks);
        hooks.step(Step::Collision);
        let replay = hooks.replay_playing();
        self.update_trains(env.camera_pos, env);
        self.update_planes(replay);
        hooks.step(Step::Helis);
        self.update_darkel(px);
        for s in [Step::Skidmarks, Step::Antennas, Step::Glass, Step::EventList, Step::Particles] {
            hooks.step(s);
        }
        self.update_fires(px);
        self.update_population(px);
        self.update_weapons(px);
        let cutscene = hooks.cutscene_running();
        if !cutscene {
            let on_train = env.player.vehicle.and_then(|v| self.try_veh(v)).is_some_and(|v| v.is_train());
            self.process_car_generators(env, hooks, px.rng, on_train);
        }
        if !replay {
            self.update_cranes(env, replay);
        }
        for s in [Step::Clouds, Step::MovingThings, Step::WaterCannons, Step::UserDisplay, Step::Replay] {
            hooks.step(s);
        }
        self.process(env, hooks, px.rng);
        self.process_player_info(px);
        hooks.step(Step::AccidentManager);
        self.update_pacman_pickups(px);
        self.update_pickups(px);
        self.update_garages(px);
        for s in [Step::Rubbish, Step::SpecialFx, Step::TimeCycle, Step::Camera, Step::CullZones] {
            hooks.step(s);
        }
        if !replay {
            self.update_game_logic(px, hooks);
        }
        let commercial_passed = hooks.commercial_passed();
        self.update_bridge(commercial_passed);
        hooks.step(Step::Coronas);
        hooks.step(Step::Shadows);
        self.update_phones(px);
        if !replay {
            self.generate_random_cars(px);
            self.generate_road_blocks(px);
            self.remove_distant_cars(px);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::game_logic::GameLogicHooks;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::vehicle::env::CarHooks;
    use crate::world::tests::world_with_cars;

    #[derive(Default)]
    struct Host {
        steps: Vec<Step>,
        paused: bool,
        days: i32,
    }
    impl CarHooks for Host {}
    impl WorldHooks for Host {}
    impl ScriptHooks for Host {}
    impl GameLogicHooks for Host {
        fn add_days_passed(&mut self, days: i32) {
            self.days += days;
        }
    }
    impl FrameHooks for Host {
        fn step(&mut self, step: Step) {
            self.steps.push(step);
        }
        fn paused(&mut self) -> bool {
            self.paused
        }
    }

    fn frame(host: &mut Host) -> World {
        let mut w = world_with_cars();
        w.ents.timer.time_in_ms = 1000;
        w.process_frame(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, &mut CarEnv::default(), host);
        w
    }

    #[test]
    fn a_frame_runs_the_systems_in_the_order_of_the_original() {
        let mut host = Host::default();
        frame(&mut host);
        let at = |s| host.steps.iter().position(|&x| x == s).unwrap();
        assert_eq!(host.steps.len(), 25, "{:?}", host.steps);
        assert!(at(Step::Streaming) < at(Step::Weather) && at(Step::Weather) < at(Step::Collision) && at(Step::Collision) < at(Step::Particles));
        assert!(at(Step::Replay) < at(Step::AccidentManager) && at(Step::AccidentManager) < at(Step::Camera) && at(Step::Camera) < at(Step::Shadows));
    }

    #[test]
    fn a_paused_game_only_runs_the_input_and_menu_steps() {
        let mut host = Host { paused: true, ..Default::default() };
        frame(&mut host);
        assert_eq!(host.steps, vec![Step::UpdatePads, Step::CutsceneUpdate, Step::FrontEnd, Step::Streaming]);
    }
}
