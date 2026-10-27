use super::callbacks::FINISH_FUCK_U;
use super::core::sort_peds;
use super::ctx::{EventEntity, EventInfo, NoPedHooks, PedHooks, Px};
use super::data::*;
use crate::anim::{tables_gta3::anim, CallbackKind, Event};
use crate::entity::physical::Physical;
use crate::entity::{Ent, Entity, EntityRef, EntityType, Kind};
use crate::general::Rng;
use crate::temp_col::TempColModels;
use crate::world::World;
use re3_formats::datafile::Game;

fn world_with_ped() -> (World, EntityRef) {
    let mut w = World::new(crate::modelinfo::ModelInfoStore::new(Game::Gta3), TempColModels::new(Game::Gta3));
    let base = Entity::new(EntityType::Ped, &mut Rng::default());
    let ped = Box::new(PedData::new(0, 0, false));
    let r = w.insert(Ent::new(base, Some(Physical::default()), Kind::Ped(ped))).unwrap();
    (w, r)
}

#[derive(Default)]
struct Spy {
    events: Vec<(u32, u32)>,
    pickups: u32,
    explosion: Option<EventInfo>,
    cleared: Vec<usize>,
    stats: Vec<crate::ped::ctx::StatEvent>,
    armour_losses: u32,
    blood: u32,
}

impl PedHooks for Spy {
    fn register_event(&mut self, event: u32, _ped: EntityRef, time: u32) {
        self.events.push((event, time));
    }

    fn create_dead_ped_pickups(&mut self, _ped: EntityRef) {
        self.pickups += 1;
    }

    fn find_closest_event(&mut self, event: u32, _pos: glam::Vec3) -> Option<EventInfo> {
        if event == event_type::EXPLOSION { self.explosion } else { None }
    }

    fn clear_event(&mut self, index: usize) {
        self.cleared.push(index);
    }

    fn stat(&mut self, stat: crate::ped::ctx::StatEvent) {
        self.stats.push(stat);
    }

    fn player_lost_armour_or_health(&mut self, armour: bool) {
        if armour {
            self.armour_losses += 1;
        }
    }

    fn add_blood(&mut self, _pos: glam::Vec3, _dir: glam::Vec3) {
        self.blood += 1;
    }
}

#[test]
fn set_idle_waits_two_to_four_seconds_and_keeps_mug() {
    let (mut w, r) = world_with_ped();
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    w.ped_mut(r).ped_state = state::WANDER_PATH;
    w.ped_mut(r).move_state = move_state::WALK;
    w.set_idle(&mut px, r);
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.move_state), (state::IDLE, move_state::STILL));
    assert!((2000..=4000).contains(&pd.wait_timer));

    w.ped_mut(r).ped_state = state::MUG;
    w.set_idle(&mut px, r);
    assert_eq!(w.ped(r).ped_state, state::MUG);
}

#[test]
fn wander_path_off_the_ground_only_remembers_the_request() {
    let (mut w, r) = world_with_ped();
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    w.ped_mut(r).health = 0.0;
    assert!(!w.set_wander_path(&mut px, r, 3));
    assert!(w.ped(r).flags.start_wander_path_on_foot);
    assert_eq!(w.ped(r).path_dir, 3);
}

#[test]
fn wander_path_with_no_graph_gives_up_and_idles() {
    let (mut w, r) = world_with_ped();
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    let nodes = w.ents.paths.nodes.len();
    assert!(!w.set_wander_path(&mut px, r, 2));
    assert_eq!(w.ped(r).ped_state, state::IDLE);
    // the search takes the graph out of the world, it must come back
    assert_eq!(w.ents.paths.nodes.len(), nodes);
}

#[test]
fn staying_in_place_idles_instead_of_wandering() {
    let (mut w, r) = world_with_ped();
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    w.ped_mut(r).flags.kinda_stay_in_same_place = true;
    w.ped_mut(r).ped_state = state::WANDER_PATH;
    assert!(!w.set_wander_path(&mut px, r, 1));
    assert_eq!(w.ped(r).ped_state, state::IDLE);
}

#[test]
fn say_queues_the_most_urgent_comment() {
    use crate::audio::sound;
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).ped_type = crate::ped::types::ped_type::CIVMALE;
    let (mut rng, mut spy) = (Rng::default(), Spy::default());
    let mut px = Px { rng: &mut rng, hooks: &mut spy };
    assert_eq!(w.ped(r).queued_sound, NO_SOUND);
    w.say(&mut px, r, sound::PED_EVADE);
    w.say(&mut px, r, sound::PED_HIT);
    w.say(&mut px, r, sound::PED_EVADE);
    assert_eq!(w.ped(r).queued_sound, sound::PED_HIT);
    // the player only voices a few sounds, and a death is a pain cry
    let p = world_with_player_and_cop().1;
    let (mut w2, _, _) = world_with_player_and_cop();
    w2.say(&mut px, p, sound::PED_EVADE);
    assert_eq!(w2.ped(p).queued_sound, NO_SOUND);
    w2.say(&mut px, p, sound::PED_DEATH);
    assert_eq!(w2.ped(p).queued_sound, sound::PED_DAMAGE);
}

#[test]
fn a_ped_does_not_repeat_a_comment_too_soon() {
    use crate::audio::sound;
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).ped_type = crate::ped::types::ped_type::CIVMALE;
    let (mut rng, mut spy) = (Rng::default(), Spy::default());
    let mut px = Px { rng: &mut rng, hooks: &mut spy };
    w.ents.timer.time_in_ms = 10;
    let pd = w.ped_mut(r);
    pd.last_queued_sound = sound::PED_EVADE;
    pd.last_sound_start = 10;
    w.say(&mut px, r, sound::PED_EVADE);
    assert_eq!(w.ped(r).queued_sound, NO_SOUND);
    w.ents.timer.time_in_ms = 60_000;
    w.say(&mut px, r, sound::PED_EVADE);
    assert_eq!(w.ped(r).queued_sound, sound::PED_EVADE);
}

#[test]
fn weapon_model_is_kept_until_the_fuck_u_animation_ends_unarmed() {
    let (mut w, r) = world_with_ped();
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    w.add_weapon_model(r, 5);
    w.add_weapon_model(r, -1);
    assert_eq!(w.ped(r).wep_model_id, 5);

    let event = Event { kind: CallbackKind::Delete, anim_id: anim::STD_PARTIAL_FUCKU as i32, token: FINISH_FUCK_U };
    w.ped_anim_callback(&mut px, r, &event);
    assert_eq!(w.ped(r).wep_model_id, -1);
}

#[test]
fn sort_peds_orders_by_distance() {
    let dist = [5.0_f32, 1.0, 4.0, 2.0, 3.0];
    let mut list: Vec<EntityRef> = (0..5).map(|i| EntityRef { pool: crate::entity::PoolKind::Ped, handle: i }).collect();
    sort_peds(&mut list, 0, 4, &|r| dist[r.handle as usize]);
    let order: Vec<u32> = list.iter().map(|r| r.handle as u32).collect();
    assert_eq!(order, vec![1, 3, 4, 2, 0]);
}

/// A world where model 10 is a 2 m box, so peds made with it can be filed in the sectors.
fn box_world() -> World {
    use crate::collision::{Box3, ColModel, Sphere};
    let mut models = crate::modelinfo::ModelInfoStore::new(Game::Gta3);
    let mi = models.add_simple_model(10).unwrap();
    let b = Box3 { min: glam::Vec3::splat(-1.0), max: glam::Vec3::splat(1.0), surface: 0, piece: 0 };
    mi.col_model = Some(std::sync::Arc::new(ColModel { bounding_sphere: Sphere::new(1.7, glam::Vec3::ZERO, 0, 0), bounding_box: b, boxes: vec![b], ..Default::default() }));
    World::new(models, crate::temp_col::TempColModels::new(Game::Gta3))
}

fn with_px<T>(f: impl FnOnce(&mut Px) -> T) -> T {
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    f(&mut Px { rng: &mut rng, hooks: &mut hooks })
}

#[test]
fn weapons_are_given_topped_up_and_cleared() {
    use crate::weapons::info::weapon_type as wt;
    let (mut w, r) = world_with_ped();
    let allowed = w.ped(r).max_weapon_type_allowed;
    w.give_weapon(r, wt::COLT45, 30);
    assert!(w.ped(r).has_weapon(wt::COLT45));
    assert_eq!((w.ped(r).weapons[wt::COLT45].ammo_total, w.ped(r).max_weapon_type_allowed), (30, allowed + 1));
    w.give_weapon(r, wt::COLT45, 99999);
    assert_eq!(w.ped(r).weapons[wt::COLT45].ammo_total, 99999);
    assert_eq!(w.get_weapon_slot(r, wt::UZI), None);

    w.set_current_weapon(r, wt::COLT45);
    assert_eq!(w.ped(r).current_weapon as usize, wt::COLT45);
    w.clear_weapons(r);
    assert!(!w.ped(r).has_weapon(wt::COLT45) && w.ped(r).current_weapon as usize == wt::UNARMED);
}

#[test]
fn a_fallen_ped_gets_up_after_the_timer() {
    let (mut w, r) = world_with_ped();
    with_px(|px| w.set_fall(px, r, 500, anim::STD_FALL_COLLAPSE, false));
    assert_eq!(w.ped(r).ped_state, state::FALL);
    assert!(w.ped(r).flags.fallen_down);
    let timer = w.ped(r).get_up_timer;
    assert!(timer >= 1500);

    with_px(|px| w.fall(px, r));
    assert_eq!(w.ped(r).ped_state, state::FALL, "still down before the timer");
    w.ents.timer.time_in_ms = timer + 1;
    with_px(|px| w.fall(px, r));
    assert_eq!(w.ped(r).ped_state, state::GETUP);
    assert!(w.ped(r).flags.get_up_anim_started);
}

#[test]
fn falling_forever_never_gets_up() {
    let (mut w, r) = world_with_ped();
    with_px(|px| w.set_fall(px, r, -1, anim::STD_FALL_COLLAPSE, false));
    w.ents.timer.time_in_ms = u32::MAX - 1;
    with_px(|px| w.fall(px, r));
    assert_eq!(w.ped(r).ped_state, state::FALL);
}

#[test]
fn timers_only_start_once_the_old_one_ran_out() {
    let (mut w, r) = world_with_ped();
    w.ents.timer.time_in_ms = 1000;
    w.set_look_timer(r, 500);
    w.set_look_timer(r, 9000);
    assert_eq!(w.ped(r).look_timer, 1500);
    w.set_shoot_timer(r, 200);
    w.set_attack_timer(r, 300);
    assert_eq!((w.ped(r).shoot_timer, w.ped(r).attack_timer), (1200, 1500));
}

#[test]
fn mugging_nobody_just_idles() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).ped_state = state::WANDER_PATH;
    with_px(|px| w.mug(px, r));
    assert_eq!(w.ped(r).ped_state, state::IDLE);
}

fn second_ped(w: &mut World, pos: glam::Vec3) -> EntityRef {
    let mut base = Entity::new(EntityType::Ped, &mut Rng::default());
    base.set_position(pos);
    w.insert(Ent::new(base, Some(Physical::default()), Kind::Ped(Box::new(PedData::new(0, 0, false))))).unwrap()
}

#[test]
fn turning_to_look_converges_on_the_direction() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).look_direction = 1.0;
    let mut turns = 0;
    while !w.turn_body(r) {
        turns += 1;
        assert!(turns < 100, "never arrives");
    }
    assert!((w.ped(r).rotation_cur - 1.0).abs() < 0.06);
}

