#![allow(clippy::field_reassign_with_default)]
use super::*;
use crate::entity::object::CreatedBy;
use crate::entity::{Entity, EntityType, ObjectData};
use crate::general::Rng;
use crate::entity::physical::Physical;

const BOX_MODEL: i16 = 10;

fn world() -> World {
    let mut models = ModelInfoStore::new(Game::Gta3);
    let mi = models.add_simple_model(BOX_MODEL as i32).unwrap();
    let b = Box3 { min: Vec3::splat(-5.0), max: Vec3::splat(5.0), surface: 0, piece: 0 };
    mi.col_model = Some(Arc::new(ColModel {
        bounding_sphere: Sphere::new(8.66, Vec3::ZERO, 0, 0),
        bounding_box: b,
        boxes: vec![b],
        ..Default::default()
    }));
    World::new(models, TempColModels::new(Game::Gta3))
}

fn ent(ty: EntityType, kind: Kind, pos: Vec3) -> Ent {
    let mut base = Entity::new(ty, &mut Rng::default());
    base.model_index = BOX_MODEL;
    base.flags.uses_collision = true;
    base.set_position(pos);
    let phys = matches!(kind, Kind::Object(_) | Kind::Vehicle(_) | Kind::Ped(_)).then(Physical::default);
    Ent::new(base, phys, kind)
}

fn building(w: &mut World, pos: Vec3) -> EntityRef {
    let r = w.insert(ent(EntityType::Building, Kind::Building, pos)).unwrap();
    w.add(r).unwrap();
    r
}

fn object(w: &mut World, pos: Vec3) -> EntityRef {
    let r = w.insert(ent(EntityType::Object, Kind::Object(ObjectData::default()), pos)).unwrap();
    w.add(r).unwrap();
    r
}

fn list(w: &World, x: i32, y: i32, l: usize) -> Vec<EntityRef> {
    w.sector(x, y).unwrap().lists[l].iter().copied().collect()
}

#[test]
fn a_box_on_a_sector_edge_is_filed_in_both_with_one_main_list() {
    let mut w = world();
    // sector edge at x = -1960 (index 1); the box spans -1965..-1955 so sectors 0 and 1
    let b = building(&mut w, Vec3::new(-1960.0, -1980.0, 0.0));
    let (a, c) = (list(&w, 0, 0, BUILDINGS), list(&w, 1, 0, BUILDINGS));
    // the centre of the bounds falls in sector 1, the other gets the overlap list
    assert_eq!((a.len(), c), (0, vec![b]));
    assert_eq!(list(&w, 0, 0, BUILDINGS + 1), vec![b]);
    w.remove(b);
    assert!(list(&w, 1, 0, BUILDINGS).is_empty() && list(&w, 0, 0, BUILDINGS + 1).is_empty());
}

#[test]
fn lists_insert_at_the_front() {
    let mut w = world();
    let (a, b) = (building(&mut w, Vec3::new(0.0, 0.0, 0.0)), building(&mut w, Vec3::new(1.0, 0.0, 0.0)));
    assert_eq!(list(&w, 50, 50, BUILDINGS), vec![b, a]);
}

#[test]
fn leaving_the_map_is_an_error_and_files_nothing() {
    let mut w = world();
    let r = w.insert(ent(EntityType::Building, Kind::Building, Vec3::new(-3000.0, 0.0, 0.0))).unwrap();
    assert_eq!(w.add(r), Err(WorldError::OutOfWorld));
}

#[test]
fn moving_objects_are_tracked_and_refiled() {
    let mut w = world();
    let o = object(&mut w, Vec3::new(20.0, 20.0, 0.0));
    assert_eq!(w.moving_entities().len(), 1);
    assert_eq!(w.get(o).unwrap().entry_info.len(), 1);
    // a physical's bounds are its bounding circle (radius 8.66), so near an edge it covers two sectors
    w.get_mut(o).unwrap().base.set_position(Vec3::new(-1960.0, 20.0, 0.0));
    w.remove_and_add(o).unwrap();
    let e = w.get(o).unwrap();
    assert_eq!(e.entry_info.len(), 2);
    assert_eq!(list(&w, 0, 50, OBJECTS).len() + list(&w, 1, 50, OBJECTS).len(), 1);
    assert_eq!(list(&w, 0, 50, OBJECTS + 1).len() + list(&w, 1, 50, OBJECTS + 1).len(), 1);
    assert!(list(&w, 50, 50, OBJECTS).is_empty());
    w.remove(o);
    assert!(w.moving_entities().is_empty());
    assert!(list(&w, 0, 50, OBJECTS + 1).is_empty() && list(&w, 1, 50, OBJECTS).is_empty());
}

#[test]
fn deleting_frees_the_slot_and_the_lists() {
    let mut w = world();
    let o = object(&mut w, Vec3::ZERO);
    assert!(w.delete(o).is_some());
    assert!(w.get(o).is_none() && w.moving_entities().is_empty() && list(&w, 50, 50, OBJECTS).is_empty());
}

#[test]
fn line_of_sight_returns_the_nearest_hit() {
    let mut w = world();
    let far = building(&mut w, Vec3::new(30.0, 0.0, 0.0));
    let near = building(&mut w, Vec3::new(10.0, 0.0, 0.0));
    let hit = w.process_line_of_sight(Vec3::new(-20.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0), Kinds::ALL, LineOpts::default()).unwrap();
    assert_eq!(hit.entity, near);
    assert!((hit.point.point.x - 5.0).abs() < 1e-3);
    assert_ne!(hit.entity, far);
    // only buildings were asked about
    assert!(w.process_line_of_sight(Vec3::new(-20.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0), Kinds { objects: true, ..Kinds::NONE }, LineOpts::default()).is_none());
}

#[test]
fn line_of_sight_honours_the_ignored_entity_and_clear_checks() {
    let mut w = world();
    let b = building(&mut w, Vec3::new(10.0, 0.0, 0.0));
    let (p1, p2) = (Vec3::new(-20.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0));
    assert!(!w.is_line_of_sight_clear(p1, p2, Kinds::ALL, LineOpts::default()));
    assert!(w.is_line_of_sight_clear(p1, Vec3::new(-20.0, 30.0, 0.0), Kinds::ALL, LineOpts::default()));
    w.ents.ignore_entity = Some(b);
    assert!(w.process_line_of_sight(p1, p2, Kinds::ALL, LineOpts::default()).is_none());
    assert!(w.is_line_of_sight_clear(p1, p2, Kinds::ALL, LineOpts::default()));
}

#[test]
fn a_line_crossing_many_sectors_visits_them_in_walking_order() {
    let w = world();
    // from the middle of sector (50,50) up and to the right
    let path = w.sectors_on_line(Vec3::new(20.0, 20.0, 0.0), Vec3::new(100.0, 70.0, 0.0));
    assert_eq!(path.first(), Some(&(50, 50)));
    assert_eq!(path.last(), Some(&(52, 51)));
    assert!(path.windows(2).all(|p| (p[0].0 - p[1].0).abs() + (p[0].1 - p[1].1).abs() == 1));
    // reversed, it is the same cells backwards
    let back = w.sectors_on_line(Vec3::new(100.0, 70.0, 0.0), Vec3::new(20.0, 20.0, 0.0));
    assert_eq!(back.first(), Some(&(52, 51)));
    assert_eq!(back.last(), Some(&(50, 50)));
    assert_eq!(w.sectors_on_line(Vec3::new(1.0, 1.0, 0.0), Vec3::new(2.0, 2.0, 0.0)), vec![(50, 50)]);
}

#[test]
fn ground_height_comes_from_the_buildings_top() {
    let mut w = world();
    building(&mut w, Vec3::new(0.0, 0.0, 10.0));
    assert!((w.find_ground_z_for_coord(1.0, 1.0) - 15.0).abs() < 1e-3);
    // nothing there: the default of 20
    assert_eq!(w.find_ground_z_for_coord(500.0, 500.0), 20.0);
    assert_eq!(w.find_ground_z_for_3d_coord(Vec3::new(500.0, 500.0, 50.0)), (0.0, false));
    let (z, found) = w.find_ground_z_for_3d_coord(Vec3::new(0.0, 0.0, 50.0));
    assert!(found && (z - 15.0).abs() < 1e-3);
    // looking up from inside the box finds its roof, looking up from below too
    assert!(w.find_roof_z_for_3d_coord(Vec3::new(500.0, 500.0, 0.0)) == (20.0, false));
}

