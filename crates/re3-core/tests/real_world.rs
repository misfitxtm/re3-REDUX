//! A whole world built from the real game folder, then the real main script run on it. Runs when GTA3_DIR is set.
use re3_core::loader::load_world;
use re3_formats::datafile::Game;
use std::{env, path::Path};

#[test]
fn the_real_gta3_world_loads() {
    let Ok(root) = env::var("GTA3_DIR") else { return };
    let (w, report) = load_world(Path::new(&root), Game::Gta3).unwrap();
    println!("{report:#?}");
    println!("{} car nodes, {} ped nodes", w.ents.paths.num_car_path_nodes, w.ents.paths.num_ped_path_nodes);
    assert!(report.models > 2500 && report.instances > 8000 && report.zones > 10);
    assert!(report.col_models > 1000);
    assert!(w.ents.paths.num_car_path_nodes > 500);
}

use re3_core::control::game_logic::GameLogicHooks;
use re3_core::game_loop::{FrameHooks, Step};
use re3_core::general::Rng;
use re3_core::ped::ctx::{CameraState, PedHooks, Px};
use re3_core::script::{Scripts, ScriptHooks};
use re3_core::vehicle::env::{CarEnv, CarHooks};
use re3_core::world::WorldHooks;

#[derive(Default)]
struct Host {
    printed: Vec<String>,
    /// Milliseconds the host has run, and when the cutscene started.
    time: i32,
    cutscene_start: i32,
}
impl CarHooks for Host {}
impl WorldHooks for Host {}
impl GameLogicHooks for Host {}
impl ScriptHooks for Host {
    fn print_big(&mut self, key: &str, _time: u32, _style: i32) {
        self.printed.push(key.to_owned());
    }
    fn print_now(&mut self, key: &str, _time: u32, _flags: i32) {
        self.printed.push(key.to_owned());
    }
    fn start_cutscene(&mut self) {
        self.cutscene_start = self.time;
    }
    fn cutscene_time_in_milliseconds(&mut self) -> i32 {
        self.time - self.cutscene_start
    }
}
impl FrameHooks for Host {
    fn step(&mut self, step: Step) {
        if step == Step::CutsceneUpdate {
            self.time += 20;
        }
    }
}

/// A camera that sits on the player and sees nothing; models count as loaded.
#[derive(Default)]
struct HeadlessPed {
    camera: glam::Vec3,
}
impl PedHooks for HeadlessPed {
    fn camera(&mut self) -> CameraState {
        CameraState { source: self.camera, front: glam::Vec3::Y, up: glam::Vec3::Z, ..Default::default() }
    }
    fn camera_generation_centre(&mut self) -> Option<glam::Vec3> {
        Some(self.camera)
    }
}

/// Runs the real main.scm frame by frame and says where it stops.
#[test]
fn the_real_main_script_runs_on_the_real_world() {
    let Ok(root) = env::var("GTA3_DIR") else { return };
    let (mut w, _) = load_world(Path::new(&root), Game::Gta3).unwrap();
    let files = re3_core::loader::GameFiles::new(&root);
    let main = files.bytes("data/main.scm").unwrap();
    w.ents.scripts = Scripts::from_main_script(&main);
    w.ents.scripts.start_new_script(0);
    let mut host = Host::default();
    let mut ped_hooks = HeadlessPed::default();
    let mut rng = Rng::default();
    let frames: u32 = env::var("FRAMES").ok().and_then(|f| f.parse().ok()).unwrap_or(3000);
    let mut done = 0;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..frames {
            w.ents.timer.update(20.0);
            let at = w.ents.player_ped.map_or(glam::Vec3::ZERO, |p| w.epos(p));
            let mut env = CarEnv { camera_pos: at, player_centre: at, generation_dist_multiplier: 1.0, ..Default::default() };
            env.player.ped = w.ents.player_ped;
            env.player.coors = at;
            env.player.playing = true;
            ped_hooks.camera = at;
            w.process_frame(&mut Px { rng: &mut rng, hooks: &mut ped_hooks }, &mut env, &mut host);
            done += 1;
        }
    }));
    println!("{done} frames, {} script commands, printed {:?}", w.ents.scripts.commands_executed, host.printed);
    println!(
        "world: {} peds, {} cars, {} objects, player {:?}, clock {}:{:02}",
        w.ents.pools.peds.iter().count(),
        w.ents.pools.vehicles.iter().count(),
        w.ents.pools.objects.iter().count(),
        w.ents.player_ped.map(|p| w.epos(p)),
        w.ents.clock.hours,
        w.ents.clock.minutes
    );
    if let Some(p) = w.ents.player_ped {
        let at = w.epos(p);
        let zone = w.ents.zones.get_zone_info(at, true);
        println!("zone info at the player: {zone:?}");
        println!("zones: level {}, player zone {:?}", w.ents.zones.curr_level, w.ents.zones.players_zone);
        println!("ped density multiplier {}, total peds {}, mission peds {}", w.ents.population.ped_density_multiplier, w.ents.population.total_peds, w.ents.population.total_mission_peds);
        println!("cars: random {}, count down {}, total {}", w.ents.car_ctrl.num_random_cars, w.ents.car_ctrl.count_down_to_cars_at_start, w.ents.car_ctrl.num_mission_cars);
    }
    let intro: Vec<_> = w.ents.scripts.recent_commands.iter().filter(|(ip, _)| *ip >= 131_072).collect();
    println!("intro mission (ip, opcode): {intro:?}");
    let mut at = w.ents.scripts.active;
    while let Some(i) = at {
        let s = &w.ents.scripts.scripts[i];
        println!("script {i} {:?}: ip {}, wake at {}, now {}", s.name_str(), s.ip, s.wake_time, w.ents.timer.time_in_ms);
        at = w.ents.scripts.next_of(i);
    }
    if let Err(e) = &r {
        let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
        println!("stopped: {msg}");
        println!("last commands (ip, opcode): {:?}", w.ents.scripts.recent_commands);
    }
    assert!(r.is_ok() && done == frames, "the script stopped after {done} frames");
    assert!(host.printed.iter().any(|k| k == "BETRA_A"), "the intro never started: {:?}", host.printed);
}

#[test]
fn the_player_can_be_created() {
    let Ok(root) = env::var("GTA3_DIR") else { return };
    let (mut w, _) = load_world(Path::new(&root), Game::Gta3).unwrap();
    let mut rng = Rng::default();
    let mut px = Px { rng: &mut rng, hooks: &mut HeadlessPed::default() };
    let r = w.create_player(&mut px, None);
    println!("create_player: {r:?}");
    println!("setup: {:?}", w.setup_player_ped(&mut px, None));
}
