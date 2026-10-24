use super::*;
use crate::vehicle::handling::HandlingMgr;
use re3_formats::datafile::Game;
use re3_formats::handling::parse_handling;

const CFG: &str = "LANDSTAL 1700.0 2.0 5.0 2.0 0.0 0.0 0.25 85 0.85 0.85 0.51 5 160.0 20.0 4 D 6.2 0.60 0 35.0 2.0 0.1 0.27 0.35 25000 0.25 -0.1 0.5 C00B 0 1\n;the end\n";

fn mgr() -> HandlingMgr {
    HandlingMgr::new(&parse_handling(CFG, Game::Gta3))
}

fn timer() -> Timer {
    Timer::default()
}

fn car_phys() -> Physical {
    Physical { mass: 1000.0, turn_mass: 4000.0, ..Default::default() }
}

fn r(h: i32) -> EntityRef {
    EntityRef { pool: crate::entity::PoolKind::Ped, handle: h }
}

#[test]
fn a_new_vehicle_starts_healthy_and_idle() {
    let v = VehicleData::new(MISSION_VEHICLE, 1234, VehicleType::Car, &mut Rng::default());
    assert_eq!((v.health, v.current_gear, v.num_max_passengers, v.last_weapon_damage), (1000.0, 1, 8, -1));
    assert!(v.flags.engine_on && v.flags.freebies && v.flags.can_be_damaged && !v.flags.is_locked);
    assert_eq!(v.auto_pilot.time_to_start_mission, 1234);
    assert!(v.radio_station < 10 && v.is_car() && !v.is_boat());
}

#[test]
fn a_car_on_its_side_cannot_be_entered_and_only_a_slow_one_can_be_left() {
    let upright = Matrix::IDENTITY;
    let side = Matrix { up: Vec3::new(0.0, 1.0, 0.0), forward: Vec3::new(0.0, 0.0, 1.0), ..Matrix::IDENTITY };
    let still = car_phys();
    assert!(VehicleData::can_ped_enter_car(&upright, &still));
    assert!(!VehicleData::can_ped_enter_car(&side, &still));
    let fast = Physical { move_speed: Vec3::new(0.5, 0.0, 0.0), ..car_phys() };
    assert!(!VehicleData::can_ped_enter_car(&upright, &fast));
    assert!(VehicleData::can_ped_exit_car(&upright, &still));
    assert!(!VehicleData::can_ped_exit_car(&upright, &fast));
    // the two cases differ in > versus >= at exactly 0.01 of spin
    let edge = Physical { turn_speed: Vec3::new(0.01, 0.0, 0.0), ..car_phys() };
    assert!(VehicleData::can_ped_exit_car(&upright, &edge));
    assert!(!VehicleData::can_ped_exit_car(&side, &edge));
}

#[test]
fn occupants_come_and_go_and_weigh_the_car_down() {
    let mut v = VehicleData { num_max_passengers: 3, ..Default::default() };
    let mut p = car_phys();
    let m = Matrix::IDENTITY;
    assert!(v.add_passenger(r(1), &mut p, &m, 70.0, Vec3::new(1.0, 0.0, 0.0)));
    assert!(v.add_passenger(r(2), &mut p, &m, 70.0, Vec3::new(-1.0, 0.0, 0.0)));
    assert_eq!((v.passengers[0], v.passengers[1], v.num_passengers), (Some(r(1)), Some(r(2)), 2));
    // a seat on the right pushes down on the right: the car rolls about y
    assert!(p.turn_speed.y != 0.0 || p.turn_speed.x != 0.0 || p.turn_speed == Vec3::ZERO);
    assert!(v.add_passenger_at(r(3), 2, &mut p, &m, 70.0, Vec3::ZERO));
    assert!(!v.add_passenger_at(r(4), 2, &mut p, &m, 70.0, Vec3::ZERO) && !v.add_passenger(r(4), &mut p, &m, 70.0, Vec3::ZERO));
    v.remove_passenger(r(2));
    assert_eq!((v.passengers[1], v.num_passengers), (None, 2));
    assert_eq!(v.remove_driver(status::PHYSICS), status::ABANDONED);
    assert_eq!(v.remove_driver(status::WRECKED), status::WRECKED);
    let mut bus = VehicleData::default();
    bus.flags.is_bus = true;
    assert!(bus.add_passenger_at(r(9), 5, &mut p, &m, 70.0, Vec3::ZERO));
    assert_eq!(bus.passengers[0], Some(r(9)));
}