#[test]
fn range_queries_dedupe_across_sectors_and_respect_limits() {
    let mut w = world();
    // sits across a sector edge so it is in two lists
    let a = building(&mut w, Vec3::new(-1960.0, -1980.0, 0.0));
    let b = object(&mut w, Vec3::new(-1950.0, -1980.0, 30.0));
    let near = Vec3::new(-1960.0, -1980.0, 0.0);
    assert_eq!(w.find_objects_in_range(near, 20.0, true, 10, Kinds::ALL), vec![a, b]);
    // 3D: the object is 30 up
    assert_eq!(w.find_objects_in_range(near, 20.0, false, 10, Kinds::ALL), vec![a]);
    assert_eq!(w.find_objects_in_range(near, 20.0, true, 1, Kinds::ALL).len(), 1);
    assert_eq!(w.find_objects_of_type_in_range(BOX_MODEL, near, 20.0, true, 10, Kinds::ALL).len(), 2);
    assert!(w.find_objects_of_type_in_range(BOX_MODEL + 1, near, 20.0, true, 10, Kinds::ALL).is_empty());
    // bounding circles reach further than positions do
    assert_eq!(w.find_objects_kinda_colliding(Vec3::new(-1950.0, -1980.0, 39.0), 1.0, false, 10, Kinds::ALL), vec![b]);
    let cube = w.find_objects_intersecting_cube(near - Vec3::splat(1.0), near + Vec3::splat(1.0), 10, Kinds::ALL);
    assert_eq!(cube, vec![a]);
}