#[test]
fn seek_is_done_inside_the_distance_and_turns_towards_it_outside() {
    let (mut w, r) = world_with_ped();
    w.ents.timer.time_in_ms = 1000;
    w.ped_mut(r).ped_state = state::SEEK_POS;
    w.ped_mut(r).seek_pos = glam::Vec3::new(0.0, 0.5, 0.0);
    assert!(with_px(|px| w.seek(px, r)));

    w.ped_mut(r).seek_pos = glam::Vec3::new(10.0, 0.0, 0.0);
    assert!(!with_px(|px| w.seek(px, r)));
    // facing east is a quarter turn clockwise from north
    assert!((w.ped(r).rotation_dest + std::f32::consts::FRAC_PI_2).abs() < 0.01);
    assert_eq!(w.ped(r).move_state, move_state::WALK);
}

#[test]
fn fleeing_an_entity_runs_and_remembers_what_from() {
    let (mut w, r) = world_with_ped();
    let other = second_ped(&mut w, glam::Vec3::new(0.0, 10.0, 0.0));
    w.ents.timer.time_in_ms = 100;
    with_px(|px| w.set_flee(px, r, other, 5000));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.move_state, pd.flee_from, pd.flee_timer), (state::FLEE_ENTITY, move_state::RUN, Some(other), 5100));
    assert!(pd.flags.use_ped_node_seek);
    // time <= 0 means flee until told otherwise
    with_px(|px| w.clear_flee(px, r));
    with_px(|px| w.set_flee(px, r, other, 0));
    assert_eq!(w.ped(r).flee_timer, 0);
}

#[test]
fn a_ped_that_stays_put_does_not_flee() {
    let (mut w, r) = world_with_ped();
    let other = second_ped(&mut w, glam::Vec3::new(0.0, 10.0, 0.0));
    w.ped_mut(r).flags.kinda_stay_in_same_place = true;
    with_px(|px| w.set_flee(px, r, other, 5000));
    assert_eq!(w.ped(r).ped_state, state::IDLE);
}

#[test]
fn only_the_player_follows_a_route() {
    let (mut w, r) = world_with_ped();
    assert!(!w.set_follow_path(r, glam::Vec3::new(10.0, 0.0, 0.0)));
    w.ents.player_ped = Some(r);
    assert!(!w.set_follow_path(r, glam::Vec3::new(1.0, 0.0, 0.0)), "too close");
}

#[test]
fn node_headings_are_opposite_for_opposite_vectors() {
    use crate::general::node_heading_from_vector as h;
    for (x, y) in [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (-1.0, 0.5)] {
        let (a, b) = (h(x, y), h(-x, -y));
        assert!((0..8).contains(&a) && (0..8).contains(&b));
        assert_eq!((a - b).rem_euclid(8), 4, "({x}, {y})");
    }
}

#[test]
fn dying_is_final_and_dead_leaves_pickups_unless_player() {
    use crate::audio::sound;
    let (mut w, r) = world_with_ped();
    let (mut rng, mut spy) = (Rng::default(), Spy::default());
    w.ents.timer.time_in_ms = 4000;
    w.set_die(&mut Px { rng: &mut rng, hooks: &mut spy }, r, anim::STD_NUM, 4.0, 0.0);
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.health, pd.bloody_footprint_count_or_death_time), (state::DIE, 0.0, 4000));
    assert_eq!(pd.queued_sound, sound::PED_DAMAGE); // this test ped is the player: a death cry becomes a pain cry

    // dying again does nothing
    w.set_die(&mut Px { rng: &mut rng, hooks: &mut spy }, r, anim::STD_NUM, 4.0, 0.0);
    assert_eq!(w.ped(r).bloody_footprint_count_or_death_time, 4000);

    w.set_dead(&mut Px { rng: &mut rng, hooks: &mut spy }, r);
    assert_eq!(w.ped(r).ped_state, state::DEAD);
    assert!(!w.ent(r).base.flags.uses_collision);
    assert_eq!(spy.events, vec![(event_type::INJURED_PED, 250), (event_type::DEAD_PED, 1000)]);
    assert_eq!(spy.pickups, 1);

    let (mut w, r) = world_with_ped();
    w.ents.player_ped = Some(r);
    w.set_dead(&mut Px { rng: &mut rng, hooks: &mut spy }, r);
    assert_eq!(spy.pickups, 1, "the player drops nothing");
}

#[test]
fn an_undamageable_player_does_not_die() {
    let (mut w, r) = world_with_ped();
    w.ents.player_ped = Some(r);
    w.ped_mut(r).player = Some(Box::default());
    w.ped_mut(r).player.as_mut().unwrap().can_be_damaged = false;
    with_px(|px| w.set_die(px, r, anim::STD_NUM, 4.0, 0.0));
    assert_eq!(w.ped(r).ped_state, state::IDLE);
}

#[test]
fn a_player_told_to_dive_remembers_the_threat() {
    let (mut w, r) = world_with_ped();
    let car = second_ped(&mut w, glam::Vec3::new(0.0, 10.0, 0.0));
    w.ped_mut(r).ped_type = crate::ped::types::ped_type::PLAYER1;
    with_px(|px| w.set_evasive_dive(px, r, car, false));
    let p = w.ped(r).player.as_ref().unwrap();
    assert_eq!((p.evade_amount, p.evading_from), (5, Some(car)));
}

#[test]
fn chatting_sets_up_the_state_and_timers() {
    let (mut w, r) = world_with_ped();
    let other = second_ped(&mut w, glam::Vec3::new(1.0, 0.0, 0.0));
    w.ents.timer.time_in_ms = 1000;
    w.set_chat(r, other, 5000);
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.move_state, pd.chat_timer, pd.look_timer), (state::CHAT, move_state::STILL, 6000, 4000));
    assert_eq!(pd.look_target, Some(other));
}

#[test]
fn a_phone_call_ends_when_the_timer_runs_out() {
    let (mut w, r) = world_with_ped();
    let (mut rng, mut spy) = (Rng::default(), Spy::default());
    w.ents.phones.phones = vec![Default::default(); 5];
    w.ents.phones.phones[3].state = crate::control::phones::phone_state::REPORTING_CRIME;
    w.ped_mut(r).phone_id = 3;
    w.ped_mut(r).phone_talk_timer = 500;
    w.ents.timer.time_in_ms = 400;
    assert!(!w.make_phonecall(&mut Px { rng: &mut rng, hooks: &mut spy }, r));
    w.ents.timer.time_in_ms = 501;
    assert!(w.make_phonecall(&mut Px { rng: &mut rng, hooks: &mut spy }, r));
    assert_eq!((w.ents.phones.phones[3].state, w.ped(r).phone_id), (crate::control::phones::phone_state::FREE, -1));
}

#[test]
fn teleporting_moves_the_ped_and_clears_the_walk_target() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).ped_state_timer = 99;
    w.ped_mut(r).action_x = 5.0;
    w.teleport(r, glam::Vec3::new(10.0, 20.0, 3.0));
    assert_eq!(w.epos(r), glam::Vec3::new(10.0, 20.0, 3.0));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state_timer, pd.action_x, pd.flags.is_standing), (0, 0.0, false));
}

#[test]
fn flagging_a_footped_for_removal_just_marks_it() {
    let (mut w, r) = world_with_ped();
    w.flag_to_destroy_when_next_processed(r);
    assert!(w.ent(r).base.flags.remove_from_world);
    assert_eq!(w.ped(r).ped_state, state::IDLE);
}

#[test]
fn an_explosion_with_a_known_cause_is_investigated_and_cleared() {
    let (mut w, r) = world_with_ped();
    let cause = second_ped(&mut w, glam::Vec3::new(5.0, 5.0, 0.0));
    let (mut rng, mut spy) = (Rng::default(), Spy::default());
    spy.explosion = Some(EventInfo { index: 7, pos: glam::Vec3::new(3.0, 4.0, 0.0), entity: EventEntity::Ped(cause) });
    let area = w.check_for_explosions(&mut Px { rng: &mut rng, hooks: &mut spy }, r);
    assert_eq!(area, Some(glam::Vec2::new(3.0, 4.0)));
    assert_eq!((w.ped(r).event_entity, w.ped(r).flags.gonna_investigate_event, spy.cleared.clone()), (Some(cause), true, vec![7]));

    spy.explosion = None;
    assert_eq!(w.check_for_explosions(&mut Px { rng: &mut rng, hooks: &mut spy }, r), None);
    assert!(!w.ped(r).flags.gonna_investigate_event);
}

#[test]
fn waiting_at_the_lights_ends_after_the_timer() {
    let (mut w, r) = world_with_ped();
    // the pedestrian light is on WALK from 12 s into its cycle
    w.ents.timer.time_in_ms = 12500;
    with_px(|px| w.set_wait_state(px, r, wait_state::TRAFFIC_LIGHTS, None));
    let pd = w.ped(r);
    assert_eq!((pd.wait_state, pd.move_state, pd.wait_timer), (wait_state::TRAFFIC_LIGHTS, move_state::STILL, 13000));

    with_px(|px| w.wait(px, r));
    assert_eq!(w.ped(r).wait_state, wait_state::TRAFFIC_LIGHTS, "timer has not run out");
    w.ents.timer.time_in_ms = 13001;
    with_px(|px| w.wait(px, r));
    assert_eq!((w.ped(r).wait_state, w.ped(r).move_state), (wait_state::FALSE, move_state::WALK));
    // on DONT WALK the ped keeps waiting
    with_px(|px| w.set_wait_state(px, r, wait_state::TRAFFIC_LIGHTS, None));
    w.ents.timer.time_in_ms = 16384 + 100 + 1000;
    with_px(|px| w.wait(px, r));
    assert_eq!(w.ped(r).wait_state, wait_state::TRAFFIC_LIGHTS);
}

#[test]
fn a_look_around_waits_for_the_given_time() {
    let (mut w, r) = world_with_ped();
    w.ents.timer.time_in_ms = 100;
    with_px(|px| w.set_wait_state(px, r, wait_state::CROSS_ROAD_LOOK, Some(2000)));
    assert_eq!(w.ped(r).wait_timer, 2100);
}

#[test]
fn a_ped_that_cannot_act_does_not_start_waiting() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).health = 0.0;
    with_px(|px| w.set_wait_state(px, r, wait_state::TRAFFIC_LIGHTS, None));
    assert_eq!(w.ped(r).wait_state, wait_state::FALSE);
}

#[test]
fn turning_round_and_doubling_back_finish_walking_on() {
    let (mut w, r) = world_with_ped();
    w.ents.timer.time_in_ms = 10;
    with_px(|px| w.set_wait_state(px, r, wait_state::TURN180, None));
    assert_eq!(w.ped(r).heading_rate, 0.0);
    w.ents.timer.time_in_ms = 6000;
    with_px(|px| w.wait(px, r));
    assert_eq!((w.ped(r).wait_state, w.ped(r).rotation_cur), (wait_state::FALSE, std::f32::consts::PI));
    // the heading rate is restored from the ped's stats once nothing is awaited
    assert_eq!(w.ped(r).heading_rate, 15.0);

    with_px(|px| w.set_wait_state(px, r, wait_state::DOUBLEBACK, None));
    w.ents.timer.time_in_ms = 20000;
    with_px(|px| w.wait(px, r));
    assert_eq!((w.ped(r).wait_state, w.ped(r).move_state), (wait_state::FALSE, move_state::WALK));
}

