use super::*;
use re3_formats::datafile::parse_ide;

struct Fake;
impl IdeLookups for Fake {
    fn handling_id(&self, _: &str) -> i32 {
        7
    }
    fn ped_type(&self, n: &str) -> i32 {
        if n == "COP" { 6 } else { 99 }
    }
    fn ped_stat_type(&self, _: &str) -> i32 {
        3
    }
    fn anim_group(&self, _: &str) -> u32 {
        1
    }
}

const IDE: &str = "objs\n\
    100, LODbuild1, big, 1, 500, 0\n\
    101, xxxbuild1, big, 2, 200, 100, 4\n\
    end\ntobj\n\
    110, shop_nt, shopt, 1, 80, 0, 20, 6\n\
    111, shop_dy, shopt, 1, 80, 0, 6, 20\n\
    end\nhier\n120 carpart carp\nend\ncars\n\
    130 taxi taxi car TAXI Taxi_Cab richfamily 5 1 ff 2 0.5\n\
    131 speed speed boat SPEEDER Fast_Boat leisure 1 1 0\n\
    end\npeds\n140 cop cop COP STAT_COP cop 3\nend\n";

fn store() -> ModelInfoStore {
    let mut s = ModelInfoStore::new(Game::Gta3);
    s.load_ide(&parse_ide(IDE, Game::Gta3), &Fake, 1.0).unwrap();
    s
}

#[test]
fn ide_fills_the_registry() {
    let s = store();
    let (id, m) = s.find_by_name("XXXBUILD1").unwrap();
    assert_eq!(id, 101);
    let d = m.simple().unwrap();
    assert_eq!((d.num_atomics, d.lod_distances[..2].to_vec(), d.first_damaged), (2, vec![200.0, 100.0], 1));
    assert!(d.flags.draw_last && !d.flags.additive);
    assert_eq!(s.txd.name(m.txd_slot as usize), Some("big"));
    // same texture dictionary name => same slot
    assert_eq!(s.get(100).unwrap().txd_slot, m.txd_slot);
    assert_eq!(s.txd.len(), 6); // big shopt carp taxi speed cop
    let taxi = s.get(130).unwrap().vehicle().unwrap();
    assert_eq!((taxi.game_name.as_str(), taxi.vehicle_type, taxi.wheel_id, taxi.handling_id), ("Taxi Cab", vehicle_type::CAR, 2, 7));
    assert!(s.is_boat_model(131) && !s.is_boat_model(130) && !s.is_bike_model(131));
    assert!(matches!(&s.get(140).unwrap().kind, Kind::Ped(p) if p.ped_type == 6 && p.ped_stat_type == 3 && p.cars_can_drive == 3));
    assert!(matches!(s.get(120).unwrap().kind, Kind::Clump));
    assert_eq!(s.get(120).unwrap().kind.type_id(Game::Gta3), 4);
}

#[test]
fn big_buildings_get_their_lod_partner() {
    // "LODbuild1" and "xxxbuild1" match after skipping the three-letter prefix
    let s = store();
    let lod = s.get(100).unwrap().simple().unwrap();
    assert!(lod.is_big_building, "a 500 unit draw distance is beyond the big-building threshold");
    assert_eq!(lod.related_model, Some(101));
    // near distance comes from the partner's largest LOD distance
    assert_eq!(lod.lod_distances[2], s.get(101).unwrap().largest_lod_distance(1.0));
    let tall = s.get(101).unwrap().simple().unwrap();
    assert!(!tall.is_big_building, "200 is below the threshold");
    // a big building without a partner: LOD-prefixed names get 100, others 0 (FIX_BUGS)
    let mut s = ModelInfoStore::new(Game::Gta3);
    let ide = parse_ide("objs\n1, LODalone, t, 1, 500, 0\n2, farthing, t, 1, 500, 0\nend\n", Game::Gta3);
    s.load_ide(&ide, &Fake, 1.0).unwrap();
    assert_eq!(s.get(1).unwrap().simple().unwrap().lod_distances[2], 100.0);
    assert_eq!(s.get(2).unwrap().simple().unwrap().lod_distances[2], 0.0);
}

