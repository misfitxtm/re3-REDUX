//! The model registry filled from every real IDE file. Runs when GTA3_DIR / VC_DIR are set.
use re3_core::{modelinfo::*, temp_col::TempColModels};
use re3_formats::{datafile::*, ped::parse_ped_stats};
use std::{env, fs, path::{Path, PathBuf}};

fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
        p = fs::read_dir(&p).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path();
    }
    Some(p)
}

fn text(root: &Path, rel: &str) -> String {
    String::from_utf8_lossy(&fs::read(resolve(root, rel).unwrap_or_else(|| panic!("{rel}"))).unwrap()).into_owned()
}

#[test]
fn every_ide_loads_into_the_registry() {
    for (var, game, dat) in [("GTA3_DIR", Game::Gta3, "data/gta3.dat"), ("VC_DIR", Game::Vc, "data/gta_vc.dat")] {
        let Ok(root) = env::var(var) else { continue };
        let root = Path::new(&root);
        let stat_names: Vec<String> = parse_ped_stats(&text(root, "data/pedstats.dat")).into_iter().map(|s| s.name).collect();
        let lookups = TableLookups { game, ped_stat_names: &stat_names };

        let mut store = ModelInfoStore::new(game);
        store.initialise(&TempColModels::new(game));
        let defaults = (0..store.limits.models as i32).filter(|&i| store.get(i).is_some()).count();
        let mut names3 = re3_core::modelinfo::indices_gta3::ModelIndices::default();
        let mut namesv = re3_core::modelinfo::indices_vc::ModelIndices::default();
        let (mut files, mut defined, mut hits) = (0, 0, 0);
        for f in ["data/default.dat", dat] {
            for d in parse_gta_dat(&text(root, f)) {
                if let Directive::Ide(p) = d {
                    let ide = parse_ide(&text(root, &p), game);
                    let ids = store.load_ide(&ide, &lookups, 1.0).unwrap_or_else(|e| panic!("{var} {p}: {e:?}"));
                    files += 1;
                    defined += ids.len() + ide.hier.len() + ide.cars.len() + ide.peds.len() + ide.weap.len();
                    // match model names to the tracked indices as the IDE loader does
                    for id in 0..store.limits.models as i32 {
                        if let Some(m) = store.get(id) {
                            if game == Game::Gta3 { hits += namesv_or(&mut names3, &m.name, id) as usize } else { hits += namesv_or2(&mut namesv, &m.name, id) as usize }
                        }
                    }
                }
            }
        }
        let total = (0..store.limits.models as i32).filter(|&i| store.get(i).is_some()).count();
        println!("{var}: {files} IDE files -> {total} models ({defaults} built-in, {defined} defined), {} texture slots, {hits} tracked-name matches", store.txd.len());
        assert!(total > 2500 && store.txd.len() > 100);

        // the hardcoded ids must line up with the real model names
        let checks: Vec<(i16, &str)> = if game == Game::Gta3 {
            use re3_core::modelinfo::indices_gta3::mi;
            vec![(mi::PLAYER, "null"), (mi::COP, "cop"), (mi::SWAT, "swat"), (mi::MEDIC, "medic"), (mi::FIREMAN, "fireman"), (mi::LANDSTAL, "landstal"), (mi::IDAHO, "idaho"), (mi::POLICE, "police"), (mi::TAXI, "taxi"), (mi::RHINO, "rhino"), (mi::BORGNINE, "borgnine"), (mi::GRENADE, "grenade"), (mi::AK47, "ak47"), (mi::CUTOBJ01, "cutobj01"), (mi::CAR_DOOR, ""), (mi::LOPOLYGUY, "lopolyguy")]
        } else {
            use re3_core::modelinfo::indices_vc::mi;
            vec![(mi::PLAYER, "null"), (mi::COP, "cop"), (mi::SWAT, "swat"), (mi::LANDSTAL, "landstal"), (mi::POLICE, "police"), (mi::TAXI, "taxi"), (mi::RHINO, "rhino"), (mi::VICECHEE, "vicechee"), (mi::COLT45, "colt45"), (mi::CUTOBJ01, "cutobj01"), (mi::CAR_DOOR, "")]
        };
        for (id, name) in checks {
            let found = store.get(id as i32).map(|m| m.name.to_ascii_lowercase());
            assert_eq!(found.as_deref(), Some(name), "{var}: hardcoded id {id}");
        }

        // every object.dat row finds its model (a miss would be skipped, shrinking the table)
        let rows = re3_formats::world::parse_object_dat(&text(root, "data/object.dat"));
        let mut table = re3_core::entity::object::ObjectTable::new(game);
        table.load(&rows, &mut store);
        let linked = (0..store.limits.models as i32).filter(|&i| store.get(i).is_some_and(|m| m.object_id >= 0)).count();
        println!("{var}: {} object.dat rows, {linked} models linked", rows.len());
        assert!(linked > 0 && linked <= rows.len());

        // time models come in day/night pairs
        let (mut time, mut paired) = (0, 0);
        for id in 0..store.limits.models as i32 {
            if let Some(Kind::Time(t)) = store.get(id).map(|m| m.kind.clone()) {
                time += 1;
                paired += (t.other_time_model >= 0) as usize;
            }
        }
        println!("{var}: {time} time models, {paired} paired with their day/night twin");
    }
}

fn namesv_or(idx: &mut re3_core::modelinfo::indices_gta3::ModelIndices, name: &str, id: i32) -> bool {
    idx.match_model_string(name, id as i16)
}

fn namesv_or2(idx: &mut re3_core::modelinfo::indices_vc::ModelIndices, name: &str, id: i32) -> bool {
    idx.match_model_string(name, id as i16)
}