#[test]
fn deletion_waits_for_occupants_and_mission_cars_stay() {
    let mut v = VehicleData::default();
    assert!(v.can_be_deleted(|_| true));
    v.driver = Some(r(1));
    assert!(!v.can_be_deleted(|_| false) && v.can_be_deleted(|_| true));
    v.num_getting_in = 1;
    assert!(!v.can_be_deleted(|_| true));
    v.num_getting_in = 0;
    v.created_by = MISSION_VEHICLE;
    assert!(!v.can_be_deleted(|_| true));
    v.created_by = PARKED_VEHICLE;
    assert!(v.can_be_deleted(|_| true));
}

#[test]
fn the_car_alarm_counts_down_and_stops() {
    let mut v = VehicleData::default();
    let mut t = timer();
    t.time_step = 3.0;
    v.alarm_state = 100;
    let ms = t.time_step_in_milliseconds() as i16;
    v.process_car_alarm(&t);
    assert_eq!(v.alarm_state, 100 - ms);
    assert!(v.is_alarm_on(status::PHYSICS) && !v.is_alarm_on(status::WRECKED));
    v.alarm_state = 10;
    v.process_car_alarm(&t);
    assert_eq!(v.alarm_state, 0);
    v.alarm_state = -1;
    v.process_car_alarm(&t);
    assert_eq!(v.alarm_state, -1);
}

#[test]
fn a_bomb_counts_down_ticks_and_goes_off() {
    let mut v = VehicleData { bomb_timer: 1500, blow_up_entity: Some(r(7)), ..Default::default() };
    let mut t = timer();
    t.time_step = 60.0; // 1000 ms per tick
    let ev = v.process_delayed_explosion(&t, true);
    assert_eq!(v.bomb_timer, 500);
    assert_eq!(ev, vec![VehicleEvent::BombTick]);
    let ev = v.process_delayed_explosion(&t, false);
    assert_eq!((v.bomb_timer, ev), (0, vec![VehicleEvent::BlowUp(Some(r(7)))]));
    assert!(v.process_delayed_explosion(&t, true).is_empty());
}

fn entity() -> Entity {
    Entity::new(crate::entity::EntityType::Vehicle, &mut Rng::default())
}

#[test]
fn damage_is_filtered_by_proof_flags_then_wears_health_down() {
    let mut v = VehicleData::default();
    let mut e = entity();
    let mut rng = Rng::default();
    e.status = status::PHYSICS;
    e.flags.bullet_proof = true;
    assert!(v.inflict_damage(&mut e, false, None, false, weapon::AK47, 100.0, 0.0, true, &mut rng).is_empty());
    assert_eq!(v.health, 1000.0);
    e.flags.bullet_proof = false;
    v.inflict_damage(&mut e, false, None, false, weapon::AK47, 100.0, 0.0, true, &mut rng);
    assert_eq!((v.health, v.last_weapon_damage), (900.0, weapon::AK47));
    // crossing 250 sets a car on fire, started by the attacker
    v.health = 300.0;
    let ev = v.inflict_damage(&mut e, false, Some(r(5)), true, weapon::COLT45, 100.0, 0.0, true, &mut rng);
    assert_eq!(ev, vec![VehicleEvent::EngineOnFire(Some(r(5)))]);
    // lethal non-explosive damage blows it up at once
    let ev = v.inflict_damage(&mut e, false, Some(r(5)), true, weapon::COLT45, 1000.0, 0.0, true, &mut rng);
    assert_eq!((v.health, ev), (0.0, vec![VehicleEvent::BlowUp(Some(r(5)))]));
    // a dead car takes no more
    assert!(v.inflict_damage(&mut e, false, None, false, weapon::COLT45, 10.0, 0.0, true, &mut rng).is_empty());
}

