use super::*;
use crate::entity::object::ObjectData;
use crate::entity::{Entity, PoolKind};
use crate::modelinfo::ModelInfoStore;
use re3_formats::datafile::Game;

fn r(pool: PoolKind, h: i32) -> EntityRef {
    EntityRef { pool, handle: h }
}

fn ent(ty: EntityType, kind: Kind, pos: Vec3, mass: f32, speed: Vec3) -> Ent {
    let mut base = Entity::new(ty, &mut Rng::default());
    base.set_position(pos);
    base.flags.is_static = false;
    // -1 would match every unset special model index
    base.model_index = 0;
    let phys = Physical { mass, turn_mass: mass * 4.0, move_speed: speed, ..Default::default() };
    Ent::new(base, Some(phys), kind)
}

fn car(pos: Vec3, speed: Vec3) -> Ent {
    ent(EntityType::Vehicle, Kind::Vehicle(Box::default()), pos, 1000.0, speed)
}

fn point(at: Vec3, normal: Vec3) -> ColPoint {
    ColPoint { point: at, normal, ..Default::default() }
}

struct Fixture {
    indices: Indices,
    models: ModelInfoStore,
    objects: ObjectTable,
    rng: Rng,
}

impl Fixture {
    fn new() -> Self {
        Self { indices: Indices::new(Game::Gta3), models: ModelInfoStore::new(Game::Gta3), objects: ObjectTable::new(Game::Gta3), rng: Rng::default() }
    }

    fn ctx(&mut self) -> Ctx<'_> {
        Ctx { step: 1.0, no_more_collision_torque: false, indices: &self.indices, models: &self.models, objects: &mut self.objects, rng: &mut self.rng, events: vec![] }
    }
}