fn car(w: &mut World, pos: glam::Vec3) -> EntityRef {
    use crate::vehicle::base::{VehicleData, VehicleType};
    let mut base = Entity::new(EntityType::Vehicle, &mut Rng::default());
    base.set_position(pos);
    let data = Box::new(VehicleData::new(1, 0, VehicleType::Car, &mut Rng::default()));
    w.insert(Ent::new(base, Some(Physical::default()), Kind::Vehicle(data))).unwrap()
}

#[test]
fn radio_categories_follow_the_ped_model() {
    use crate::modelinfo::indices_gta3::mi;
    assert_eq!((super::car::ped_radio_category(mi::MALE01), super::car::ped_radio_category(mi::TAXI_D), super::car::ped_radio_category(0)), (3, 9, 0));
}

#[test]
fn warping_a_driver_into_a_car_seats_and_activates_it() {
    use crate::entity::status;
    let (mut w, r) = world_with_ped();
    let v = car(&mut w, glam::Vec3::new(4.0, 0.0, 0.0));
    w.ped_mut(r).ped_type = crate::ped::types::ped_type::CIVMALE;
    w.ped_mut(r).objective = objective::ENTER_CAR_AS_DRIVER;
    with_px(|px| w.warp_ped_into_car(px, r, v));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.in_vehicle, pd.my_vehicle, pd.flags.changed_seat), (state::DRIVING, true, Some(v), true));
    assert_eq!(w.try_veh(v).unwrap().driver, Some(r));
    assert_eq!(w.ent(v).base.status, status::PHYSICS);
    assert_eq!(w.epos(r), glam::Vec3::new(4.0, 0.0, 0.0));
    assert!(!w.ent(r).base.flags.uses_collision);
}

#[test]
fn warping_as_a_player_marks_the_car_as_the_players() {
    use crate::entity::status;
    let (mut w, r) = world_with_ped();
    let v = car(&mut w, glam::Vec3::ZERO);
    w.ped_mut(r).ped_type = crate::ped::types::ped_type::PLAYER1;
    w.ped_mut(r).objective = objective::ENTER_CAR_AS_PASSENGER;
    with_px(|px| w.warp_ped_into_car(px, r, v));
    assert_eq!(w.ent(v).base.status, status::PLAYER);
    assert_eq!(w.try_veh(v).unwrap().passengers[0], Some(r));
}

#[test]
fn seeking_a_car_that_is_gone_gives_up() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).ped_state = state::SEEK_CAR;
    w.ped_mut(r).car_in_objective = None;
    with_px(|px| w.seek_car(px, r));
    assert_ne!(w.ped(r).ped_state, state::SEEK_CAR);
}

#[test]
fn jumping_is_started_once_and_ends_with_the_landing_flag_cleared() {
    let (mut w, r) = world_with_ped();
    assert!(w.can_ped_jump_this(r), "nothing in the way");
    w.ped_mut(r).rotation_cur = 1.0;
    w.set_jump(r);
    assert_eq!((w.ped(r).ped_state, w.ped(r).rotation_dest), (state::JUMP, 1.0));
    w.ped_mut(r).flags.is_landing = true;
    w.ped_mut(r).flags.reset_walk_anims = false;
    w.finish_jump_cb(r, 0);
    assert!(!w.ped(r).flags.is_landing && w.ped(r).flags.reset_walk_anims);
    with_px(|px| w.finish_hit_head_cb(px, r, None));
    assert_ne!(w.ped(r).ped_state, state::JUMP, "the jump state is left");
}

#[test]
fn a_sitting_ped_does_not_jump() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).in_vehicle = true;
    w.set_jump(r);
    assert_eq!(w.ped(r).ped_state, state::IDLE);
}

#[test]
fn a_driver_waits_for_a_friend_who_is_still_getting_in() {
    let (mut w, r) = world_with_ped();
    let v = car(&mut w, glam::Vec3::ZERO);
    let friend = second_ped(&mut w, glam::Vec3::new(1.0, 0.0, 0.0));
    w.ents.timer.time_in_ms = 100;
    w.ped_mut(r).ped_state = state::DRIVING;
    w.ped_mut(r).car_in_objective = Some(v);
    w.ped_mut(r).near_peds[0] = Some(friend);
    w.ped_mut(r).num_near_peds = 1;
    w.ped_mut(friend).objective = objective::ENTER_CAR_AS_PASSENGER;
    w.ped_mut(friend).car_in_objective = Some(v);
    assert!(!w.can_ped_drive_off(r));
    assert_eq!(w.ped(r).look_timer, 1100);
    w.ped_mut(friend).objective = objective::NONE;
    w.ents.timer.time_in_ms = 5000;
    assert!(w.can_ped_drive_off(r));
}

#[test]
fn a_kill_objective_remembers_the_victim_and_the_old_objective() {
    let (mut w, r) = world_with_ped();
    let victim = second_ped(&mut w, glam::Vec3::new(5.0, 0.0, 0.0));
    w.ped_mut(r).objective = objective::GUARD_SPOT;
    with_px(|px| w.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, victim));
    let pd = w.ped(r);
    assert_eq!((pd.objective, pd.prev_objective, pd.ped_in_objective, pd.look_target), (objective::KILL_CHAR_ON_FOOT, objective::GUARD_SPOT, Some(victim), Some(victim)));
    // asking again for the same victim changes nothing
    with_px(|px| w.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, victim));
    assert_eq!(w.ped(r).prev_objective, objective::GUARD_SPOT);

    with_px(|px| w.restore_previous_objective(px, r));
    assert_eq!((w.ped(r).objective, w.ped(r).prev_objective), (objective::GUARD_SPOT, objective::NONE));
}

#[test]
fn a_ped_cannot_target_itself_or_be_told_while_dead() {
    let (mut w, r) = world_with_ped();
    with_px(|px| w.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, r));
    assert_eq!(w.ped(r).objective, objective::NONE);
    let other = second_ped(&mut w, glam::Vec3::X);
    w.ped_mut(r).ped_state = state::DEAD;
    with_px(|px| w.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, other));
    assert_eq!(w.ped(r).objective, objective::NONE);
}

#[test]
fn leaving_a_car_is_only_asked_of_a_ped_in_one() {
    let (mut w, r) = world_with_ped();
    let v = car(&mut w, glam::Vec3::ZERO);
    with_px(|px| w.set_objective_entity(px, r, objective::LEAVE_CAR, v));
    assert_eq!(w.ped(r).objective, objective::NONE);
    w.ped_mut(r).in_vehicle = true;
    w.ped_mut(r).my_vehicle = Some(v);
    with_px(|px| w.set_objective_entity(px, r, objective::LEAVE_CAR, v));
    assert_eq!((w.ped(r).objective, w.ped(r).car_in_objective), (objective::LEAVE_CAR, Some(v)));
}

#[test]
fn a_waiting_ped_idles_and_drops_the_objective() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).objective = objective::WAIT_ON_FOOT;
    w.ped_mut(r).ped_state = state::WANDER_PATH;
    w.ped_mut(r).move_state = move_state::WALK;
    with_px(|px| w.process_objective(px, r));
    let pd = w.ped(r);
    assert_eq!((pd.objective, pd.ped_state, pd.move_state), (objective::NONE, state::IDLE, move_state::STILL));
}

#[test]
fn reaching_the_area_completes_the_objective() {
    let (mut w, r) = world_with_ped();
    w.ents.timer.time_in_ms = 1000;
    w.set_objective_area(r, objective::GOTO_AREA_ON_FOOT, glam::Vec3::new(0.2, 0.0, 0.0), 0.5);
    assert_eq!(w.ped(r).objective, objective::GOTO_AREA_ON_FOOT);
    with_px(|px| w.process_objective(px, r));
    let pd = w.ped(r);
    assert!(pd.flags.script_objective_completed);
    assert_eq!(pd.objective, objective::NONE, "restored to the stored objective");
}

#[test]
fn formations_stand_beside_the_leader_and_a_dead_leader_is_replaced() {
    let (mut w, r) = world_with_ped();
    let leader = second_ped(&mut w, glam::Vec3::new(10.0, 10.0, 0.0));
    w.ped_mut(r).ped_in_objective = Some(leader);
    w.ped_mut(r).ped_formation = formation::REAR_LEFT;
    assert_eq!(w.get_formation_position(r), glam::Vec3::new(8.5, 8.5, 0.0));

    w.ped_mut(leader).ped_state = state::DEAD;
    assert_eq!(w.get_formation_position(r), w.epos(r));
    assert_eq!(w.ped(r).ped_in_objective, None);
}

#[test]
fn routes_hold_points_in_order_and_close_up_after_a_removal() {
    use super::routes::PedRoutes;
    let mut routes = PedRoutes::default();
    routes.add_point(3, glam::Vec3::X);
    routes.add_point(3, glam::Vec3::Y);
    routes.add_point(7, glam::Vec3::Z);
    assert_eq!((routes.route_start(3), routes.route_start(7), routes.route_of_point(1)), (0, 2, 3));
    routes.remove_route(3);
    assert_eq!((routes.route_start(7), routes.point_position(0), routes.route_start(3)), (0, glam::Vec3::Z, -1));
}

#[test]
fn a_route_that_stops_when_done_runs_out_of_points() {
    let (mut w, r) = world_with_ped();
    w.ents.ped_routes.add_point(5, glam::Vec3::new(1.0, 0.0, 0.0));
    w.ents.ped_routes.add_point(5, glam::Vec3::new(2.0, 0.0, 0.0));
    w.set_follow_route(r, 5, route_type::STOP_WHEN_DONE);
    // the ped starts at the first point and heads for the second
    assert_eq!(w.ped(r).next_route_point_pos, glam::Vec3::new(2.0, 0.0, 0.0));
    w.ped_mut(r).next_route_point_pos = w.epos(r);
    assert!(w.have_reached_next_point_on_route(r, 1.0));
    assert_eq!(w.get_next_point_on_route(r), -1, "past the last point");
}

#[test]
fn a_leader_that_is_too_far_away_is_dropped() {
    let (mut w, r) = world_with_ped();
    let leader = second_ped(&mut w, glam::Vec3::new(100.0, 0.0, 0.0));
    w.ents.timer.time_in_ms = 1000;
    w.set_leader(r, Some(leader));
    with_px(|px| w.update_from_leader(px, r));
    assert_eq!(w.ped(r).leader, None);
}

fn civ(w: &mut World, pos: glam::Vec3) -> EntityRef {
    let r = second_ped(w, pos);
    w.ped_mut(r).ped_type = crate::ped::types::ped_type::CIVMALE;
    r
}

#[test]
fn getting_into_a_car_marks_the_door_and_counts_the_ped() {
    use crate::vehicle::automobile::{car_door_flag, node};
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    let v = car(&mut w, glam::Vec3::new(3.0, 0.0, 0.0));
    w.ped_mut(r).veh_door = node::DOOR_LF as u16;
    w.ped_mut(r).objective = objective::ENTER_CAR_AS_DRIVER;
    with_px(|px| w.set_enter_car(px, r, v, 0));
    assert_eq!(w.ped(r).ped_state, state::ENTER_CAR);
    let vd = w.try_veh(v).unwrap();
    assert_eq!((vd.num_getting_in, vd.getting_in_flags & car_door_flag(node::DOOR_LF) != 0), (1, true));
    assert!(!w.ent(r).base.flags.uses_collision);

    // the same door is taken: a second ped has to wait
    let other = civ(&mut w, glam::Vec3::new(0.0, 1.0, 0.0));
    w.ped_mut(other).veh_door = node::DOOR_LF as u16;
    with_px(|px| w.set_enter_car(px, other, v, 0));
    assert_ne!(w.ped(other).ped_state, state::ENTER_CAR);

    with_px(|px| w.quit_entering_car(px, r));
    let vd = w.try_veh(v).unwrap();
    assert_eq!((vd.num_getting_in, vd.getting_in_flags & car_door_flag(node::DOOR_LF)), (0, 0));
    assert!(w.ent(r).base.flags.uses_collision);
}