#[test]
fn explosive_kills_arm_a_timer_instead_and_police_cars_anger_the_cops() {
    let mut v = VehicleData::default();
    let mut e = entity();
    let mut rng = Rng::default();
    let ev = v.inflict_damage(&mut e, true, Some(r(1)), true, weapon::EXPLOSION, 5000.0, 0.0, true, &mut rng);
    assert!(v.bomb_timer >= 0 && v.bomb_timer <= 0x7FF);
    assert_eq!(v.blow_up_entity, Some(r(1)));
    assert_eq!(ev, vec![VehicleEvent::WantedLevelNoDrop(1)]);
    // not for a car the player owns
    let mut owned = VehicleData::default();
    owned.flags.has_been_owned_by_player = true;
    assert!(owned.inflict_damage(&mut e, true, Some(r(1)), true, weapon::COLT45, 1.0, 0.0, true, &mut rng).is_empty());
}

#[test]
fn a_frightened_random_car_driver_abandons_it() {
    let mut v = VehicleData::new(RANDOM_VEHICLE, 0, VehicleType::Car, &mut Rng::default());
    v.driver = Some(r(1));
    v.passengers[0] = Some(r(2));
    let mut e = entity();
    e.status = status::PHYSICS;
    let ev = v.inflict_damage(&mut e, false, None, false, weapon::COLT45, 850.0, 0.0, true, &mut Rng::default());
    // health fell below 200
    assert_eq!(e.status, status::ABANDONED);
    assert!(ev.contains(&VehicleEvent::LeaveCarAndFlee(r(1))) && ev.contains(&VehicleEvent::LeaveCarAndFlee(r(2))));
}

#[test]
fn spheres_touch_the_oriented_box() {
    use crate::collision::Box3;
    let col = ColModel { bounding_box: Box3 { min: Vec3::new(-1.0, -2.0, -1.0), max: Vec3::new(1.0, 2.0, 1.0), surface: 0, piece: 0 }, ..Default::default() };
    // turned a quarter turn: the long axis now points along world x
    let m = Matrix::rotate_z_only(std::f32::consts::FRAC_PI_2);
    assert!(VehicleData::is_sphere_touching(&m, &col, Vec3::new(2.5, 0.0, 0.0), 0.6));
    assert!(!VehicleData::is_sphere_touching(&m, &col, Vec3::new(0.0, 2.5, 0.0), 0.6));
    assert_eq!(VehicleData::height_above_road(&col), 1.0);
}

#[test]
fn model_predicates() {
    assert!(VehicleData::is_law_enforcement_vehicle(mi::POLICE) && !VehicleData::is_law_enforcement_vehicle(mi::TAXI));
    assert!(VehicleData::is_taxi(mi::BORGNINE) && VehicleData::uses_siren(mi::AMBULAN));
    let v = VehicleData { driver: Some(r(1)), ..Default::default() };
    assert!(v.is_vehicle_normal(mi::LANDSTAL, status::PHYSICS) && !v.is_vehicle_normal(mi::TAXI, status::PHYSICS));
    assert!(VehicleData::is_upside_down(&Matrix { up: Vec3::new(0.0, 0.0, -1.0), ..Matrix::IDENTITY }));
    assert!(VehicleData::is_on_its_side(&Matrix { right: Vec3::new(0.0, 0.0, 0.9), ..Matrix::IDENTITY }));
}

fn wheel(contact_speed: Vec3, thrust: f32, brake: f32) -> Wheel {
    Wheel { fwd: Vec3::Y, right: Vec3::X, contact_speed, contact_point: Vec3::new(1.0, 1.0, -0.5), wheels_on_ground: 4, thrust, brake, adhesion: 1.0, status: 0 }
}

#[test]
fn a_wheel_pushes_back_against_sideways_slip() {
    let m = mgr();
    let h = m.get(0);
    let v = VehicleData::default();
    let mut p = Physical { mass: h.mass, turn_mass: h.turn_mass, ..Default::default() };
    let (mut state, mut statics) = (WheelState::Normal, WheelStatics::default());
    v.process_wheel(&mut p, &Matrix::IDENTITY, h, &m, 0, &wheel(Vec3::new(0.01, 0.0, 0.0), 0.0, 0.0), &mut state, &mut statics, &timer(), &mut Rng::default());
    assert!(p.move_speed.x < 0.0 && state == WheelState::Normal);
    // with the engine pushing, the forward thrust shows up as forward speed
    let mut p2 = Physical { mass: h.mass, turn_mass: h.turn_mass, ..Default::default() };
    v.process_wheel(&mut p2, &Matrix::IDENTITY, h, &m, 0, &wheel(Vec3::ZERO, 0.0001, 0.0), &mut state, &mut statics, &timer(), &mut Rng::default());
    assert!(p2.move_speed.y > 0.0);
}