#[test]
fn a_sphere_inside_a_box_touches_it() {
    let mut w = world();
    let b = building(&mut w, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(w.test_sphere_against_world(Vec3::new(5.5, 0.0, 0.0), 1.0, None, Kinds::ALL, false), Some(b));
    assert_eq!(w.test_sphere_against_world(Vec3::new(20.0, 0.0, 0.0), 1.0, None, Kinds::ALL, false), None);
    assert_eq!(w.test_sphere_against_world(Vec3::new(5.5, 0.0, 0.0), 1.0, Some(b), Kinds::ALL, false), None);
}

#[test]
fn scan_codes_wrap_and_clear() {
    let mut w = world();
    let b = building(&mut w, Vec3::ZERO);
    w.ents.scan_code = 65535;
    w.get_mut(b).unwrap().base.scan_code = 65535;
    w.advance_scan_code();
    // the wrap clears every main-list entity and restarts at 1
    assert_eq!((w.scan_code(), w.get(b).unwrap().base.scan_code), (1, 0));
}

#[test]
fn big_buildings_go_to_their_level_list_only() {
    let mut w = world();
    let mut e = ent(EntityType::Building, Kind::Building, Vec3::ZERO);
    e.base.flags.is_big_building = true;
    e.base.level = 2;
    let r = w.insert(e).unwrap();
    w.add(r).unwrap();
    assert_eq!(w.big_buildings(2).iter().copied().collect::<Vec<_>>(), vec![r]);
    assert!(list(&w, 50, 50, BUILDINGS).is_empty());
    w.remove(r);
    assert!(w.big_buildings(2).is_empty());
}

use crate::entity::object::{damage_effect, ObjectEvent, ScriptSound};
use re3_formats::world::ObjectInfo;

fn with_object_row(w: &mut World, effect: i32) {
    w.ents.models.get_mut(BOX_MODEL as i32).unwrap().name = "crate".into();
    let info = ObjectInfo { name: "crate".into(), mass: 50.0, turn_mass: 40.0, air_resistance: 0.99, elasticity: 0.2, percent_submerged: 50.0, uproot_limit: 0.0, collision_damage_multiplier: 1.0, damage_effect: effect, response_case: 2, camera_avoid: 1 };
    w.ents.objects.load(&[info, ObjectInfo { name: "nothing".into(), mass: 1.0, turn_mass: 1.0, air_resistance: 1.0, elasticity: 1.0, percent_submerged: 1.0, uproot_limit: 0.0, collision_damage_multiplier: 1.0, damage_effect: 0, response_case: 0, camera_avoid: 0 }], &mut w.ents.models);
}

#[test]
fn objects_take_their_physics_from_object_dat() {
    let mut w = world();
    with_object_row(&mut w, damage_effect::SMASH_CARDBOARD_COMPLETELY as i32);
    // the unknown model name did not consume a row
    assert_eq!(w.ents.models.get(BOX_MODEL as i32).unwrap().object_id, 0);
    let r = w.new_object(BOX_MODEL, Matrix::from_translation(Vec3::new(20.0, 20.0, 0.0)), &mut Rng::default()).unwrap();
    let e = w.get(r).unwrap();
    let p = e.phys.as_ref().unwrap();
    assert_eq!((p.mass, p.turn_mass, p.elasticity), (50.0, 40.0, 0.2));
    assert!((p.buoyancy - 100.0 / 50.0 * crate::entity::physical::GRAVITY * 50.0).abs() < 1e-6);
    assert!(e.base.flags.is_static);
    let Kind::Object(o) = &e.kind else { panic!() };
    assert!(o.camera_to_avoid && o.special_collision_response == 2 && o.can_be_deleted());
}

#[test]
fn smashing_a_cardboard_box_hides_it_and_throws_debris() {
    let mut w = world();
    with_object_row(&mut w, damage_effect::SMASH_CARDBOARD_COMPLETELY as i32);
    let r = w.new_object(BOX_MODEL, Matrix::from_translation(Vec3::new(20.0, 20.0, 0.0)), &mut Rng::default()).unwrap();
    let mut rng = Rng::default();
    let mut e = w.get(r).unwrap().clone();
    // the loaders switch collision on, not the constructor
    e.base.flags.uses_collision = true;
    // a soft knock does nothing
    assert!(w.ents.objects.object_damage(&mut e, 100.0, -1, &mut rng).is_empty());
    let ev = w.ents.objects.object_damage(&mut e, 200.0, -1, &mut rng);
    assert_eq!(ev.iter().filter(|x| matches!(x, ObjectEvent::Debris { .. })).count(), 25);
    assert!(matches!(ev.last(), Some(ObjectEvent::Sound { sound: ScriptSound::BoxDestroyed2, .. })));
    assert!(!e.base.flags.is_visible && !e.base.flags.uses_collision && e.base.flags.explosion_proof);
    // once smashed it no longer reacts
    assert!(w.ents.objects.object_damage(&mut e, 900.0, -1, &mut rng).is_empty());
}

#[test]
fn temp_and_mission_objects_can_be_swept() {
    let mut w = world();
    let mut rng = Rng::default();
    let mk = |w: &mut World, rng: &mut Rng, by| {
        let r = w.new_object(BOX_MODEL, Matrix::from_translation(Vec3::new(20.0, 20.0, 0.0)), rng).unwrap();
        w.add(r).unwrap();
        if let Kind::Object(o) = &mut w.get_mut(r).unwrap().kind {
            o.created_by = by;
        }
        r
    };
    let (g, t, m) = (mk(&mut w, &mut rng, CreatedBy::Game), mk(&mut w, &mut rng, CreatedBy::Temp), mk(&mut w, &mut rng, CreatedBy::Mission));
    w.delete_temp_objects_in_area(Vec3::new(500.0, 500.0, 0.0), 5.0);
    assert!(w.get(t).is_some());
    w.delete_temp_objects_in_area(Vec3::new(20.0, 20.0, 0.0), 5.0);
    assert!(w.get(t).is_none() && w.get(g).is_some());
    w.delete_objects_created_by(CreatedBy::Mission);
    assert!(w.get(m).is_none() && w.get(g).is_some());
    assert_eq!(list(&w, 50, 50, OBJECTS), vec![g]);
}

// ---- physics ----

const CAR_MODEL: i16 = 11;

fn world_with_car_model() -> World {
    let mut w = world();
    let mi = w.ents.models.add_simple_model(CAR_MODEL as i32).unwrap();
    let ball = Sphere::new(1.0, Vec3::ZERO, 0, 0);
    mi.col_model = Some(Arc::new(ColModel {
        bounding_sphere: Sphere::new(2.0, Vec3::ZERO, 0, 0),
        bounding_box: Box3 { min: Vec3::splat(-1.0), max: Vec3::splat(1.0), surface: 0, piece: 0 },
        spheres: vec![ball],
        ..Default::default()
    }));
    w
}

fn car_at(w: &mut World, pos: Vec3, speed: Vec3) -> EntityRef {
    let mut e = ent(EntityType::Vehicle, Kind::Vehicle(Box::default()), pos);
    e.base.model_index = CAR_MODEL;
    e.base.status = crate::entity::status::PHYSICS;
    let p = e.phys.as_mut().unwrap();
    p.mass = 1000.0;
    p.turn_mass = 4000.0;
    p.move_speed = speed;
    let r = w.insert(e).unwrap();
    w.add(r).unwrap();
    r
}

#[test]
fn a_car_driving_into_a_building_bounces_and_is_not_moved_into_it() {
    let mut w = world_with_car_model();
    building(&mut w, Vec3::new(20.0, 20.0, 0.0));
    // the box spans x 15..25; the car sits just outside its face, heading in
    let r = car_at(&mut w, Vec3::new(13.9, 20.0, 0.0), Vec3::new(0.1, 0.0, 0.0));
    w.ents.timer.time_step = 1.0;
    w.process_collision(r, &mut Rng::default());
    let e = w.get(r).unwrap();
    // the move into the wall was undone and the car now rolls away from it
    assert!((e.base.position().x - 13.9).abs() < 1e-4);
    assert!(e.phys.as_ref().unwrap().move_speed.x < 0.1);
    assert!(e.base.flags.has_hit_wall);
    assert!(w.drain_events().iter().any(|ev| matches!(ev, WorldEvent::ReportCollision { .. })));
}

#[test]
fn a_free_car_just_moves_and_gets_refiled() {
    let mut w = world_with_car_model();
    let r = car_at(&mut w, Vec3::new(20.0, 20.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
    w.ents.timer.time_step = 1.0;
    // a 1 unit step is below the sub-stepping threshold for vehicles
    w.process_collision(r, &mut Rng::default());
    let e = w.get(r).unwrap();
    assert!((e.base.position().x - 21.0).abs() < 1e-4);
    assert!(e.base.flags.is_in_safe_position && !e.base.flags.is_stuck);
    assert!((e.phys.as_ref().unwrap().distance_travelled - 1.0).abs() < 1e-4);
    assert_eq!(w.get(r).unwrap().entry_info.len(), 1);
}

#[test]
fn a_fast_car_is_stepped_so_it_cannot_tunnel_through_a_wall() {
    let mut w = world_with_car_model();
    building(&mut w, Vec3::new(30.0, 20.0, 0.0));
    // 3 units per frame is more than the sphere's diameter, so only sub-stepping sees the wall in time
    let r = car_at(&mut w, Vec3::new(10.0, 20.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
    w.ents.timer.time_step = 3.0;
    let mut furthest = 0.0f32;
    for _ in 0..4 {
        w.process_collision(r, &mut Rng::default());
        // the box face is at x = 25 and the car's sphere has radius 1
        let x = w.get(r).unwrap().base.position().x;
        assert!(x < 24.5, "inside the building: {x}");
        furthest = furthest.max(x);
    }
    // it did drive up to the wall rather than stall at the start
    assert!(furthest > 15.0, "{furthest}");
    assert_eq!(w.ents.timer.time_step, 3.0);
}

#[test]
fn shifting_pushes_an_overlapping_car_out_of_a_building() {
    let mut w = world_with_car_model();
    building(&mut w, Vec3::new(20.0, 20.0, 0.0));
    // the sphere (radius 1) is 0.5 into the box face at x = 15
    let r = car_at(&mut w, Vec3::new(14.5, 20.0, 0.0), Vec3::ZERO);
    w.ents.timer.time_step = 1.0;
    w.process_shift(r, &mut Rng::default());
    let x = w.get(r).unwrap().base.position().x;
    assert!(x < 14.5, "pushed back out: {x}");
}

#[test]
fn two_cars_meeting_exchange_speed() {
    let mut w = world_with_car_model();
    let a = car_at(&mut w, Vec3::new(20.0, 20.0, 0.0), Vec3::new(0.5, 0.0, 0.0));
    let b = car_at(&mut w, Vec3::new(21.9, 20.0, 0.0), Vec3::ZERO);
    w.ents.timer.time_step = 1.0;
    w.process_collision(a, &mut Rng::default());
    let (va, vb) = (w.get(a).unwrap().phys.as_ref().unwrap().move_speed.x, w.get(b).unwrap().phys.as_ref().unwrap().move_speed.x);
    assert!(va < 0.5 && vb > 0.0, "{va} {vb}");
    assert!(w.get(a).unwrap().phys.as_ref().unwrap().damage_entity == Some(b));
}

#[test]
fn colliding_with_a_sleeping_car_wakes_it() {
    let mut w = world_with_car_model();
    let a = car_at(&mut w, Vec3::new(20.0, 20.0, 0.0), Vec3::new(0.5, 0.0, 0.0));
    let b = car_at(&mut w, Vec3::new(21.9, 20.0, 0.0), Vec3::ZERO);
    w.get_mut(b).unwrap().base.status = crate::entity::status::SIMPLE;
    w.ents.timer.time_step = 1.0;
    w.process_collision(a, &mut Rng::default());
    assert_eq!(w.get(b).unwrap().base.status, crate::entity::status::PHYSICS);
    assert!(w.drain_events().contains(&WorldEvent::SwitchToRealPhysics(b)));
}

#[test]
fn hitting_a_static_object_frees_it_and_puts_it_on_the_moving_list() {
    let mut w = world_with_car_model();
    let a = car_at(&mut w, Vec3::new(20.0, 20.0, 0.0), Vec3::new(0.5, 0.0, 0.0));
    let o = object(&mut w, Vec3::new(21.9, 20.0, 0.0));
    {
        let e = w.get_mut(o).unwrap();
        e.base.model_index = CAR_MODEL;
        e.base.flags.is_static = true;
        let p = e.phys.as_mut().unwrap();
        p.mass = 50.0;
        p.turn_mass = 50.0;
    }
    assert!(!w.moving_entities().contains(&o) || w.get(o).unwrap().base.flags.is_static);
    w.moving.retain(|&m| m != o);
    w.get_mut(o).unwrap().phys.as_mut().unwrap().in_moving_list = false;
    w.ents.timer.time_step = 1.0;
    w.process_collision(a, &mut Rng::default());
    let e = w.get(o).unwrap();
    assert!(!e.base.flags.is_static, "uproot limit 0 frees it");
    assert!(w.moving_entities().contains(&o) && e.phys.as_ref().unwrap().in_moving_list);
}

// ---- automobiles ----

use crate::vehicle::env::{CarEnv, CarEvent, NoHooks};

const LANDSTAL_MODEL: i16 = 12;

const HANDLING_CFG: &str = "LANDSTAL 1700.0 2.0 5.0 2.0 0.0 0.0 0.25 85 0.85 0.85 0.51 5 160.0 20.0 4 D 6.2 0.60 0 35.0 2.0 0.1 0.27 0.35 25000 0.25 -0.1 0.5 C00B 0 1\n;the end\n";

fn world_with_cars() -> World {
    let mut w = world();
    w.ents.handling = crate::vehicle::handling::HandlingMgr::new(&re3_formats::handling::parse_handling(HANDLING_CFG, Game::Gta3));
    w.ents.adhesive_limits = vec![vec![1.0; 6]; 6];
    let mi = w.ents.models.add_vehicle_model(LANDSTAL_MODEL as i32).unwrap();
    if let crate::modelinfo::Kind::Vehicle(v) = &mut mi.kind {
        v.wheel_scale = 0.7;
        v.num_doors = 4;
        v.handling_id = 0;
        v.wheel_positions = [Vec3::new(-0.9, 1.5, -0.2), Vec3::new(-0.9, -1.5, -0.2), Vec3::new(0.9, 1.5, -0.2), Vec3::new(0.9, -1.5, -0.2)];
        v.positions[0] = Vec3::new(0.0, 2.0, 0.0);
        v.node_positions[crate::vehicle::automobile::node::BONNET - 1] = Some(Vec3::new(0.0, 1.5, 0.3));
        v.node_positions[crate::vehicle::automobile::node::DOOR_LF - 1] = Some(Vec3::new(-1.0, 0.5, 0.2));
        v.num_colours = 2;
        v.colours1[0] = 3;
        v.colours1[1] = 7;
    }
    mi.col_model = Some(Arc::new(ColModel {
        bounding_sphere: Sphere::new(3.0, Vec3::ZERO, 0, 0),
        bounding_box: Box3 { min: Vec3::new(-1.0, -2.5, -0.5), max: Vec3::new(1.0, 2.5, 0.8), surface: 0, piece: 0 },
        spheres: vec![Sphere::new(0.6, Vec3::new(0.0, 1.0, 0.5), 0, 0), Sphere::new(0.6, Vec3::new(0.0, -1.0, 0.5), 0, 0)],
        ..Default::default()
    }));
    w
}

/// A road: the top of a big box at z = 0 around (20, 20).
fn road(w: &mut World) {
    building(w, Vec3::new(20.0, 20.0, -5.0));
}

fn env() -> CarEnv {
    CarEnv::default()
}

#[test]
fn a_new_car_has_suspension_lines_doors_and_the_handlings_mass() {
    let mut w = world_with_cars();
    let r = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    let e = w.get(r).unwrap();
    let p = e.phys.as_ref().unwrap();
    assert_eq!((p.mass, p.elasticity), (1700.0, 0.05));
    let Kind::Vehicle(v) = &e.kind else { panic!() };
    assert_eq!(v.num_max_passengers, 3);
    assert_eq!(e.base.status, crate::entity::status::SIMPLE);
    let crate::vehicle::base::VehicleSub::Car(c) = &v.sub else { panic!() };
    assert!(c.height_above_road > 0.0 && c.suspension_line_length[0] > c.suspension_spring_length[0]);
    // front doors swing outwards in opposite directions
    assert!(c.doors[crate::vehicle::automobile::door_id::FRONT_LEFT].min_angle < 0.0 && c.doors[crate::vehicle::automobile::door_id::FRONT_RIGHT].max_angle > 0.0);
    // the model's collision model got its four lines
    assert_eq!(w.ents.models.get(LANDSTAL_MODEL as i32).unwrap().col_model.as_ref().unwrap().lines.len(), 4);
    // the second car takes the next colour in the table
    let r2 = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    let c1 = |r| match &w.get(r).unwrap().kind { Kind::Vehicle(v) => v.current_colour1, _ => 0 };
    assert_ne!(c1(r), c1(r2));
}

#[test]
fn a_physics_car_settles_on_its_springs_above_the_road() {
    let mut w = world_with_cars();
    road(&mut w);
    let r = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    {
        let e = w.get_mut(r).unwrap();
        e.base.status = crate::entity::status::ABANDONED;
        e.base.set_position(Vec3::new(20.0, 20.0, 1.2));
        e.base.flags.uses_collision = true;
    }
    w.add(r).unwrap();
    w.ents.timer.time_step = 1.0;
    let (mut ev, mut hooks, mut rng) = (env(), NoHooks, Rng::default());
    for _ in 0..400 {
        w.process_control_car(r, &mut ev, &mut hooks, &mut rng);
        w.process_collision(r, &mut rng);
        ev.events.clear();
    }
    let e = w.get(r).unwrap();
    let z = e.base.position().z;
    // the body floats a little above the ground on its suspension, not through it
    assert!((z - 0.6).abs() < 0.1, "z = {z}");
    assert!(e.phys.as_ref().unwrap().move_speed.length() < 0.05);
}

#[test]
fn a_player_car_accelerates_forward_on_the_road() {
    let mut w = world_with_cars();
    road(&mut w);
    let r = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    {
        let e = w.get_mut(r).unwrap();
        e.base.status = crate::entity::status::PLAYER;
        e.base.set_position(Vec3::new(20.0, 17.5, 0.7));
        e.base.flags.uses_collision = true;
        if let Kind::Vehicle(v) = &mut e.kind {
            v.driver = Some(EntityRef { pool: PoolKind::Ped, handle: 1 });
        }
    }
    w.add(r).unwrap();
    w.ents.timer.time_step = 1.0;
    let mut ev = env();
    ev.pad.state.accelerate = 255;
    // the driver is the player's ped
    struct Player;
    impl crate::vehicle::env::CarHooks for Player {
        fn driver_controlled_by_player(&mut self, _: EntityRef) -> bool {
            true
        }
    }
    let (mut hooks, mut rng) = (Player, Rng::default());
    for _ in 0..60 {
        w.process_control_car(r, &mut ev, &mut hooks, &mut rng);
        w.process_collision(r, &mut rng);
        ev.events.clear();
    }
    let e = w.get(r).unwrap();
    assert!(e.base.position().y > 17.6, "drove forward: {:?}", e.base.position());
    let p = e.phys.as_ref().unwrap();
    assert!(p.move_speed.y > 0.0 && p.move_speed.x.abs() < p.move_speed.y);
}

#[test]
fn blowing_up_a_car_wrecks_it_and_asks_for_the_fireworks() {
    let mut w = world_with_cars();
    let r = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    let h = *w.ents.handling.get(0);
    let e = w.get_mut(r).unwrap();
    let mut p = crate::vehicle::automobile::Parts::of(r, e).unwrap();
    let mut events = vec![];
    assert!(p.blow_up_car(None, &h, LANDSTAL_MODEL, 1234, &mut events));
    assert_eq!((p.base.status, p.veh.health, p.veh.time_of_death), (crate::entity::status::WRECKED, 0.0, 1234));
    assert!(p.base.flags.render_scorched && !p.veh.flags.engine_on);
    p.finish();
    assert!(events.contains(&CarEvent::KillOccupants));
    assert!(events.iter().any(|e| matches!(e, CarEvent::Explosion { kind: crate::vehicle::env::ExplosionKind::Car, .. })));
    // the bonnet and a door have nodes, so they fly off
    assert!(events.iter().filter(|e| matches!(e, CarEvent::FlyingComponent { .. })).count() >= 2);
    // the world turns them into loose temp objects
    let mut env = CarEnv { events, ..Default::default() };
    w.ents.models.add_simple_model(crate::modelinfo::indices_gta3::mi::CAR_DOOR as i32).unwrap();
    w.flush_after_car_for_tests(&mut env);
    assert!(w.ents.pools.objects.used() >= 2);
}

#[test]
fn collision_damage_smashes_the_hit_part_and_costs_health() {
    use crate::vehicle::automobile::{damage::DamageCtx, piece, Parts};
    let mut w = world_with_cars();
    let r = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    let h = *w.ents.handling.get(0);
    let e = w.get_mut(r).unwrap();
    e.base.status = crate::entity::status::PHYSICS;
    let mut p = Parts::of(r, e).unwrap();
    let (player, timer, mut rng, mut events) = (Default::default(), Timer::default(), Rng::default(), vec![]);
    let mut ctx = DamageCtx { player: &player, source: None, on_screen: true, model_index: LANDSTAL_MODEL, player_vehicle_move_speed: 0.0, timer: &timer, rng: &mut rng, events: &mut events };
    p.phys.move_speed = Vec3::new(0.0, 1.0, 0.0);
    p.vehicle_damage(&h, 200.0, piece::BONNET, [0; 4], &mut ctx);
    // a hard hit to the bonnet progresses its damage and drains health
    assert!(p.car.damage.door_status(crate::vehicle::automobile::door_id::BONNET) > 0);
    assert!(p.veh.health < 1000.0);
    // a feeble hit changes nothing
    let before = p.veh.health;
    p.vehicle_damage(&h, 10.0, piece::BONNET, [0; 4], &mut ctx);
    assert_eq!(p.veh.health, before);
    p.finish();
    assert!(events.iter().any(|e| matches!(e, CarEvent::Particle(_))));
}

#[test]
fn a_car_is_levelled_onto_the_road_and_doors_check_for_room() {
    let mut w = world_with_cars();
    road(&mut w);
    let r = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    w.get_mut(r).unwrap().base.set_position(Vec3::new(20.0, 20.0, 3.0));
    w.place_car_on_road_properly(r);
    let (z, height) = {
        let e = w.get(r).unwrap();
        let Kind::Vehicle(v) = &e.kind else { panic!() };
        let crate::vehicle::base::VehicleSub::Car(c) = &v.sub else { panic!() };
        (e.base.position().z, c.height_above_road)
    };
    assert!((z - height).abs() < 1e-3, "{z} vs {height}");
    assert!(w.get(r).unwrap().base.matrix.up.z > 0.99);

    w.add(r).unwrap();
    // beside the left front door on the open road: room; the door is below the car's seat
    let door = Vec3::new(18.5, 20.5, 0.9);
    assert!(w.is_room_for_ped_to_leave_car(r, crate::vehicle::automobile::node::DOOR_LF, door, None));
    // a wall right outside the door leaves no room
    building(&mut w, Vec3::new(14.5, 20.5, 3.0));
    assert!(!w.is_room_for_ped_to_leave_car(r, crate::vehicle::automobile::node::DOOR_LF, Vec3::new(17.8, 20.5, 0.9), None));
}

// ---- boats ----

const BOAT_MODEL: i16 = crate::modelinfo::indices_gta3::mi::SPEEDER;

#[derive(Debug)]
struct Sea(f32);
impl crate::vehicle::floater::WaterLevel for Sea {
    fn height(&self, _: f32, _: f32, _: f32, _: bool) -> Option<f32> {
        Some(self.0)
    }
}

fn world_with_boat() -> World {
    let mut w = world_with_cars();
    w.ents.handling = crate::vehicle::handling::HandlingMgr::new(&re3_formats::handling::parse_handling(
        "LANDSTAL 1700.0 2.0 5.0 2.0 0.0 0.0 0.25 85 0.85 0.85 0.51 5 160.0 20.0 4 D 6.2 0.60 0 35.0 2.0 0.1 0.27 0.35 25000 0.25 -0.1 0.5 C00B 0 1\nSPEEDER 1200.0 2.4 6.0 2.0 0.0 0.0 0.3 55 0.8 0.8 0.5 1 200.0 25.0 4 P 2.0 0.5 0 40.0 1.5 0.2 0.0 0.5 20000 0.0 0.0 0.5 0 0 0\n;the end\n",
        Game::Gta3,
    ));
    let mi = w.ents.models.add_vehicle_model(BOAT_MODEL as i32).unwrap();
    if let crate::modelinfo::Kind::Vehicle(v) = &mut mi.kind {
        v.handling_id = crate::vehicle::handling::HandlingMgr::handling_id("SPEEDER") as i32;
        v.num_doors = 2;
    }
    mi.col_model = Some(Arc::new(ColModel {
        bounding_sphere: Sphere::new(4.0, Vec3::ZERO, 0, 0),
        bounding_box: Box3 { min: Vec3::new(-1.2, -3.0, -0.6), max: Vec3::new(1.2, 3.0, 1.2), surface: 0, piece: 0 },
        ..Default::default()
    }));
    w.ents.water = Box::new(Sea(0.0));
    w
}

#[test]
fn a_boat_floats_up_to_the_water_and_wakes_when_driven() {
    let mut w = world_with_boat();
    let r = w.new_boat(BOAT_MODEL, 1, &mut Rng::default()).unwrap();
    {
        let e = w.get_mut(r).unwrap();
        e.base.set_position(Vec3::new(20.0, 20.0, -1.5));
        e.base.status = crate::entity::status::ABANDONED;
    }
    w.add(r).unwrap();
    w.ents.timer.time_step = 1.0;
    let (mut ev, mut hooks, mut rng) = (CarEnv::default(), NoHooks, Rng::default());
    for _ in 0..300 {
        w.process_control_boat(r, &mut ev, &mut hooks, &mut rng);
        let e = w.get_mut(r).unwrap();
        let p = e.phys.as_mut().unwrap();
        let step = 1.0;
        let m = &mut e.base.matrix;
        p.apply_move_speed(m, step);
        ev.events.clear();
    }
    let e = w.get(r).unwrap();
    // it has risen from 1.5 under to roughly floating level, and is anchored (no drift)
    let z = e.base.position().z;
    assert!(z > -1.0 && z < 1.5, "z = {z}");
    assert!(e.phys.as_ref().unwrap().move_speed.x == 0.0);

    // a player driver opens the throttle: the boat accelerates and lays a wake
    {
        let e = w.get_mut(r).unwrap();
        e.base.status = crate::entity::status::PLAYER;
    }
    ev.pad.state.accelerate = 255;
    ev.player.coors = Vec3::new(20.0, 20.0, 0.0);
    for _ in 0..200 {
        w.process_control_boat(r, &mut ev, &mut hooks, &mut rng);
        let e = w.get_mut(r).unwrap();
        let p = e.phys.as_mut().unwrap();
        p.apply_move_speed(&mut e.base.matrix, 1.0);
        ev.events.clear();
    }
    let e = w.get(r).unwrap();
    let Kind::Vehicle(v) = &e.kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Boat(b) = &v.sub else { panic!() };
    assert!(e.phys.as_ref().unwrap().move_speed.y > 0.0, "{:?}", e.phys.as_ref().unwrap().move_speed);
    assert!(b.num_wake_points > 0 && !b.is_anchored);
}

#[test]
fn boat_wake_points_age_and_drop_out() {
    use crate::vehicle::boat::BoatData;
    let mut rng = Rng::default();
    let mut w = world_with_boat();
    let r = w.new_boat(BOAT_MODEL, 1, &mut rng).unwrap();
    let Kind::Vehicle(v) = &mut w.get_mut(r).unwrap().kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Boat(b) = &mut v.sub else { panic!() };
    let b: &mut BoatData = b;
    b.add_wake_point(Vec3::ZERO, Vec3::ZERO);
    b.add_wake_point(Vec3::new(0.5, 0.0, 0.0), Vec3::new(0.5, 0.0, 0.0));
    assert_eq!(b.num_wake_points, 2);
    assert!(b.vertex_affected_by_wake(Vec3::new(0.2, 0.0, 0.0)) > 0.0);
    b.prune_wake_trail(500.0);
    assert_eq!(b.num_wake_points, 0);
    assert_eq!(b.vertex_affected_by_wake(Vec3::ZERO), 0.0);
}

// ---- helicopters ----

use crate::vehicle::heli::{HeliStatus, HeliType};

fn world_with_heli() -> World {
    let mut w = world_with_cars();
    for id in [crate::modelinfo::indices_gta3::mi::CHOPPER, crate::modelinfo::indices_gta3::mi::ESCAPE] {
        let mi = w.ents.models.add_vehicle_model(id as i32).unwrap();
        if let crate::modelinfo::Kind::Vehicle(v) = &mut mi.kind {
            v.handling_id = 0;
            v.node_positions[crate::vehicle::heli::node::SKID_LEFT - 1] = Some(Vec3::new(-1.0, 0.0, -1.0));
            v.node_positions[crate::vehicle::heli::node::SKID_RIGHT - 1] = Some(Vec3::new(1.0, 0.0, -1.0));
            v.node_positions[crate::vehicle::heli::node::TOPROTOR - 1] = Some(Vec3::new(0.0, 0.0, 1.5));
            v.node_positions[crate::vehicle::heli::node::TAIL - 1] = Some(Vec3::new(0.0, -4.0, 0.5));
            v.node_positions[crate::vehicle::heli::node::BACKROTOR - 1] = Some(Vec3::new(0.0, -5.0, 0.5));
        }
    }
    w.ents.models.add_simple_model(crate::modelinfo::indices_gta3::mi::CAR_WHEEL as i32).unwrap();
    w
}

#[test]
fn a_police_heli_flies_to_the_player_and_hovers() {
    let mut w = world_with_heli();
    let mut env = CarEnv::default();
    env.player.coors = Vec3::new(20.0, 20.0, 0.0);
    env.player.num_helis_required = 1;
    env.player.chopper_loaded = true;
    w.ents.timer.time_in_ms = 1;
    let mut rng = Rng::default();
    w.update_helis(&mut env, &mut rng);
    let r = w.ents.helis.helis[0].expect("a police heli was spawned");
    assert_eq!(w.ents.helis.num_random_helis, 1);
    let start = w.get(r).unwrap().base.position();
    assert!((crate::math::magnitude(start - env.player.coors) - 250.0).abs() < 60.0);
    w.ents.timer.time_step = 1.0;
    for _ in 0..600 {
        env.events.clear();
        w.process_control_heli(r, &mut env, &mut rng);
    }
    let e = w.get(r).unwrap();
    let d = crate::math::magnitude(e.base.position() - env.player.coors);
    // it closed in a long way and its status moved from hover to chase
    assert!(d < crate::math::magnitude(start - env.player.coors) - 100.0, "{d}");
    let Kind::Vehicle(v) = &e.kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Heli(h) = &v.sub else { panic!() };
    assert_eq!(h.heli_type, HeliType::Random);
    assert!(matches!(h.status, HeliStatus::ChasePlayer | HeliStatus::Hover));
    // the player calms down: it is sent away
    env.player.num_helis_required = 0;
    w.update_helis(&mut env, &mut rng);
    let Kind::Vehicle(v) = &w.get(r).unwrap().kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Heli(h) = &v.sub else { panic!() };
    assert_eq!(h.status, HeliStatus::FlyAway);
}

#[test]
fn a_shot_down_heli_blows_up_in_two_stages_and_pays_out() {
    let mut w = world_with_heli();
    let mut env = CarEnv::default();
    env.player.coors = Vec3::new(20.0, 20.0, 0.0);
    env.player.num_helis_required = 1;
    env.player.chopper_loaded = true;
    w.ents.timer.time_in_ms = 1;
    let mut rng = Rng::default();
    w.update_helis(&mut env, &mut rng);
    let r = w.ents.helis.helis[0].unwrap();
    let pos = w.get(r).unwrap().base.position();
    // a rocket nearby brings it down
    assert!(w.test_heli_rocket_collision(pos + Vec3::new(1.0, 0.0, 0.0), &mut rng));
    assert!(!w.test_heli_rocket_collision(pos + Vec3::new(100.0, 0.0, 0.0), &mut rng));
    // first stage: 3 seconds before the end the tail and rotor fly off
    w.ents.timer.previous_time_in_ms = 1;
    w.ents.timer.time_in_ms = 4000;
    env.events.clear();
    w.update_helis(&mut env, &mut rng);
    assert!(w.ents.pools.objects.used() >= 2 && w.ents.helis.helis[0].is_some());
    // second stage: at the end it explodes and is removed
    w.ents.timer.previous_time_in_ms = 4000;
    w.ents.timer.time_in_ms = 12000;
    env.events.clear();
    w.update_helis(&mut env, &mut rng);
    assert!(w.ents.helis.helis[0].is_none() && w.ents.helis.num_random_helis == 0);
    assert!(env.events.iter().any(|e| matches!(e, CarEvent::HeliDestroyed { slot: 0, .. })));
    assert!(env.events.iter().any(|e| matches!(e, CarEvent::HeliExplosion { bomb: false, .. })));
    assert!(w.get(r).is_none());
    // bullets take 700 damage in total
    w.ents.timer.time_in_ms = 100_000;
    env.events.clear();
    w.ents.helis.test_for_new_random_helis_timer = 0;
    w.update_helis(&mut env, &mut rng);
    let r2 = w.ents.helis.helis[0].unwrap();
    let p2 = w.get(r2).unwrap().base.position();
    let line = (p2 + Vec3::new(-50.0, 0.0, 0.0), p2 + Vec3::new(50.0, 0.0, 0.0));
    for _ in 0..7 {
        assert!(w.test_heli_bullet_collision(line.0, line.1, 100, &mut rng).is_some());
    }
    let Kind::Vehicle(v) = &w.get(r2).unwrap().kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Heli(h) = &v.sub else { panic!() };
    assert_ne!(h.status, HeliStatus::ShotDown, "700 is not over the limit");
    assert!(w.test_heli_bullet_collision(line.0, line.1, 100, &mut rng).is_some());
    let Kind::Vehicle(v) = &w.get(r2).unwrap().kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Heli(h) = &v.sub else { panic!() };
    assert_eq!(h.status, HeliStatus::ShotDown);
}

// ---- planes ----

use crate::vehicle::plane::FlightPath;

#[test]
fn a_dodo_follows_its_flight_path_and_a_rocket_brings_it_down() {
    let mut w = world_with_cars();
    let dodo = crate::modelinfo::indices_gta3::mi::DEADDODO;
    let mi = w.ents.models.add_vehicle_model(dodo as i32).unwrap();
    if let crate::modelinfo::Kind::Vehicle(v) = &mut mi.kind {
        v.handling_id = 0;
    }
    // a 4 x 1000 unit square at height 100
    w.ents.planes.paths[1] = FlightPath::load(&[[0.0, 0.0, 100.0], [1000.0, 0.0, 100.0], [1000.0, 1000.0, 100.0], [0.0, 1000.0, 100.0]], true);
    w.ents.planes.durations[1] = w.ents.planes.paths[1].total_length / crate::vehicle::plane::CRUISE_SPEED;
    let mut rng = Rng::default();
    let r = w.new_plane(dodo, crate::vehicle::base::PERMANENT_VEHICLE, &mut rng).unwrap();
    w.add(r).unwrap();
    assert!(w.big_buildings(0).contains(&r), "planes live in the big-building list");
    let mut env = CarEnv::default();
    env.camera_pos = Vec3::new(100.0, 100.0, 0.0);
    w.ents.timer.time_in_ms = 10_000;
    w.ents.timer.frame_counter = 0;
    w.update_planes(false);
    w.process_control_plane(r, &mut env, &mut rng);
    let e = w.get(r).unwrap();
    // it is on the first leg (y ~ 0), heading along +x, banked flat, at the path height + 1
    let p = e.base.position();
    assert!(p.y.abs() < 1.0 && (p.z - 101.0).abs() < 0.5 && p.x > 0.0 && p.x < 1000.0, "{p:?}");
    assert!(e.base.matrix.forward.x > 0.99);
    assert!(e.phys.as_ref().unwrap().move_speed.x > 0.0);
    // time passes: it moves along
    w.ents.timer.time_in_ms = 20_000;
    w.update_planes(false);
    w.process_control_plane(r, &mut env, &mut rng);
    assert_ne!(w.get(r).unwrap().base.position().x, p.x);

    // a rocket 10 units away hits it; 20 frames later it explodes, 62 frames later it is removed
    assert!(!w.test_plane_rocket_collision(Vec3::new(5000.0, 0.0, 0.0)));
    let pos = w.get(r).unwrap().base.position();
    w.ents.timer.frame_counter = 1000;
    assert!(w.test_plane_rocket_collision(pos + Vec3::new(10.0, 0.0, 0.0)));
    assert!(!w.test_plane_rocket_collision(pos + Vec3::new(10.0, 0.0, 0.0)), "already hit");
    for f in 1001..=1062 {
        w.ents.timer.frame_counter = f;
        env.events.clear();
        w.process_control_plane(r, &mut env, &mut rng);
        if f == 1020 {
            assert!(env.events.iter().any(|e| matches!(e, CarEvent::HeliExplosion { .. })));
        }
    }
    let e = w.get(r).unwrap();
    assert!(e.base.flags.render_scorched && e.base.flags.remove_from_world);
    assert!(env.events.iter().any(|e| matches!(e, CarEvent::CameraFlash { rgb: [200, 200, 200] })));
}

// ---- trains ----

#[test]
fn wagons_follow_the_engine_open_their_doors_in_stations_and_hit_what_is_in_front() {
    let mut w = world_with_car_model();
    let train = crate::modelinfo::indices_gta3::mi::TRAIN;
    let mi = w.ents.models.add_vehicle_model(train as i32).unwrap();
    mi.col_model = Some(Arc::new(ColModel { bounding_sphere: Sphere::new(12.0, Vec3::ZERO, 0, 0), bounding_box: Box3 { min: Vec3::new(-1.5, -10.0, -1.0), max: Vec3::new(1.5, 10.0, 2.0), surface: 0, piece: 0 }, ..Default::default() }));
    // a straight track in the El-Train's area, 8000 units long
    let pts: Vec<[f32; 3]> = (0..9).map(|i| [-1900.0 + i as f32 * 20.0, 0.0, 10.0]).collect();
    // too short for the stations at 873+, so stretch it: nodes every 1000 along y instead
    let pts: Vec<[f32; 3]> = (0..9).map(|i| [300.0, -1900.0 + i as f32 * 400.0, 10.0]).collect::<Vec<_>>().into_iter().chain(pts.into_iter().take(0)).collect();
    let sub: Vec<[f32; 3]> = (0..9).map(|i| [-300.0, -1900.0 + i as f32 * 450.0, 10.0]).collect();
    let mut rng = Rng::default();
    w.init_trains([pts, sub], &mut rng);
    assert_eq!(w.ents.pools.vehicles.used(), 13);
    assert!(w.ents.trains.el.path.total_length > 3000.0);

    let mut env = CarEnv::default();
    env.camera_pos = Vec3::new(300.0, -900.0, 10.0);
    // the first stop of the El-Train (station 873 along the track)
    let stop = *w.ents.trains.el.lines.iter().find(|l| l.kind == 0).unwrap();
    let d = w.ents.trains.el.total_duration;
    w.ents.timer.time_in_ms = ((stop.time + 5.0) / d * 0x20000 as f32) as u32;
    w.update_trains(env.camera_pos, &mut env);
    assert_eq!((w.ents.trains.engine_speed[0], w.ents.trains.engine_position[0]), (0.0, 873.0));
    let wagons: Vec<EntityRef> = w.ents.pools.vehicles.iter().map(|(i, _)| EntityRef { pool: PoolKind::Vehicle, handle: w.ents.pools.vehicles.handle_of(i) }).collect();
    let first = wagons[0];
    w.ents.timer.frame_counter = 0;
    w.process_control_train(first, &mut env);
    let e = w.get(first).unwrap();
    // the train is on the track (x = 300 -+ 0.9 is the centre shift) and has stopped
    assert!((e.base.position().x - 300.0).abs() < 2.0, "{:?}", e.base.position());
    assert_eq!(e.base.status, crate::entity::status::TRAIN_NOT_MOVING);
    let Kind::Vehicle(v) = &e.kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Train(t) = &v.sub else { panic!() };
    assert!(t.train_stopping && t.door_state == crate::vehicle::train::DoorState::Opening);
    // a second later the doors are fully open
    w.ents.timer.time_in_ms += 1200;
    w.ents.timer.frame_counter = 16;
    w.process_control_train(first, &mut env);
    let Kind::Vehicle(v) = &w.get(first).unwrap().kind else { panic!() };
    let crate::vehicle::base::VehicleSub::Train(t) = &v.sub else { panic!() };
    assert_eq!(t.door_state, crate::vehicle::train::DoorState::Open);
    assert!(t.doors[0].is_fully_open());
    // passengers take their door seat, or the first free one
    let ped = EntityRef { pool: PoolKind::Ped, handle: 5 };
    w.train_add_passenger(first, ped, 3);
    w.train_add_passenger(first, EntityRef { pool: PoolKind::Ped, handle: 6 }, 3);
    let Kind::Vehicle(v) = &w.get(first).unwrap().kind else { panic!() };
    assert_eq!((v.passengers[3], v.passengers[0].is_some(), v.num_passengers), (Some(ped), true, 2));

    // while moving, the front of the first wagon marks nearby cars as hit by the train
    let victim = car_at(&mut w, Vec3::new(300.0, 0.0, 10.0), Vec3::ZERO);
    w.ents.timer.time_in_ms = ((stop.time + 40.0) / d * 0x20000 as f32) as u32;
    w.update_trains(env.camera_pos, &mut env);
    assert!(w.ents.trains.engine_speed[0] > 0.0);
    w.ents.timer.frame_counter = 32;
    w.process_control_train(first, &mut env);
    let p = w.get(first).unwrap().base.position();
    w.get_mut(victim).unwrap().base.set_position(p + Vec3::new(0.0, 12.0, 0.0));
    w.remove_and_add(victim).unwrap();
    w.ents.timer.frame_counter = 48;
    w.process_control_train(first, &mut env);
    assert!(w.get(victim).unwrap().phys.as_ref().unwrap().flags.hit_by_train);
}

// ---- cranes ----

#[test]
fn a_crane_finds_a_parked_car_lifts_it_across_and_sets_it_down() {
    use crate::vehicle::cranes::CraneState;
    let mut w = world_with_car_model();
    let crane_model: i16 = 60;
    let magnet: i16 = 61;
    {
        let col = w.ents.models.get(BOX_MODEL as i32).and_then(|m| m.col_model.clone());
        w.ents.models.add_simple_model(crane_model as i32).unwrap().col_model = col;
    }
    let b = Box3 { min: Vec3::splat(-0.5), max: Vec3::splat(0.5), surface: 0, piece: 0 };
    w.ents.models.add_simple_model(magnet as i32).unwrap().col_model = Some(Arc::new(ColModel { bounding_sphere: Sphere::new(1.0, Vec3::ZERO, 0, 0), bounding_box: b, boxes: vec![b], ..Default::default() }));
    w.ents.indices.match_model_string("cranetopb", crane_model);
    w.ents.indices.match_model_string("magnet", magnet);
    let mut rng = Rng::default();
    // a crane top: tops always update
    let cr = w.insert({
        let mut e = ent(EntityType::Building, Kind::Building, Vec3::new(20.0, 100.0, 0.0));
        e.base.model_index = crane_model;
        e
    }).unwrap();
    w.add(cr).unwrap();
    w.init_cranes(&mut rng);
    assert_eq!(w.ents.cranes.cranes.len(), 1);
    assert!(w.ents.cranes.cranes[0].hook.is_some(), "a top gets a hook");

    // a parked car in the pickup box
    let car = car_at(&mut w, Vec3::new(50.0, 100.0, 0.7), Vec3::ZERO);
    w.get_mut(car).unwrap().base.status = crate::entity::status::ABANDONED;
    w.activate_crane(40.0, 60.0, 90.0, 110.0, Vec3::new(20.0, 130.0, 0.0), 0.0, false, false, 20.0, 100.0);
    assert_eq!(w.ents.cranes.cranes[0].status, crate::vehicle::cranes::CraneStatus::Activated);

    let mut env = CarEnv::default();
    env.player.game_cam_pos = Vec3::new(20.0, 100.0, 0.0);
    env.player.coors = Vec3::new(500.0, 500.0, 0.0);
    w.ents.timer.time_step = 1.0;
    let mut seen = std::collections::BTreeSet::new();
    for frame in 0..30_000u32 {
        w.ents.timer.time_in_ms = frame * 20;
        env.events.clear();
        w.update_cranes(&mut env, false);
        seen.insert(format!("{:?}", w.ents.cranes.cranes[0].state));
        if w.ents.cranes.cranes[0].state == CraneState::DroppingTarget {
            break;
        }
    }
    // it went through the whole sequence and the car is at the drop-off with collision back on
    for s in ["GoingTowardsTarget", "GoingTowardsTargetOnlyHeight", "LiftingTarget", "RotatingTarget", "DroppingTarget"] {
        assert!(seen.contains(s), "never reached {s}: {seen:?}");
    }
    let e = w.get(car).unwrap();
    assert!(e.base.flags.uses_collision);
    let p = e.base.position();
    assert!((p.x - 20.0).abs() < 4.0 && (p.y - 130.0).abs() < 4.0, "{p:?}");
    assert!(w.ents.cranes.cranes[0].vehicle_picked_up.is_none());
    assert_eq!(w.ents.cranes.cranes[0].vehicles_collected, 1);
}

// ---- car generators ----

#[test]
fn a_switched_on_generator_makes_a_parked_car_when_the_player_comes_near() {
    let mut w = world_with_cars();
    road(&mut w);
    let mut rng = Rng::default();
    let i = w.create_car_generator(Vec3::new(20.0, 20.0, 5.0), 90.0, LANDSTAL_MODEL as i32, 4, 9, 0, 100, 100, 0, 0).unwrap();
    w.ents.timer.time_in_ms = 10;
    w.ents.cargens.switch_on(i, 10);
    let mut env = CarEnv::default();
    env.generation_dist_multiplier = 1.0;
    // the player is 110 away and driving towards the spot
    env.player_centre = Vec3::new(20.0, 130.0, 0.0);
    env.player.speed = Vec3::new(0.0, -1.0, 0.0);
    let mut hooks = NoHooks;
    w.ents.timer.time_in_ms = 100;
    for _ in 0..4 {
        w.process_car_generators(&env, &mut hooks, &mut rng, false);
    }
    let g = &w.ents.cargens.generators[i];
    assert_ne!(g.vehicle_handle, -1, "a car was made");
    let r = EntityRef { pool: PoolKind::Vehicle, handle: g.vehicle_handle };
    let e = w.get(r).unwrap();
    assert_eq!(e.base.status, crate::entity::status::ABANDONED);
    let Kind::Vehicle(v) = &e.kind else { panic!() };
    // 100% chance of an alarm and locked doors; the requested colours
    assert!(v.alarm_state == -1 && v.door_lock == crate::vehicle::base::CarLock::Locked && (v.current_colour1, v.current_colour2) == (4, 9));
    assert!(!v.flags.engine_on && v.created_by == crate::vehicle::base::PARKED_VEHICLE);
    // it stands on the ground
    assert!((e.base.position().z - 0.0).abs() < 2.5, "{:?}", e.base.position());
    // an unlimited generator keeps its uses
    assert_eq!(g.uses_remaining, u16::MAX);
    // once the player takes the car the generator lets go of it and waits a minute
    let before = w.ents.cargens.generators[i].timer;
    w.get_mut(r).unwrap().base.status = crate::entity::status::PLAYER;
    for _ in 0..4 {
        w.process_car_generators(&env, &mut hooks, &mut rng, false);
    }
    let g = &w.ents.cargens.generators[i];
    assert!(g.is_blocking && g.timer >= before + 60000);
}

// ---- process and explosions ----

#[test]
fn the_world_loop_settles_a_car_and_wakes_nothing_else() {
    use super::process::WorldHooks as _;
    let mut w = world_with_cars();
    road(&mut w);
    let r = w.new_car(LANDSTAL_MODEL, 1, &mut Rng::default()).unwrap();
    {
        let e = w.get_mut(r).unwrap();
        e.base.status = crate::entity::status::ABANDONED;
        e.base.set_position(Vec3::new(20.0, 20.0, 1.2));
        e.base.flags.uses_collision = true;
    }
    w.add(r).unwrap();
    w.ents.timer.time_step = 1.0;
    let (mut ev, mut hooks, mut rng) = (env(), NoHooks, Rng::default());
    for _ in 0..400 {
        w.process(&mut ev, &mut hooks, &mut rng);
        w.ents.timer.frame_counter += 1;
        ev.events.clear();
    }
    let z = w.get(r).unwrap().base.position().z;
    assert!((z - 0.6).abs() < 0.1, "z = {z}");
    let _ = hooks.closest_path_node(true, Vec3::ZERO);
}

#[test]
fn an_explosion_throws_and_damages_what_is_in_range_only() {
    let mut w = world_with_cars();
    let mut rng = Rng::default();
    let near = w.new_car(LANDSTAL_MODEL, 1, &mut rng).unwrap();
    let far = w.new_car(LANDSTAL_MODEL, 1, &mut rng).unwrap();
    for (r, x) in [(near, 23.0), (far, 60.0)] {
        let e = w.get_mut(r).unwrap();
        e.base.status = crate::entity::status::PHYSICS;
        e.base.set_position(Vec3::new(x, 20.0, 1.0));
        w.add(r).unwrap();
    }
    let health = |w: &World, r| match &w.get(r).unwrap().kind {
        Kind::Vehicle(v) => v.health,
        _ => unreachable!(),
    };
    let before = health(&w, near);
    w.trigger_explosion(&mut rng, Vec3::new(20.0, 20.0, 1.0), 10.0, 300.0, None, false);
    assert!(health(&w, near) < before && health(&w, far) == before);
    let ms = w.get(near).unwrap().phys.as_ref().unwrap().move_speed;
    assert!(ms.x > 0.0 && ms.z >= 0.0, "pushed away from the blast: {ms:?}");
    assert_eq!(w.get(far).unwrap().phys.as_ref().unwrap().move_speed, Vec3::ZERO);
}

#[test]
fn fire_helpers_report_events_and_area_clearing_deletes_cars() {
    let mut w = world_with_cars();
    let mut rng = Rng::default();
    let c = w.new_car(LANDSTAL_MODEL, 1, &mut rng).unwrap();
    w.get_mut(c).unwrap().base.set_position(Vec3::new(20.0, 20.0, 1.0));
    w.add(c).unwrap();
    w.set_cars_on_fire(Vec3::new(21.0, 20.0, 1.0), 4.0, None);
    w.extinguish_all_car_fires_in_area(Vec3::new(20.0, 20.0, 1.0), 5.0);
    let evs = w.drain_events();
    assert!(evs.contains(&WorldEvent::StartFire { target: c, culprit: None }) && evs.contains(&WorldEvent::ExtinguishCarFire(c)));
    w.clear_cars_from_area(Vec3::splat(0.0), Vec3::splat(100.0), &mut NoHooks);
    assert!(w.get(c).is_none());
}

#[test]
fn a_rocket_explosion_goes_through_the_world_and_a_barrel_chains() {
    use crate::weapons::explosion::ExplosionType;
    let mut w = world_with_cars();
    let mut rng = Rng::default();
    let barrel_model = 20;
    w.ents.models.add_simple_model(barrel_model).unwrap();
    if let Indices::Gta3(i) = &mut w.ents.indices {
        i.match_model_string("barrel4", barrel_model as i16);
    }
    let car = w.new_car(LANDSTAL_MODEL, 1, &mut rng).unwrap();
    w.get_mut(car).unwrap().base.status = crate::entity::status::PHYSICS;
    w.get_mut(car).unwrap().base.set_position(Vec3::new(24.0, 20.0, 1.0));
    w.add(car).unwrap();
    let barrel = object(&mut w, Vec3::new(20.0, 24.0, 1.0));
    w.get_mut(barrel).unwrap().base.model_index = barrel_model as i16;
    w.get_mut(barrel).unwrap().base.flags.is_static = true;
    assert!(w.add_explosion(&mut rng, Vec3::ZERO, None, None, ExplosionType::Rocket, Vec3::new(20.0, 20.0, 1.0), 0));
    assert!(matches!(&w.get(barrel).unwrap().kind, Kind::Object(o) if o.has_been_damaged));
    // the car was hurt and the barrel's own explosion is now in the list
    assert!(matches!(&w.get(car).unwrap().kind, Kind::Vehicle(v) if v.health < 1000.0));
    assert!(w.ents.explosions.list.iter().filter(|e| e.iteration != 0).count() == 2);
}

#[test]
fn restart_clears_vehicles_and_temp_objects_and_signs_drop_to_the_ground() {
    let mut w = world_with_cars();
    road(&mut w);
    let mut rng = Rng::default();
    let car = w.new_car(LANDSTAL_MODEL, 1, &mut rng).unwrap();
    let o = object(&mut w, Vec3::new(30.0, 30.0, 1.0));
    if let Kind::Object(d) = &mut w.get_mut(o).unwrap().kind {
        d.created_by = CreatedBy::Temp;
    }
    // a dummy of a repositioned model hangs in the air and lands on the road
    let id = 22;
    w.ents.models.add_simple_model(id).unwrap();
    if let Indices::Gta3(i) = &mut w.ents.indices {
        i.match_model_string("trafficcone", id as i16);
    }
    let dummy = w.insert(ent(EntityType::Dummy, Kind::Dummy, Vec3::new(20.0, 20.0, 1.5))).unwrap();
    w.get_mut(dummy).unwrap().base.model_index = id as i16;
    w.ents.models.get_mut(id).unwrap().col_model = Some(Arc::new(ColModel { bounding_box: Box3 { min: Vec3::new(-1.0, -1.0, -0.4), max: Vec3::splat(1.0), surface: 0, piece: 0 }, ..Default::default() }));
    w.reposition_certain_dynamic_objects();
    assert!((w.get(dummy).unwrap().base.position().z - 0.4).abs() < 1e-4);
    w.clear_for_restart();
    assert!(w.get(car).is_none() && w.get(o).is_none());
}
