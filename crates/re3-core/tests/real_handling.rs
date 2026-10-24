//! Every real GTA III handling row converts to sane game units. Runs when GTA3_DIR is set.
use re3_core::vehicle::handling::{HandlingMgr, VEHICLE_NAMES};
use re3_formats::{datafile::Game, handling::parse_handling};
use std::{env, fs, path::Path};

#[test]
fn every_row_has_a_consistent_top_speed() {
    let Ok(root) = env::var("GTA3_DIR") else { return };
    let dir = fs::read_dir(Path::new(&root)).unwrap().flatten().find(|e| e.file_name().eq_ignore_ascii_case("data")).unwrap().path();
    let file = fs::read_dir(dir).unwrap().flatten().find(|e| e.file_name().eq_ignore_ascii_case("handling.cfg")).unwrap().path();
    let cfg = parse_handling(&String::from_utf8_lossy(&fs::read(file).unwrap()), Game::Gta3);
    let m = HandlingMgr::new(&cfg);
    let mut loaded = 0;
    for (i, name) in VEHICLE_NAMES.iter().enumerate() {
        let h = m.get(i);
        if h.mass == 0.0 {
            println!("no row for {name}");
            continue;
        }
        loaded += 1;
        let t = &h.transmission;
        assert!(h.turn_mass > 0.0 && h.buoyancy > 0.0, "{name}");
        assert!(t.max_cruise_velocity > 0.0 && t.max_velocity >= t.max_cruise_velocity, "{name}");
        assert!(t.gears[t.num_gears as usize].max_velocity > t.gears[1].max_velocity || t.num_gears == 1, "{name}");
    }
    println!("{loaded}/{} handling rows", VEHICLE_NAMES.len());
    assert!(loaded >= 50);
}