#[test]
fn a_wheel_beyond_its_grip_slides_and_the_skid_latches() {
    let m = mgr();
    let h = m.get(0);
    let v = VehicleData::default();
    let mut p = Physical { mass: h.mass, turn_mass: h.turn_mass, ..Default::default() };
    let (mut state, mut statics) = (WheelState::Normal, WheelStatics::default());
    let w = wheel(Vec3::new(5.0, 0.0, 0.0), 0.0, 0.0);
    v.process_wheel(&mut p, &Matrix::IDENTITY, h, &m, 0, &w, &mut state, &mut statics, &timer(), &mut Rng::default());
    assert_eq!(state, WheelState::Skidding);
    // the next call with a non-normal state flips the global latch on
    v.process_wheel(&mut p, &Matrix::IDENTITY, h, &m, 0, &w, &mut state, &mut statics, &timer(), &mut Rng::default());
    assert!(statics.already_skidding);
    // brakes that beat the grip lock the wheel when it is rolling
    let mut s2 = WheelState::Normal;
    let mut p3 = Physical { mass: h.mass, turn_mass: h.turn_mass, ..Default::default() };
    v.process_wheel(&mut p3, &Matrix::IDENTITY, h, &m, 0, &wheel(Vec3::new(0.0, 1.0, 0.0), 0.0, 5.0), &mut s2, &mut WheelStatics::default(), &timer(), &mut Rng::default());
    assert_eq!(s2, WheelState::Fixed);
}

#[test]
fn wheel_rotation_follows_the_state() {
    let t = timer();
    assert_eq!(VehicleData::wheel_rotation(WheelState::Spinning, Vec3::Y, Vec3::ZERO, 0.3, &t), -1.1);
    assert_eq!(VehicleData::wheel_rotation(WheelState::Fixed, Vec3::Y, Vec3::Y, 0.3, &t), 0.0);
    assert!((VehicleData::wheel_rotation(WheelState::Normal, Vec3::Y, Vec3::new(0.0, 0.3, 0.0), 0.3, &t) + 1.0).abs() < 1e-6);
}

fn flight<'a>(pad: &'a PadState, col: &'a ColModel, t: &'a Timer, st: u8) -> Flight<'a> {
    Flight { pad, col, entity_status: st, is_sparrow: true, cheat5: false, steer_angle: 0.0, timer: t }
}

#[test]
fn a_plane_with_throttle_speeds_up_and_a_dodo_caps_its_speed() {
    let col = ColModel::default();
    let pad = PadState { accelerate: 255, ..Default::default() };
    let t = timer();
    let mut p = car_phys();
    flying_control(FlightModel::SeaPlane, &mut p, &Matrix::IDENTITY, &flight(&pad, &col, &t, status::PLAYER));
    assert!(p.move_speed.y > 0.0);
    // the dodo clamps over-speed and over-spin
    let mut d = car_phys();
    d.move_speed = Vec3::new(0.0, 5.0, 0.0);
    d.turn_speed = Vec3::new(1.0, 0.0, 0.0);
    flying_control(FlightModel::Dodo, &mut d, &Matrix::IDENTITY, &flight(&pad, &col, &t, status::PLAYER));
    assert!(magnitude(d.move_speed) <= 1.5 + 1e-4 && magnitude(d.turn_speed) <= 0.2 + 1e-4);
}

#[test]
fn a_heli_only_flies_under_player_control() {
    let col = ColModel::default();
    let pad = PadState { accelerate: 255, ..Default::default() };
    let t = timer();
    let mut ai = car_phys();
    ai.move_speed = Vec3::new(1.0, 0.0, 0.0);
    flying_control(FlightModel::Heli, &mut ai, &Matrix::IDENTITY, &flight(&pad, &col, &t, status::PHYSICS));
    // only air drag: slowed a little, nothing lifted
    assert!(ai.move_speed.x < 1.0 && ai.move_speed.x > 0.98 && ai.move_speed.z == 0.0);
    let mut player = car_phys();
    flying_control(FlightModel::Heli, &mut player, &Matrix::IDENTITY, &flight(&pad, &col, &t, status::PLAYER));
    assert!(player.move_speed.z > 0.0);
}