#[test]
fn night_and_day_models_pair_up() {
    let s = store();
    let Kind::Time(night) = &s.get(110).unwrap().kind else { panic!() };
    let Kind::Time(day) = &s.get(111).unwrap().kind else { panic!() };
    assert_eq!((night.time_on, night.time_off, night.other_time_model), (20, 6, 111));
    assert_eq!(day.other_time_model, 110);
}

#[test]
fn lod_selection() {
    let s = store();
    let m = s.get(101).unwrap();
    // two LODs at 200 (near) and 100; first_damaged = 1 means the second is a damage model
    assert_eq!(m.atomic_from_distance(50.0, 1.0), Some(0));
    assert_eq!(m.atomic_from_distance(150.0, 1.0), Some(0));
    assert_eq!(m.atomic_from_distance(250.0, 1.0), None);
    assert_eq!(m.largest_lod_distance(1.0), 200.0);
    assert_eq!(m.largest_lod_distance(2.0), 400.0);
    let mut m = m.clone();
    m.simple_mut().unwrap().is_damaged = true;
    assert_eq!(m.atomic_from_distance(50.0, 1.0), Some(1));
    m.increase_alpha();
    assert_eq!(m.simple().unwrap().alpha, 0x10);
    for _ in 0..20 {
        m.increase_alpha();
    }
    assert_eq!(m.simple().unwrap().alpha, 0xFF);
}

#[test]
fn txd_refs_and_slots() {
    let mut s = store();
    s.add_ref(100);
    s.add_ref(101); // same dictionary
    let slot = s.get(100).unwrap().txd_slot as usize;
    assert_eq!(s.txd.num_refs(slot), 2);
    assert!(!s.remove_ref(100));
    assert!(s.remove_ref(101), "last user gone: unload the dictionary");
    s.txd.remove_slot(slot);
    assert_eq!(s.txd.add_slot("new"), slot, "freed slot is reused first");
}

#[test]
fn limits_and_defaults() {
    let mut s = ModelInfoStore::new(Game::Gta3);
    for i in 0..s.limits.clump as i32 {
        s.add_clump_model(i).unwrap();
    }
    assert!(matches!(s.add_clump_model(100), Err(Error::Full(_))));
    assert!(matches!(s.add_simple_model(99999), Err(Error::BadId(_))));
    let mut s = ModelInfoStore::new(Game::Gta3);
    s.initialise(&crate::temp_col::TempColModels::new(Game::Gta3));
    let door = s.get(indices_gta3::mi::CAR_DOOR as i32).unwrap();
    assert_eq!((door.col_model.as_ref().unwrap().spheres.len(), door.simple().unwrap().lod_distances[0]), (3, 80.0));
    assert_eq!(s.txd.find_slot("GENERIC"), Some(0));
}

#[test]
fn model_name_matching_and_predicates() {
    let mut idx = indices_gta3::ModelIndices::default();
    assert!(idx.match_model_string("fire_hydrant", 321));
    assert!(!idx.match_model_string("unknown_thing", 5));
    assert_eq!(idx.get(indices_gta3::slot::FIRE_HYDRANT), 321);
    assert!(idx.is_police_ped_model(indices_gta3::mi::COP) && !idx.is_police_ped_model(indices_gta3::mi::MEDIC));
    assert!(idx.is_ped_model(0) && idx.is_ped_model(89) && !idx.is_ped_model(90));
    idx.match_model_string("veg_tree1", 500);
    assert!(idx.is_tree_model(500) && !idx.is_tree_model(501));
    idx.reset();
    assert_eq!(idx.get(indices_gta3::slot::FIRE_HYDRANT), -1);
}

#[test]
fn table_lookups() {
    let l = TableLookups { game: Game::Gta3, ped_stat_names: &["STAT_PLAYER".to_string(), "STAT_COP".to_string()] };
    assert_eq!(l.handling_id("LANDSTAL"), 0);
    assert_eq!(l.handling_id("NOSUCH"), 57);
    assert_eq!(l.ped_type("COP"), 6);
    assert_eq!(l.ped_type("NOPE"), names_gta3::NUM_PED_TYPES);
    assert_eq!(l.ped_stat_type("STAT_COP"), 1);
    assert_eq!(l.anim_group("player"), 1);
}