#[test]
fn sitting_down_makes_the_ped_the_driver_of_a_physics_car() {
    use crate::entity::status;
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    let v = car(&mut w, glam::Vec3::new(3.0, 0.0, 0.0));
    w.ped_mut(r).my_vehicle = Some(v);
    w.ped_mut(r).ped_state = state::ENTER_CAR;
    w.ped_mut(r).objective = objective::ENTER_CAR_AS_DRIVER;
    w.ped_mut(r).veh_door = crate::vehicle::automobile::node::DOOR_LF as u16;
    w.try_veh_mut(v).unwrap().num_getting_in = 1;
    with_px(|px| w.ped_set_in_car_cb(px, r, None));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.in_vehicle, pd.flags.changed_seat), (state::DRIVING, true, true));
    assert_eq!(w.try_veh(v).unwrap().driver, Some(r));
    assert_eq!((w.ent(v).base.status, w.try_veh(v).unwrap().num_getting_in), (status::PHYSICS, 0));
    assert_eq!(pd.objective, objective::NONE, "the enter objective is spent");
}

#[test]
fn stepping_out_of_the_car_gives_up_the_seat() {
    use crate::entity::status;
    use crate::vehicle::automobile::node;
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    let v = car(&mut w, glam::Vec3::new(3.0, 0.0, 0.0));
    w.vehicle_set_driver(v, r);
    w.ped_mut(r).my_vehicle = Some(v);
    w.ped_mut(r).in_vehicle = true;
    w.ped_mut(r).ped_state = state::EXIT_CAR;
    w.ped_mut(r).veh_door = node::DOOR_LF as u16;
    w.try_veh_mut(v).unwrap().getting_out_flags = crate::vehicle::automobile::car_door_flag(node::DOOR_LF);
    w.get_mut(v).unwrap().base.status = status::PHYSICS;
    with_px(|px| w.ped_set_out_car_cb(px, r, None));
    assert!(!w.ped(r).in_vehicle);
    assert!(w.ent(r).base.flags.uses_collision);
    let vd = w.try_veh(v).unwrap();
    assert_eq!((vd.driver, vd.getting_out_flags), (None, 0));
    assert_eq!(w.ent(v).base.status, status::ABANDONED);
}

#[test]
fn being_dragged_out_is_marked_on_the_car() {
    use crate::vehicle::automobile::{car_door_flag, node};
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    let v = car(&mut w, glam::Vec3::new(3.0, 0.0, 0.0));
    w.vehicle_set_driver(v, r);
    w.ped_mut(r).my_vehicle = Some(v);
    w.ped_mut(r).in_vehicle = true;
    with_px(|px| w.set_being_dragged_from_car(px, r, v, node::DOOR_LF as u32, false));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.flags.will_be_quick_jacked, pd.seek_target), (state::DRAG_FROM_CAR, false, Some(v)));
    assert!(w.try_veh(v).unwrap().getting_out_flags & car_door_flag(node::DOOR_LF) != 0);
    // asking again changes nothing
    with_px(|px| w.set_being_dragged_from_car(px, r, v, node::DOOR_RF as u32, true));
    assert!(!w.ped(r).flags.will_be_quick_jacked);
}

#[test]
fn passengers_shuffle_across_when_a_door_is_blocked() {
    use crate::vehicle::automobile::node;
    let mut w = world_with_ped().0;
    let v = car(&mut w, glam::Vec3::ZERO);
    let p = civ(&mut w, glam::Vec3::new(1.0, 0.0, 0.0));
    {
        let d = w.try_veh_mut(v).unwrap();
        d.num_max_passengers = 3;
        d.passengers[1] = Some(p);
        d.num_passengers = 1;
    }
    // no car model: IsRoomForPedToLeaveCar is false, so nobody moves
    assert!(!w.shuffle_passengers_to_make_space(v));
    assert_eq!(w.try_veh(v).unwrap().passengers[1], Some(p));
    let _ = node::DOOR_LR;
}

#[test]
fn a_full_car_does_not_shuffle() {
    let mut w = world_with_ped().0;
    let v = car(&mut w, glam::Vec3::ZERO);
    {
        let d = w.try_veh_mut(v).unwrap();
        d.num_max_passengers = 3;
        d.num_passengers = 3;
    }
    assert!(!w.shuffle_passengers_to_make_space(v));
}

#[test]
fn the_door_animation_offset_comes_from_the_anim_end() {
    let mut w = world_with_ped().0;
    w.set_anim_offset_for_enter_or_exit_vehicle();
    // no animations are loaded here, so every offset stays zero
    assert_eq!(w.ents.ped_anim_offsets, super::doors::AnimOffsets::default());
}

fn hit(by: Option<EntityRef>, weapon: usize, damage: f32) -> super::pending::Hit {
    super::pending::Hit { by, weapon, damage, piece: piece::TORSO, direction: 0 }
}

#[test]
fn a_weak_punch_hurts_and_a_strong_one_kills() {
    use crate::weapons::info::weapon_type as wt;
    let mut w = world_with_ped().0;
    let victim = civ(&mut w, glam::Vec3::ZERO);
    w.ent_mut_for_test(victim).base.flags.uses_collision = true;
    let died = with_px(|px| w.inflict_damage(px, victim, hit(None, wt::UNARMED, 10.0)));
    assert!(!died);
    // civilians take damage x defend weakness (1.0 by default)
    assert_eq!(w.ped(victim).health, 90.0);

    let attacker = civ(&mut w, glam::Vec3::X);
    let (mut rng, mut spy) = (Rng::default(), Spy::default());
    let died = w.inflict_damage(&mut Px { rng: &mut rng, hooks: &mut spy }, victim, hit(Some(attacker), wt::UNARMED, 500.0));
    assert!(died);
    assert_eq!((w.ped(victim).health, w.ped(victim).ped_state), (0.0, state::DIE));
    assert_eq!(spy.stats, vec![crate::ped::ctx::StatEvent::PeopleKilledByOthers]);
    // already dead: nothing more happens
    assert!(!w.inflict_damage(&mut Px { rng: &mut rng, hooks: &mut spy }, victim, hit(Some(attacker), wt::UNARMED, 500.0)));
}

#[test]
fn armour_soaks_up_damage_first_and_proof_peds_ignore_their_weapon() {
    use crate::weapons::info::weapon_type as wt;
    let mut w = world_with_ped().0;
    let victim = civ(&mut w, glam::Vec3::ZERO);
    w.ent_mut_for_test(victim).base.flags.uses_collision = true;
    w.ped_mut(victim).armour = 20.0;
    with_px(|px| w.inflict_damage(px, victim, hit(None, wt::UNARMED, 30.0)));
    assert_eq!((w.ped(victim).armour, w.ped(victim).health), (0.0, 90.0));

    w.ent_mut_for_test(victim).base.flags.bullet_proof = true;
    assert!(!with_px(|px| w.inflict_damage(px, victim, hit(None, wt::COLT45, 500.0))));
    assert_eq!(w.ped(victim).health, 90.0, "bullet proof");
    w.ent_mut_for_test(victim).base.flags.melee_proof = true;
    assert!(!with_px(|px| w.inflict_damage(px, victim, hit(None, wt::UNARMED, 500.0))));
    assert_eq!(w.ped(victim).health, 90.0, "melee proof");
}

#[test]
fn a_ped_without_collision_cannot_be_hurt_except_by_drowning() {
    use crate::weapons::info::weapon_type as wt;
    let mut w = world_with_ped().0;
    let victim = civ(&mut w, glam::Vec3::ZERO);
    w.ent_mut_for_test(victim).base.flags.uses_collision = false;
    assert!(!with_px(|px| w.inflict_damage(px, victim, hit(None, wt::UNARMED, 500.0))));
    assert_eq!(w.ped(victim).health, 100.0);
    assert!(with_px(|px| w.inflict_damage(px, victim, hit(None, wt::DROWNING, 500.0))));
    assert_eq!(w.ped(victim).ped_state, state::DIE);
}

#[test]
fn a_player_who_cannot_be_damaged_takes_nothing() {
    use crate::weapons::info::weapon_type as wt;
    let (mut w, r) = world_with_ped();
    w.ents.player_ped = Some(r);
    w.ped_mut(r).player = Some(Box::default());
    w.ped_mut(r).player.as_mut().unwrap().can_be_damaged = false;
    w.ent_mut_for_test(r).base.flags.uses_collision = true;
    assert!(!with_px(|px| w.inflict_damage(px, r, hit(None, wt::UNARMED, 500.0))));
    assert_eq!(w.ped(r).health, 100.0);
}

#[test]
fn a_head_shot_pops_the_head_and_kills() {
    let mut w = world_with_ped().0;
    let victim = civ(&mut w, glam::Vec3::ZERO);
    w.ent_mut_for_test(victim).base.flags.uses_collision = true;
    with_px(|px| w.apply_head_shot(px, victim, glam::Vec3::ZERO, false));
    let pd = w.ped(victim);
    assert_eq!((pd.ped_state, pd.flags.body_part_just_came_off), (state::DIE, true));
    // the player is spared unless asked
    let (mut w2, p) = world_with_ped();
    with_px(|px| w2.apply_head_shot(px, p, glam::Vec3::ZERO, false));
    assert_eq!(w2.ped(p).ped_state, state::IDLE);
}

#[test]
fn local_directions_are_front_left_back_right() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).rotation_cur = 0.0;
    // facing north: something to the north is in front, to the west on the left
    assert_eq!(w.get_local_direction(r, glam::Vec2::new(0.0, 1.0)), 0);
    assert_eq!(w.get_local_direction(r, glam::Vec2::new(-1.0, 0.0)), 1);
    assert_eq!(w.get_local_direction(r, glam::Vec2::new(0.0, -1.0)), 2);
    assert_eq!(w.get_local_direction(r, glam::Vec2::new(1.0, 0.0)), 3);
}

#[test]
fn aiming_a_gun_enters_the_aim_state_and_clearing_goes_back() {
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    let target = civ(&mut w, glam::Vec3::new(0.0, 5.0, 0.0));
    with_px(|px| w.set_point_gun_at(px, r, target));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.flags.is_pointing_gun_at, pd.look_target), (state::AIM_GUN, true, Some(target)));
    assert!(pd.flags.is_aiming_gun);
    with_px(|px| w.clear_point_gun_at(px, r));
    let pd = w.ped(r);
    assert_ne!(pd.ped_state, state::AIM_GUN);
    assert!(!pd.flags.is_pointing_gun_at && !pd.flags.is_aiming_gun);
}