#[test]
fn two_cars_head_on_exchange_momentum_and_stop_approaching() {
    let mut f = Fixture::new();
    let mut a = car(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
    let mut b = car(Vec3::new(2.0, 0.0, 0.0), Vec3::ZERO);
    // A hits B's face; the normal points from B towards A
    let c = point(Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
    let before = phys(&mut a).move_speed.x * 1000.0;
    let hit = apply_collision(&mut a, r(PoolKind::Vehicle, 1), &mut b, r(PoolKind::Vehicle, 2), &c, &mut f.ctx());
    assert!(hit.applied && hit.impulse_a > 0.0);
    let (va, vb) = (phys(&mut a).move_speed.x, phys(&mut b).move_speed.x);
    // vehicles get 1.4x on the sideways axes only, so the along-normal exchange is exact
    assert!(va < 1.0 && vb > 0.0 && vb >= va);
    assert!((va * 1000.0 + vb * 1000.0 - before).abs() < 1e-2 * before.abs(), "{va} {vb}");
}

#[test]
fn separating_bodies_are_left_alone() {
    let mut f = Fixture::new();
    let mut a = car(Vec3::ZERO, Vec3::new(-1.0, 0.0, 0.0));
    let mut b = car(Vec3::new(2.0, 0.0, 0.0), Vec3::ZERO);
    let c = point(Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
    let hit = apply_collision(&mut a, r(PoolKind::Vehicle, 1), &mut b, r(PoolKind::Vehicle, 2), &c, &mut f.ctx());
    assert!(!hit.applied);
    assert_eq!(phys(&mut a).move_speed, Vec3::new(-1.0, 0.0, 0.0));
}

#[test]
fn a_car_pushes_a_static_object_over_only_beyond_its_uproot_limit() {
    for (uproot, uprooted) in [(1.0, true), (1e9, false)] {
        let mut f = Fixture::new();
        let mut a = car(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        let mut o = ent(EntityType::Object, Kind::Object(ObjectData { uproot_limit: uproot, ..ObjectData::new() }), Vec3::new(2.0, 0.0, 0.0), 50.0, Vec3::ZERO);
        o.base.flags.is_static = true;
        let c = point(Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        let hit = apply_collision(&mut a, r(PoolKind::Vehicle, 1), &mut o, r(PoolKind::Object, 2), &c, &mut f.ctx());
        assert_eq!(!o.base.flags.is_static, uprooted);
        assert_eq!(hit.b_to_moving_list, uprooted);
        // either way the car is slowed: bounced off a fixed object, or sharing momentum with a freed one
        assert!(hit.applied && phys(&mut a).move_speed.x < 1.0);
        assert_eq!(phys(&mut o).move_speed.x > 0.0, uprooted);
    }
}

#[test]
fn a_ped_walking_into_a_pole_is_pushed_back() {
    let mut f = Fixture::new();
    let mut p = ent(EntityType::Ped, Kind::Ped(Box::default()), Vec3::ZERO, 60.0, Vec3::new(0.1, 0.0, 0.0));
    p.base.flags.ped_physics = true;
    let mut o = ent(EntityType::Object, Kind::Object(ObjectData { uproot_limit: 1e9, ..ObjectData::new() }), Vec3::new(1.0, 0.0, 0.0), 50.0, Vec3::ZERO);
    o.base.flags.is_static = true;
    let c = point(Vec3::new(0.5, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
    let hit = apply_collision(&mut p, r(PoolKind::Ped, 1), &mut o, r(PoolKind::Object, 2), &c, &mut f.ctx());
    assert!(hit.applied && phys(&mut p).move_speed.x.abs() < 1e-6);
}

#[test]
fn alt_response_bounces_by_elasticity_and_leaves_the_entity_untouched() {
    let mut a = car(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    phys(&mut a).elasticity = 0.5;
    let c = point(Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 0.0, 1.0));
    let (mut imp, mut mv, mut turn) = (0.0, Vec3::ZERO, Vec3::ZERO);
    assert!(apply_collision_alt(&mut a, true, &c, 1.0, &mut imp, &mut mv, &mut turn));
    // vehicles hitting something solid get the 1.2 boost: v' = 1.5 * 1.2
    assert!((mv.z - 1.5 * 1.2).abs() < 1e-3, "{mv:?}");
    assert_eq!(phys(&mut a).move_speed, Vec3::new(0.0, 0.0, -1.0));
    // moving away: nothing to do
    phys(&mut a).move_speed = Vec3::new(0.0, 0.0, 1.0);
    assert!(!apply_collision_alt(&mut a, true, &c, 1.0, &mut imp, &mut mv, &mut turn));
}

#[test]
fn ground_friction_is_capped_by_the_adhesive_limit() {
    let mut events = vec![];
    let c = ColPoint { point: Vec3::new(0.0, 0.0, -1.0), normal: Vec3::Z, ..Default::default() };
    // sliding at 0.5 along x; the cap (0.001 * 1.5) is far below the stopping impulse
    let mut a = car(Vec3::ZERO, Vec3::new(0.5, 0.0, 0.0));
    assert!(apply_friction(&mut a, 0.001, &c, 1.0, Game::Gta3, &mut events));
    let fr = phys(&mut a).move_friction;
    assert!(fr.x < 0.0 && (fr.x * 1000.0 + 0.0015).abs() < 1e-5, "{fr:?}");
    // standing still: no friction
    let mut still = car(Vec3::ZERO, Vec3::ZERO);
    assert!(!apply_friction(&mut still, 0.5, &c, 1.0, Game::Gta3, &mut events));
}

#[test]
fn friction_between_two_cars_pulls_their_sliding_speeds_together() {
    let mut a = car(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
    let mut b = car(Vec3::new(2.0, 0.0, 0.0), Vec3::ZERO);
    let c = point(Vec3::new(1.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
    assert!(apply_friction_pair(&mut a, &mut b, 10.0, &c, 1.0));
    assert!(phys(&mut a).move_friction.y < 0.0 && phys(&mut b).move_friction.y > 0.0);
}