#[test]
fn starting_a_fight_enters_the_fight_state_and_ending_leaves_it() {
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    with_px(|px| w.start_fight_attack(px, r, 100));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.cur_fight_move, pd.flags.is_attacking, pd.fight_state), (state::FIGHT, fight_move::STDPUNCH, true, fight_state::NO_MOVE));
    // pressing again while fighting only stores the pressure
    with_px(|px| w.start_fight_attack(px, r, 77));
    assert_eq!(w.ped(r).fight_button_pressure, 77);
    with_px(|px| w.end_fight(px, r, ENDFIGHT_FAST));
    let pd = w.ped(r);
    assert_ne!(pd.ped_state, state::FIGHT);
    assert_eq!(pd.cur_fight_move, fight_move::NULL);
}

#[test]
fn point_blank_peds_are_found_in_front_only() {
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    let front = civ(&mut w, glam::Vec3::new(0.0, 0.8, 0.0));
    let behind = civ(&mut w, glam::Vec3::new(0.0, -0.8, 0.0));
    w.ped_mut(r).near_peds[0] = Some(behind);
    w.ped_mut(r).num_near_peds = 1;
    assert_eq!(w.check_for_point_blank_peds(r, None), point_blank::NONE);
    w.ped_mut(r).near_peds[0] = Some(front);
    assert_eq!(w.check_for_point_blank_peds(r, None), point_blank::FOR_SOMEONE_ELSE);
    assert_eq!(w.check_for_point_blank_peds(r, Some(front)), point_blank::FOR_WANTED_PED);
}

#[test]
fn investigating_walks_to_the_event_and_clearing_resumes_the_old_state() {
    use super::pending::Investigate;
    let mut w = world_with_ped().0;
    let r = civ(&mut w, glam::Vec3::ZERO);
    w.ents.timer.time_in_ms = 1000;
    w.ped_mut(r).ped_state = state::WANDER_PATH;
    with_px(|px| w.set_investigate_event(px, r, Investigate { event: event_type::ICECREAM, pos: glam::Vec2::new(10.0, 0.0), distance_to_count_done: 0.5, time: 5000, angle: 0.0 }));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.event_type, pd.chat_timer), (state::INVESTIGATE, event_type::ICECREAM, 6000));
    with_px(|px| w.investigate_event(px, r));
    assert_eq!(w.ped(r).move_state, move_state::WALK, "heads for the event");
    with_px(|px| w.clear_investigate_event(px, r));
    assert_ne!(w.ped(r).ped_state, state::INVESTIGATE);
    assert!(!w.ped(r).flags.gonna_investigate_event);
}

#[test]
fn a_body_part_coming_off_is_remembered() {
    let (mut w, r) = world_with_ped();
    with_px(|px| w.remove_body_part(px, r, node::HEAD, 1));
    let pd = w.ped(r);
    assert_eq!((pd.flags.body_part_just_came_off, pd.body_part_bleeding), (true, node::HEAD as i8));
}

#[test]
fn nothing_to_stand_on_means_no_dry_land() {
    let (mut w, r) = world_with_ped();
    assert!(!w.place_ped_on_dry_land(r));
}

#[test]
fn a_queued_death_cry_is_played_at_once_and_the_next_one_waits() {
    let mut spy = Spy::default();
    let (mut w, r) = world_with_ped();
    w.ents.timer.time_in_ms = 5000;
    w.ped_mut(r).queued_sound = crate::audio::sound::PED_DEATH;
    let mut rng = Rng::default();
    w.service_talking(&mut Px { rng: &mut rng, hooks: &mut spy }, r);
    let pd = w.ped(r);
    assert_eq!((pd.queued_sound, pd.last_queued_sound, pd.last_sound_start), (NO_SOUND, crate::audio::sound::PED_DEATH, 5000));
    assert!(pd.sound_start >= 5500, "a fixed delay of 500 ms comes before the next remark");
}

#[test]
fn a_frame_of_an_idle_ped_leaves_it_idle() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).panic_counter = 7;
    with_px(|px| w.process_control(px, r));
    assert_eq!((w.ped(r).ped_state, w.ped(r).panic_counter), (state::IDLE, 0));
}

#[test]
fn a_ped_with_no_health_left_starts_to_die() {
    let (mut w, r) = world_with_ped();
    w.ped_mut(r).health = 1.0;
    with_px(|px| w.process_control(px, r));
    assert_eq!(w.ped(r).ped_state, state::DIE);
}

#[test]
fn a_made_ped_is_unarmed_and_a_destroyed_one_leaves_its_neighbours_lists() {
    let mut w = World::new(crate::modelinfo::ModelInfoStore::new(Game::Gta3), TempColModels::new(Game::Gta3));
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    let a = w.create_ped(&mut px, crate::ped::types::ped_type::CIVMALE, -1, None).unwrap();
    let b = w.create_ped(&mut px, crate::ped::types::ped_type::CIVMALE, -1, None).unwrap();
    assert_eq!((w.ped(a).health, w.ped(a).current_weapon as usize), (100.0, crate::weapons::info::weapon_type::UNARMED));
    w.ents.player_ped = Some(b); // counts as a valid pointer without a collision model
    let pd = w.ped_mut(b);
    pd.near_peds[0] = Some(a);
    pd.num_near_peds = 1;
    w.ped_mut(a).near_peds[0] = Some(b);
    w.ped_mut(a).num_near_peds = 1;
    w.destroy_ped(&mut px, a);
    assert!(w.try_ped(a).is_none());
    assert_eq!((w.ped(b).near_peds[0], w.ped(b).num_near_peds), (None, 0));
}

fn world_with_player_and_cop() -> (World, EntityRef, EntityRef) {
    let mut w = World::new(crate::modelinfo::ModelInfoStore::new(Game::Gta3), TempColModels::new(Game::Gta3));
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    let player = w.create_ped(&mut px, crate::ped::types::ped_type::PLAYER1, -1, None).unwrap();
    w.ped_mut(player).player = Some(Default::default());
    w.ents.player_ped = Some(player);
    let cop = w.create_cop(&mut px, crate::ped::cop::cop_type::FBI, None).unwrap();
    (w, player, cop)
}

#[test]
fn an_fbi_agent_carries_an_ak_and_armour() {
    use crate::weapons::info::weapon_type as wt;
    let (w, _, cop) = world_with_player_and_cop();
    let pd = w.ped(cop);
    assert_eq!((pd.current_weapon as usize, pd.armour, pd.wep_accuracy), (wt::AK47, 100.0, 76));
}

#[test]
fn a_cop_takes_a_slot_in_the_pursuit_and_gives_it_back() {
    let (mut w, player, cop) = world_with_player_and_cop();
    w.player_wanted_mut().unwrap().set_wanted_level(2, 0);
    with_px(|px| w.set_pursuit(px, cop, false));
    assert!(w.ped(cop).cop.as_ref().unwrap().is_in_pursuit);
    let wanted = w.player_wanted().unwrap();
    assert_eq!((wanted.current_cops, wanted.cops[0]), (1, Some(cop)));
    assert_eq!((w.ped(cop).objective, w.ped(cop).ped_in_objective), (objective::KILL_CHAR_ON_FOOT, Some(player)));
    with_px(|px| w.clear_pursuit(px, cop));
    assert_eq!((w.player_wanted().unwrap().current_cops, w.player_wanted().unwrap().cops[0]), (0, None));
}

#[test]
fn arresting_the_player_stops_them_and_makes_them_harmless() {
    let (mut w, player, cop) = world_with_player_and_cop();
    with_px(|px| w.set_arrest_player(px, cop, player));
    assert_eq!((w.ped(player).ped_state, w.ped(cop).ped_state, w.ped(cop).seek_target), (state::ARRESTED, state::ARREST_PLAYER, Some(player)));
    let p = w.ped(player).player.as_ref().unwrap();
    assert_eq!((p.arresting_cop, p.can_be_damaged), (Some(cop), false));
}


#[test]
fn a_witness_runs_to_the_nearest_free_phone() {
    use crate::control::wanted::crime;
    let (mut w, r) = world_with_ped();
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    w.ents.phones.phones = vec![Default::default(); 4];
    for p in &mut w.ents.phones.phones[..3] {
        p.state = crate::control::phones::phone_state::MESSAGE_REMOVED;
    }
    w.ents.phones.phones[3].pos = glam::Vec3::new(5.0, 0.0, 0.0);
    w.ents.phones.max = 4;
    assert!(w.run_to_report_crime(&mut Px { rng: &mut rng, hooks: &mut hooks }, r, crime::POSSESSION_GUN));
    let pd = w.ped(r);
    assert_eq!((pd.ped_state, pd.phone_id, pd.running_to_phone, pd.move_state), (state::SEEK_POS, 3, true, move_state::RUN));
    assert!(!w.run_to_report_crime(&mut Px { rng: &mut rng, hooks: &mut hooks }, r, crime::POSSESSION_GUN), "already on its way");
}

#[test]
fn a_prostitute_costs_the_player_a_dollar_a_second() {
    let (mut w, player, hooker) = world_with_player_and_cop();
    w.ents.player_ped = Some(player);
    w.ped_mut(hooker).ped_type = crate::ped::types::ped_type::PROSTITUTE;
    w.ents.player_info.hooker = Some(hooker);
    w.ents.player_info.money = 50;
    w.ents.timer.time_in_ms = 5000;
    w.ents.player_info.next_sex_frequency_update_time = 99999;
    w.ped_mut(hooker).my_vehicle = Some(player); // only read when the frequency timer runs out
    with_px(|px| w.hooker_in_car(px, hooker));
    assert_eq!((w.ents.player_info.money, w.ents.player_info.next_sex_money_update_time), (49, 6000));
}

#[test]
fn a_medic_goes_for_the_nearest_dead_ped_and_counts_as_attending() {
    use crate::ped::emergency::emergency_state;
    let mut w = World::new(crate::modelinfo::ModelInfoStore::new(Game::Gta3), TempColModels::new(Game::Gta3));
    let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
    let mut px = Px { rng: &mut rng, hooks: &mut hooks };
    let medic = w.create_emergency(&mut px, crate::ped::types::ped_type::EMERGENCY, None).unwrap();
    let victim = w.create_ped(&mut px, crate::ped::types::ped_type::CIVMALE, -1, None).unwrap();
    w.get_mut(victim).unwrap().base.set_position(glam::Vec3::new(10.0, 0.0, 0.0));
    w.ped_mut(victim).health = 0.0;
    w.ents.accidents.accidents[0].victim = Some(victim);
    w.medic_ai(&mut px, medic);
    let e = w.ped(medic).emergency.as_ref().unwrap();
    assert_eq!((e.state, e.revived_ped, e.attended_accident), (emergency_state::DETERMINE_NEXT_STATE, Some(victim), Some(0)));
    assert_eq!((w.ents.accidents.accidents[0].medics_attending, w.ped(medic).ped_state), (1, state::SEEK_POS));
}

#[test]
fn the_nearest_fire_is_the_one_with_the_fewest_firemen() {
    use crate::control::fire::{Fire, FireManager};
    let mut m = FireManager::default();
    m.fires[0] = Fire { is_ongoing: true, pos: glam::Vec3::new(5.0, 0.0, 0.0), firemen_putting_out: 1, ..Default::default() };
    m.fires[1] = Fire { is_ongoing: true, pos: glam::Vec3::new(30.0, 0.0, 0.0), ..Default::default() };
    assert_eq!(m.find_nearest(glam::Vec3::ZERO), (Some(1), 30.0), "the unattended fire first, however far");
    m.fires[1].firemen_putting_out = 1;
    assert_eq!(m.find_nearest(glam::Vec3::ZERO).0, Some(0), "with one fireman each, the closer one wins");
    m.fires[0].firemen_putting_out = 2;
    m.fires[1].firemen_putting_out = 2;
    assert_eq!(m.find_nearest(glam::Vec3::ZERO).0, None, "two firemen on a fire is enough");
}

mod population {
    use super::*;
    use crate::ped::cop::cop_type;
    use crate::ped::population::{find_safe_zone, level};
    use crate::ped::types::ped_type;
    use glam::Vec3;

    struct Offscreen;
    impl PedHooks for Offscreen {
        fn is_on_screen(&mut self, _entity: EntityRef) -> bool {
            false
        }
    }

    #[test]
    fn counts_follow_the_ped_type() {
        let (mut w, _) = world_with_ped();
        let pop = &mut w.ents.population;
        pop.update_ped_count(ped_type::CRIMINAL, false);
        pop.update_ped_count(ped_type::PROSTITUTE, false);
        pop.update_ped_count(ped_type::FIREMAN, false);
        pop.update_ped_count(ped_type::GANG3, false);
        pop.update_ped_count(ped_type::PLAYER1, false);
        assert_eq!((pop.num_civ_male, pop.num_civ_female, pop.num_emergency, pop.num_gang[2]), (1, 1, 1, 1));
        pop.update_ped_count(ped_type::GANG3, true);
        assert_eq!(pop.num_gang[2], 0);
    }

    #[test]
    fn making_and_destroying_a_ped_moves_the_count() {
        let (mut w, _) = world_with_ped();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let r = w.add_ped(&mut px, ped_type::CIVFEMALE, -1, Vec3::new(5.0, 6.0, 7.0)).unwrap();
        assert_eq!((w.ents.population.num_civ_female, w.epos(r)), (1, Vec3::new(5.0, 6.0, 7.0)));
        w.remove_ped(&mut px, r);
        assert_eq!(w.ents.population.num_civ_female, 0);
        assert!(w.try_ped(r).is_none());
    }

    #[test]
    fn a_cop_comes_with_its_type_and_weapons() {
        let (mut w, _) = world_with_ped();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let r = w.add_ped(&mut px, ped_type::COP, cop_type::SWAT as i32, Vec3::ZERO).unwrap();
        assert_eq!((w.ped(r).class, w.ped(r).armour, w.ents.population.num_cop), (PedClass::Cop, 50.0, 1));
        assert_eq!(w.ped(r).current_weapon as usize, crate::weapons::info::weapon_type::UZI);
        let f = w.add_ped(&mut px, ped_type::FIREMAN, 0, Vec3::ZERO).unwrap();
        assert_eq!((w.ped(f).class, w.ents.population.num_emergency), (PedClass::Emergency, 1));
    }

    #[test]
    fn gangs_pick_their_model_pair() {
        let (mut w, _) = world_with_ped();
        let mut rng = Rng::default();
        let first = crate::modelinfo::indices_gta3::mi::GANG01 as i32 + 2 * 3;
        w.ents.gangs.0[3].ped_model_override = 0;
        assert_eq!(w.choose_gang_occupation(&mut rng, 3), first);
        w.ents.gangs.0[3].ped_model_override = 1;
        assert_eq!(w.choose_gang_occupation(&mut rng, 3), first + 1);
        w.ents.gangs.0[3].ped_model_override = -1;
        assert!((first..first + 2).contains(&w.choose_gang_occupation(&mut rng, 3)));
    }

    #[test]
    fn a_zone_change_picks_the_regeneration_side() {
        let (mut w, _) = world_with_ped();
        assert_eq!(find_safe_zone(Vec3::new(600.0, -930.0, 50.0)), Some(0));
        assert_eq!(find_safe_zone(Vec3::new(0.0, 0.0, 0.0)), None);
        w.deal_with_zone_change(level::COMMERCIAL, level::INDUSTRIAL, true);
        let pop = &w.ents.population;
        // the Callahan Bridge zone: the industrial side is where the zone's source points are
        assert_eq!(pop.regeneration_point_a, Vec3::new(424.0, -942.0, 38.0));
        assert!((pop.regeneration_front.length() - 1.0).abs() < 1e-5 && pop.regeneration_front.x > 0.9);
        w.deal_with_zone_change(level::INDUSTRIAL, level::COMMERCIAL, true);
        assert_eq!(w.ents.population.regeneration_point_a, Vec3::new(790.0, -917.0, 39.0));
    }

    #[test]
    fn far_peds_are_removed_or_faded() {
        let (mut w, player) = world_with_ped();
        w.ents.player_ped = Some(player);
        w.ped_mut(player).ped_type = ped_type::PLAYER1;
        let (mut rng, mut hooks) = (Rng::default(), Offscreen);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let near = w.add_ped(&mut px, ped_type::CIVMALE, -1, Vec3::new(5.0, 0.0, 0.0)).unwrap();
        let far = w.add_ped(&mut px, ped_type::CIVMALE, -1, Vec3::new(500.0, 0.0, 0.0)).unwrap();
        let gone = w.add_ped(&mut px, ped_type::CIVMALE, -1, Vec3::new(5.0, 0.0, 0.0)).unwrap();
        w.ped_mut(gone).flags.fade_out = true;
        w.ped_mut(gone).clump_alpha = 0;
        w.manage_population(&mut px);
        assert!(w.try_ped(near).is_some() && w.try_ped(far).is_none() && w.try_ped(gone).is_none());
        assert_eq!(w.ents.population.num_civ_male, 1);

        // on screen it fades first, and a long-dead ped starts fading by itself
        let mut hooks = NoPedHooks;
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let seen = w.add_ped(&mut px, ped_type::CIVMALE, -1, Vec3::new(500.0, 0.0, 0.0)).unwrap();
        w.manage_population(&mut px);
        assert!(w.ped(seen).flags.fade_out);
        w.ped_mut(near).ped_state = state::DEAD;
        w.ents.timer.time_in_ms = 70_000;
        w.manage_population(&mut px);
        assert!(w.ped(near).flags.fade_out);
    }

    #[test]
    fn nothing_spawns_without_a_path_graph() {
        let (mut w, player) = world_with_ped();
        w.ents.player_ped = Some(player);
        w.ped_mut(player).player = Some(Default::default());
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.add_to_population(&mut px, 40.0, 50.0, 20.0, 25.0);
        assert_eq!(w.ents.pools.peds.used(), 1);
    }
}

mod dispatch {
    use super::*;
    use crate::world::WorldEvent;
    use glam::Vec3;

    fn px_parts() -> (Rng, NoPedHooks) {
        (Rng::default(), NoPedHooks)
    }

    #[test]
    fn the_cop_branch_runs_for_a_cop() {
        let (mut w, player, cop) = world_with_player_and_cop();
        w.ent_mut_for_test(player).base.set_position(Vec3::new(10.0, 0.0, 0.0));
        let (mut rng, mut hooks) = px_parts();
        w.process_ped_control(&mut Px { rng: &mut rng, hooks: &mut hooks }, cop);
        // only CCopPed::ProcessControl measures the way to the player
        assert!((w.ped(cop).cop.as_ref().unwrap().distance_to_target - 10.0).abs() < 1.0);
    }

    #[test]
    fn a_hard_landing_hurts_and_a_rest_is_handed_back() {
        let (mut w, a) = world_with_ped();
        let (mut rng, mut hooks) = px_parts();
        w.ent_mut_for_test(a).base.flags.uses_collision = true;
        let health = w.ped(a).health;
        let by = EntityRef { pool: crate::entity::PoolKind::Building, handle: 0 };
        let rest = w.handle_ped_events(
            &mut Px { rng: &mut rng, hooks: &mut hooks },
            vec![WorldEvent::PedFall { ped: a, by, damage: 30.0, direction: 2 }, WorldEvent::SwitchToRealPhysics(by)],
        );
        assert!(w.ped(a).health < health);
        assert_eq!(rest, vec![WorldEvent::SwitchToRealPhysics(by)]);
    }

    #[test]
    fn an_explosion_throws_the_ped_and_blames_the_culprit() {
        let (mut w, player, cop) = world_with_player_and_cop();
        let (mut rng, mut hooks) = px_parts();
        let health = w.ped(cop).health;
        w.handle_ped_events(
            &mut Px { rng: &mut rng, hooks: &mut hooks },
            vec![WorldEvent::ExplosionHitPed { ped: cop, culprit: Some(player), damage: 20.0, force_dir: Vec3::new(1.0, 0.0, 0.0) }],
        );
        assert!(w.ped(cop).health < health || w.ped(cop).armour < 100.0);
        assert!(matches!(w.ped(cop).ped_state, state::FALL | state::DIE | state::DEAD));
    }

    #[test]
    fn events_about_a_ped_that_is_gone_are_ignored() {
        let (mut w, a) = world_with_ped();
        let (mut rng, mut hooks) = px_parts();
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.destroy_ped(&mut px, a);
        let rest = w.handle_ped_events(&mut px, vec![WorldEvent::RemovePed(a), WorldEvent::PedFall { ped: a, by: a, damage: 5.0, direction: 0 }]);
        assert!(rest.is_empty());
    }
}

mod player_info {
    use super::*;
    use crate::player_info::wb_state;

    #[derive(Default)]
    struct Rec {
        ended: Vec<bool>,
        safe: Vec<bool>,
        money: Vec<i32>,
        distance: Vec<(bool, f32)>,
    }
    impl PedHooks for Rec {
        fn player_wasted_or_busted(&mut self, busted: bool) {
            self.ended.push(busted);
        }
        fn set_player_safe_controls(&mut self, safe: bool) {
            self.safe.push(safe);
        }
        fn money_message(&mut self, _pos: glam::Vec3, amount: i32) {
            self.money.push(amount);
        }
        fn add_distance_travelled(&mut self, in_vehicle: bool, distance: f32) {
            self.distance.push((in_vehicle, distance));
        }
    }

    #[test]
    fn dying_or_being_caught_happens_once() {
        let (mut w, _, _) = world_with_player_and_cop();
        let (mut rng, mut hooks) = (Rng::default(), Rec::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.ents.timer.time_in_ms = 500;
        w.kill_player(&mut px);
        w.arrest_player_info(&mut px);
        w.kill_player(&mut px);
        assert_eq!((w.ents.player_info.wb_state, w.ents.player_info.wb_time), (wb_state::WASTED, 500));
        assert_eq!(hooks.ended, vec![false]);
    }

    #[test]
    fn the_money_counter_catches_up_in_steps() {
        let (mut w, _, _) = world_with_player_and_cop();
        let (mut rng, mut hooks) = (Rng::default(), Rec::default());
        w.ents.player_info.money = 20_000;
        w.process_player_info(&mut Px { rng: &mut rng, hooks: &mut hooks });
        assert_eq!(w.ents.player_info.visible_money, 1234);
        w.ents.player_info.money = 1250;
        w.process_player_info(&mut Px { rng: &mut rng, hooks: &mut hooks });
        assert_eq!(w.ents.player_info.visible_money, 1235);
        assert!((0.4..=1.45).contains(&w.ents.player_info.road_density));
        assert_eq!(hooks.distance.len(), 2);
        assert!(!hooks.distance[0].0);
    }

    #[test]
    fn making_the_player_safe_and_back() {
        let (mut w, player, _) = world_with_player_and_cop();
        let (mut rng, mut hooks) = (Rng::default(), Rec::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.make_player_safe(&mut px, true);
        let e = w.ent(player);
        assert!(e.base.flags.bullet_proof && e.base.flags.explosion_proof && e.base.flags.only_damaged_by_player);
        assert!(!w.ped(player).player.as_ref().unwrap().can_be_damaged);
        assert!(w.player_wanted().unwrap().ignored_by_everyone);
        w.make_player_safe(&mut px, false);
        assert!(!w.ent(player).base.flags.bullet_proof && w.ped(player).player.as_ref().unwrap().can_be_damaged);
        assert!(!w.player_wanted().unwrap().ignored_by_everyone);
        assert_eq!(hooks.safe, vec![true, false]);
    }

    #[test]
    fn chained_explosions_pay_more() {
        let (mut w, _, _) = world_with_player_and_cop();
        let (mut rng, mut hooks) = (Rng::default(), Rec::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let car = w.ents.player_ped.unwrap(); // no vehicle data: worth nothing, but the chain still counts
        w.ents.timer.time_in_ms = 10_000;
        w.award_money_for_explosion(&mut px, car);
        w.ents.timer.time_in_ms = 12_000;
        w.award_money_for_explosion(&mut px, car);
        assert_eq!(w.ents.player_info.explosions_since_last_reward, 2);
        w.ents.timer.time_in_ms = 30_000;
        w.award_money_for_explosion(&mut px, car);
        assert_eq!(w.ents.player_info.explosions_since_last_reward, 1);
    }
}

mod shots {
    use super::*;
    use crate::ped::types::ped_type;
    use crate::weapons::info::weapon_type as wt;
    use glam::Vec3;

    #[derive(Default)]
    struct Events {
        got: Vec<(u32, EntityRef, EntityRef, u32)>,
    }
    impl PedHooks for Events {
        fn register_event_with_culprit(&mut self, event: u32, entity: EntityRef, culprit: EntityRef, time: u32) {
            self.got.push((event, entity, culprit, time));
        }
    }

    #[test]
    fn a_punch_hurts_and_is_reported_as_an_assault() {
        let (mut w, _) = world_with_ped();
        let (mut rng, mut hooks) = (Rng::default(), Events::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let a = w.add_ped(&mut px, ped_type::CIVMALE, -1, Vec3::ZERO).unwrap();
        let b = w.add_ped(&mut px, ped_type::CIVFEMALE, -1, Vec3::new(0.3, 0.0, 0.0)).unwrap();
        w.ents.weapon_infos.0[wt::UNARMED].radius = 1.0;
        w.ents.weapon_infos.0[wt::UNARMED].damage = 10;
        {
            let pd = w.ped_mut(a);
            pd.near_peds[0] = Some(b);
            pd.num_near_peds = 1;
        }
        let health = w.ped(b).health;
        assert!(w.fire_melee_shot(&mut px, a, wt::UNARMED, Vec3::new(0.2, 0.0, 0.0)));
        assert!(w.ped(b).health < health);
        assert_eq!(hooks.got.len(), 1);
        assert_eq!((hooks.got[0].0, hooks.got[0].1, hooks.got[0].2, hooks.got[0].3), (event_type::ASSAULT, b, a, 2000));
    }

    #[derive(Default)]
    struct Fx {
        effects: Vec<crate::ped::ctx::ShotEffect>,
    }
    impl PedHooks for Fx {
        fn shot_effect(&mut self, e: crate::ped::ctx::ShotEffect) {
            self.effects.push(e);
        }
        fn is_on_screen(&mut self, _e: EntityRef) -> bool {
            true
        }
    }

    fn two_peds(px: &mut Px) -> (World, EntityRef, EntityRef) {
        let (mut w, _) = world_with_ped();
        let a = w.add_ped(px, ped_type::CIVMALE, -1, Vec3::ZERO).unwrap();
        let b = w.add_ped(px, ped_type::CIVFEMALE, -1, Vec3::new(0.0, 5.0, 0.0)).unwrap();
        w.ents.weapon_infos.0[wt::COLT45].damage = 25;
        (w, a, b)
    }

    #[test]
    fn a_bullet_hurts_the_ped_it_hits_and_reports_it() {
        use crate::ped::ctx::ShotEffect;
        let (mut rng, mut hooks) = (Rng::default(), Fx::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a, b) = two_peds(&mut px);
        let health = w.ped(b).health;
        let point = crate::collision::ColPoint { point: Vec3::new(0.0, 4.9, 1.0), ..Default::default() };
        w.do_bullet_impact(&mut px, a, wt::COLT45, Some(b), Vec3::ZERO, Vec3::new(0.0, 50.0, 0.0), point, glam::Vec2::Y);
        assert!(w.ped(b).health < health);
        assert_eq!(w.ped(b).queued_sound, crate::audio::sound::PED_BULLET_HIT);
        assert!(hooks.effects.iter().any(|e| matches!(e, ShotEffect::BloodSmall { count: 8, .. })));
        assert!(hooks.effects.iter().any(|e| matches!(e, ShotEffect::Trace { to, .. } if *to == point.point)));
    }

    #[test]
    fn a_bullet_at_nothing_only_leaves_a_trace() {
        use crate::ped::ctx::ShotEffect;
        let (mut rng, mut hooks) = (Rng::default(), Fx::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a, _) = two_peds(&mut px);
        let to = Vec3::new(0.0, 50.0, 0.0);
        w.do_bullet_impact(&mut px, a, wt::COLT45, None, Vec3::ZERO, to, Default::default(), glam::Vec2::Y);
        assert_eq!(hooks.effects, vec![ShotEffect::Trace { from: Vec3::ZERO, to }]);
    }

    #[test]
    fn a_colt_shot_draws_the_shell_numbers_and_an_uzi_adds_smoke() {
        let (mut rng, mut hooks) = (Rng::default(), Fx::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a, _) = two_peds(&mut px);
        let mut expect = Rng::default();
        expect.range_f32(0.02, 0.08);
        expect.range_f32(-20.0, 20.0);
        let (mut rng2, mut hooks2) = (Rng::default(), Fx::default());
        w.gun_effects(&mut Px { rng: &mut rng2, hooks: &mut hooks2 }, a, wt::COLT45, Vec3::ZERO, glam::Vec2::Y, None);
        assert_eq!(rng2.number(), expect.number());
        assert_eq!(hooks2.effects.len(), 1 + 3 + 1);
        // AK47 and M16 fire their effects on every second shot only
        let mut hooks3 = Fx::default();
        for _ in 0..2 {
            w.gun_effects(&mut Px { rng: &mut rng2, hooks: &mut hooks3 }, a, wt::AK47, Vec3::ZERO, glam::Vec2::Y, None);
        }
        assert_eq!(hooks3.effects.len(), 3);
    }

    #[test]
    fn doom_aiming_tilts_the_shot_towards_the_nearest_ped_in_view() {
        let (mut rng, mut hooks) = (Rng::default(), Fx::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a, b) = two_peds(&mut px);
        let c = w.add_ped(&mut px, ped_type::CIVMALE, -1, Vec3::new(0.0, 10.0, 2.0)).unwrap();
        let mut target = Vec3::new(0.0, 40.0, 0.0);
        w.aim_at_closest(a, &[a, c, b], Vec3::ZERO, &mut target);
        // b (5 m, level) is nearest: the aim reaches its chest (0.3) at the target's distance
        assert!((target.z - 40.0 / 5.0 * 0.3).abs() < 1.0e-4, "{target:?}");
        // a ducking ped is aimed at lower
        w.ped_mut(b).flags.is_ducking = true;
        let mut target = Vec3::new(0.0, 40.0, 0.0);
        w.aim_at_closest(a, &[b], Vec3::ZERO, &mut target);
        assert!((target.z - 40.0 / 5.0 * -0.5).abs() < 1.0e-4);
    }

    #[derive(Default)]
    struct Gun {
        effects: Vec<crate::ped::ctx::ShotEffect>,
        one_shots: Vec<(EntityRef, u16)>,
        events: Vec<u32>,
        first_person: bool,
    }
    impl PedHooks for Gun {
        fn shot_effect(&mut self, e: crate::ped::ctx::ShotEffect) {
            self.effects.push(e);
        }
        fn play_one_shot(&mut self, ped: EntityRef, sound: u16, _volume: f32) {
            self.one_shots.push((ped, sound));
        }
        fn register_event_with_culprit(&mut self, event: u32, _e: EntityRef, _c: EntityRef, _t: u32) {
            self.events.push(event);
        }
        fn camera(&mut self) -> crate::ped::ctx::CameraState {
            crate::ped::ctx::CameraState { weapon_first_person: self.first_person, front: Vec3::Y, up: Vec3::Z, source: Vec3::new(0.0, -2.0, 1.0), ..Default::default() }
        }
    }

    fn armed(px: &mut Px, weapon: usize, fire: crate::weapons::info::WeaponFire, ammo: i32) -> (World, EntityRef) {
        let (mut w, _) = world_with_ped();
        let a = w.add_ped(px, ped_type::CIVMALE, -1, Vec3::ZERO).unwrap();
        let info = &mut w.ents.weapon_infos.0[weapon];
        info.fire = fire;
        info.range = 100.0;
        info.ammo = 7;
        info.reload = 1000;
        info.damage = 10;
        w.give_weapon(a, weapon, ammo);
        w.set_current_weapon(a, weapon);
        (w, a)
    }

    #[test]
    fn firing_a_colt_uses_a_round_and_reports_the_shot() {
        use crate::weapons::info::WeaponFire;
        let (mut rng, mut hooks) = (Rng::default(), Gun::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::COLT45, WeaponFire::InstantHit, 30);
        w.ents.timer.time_in_ms = 1000;
        assert!(w.fire_weapon(&mut px, a, Vec3::new(0.0, 0.5, 0.6)));
        let wp = &w.ped(a).weapons[wt::COLT45];
        assert_eq!((wp.ammo_in_clip, wp.ammo_total, wp.state, wp.timer), (6, 29, crate::weapons::weapon::WeaponState::Firing, 2000));
        assert!(w.ped(a).flags.is_shooting);
        assert_eq!(hooks.one_shots, vec![(a, crate::audio::sound::WEAPON_SHOT_FIRED)]);
        assert!(hooks.events.contains(&event_type::GUNSHOT));
        // nothing was in the way: one trace to the end of the range
        assert!(hooks.effects.iter().any(|e| matches!(e, crate::ped::ctx::ShotEffect::Trace { .. })));
    }

    #[test]
    fn an_empty_clip_does_not_fire() {
        use crate::weapons::info::WeaponFire;
        let (mut rng, mut hooks) = (Rng::default(), Gun::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::COLT45, WeaponFire::InstantHit, 0);
        assert!(!w.fire_weapon(&mut px, a, Vec3::ZERO));
        assert!(hooks.one_shots.is_empty());
    }

    #[test]
    fn a_shotgun_blast_is_five_traces() {
        use crate::weapons::info::WeaponFire;
        let (mut rng, mut hooks) = (Rng::default(), Gun::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::SHOTGUN, WeaponFire::InstantHit, 20);
        assert!(w.fire_weapon(&mut px, a, Vec3::new(0.0, 0.5, 0.6)));
        let traces = hooks.effects.iter().filter(|e| matches!(e, crate::ped::ctx::ShotEffect::Trace { .. })).count();
        assert_eq!(traces, 5);
    }

    #[test]
    fn a_thrown_grenade_becomes_a_projectile() {
        use crate::weapons::info::WeaponFire;
        let (mut rng, mut hooks) = (Rng::default(), Gun::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::GRENADE, WeaponFire::Projectile, 3);
        assert!(w.fire_weapon(&mut px, a, Vec3::new(0.0, 0.5, 0.6)));
        assert_eq!(w.ents.projectiles.0.iter().filter(|p| p.in_use).count(), 1);
        assert_eq!(w.ped(a).weapons[wt::GRENADE].ammo_total, 2);
    }

    #[test]
    fn the_sniper_only_fires_through_the_scope() {
        use crate::weapons::info::WeaponFire;
        let (mut rng, mut hooks) = (Rng::default(), Gun::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::SNIPERRIFLE, WeaponFire::InstantHit, 10);
        assert!(!w.fire_weapon(&mut px, a, Vec3::ZERO));
        assert!(w.ents.bullets.list.iter().all(|b| !b.in_use));
        let mut scoped = Gun { first_person: true, ..Default::default() };
        let mut px = Px { rng: &mut rng, hooks: &mut scoped };
        assert!(w.fire_weapon(&mut px, a, Vec3::ZERO));
        assert_eq!(w.ents.bullets.list.iter().filter(|b| b.in_use).count(), 1);
    }

    #[test]
    fn a_punch_through_fire_weapon_marks_the_contact() {
        use crate::weapons::info::WeaponFire;
        let (mut rng, mut hooks) = (Rng::default(), Gun::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::UNARMED, WeaponFire::Melee, 0);
        let b = w.add_ped(&mut px, ped_type::CIVFEMALE, -1, Vec3::new(0.3, 0.0, 0.0)).unwrap();
        w.ents.weapon_infos.0[wt::UNARMED].radius = 1.0;
        let pd = w.ped_mut(a);
        pd.near_peds[0] = Some(b);
        pd.num_near_peds = 1;
        assert!(w.fire_weapon(&mut px, a, Vec3::new(0.2, 0.0, 0.0)));
        assert_eq!(w.ped(a).weapons[wt::UNARMED].state, crate::weapons::weapon::WeaponState::MeleeMadeContact);
    }

    #[test]
    fn only_the_player_with_an_uzi_looking_aside_is_driving_by() {
        let (mut rng, mut hooks) = (Rng::default(), Gun::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::UZI, crate::weapons::info::WeaponFire::InstantHit, 30);
        assert!(!w.is_ped_doing_drive_by_shooting(a, true));
        w.ents.player_ped = Some(a);
        assert!(w.is_ped_doing_drive_by_shooting(a, true));
        assert!(!w.is_ped_doing_drive_by_shooting(a, false));
        w.set_current_weapon(a, wt::UNARMED);
        assert!(!w.is_ped_doing_drive_by_shooting(a, true));
    }

    #[test]
    fn a_sniper_bullet_flies_on_and_expires() {
        use crate::weapons::info::WeaponFire;
        let (mut rng, mut hooks) = (Rng::default(), Gun { first_person: true, ..Default::default() });
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a) = armed(&mut px, wt::SNIPERRIFLE, WeaponFire::InstantHit, 10);
        w.ents.weapon_infos.0[wt::SNIPERRIFLE].lifespan = 1000.0;
        w.ents.weapon_infos.0[wt::SNIPERRIFLE].speed = 16.0;
        w.ents.timer.time_step = 1.0;
        assert!(w.fire_weapon(&mut px, a, Vec3::ZERO));
        let start = w.ents.bullets.list.iter().find(|b| b.in_use).unwrap().pos;
        w.update_weapons(&mut px);
        let b = w.ents.bullets.list.iter().find(|b| b.in_use).unwrap();
        assert!(b.pos.y > start.y && w.ents.bullets.player_sniper_bullet);
        w.ents.timer.time_in_ms = 100_000;
        w.update_weapons(&mut px);
        assert!(w.ents.bullets.list.iter().all(|b| !b.in_use));
    }

    #[test]
    fn a_sniper_hit_hurts_the_ped_and_stops_the_bullet() {
        use crate::weapons::bullet::{Bullet, BulletHit};
        let (mut rng, mut hooks) = (Rng::default(), Fx::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let (mut w, a, b) = two_peds(&mut px);
        let bullet = Bullet { in_use: true, weapon: wt::SNIPERRIFLE, source: Some(a), damage: 40, ..Default::default() };
        let hit = BulletHit { entity: b, ty: crate::entity::EntityType::Ped, point: Vec3::new(0.0, 4.9, 1.0), normal: Vec3::NEG_Y, piece: 0 };
        let health = w.ped(b).health;
        let r = w.sniper_bullet_impact(&mut px, &bullet, &hit, true);
        assert!(r.stop && !r.quiet);
        assert!(w.ped(b).health < health);
        assert_eq!(w.ped(b).queued_sound, crate::audio::sound::PED_BULLET_HIT);
    }

    #[test]
    fn a_flame_burst_sets_a_near_ped_alight() {
        use crate::weapons::info::{flag, WeaponFire};
        let (mut rng, mut hooks) = (Rng::default(), Fx::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let mut w = box_world();
        let a = w.add_ped(&mut px, ped_type::CIVMALE, 10, Vec3::ZERO).unwrap();
        let b = w.add_ped(&mut px, ped_type::CIVFEMALE, 10, Vec3::new(0.5, 0.0, 0.0)).unwrap();
        assert!(w.ped_pointer_valid(a) && w.ped_pointer_valid(b));
        w.ped_mut(a).near_peds[0] = Some(b);
        w.ped_mut(a).num_near_peds = 1;
        let info = &mut w.ents.weapon_infos.0[wt::FLAMETHROWER];
        info.fire = WeaponFire::AreaEffect;
        info.flags = flag::SLOWS_DOWN | flag::EXPANDS;
        info.speed = 0.0;
        info.lifespan = 1000.0;
        info.radius = 2.0;
        w.ents.timer.time_step = 1.0;
        let (infos, timer) = (w.ents.weapon_infos.clone(), w.ents.timer.clone());
        w.ents.shots.add(&infos, px.rng, &timer, Some(a), wt::FLAMETHROWER, Vec3::ZERO, Vec3::ZERO);
        w.update_weapons(&mut px);
        assert!(w.ped(b).fire.is_some());
        assert_eq!(w.ped(b).ped_state, state::ON_FIRE);
    }

    #[test]
    fn aiming_finds_the_ped_in_front_through_the_sectors() {
        let (mut rng, mut hooks) = (Rng::default(), Fx::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let mut w = box_world();
        let a = w.add_ped(&mut px, ped_type::CIVMALE, 10, Vec3::ZERO).unwrap();
        w.add_ped(&mut px, ped_type::CIVFEMALE, 10, Vec3::new(0.0, 10.0, 2.0)).unwrap();
        let mut target = Vec3::new(0.0, 40.0, 0.0);
        w.do_doom_aiming(a, Vec3::ZERO, &mut target);
        // the ped at 10 m, 2 m up: the aim reaches its chest at the target's distance
        assert!((target.z - 40.0 / 10.0 * 2.3).abs() < 1.0e-3, "{target:?}");
        let mut target = Vec3::new(0.0, 40.0, 0.0);
        w.do_drive_by_auto_aiming(a, Vec3::new(0.0, 0.0, 2.0), &mut target);
        // the drive-by aim pulls the shot's end onto the line through the ped, scaled to the old length
        let scale = (4.0f32 + 1600.0).sqrt() / 10.0;
        assert!((target - Vec3::new(0.0, 10.0 * scale, 2.0)).length() < 1.0e-3, "{target:?}");
    }

    #[test]
    fn a_punch_that_misses_changes_nothing() {
        let (mut w, _) = world_with_ped();
        let (mut rng, mut hooks) = (Rng::default(), Events::default());
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let a = w.add_ped(&mut px, ped_type::CIVMALE, -1, Vec3::ZERO).unwrap();
        let b = w.add_ped(&mut px, ped_type::CIVFEMALE, -1, Vec3::new(50.0, 0.0, 0.0)).unwrap();
        w.ents.weapon_infos.0[wt::UNARMED].radius = 1.0;
        let pd = w.ped_mut(a);
        pd.near_peds[0] = Some(b);
        pd.num_near_peds = 1;
        assert!(!w.fire_melee_shot(&mut px, a, wt::UNARMED, Vec3::ZERO));
        assert!(hooks.got.is_empty());
    }
}

mod object_population {
    use super::*;
    use crate::entity::object::CreatedBy;
    use crate::ped::types::ped_type;
    use glam::Vec3;

    fn object_at(w: &mut World, px: &mut Px, pos: Vec3, by: CreatedBy) -> EntityRef {
        let mut m = crate::math::Matrix::IDENTITY;
        m.pos = pos;
        let r = w.new_object(10, m, px.rng).unwrap();
        if let Kind::Object(o) = &mut w.ent_mut_for_test(r).kind {
            o.object_matrix = m;
            o.created_by = by;
        }
        w.add(r).unwrap();
        r
    }

    fn player_at_origin(w: &mut World, px: &mut Px) {
        let p = w.create_ped(px, ped_type::PLAYER1, 10, None).unwrap();
        w.add(p).unwrap();
        w.ents.player_ped = Some(p);
    }

    #[test]
    fn a_far_loose_object_becomes_scenery_and_comes_back_when_near() {
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let mut w = box_world();
        player_at_origin(&mut w, &mut px);
        let far = Vec3::new(500.0, 0.0, 0.0);
        let obj = object_at(&mut w, &mut px, far, CreatedBy::Game);
        w.manage_population(&mut px);
        assert!(w.get(obj).is_none());
        assert_eq!(w.ents.pools.dummies.used(), 1);
        let dummy = EntityRef { pool: crate::entity::PoolKind::Dummy, handle: w.ents.pools.dummies.handle_of(w.ents.pools.dummies.iter().next().unwrap().0) };
        assert_eq!(w.epos(dummy), far);
        // the player walks over: the scenery turns real again
        w.ent_mut_for_test(w.ents.player_ped.unwrap()).base.set_position(Vec3::new(490.0, 0.0, 0.0));
        w.manage_population(&mut px);
        assert_eq!((w.ents.pools.dummies.used(), w.ents.pools.objects.used()), (0, 1));
    }

    #[test]
    fn temp_objects_and_mission_objects_are_handled_by_their_origin() {
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let mut w = box_world();
        player_at_origin(&mut w, &mut px);
        let temp_far = object_at(&mut w, &mut px, Vec3::new(500.0, 0.0, 0.0), CreatedBy::Temp);
        let mission_far = object_at(&mut w, &mut px, Vec3::new(500.0, 10.0, 0.0), CreatedBy::Mission);
        let temp_near = object_at(&mut w, &mut px, Vec3::new(10.0, 0.0, 0.0), CreatedBy::Temp);
        if let Kind::Object(o) = &mut w.ent_mut_for_test(temp_near).kind {
            o.end_of_life_time = 100;
        }
        w.ents.timer.time_in_ms = 5000;
        w.manage_population(&mut px);
        // a far temp object is dropped, a mission object stays as it is, an expired near temp object is dropped
        assert!(w.get(temp_far).is_none() && w.get(mission_far).is_some() && w.get(temp_near).is_none());
        assert_eq!(w.ents.pools.dummies.used(), 0);
    }

    #[test]
    fn scenery_with_no_car_near_is_safe_to_turn_real() {
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let px = Px { rng: &mut rng, hooks: &mut hooks };
        let mut w = box_world();
        let d = w.new_dummy(10, px.rng).unwrap();
        w.ent_mut_for_test(d).base.set_position(Vec3::new(30.0, 0.0, 0.0));
        w.add(d).unwrap();
        assert!(w.test_safe_for_real_object(d));
    }
}
