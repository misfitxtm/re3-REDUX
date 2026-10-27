//! The per-frame life of projectiles, sniper bullets and flame bursts (`CProjectileInfo`, `CBulletInfo` and `CShotInfo::Update`).
use super::ctx::{Px, ShotEffect};
use super::data::*;
use super::fight::local_direction;
use super::fire::WorldProjectiles;
use super::pending::Hit;
use super::types::ped_type;
use crate::anim::tables_gta3::anim;
use crate::anim::{assoc_group, flags as aflags};
use crate::audio::{script_sound, sound};
use crate::entity::{EntityRef, EntityType};
use crate::weapons::bullet::{Bullet, BulletEnv, BulletHit, Impact};
use crate::weapons::info::weapon_type as wt;
use crate::weapons::shot::ShotEnv;
use crate::world::World;
use glam::{Vec2, Vec3};

const BULLET_HIT_FORCE: f32 = 7.5;

/// Sniper bullets fly through the world and hurt what they meet.
struct WorldBullets<'a, 'b> {
    world: &'a mut World,
    px: &'a mut Px<'b>,
}

impl BulletEnv for WorldBullets<'_, '_> {
    fn line_of_sight(&mut self, from: Vec3, to: Vec3, ignore: Option<EntityRef>) -> Option<BulletHit> {
        // shortcut: car tyres (`bIncludeCarTyres`) are not part of the line test yet
        self.world.ents.ignore_entity = ignore;
        let hit = self.world.shot_line(from, to, true);
        self.world.ents.ignore_entity = None;
        hit.map(|h| BulletHit { entity: h.entity, ty: self.world.ent(h.entity).base.ty, point: h.point.point, normal: h.point.normal, piece: h.point.piece_b })
    }

    fn impact(&mut self, bullet: &Bullet, hit: &BulletHit, add_sound: bool) -> Impact {
        let (w, px) = (&mut *self.world, &mut *self.px);
        w.sniper_bullet_impact(px, bullet, hit, add_sound)
    }

    fn source_valid(&self, source: EntityRef) -> bool {
        if self.world.is_ped(source) { self.world.ped_pointer_valid(source) } else { self.world.get(source).is_some() }
    }
}

/// Flame bursts set peds and cars alight.
struct WorldShots<'a, 'b> {
    world: &'a mut World,
    px: &'a mut Px<'b>,
}

impl ShotEnv for WorldShots<'_, '_> {
    fn burn_near_peds(&mut self, source: EntityRef, pos: Vec3, radius: f32) {
        let Some(sp) = self.world.try_ped(source) else { return };
        let near: Vec<EntityRef> = sp.near_peds[..sp.num_near_peds as usize].iter().flatten().copied().collect();
        for n in near {
            if !self.world.ped_pointer_valid(n) {
                continue;
            }
            let Some(np) = self.world.try_ped(n) else { continue };
            let fireproof = self.world.ent(n).base.flags.fire_proof;
            // the original compares the squared distance with the radius itself
            if np.is_ped_in_control() && (self.world.epos(n) - pos).length_squared() < radius && !fireproof {
                if !np.is_player() {
                    self.world.set_find_path_and_flee(self.px, n, source, 10000, false);
                    self.world.ped_mut(n).set_move_state(move_state::SPRINT);
                }
                self.world.start_fire_on(self.px, n, Some(source), 0.8, true);
            }
        }
    }

    fn set_cars_on_fire(&mut self, pos: Vec3, radius: f32, source: Option<EntityRef>) {
        self.world.set_cars_on_fire(pos, radius, source);
    }

    fn source_valid(&self, source: EntityRef) -> bool {
        if self.world.is_ped(source) { self.world.ped_pointer_valid(source) } else { self.world.get(source).is_some() }
    }
}

impl World {
    /// The three weapon managers' `Update`s.
    pub fn update_weapons(&mut self, px: &mut Px) {
        let timer = self.ents.timer.clone();
        let infos = self.ents.weapon_infos.clone();

        let camera = px.hooks.camera_position();
        let mut projectiles = std::mem::take(&mut self.ents.projectiles);
        let mut env = WorldProjectiles { world: self, rng: px.rng, camera, smoke: vec![] };
        projectiles.update(&mut env, &timer);
        let smoke = std::mem::take(&mut env.smoke);
        self.ents.projectiles = projectiles;
        for pos in smoke {
            px.hooks.shot_effect(ShotEffect::RocketSmoke { pos });
        }

        let mut bullets = std::mem::take(&mut self.ents.bullets);
        bullets.update(&timer, &mut WorldBullets { world: self, px });
        self.ents.bullets = bullets;

        let mut shots = std::mem::take(&mut self.ents.shots);
        shots.update(&infos, &timer, &mut WorldShots { world: self, px });
        self.ents.shots = shots;
    }

    /// What one step of a bullet does where it meets something (`CBulletInfo::Update`).
    pub(super) fn sniper_bullet_impact(&mut self, px: &mut Px, bullet: &Bullet, hit: &BulletHit, add_sound: bool) -> Impact {
        let mut out = Impact::default();
        let (v, at) = (hit.entity, hit.point);
        let ent_pos = self.epos(v);
        if bullet.source.is_some() && matches!(hit.ty, EntityType::Ped | EntityType::Vehicle) {
            px.hooks.shot_effect(ShotEffect::InstantHitHitByPlayer);
        }
        match hit.ty {
            EntityType::Ped => {
                if !self.ped(v).dying_or_dead() && Some(v) != bullet.source {
                    if self.does_los_bullet_hit_ped(at.z) != 0 {
                        let dir = local_direction(self.ped(v).rotation_cur, Vec2::new(ent_pos.x - at.x, ent_pos.y - at.y));
                        let pd = self.ped(v);
                        if pd.is_ped_in_control() && !pd.flags.is_ducking {
                            self.clear_attack_by_removing_anim(px, v);
                            if let Some(i) = self.ped_add_anim(v, assoc_group::STD, anim::STD_HITBYGUN_FRONT)
                                && let Some(a) = self.ped_assoc_at(v, i)
                            {
                                a.blend_amount = 0.0;
                                a.blend_delta = 8.0;
                            }
                        }
                        self.inflict_damage(px, v, Hit { by: bullet.source, weapon: bullet.weapon, damage: bullet.damage as f32, piece: hit.piece, direction: dir as u8 });
                        if let Some(by) = bullet.source {
                            let event = if self.ped(v).ped_type == ped_type::COP { event_type::SHOOT_COP } else { event_type::SHOOT_PED };
                            px.hooks.register_event_with_culprit(event, v, by, 1000);
                        }
                        out.stop = true;
                    } else {
                        out.quiet = true;
                    }
                }
                if px.hooks.nasty_game() {
                    if px.hooks.is_on_screen(v) {
                        let dir = Vec3::new((at.x - ent_pos.x) * 0.01, (at.y - ent_pos.y) * 0.01, 0.01);
                        px.hooks.shot_effect(ShotEffect::BloodSmall { pos: at, dir, count: 8 });
                    }
                    if self.ped(v).ped_state == state::DEAD {
                        let anim_id = if self.ped_first_assoc(v, aflags::FRONTAL).is_some() { anim::STD_HIT_FLOOR_FRONT } else { anim::STD_HIT_FLOOR };
                        if let Some(i) = self.ped_blend_anim(v, assoc_group::STD, anim_id, 8.0)
                            && let Some(a) = self.ped_assoc_at(v, i)
                        {
                            a.set_current_time(0.0);
                            a.flags |= aflags::RUNNING;
                            a.flags &= !aflags::FADE_OUT_WHEN_DONE;
                        }
                    }
                    out.stop = true;
                }
            }
            EntityType::Vehicle => {
                if let Some(mut e) = self.ents.pools.take(v) {
                    self.inflict_vehicle_damage(px.rng, &mut e, bullet.source, bullet.weapon as i8, bullet.damage as f32);
                    self.ents.pools.restore(v, e);
                }
                if bullet.weapon == wt::FLAMETHROWER {
                    self.start_fire_on(px, v, bullet.source, 0.8, true);
                } else {
                    px.hooks.shot_effect(ShotEffect::Sparks { pos: at, dir: hit.normal / 20.0, count: 16 });
                }
                out.stop = true;
            }
            _ => {
                px.hooks.shot_effect(ShotEffect::Sparks { pos: at, dir: hit.normal / 20.0, count: 8 });
                if hit.ty == EntityType::Object {
                    self.shot_pushes_object(v, hit.normal, BULLET_HIT_FORCE);
                }
                out.stop = true;
            }
        }
        if bullet.weapon == wt::SNIPERRIFLE && add_sound {
            match hit.ty {
                EntityType::Building => px.hooks.shot_effect(ShotEffect::ScriptSound { sound: script_sound::BULLET_HIT_GROUND_1, pos: ent_pos }),
                EntityType::Object => px.hooks.shot_effect(ShotEffect::ScriptSound { sound: script_sound::BULLET_HIT_GROUND_2, pos: ent_pos }),
                EntityType::Dummy => px.hooks.shot_effect(ShotEffect::ScriptSound { sound: script_sound::BULLET_HIT_GROUND_3, pos: ent_pos }),
                EntityType::Ped => {
                    px.hooks.shot_effect(ShotEffect::Sound { entity: v, sound: sound::WEAPON_HIT_PED });
                    self.say(px, v, sound::PED_BULLET_HIT);
                }
                EntityType::Vehicle => px.hooks.shot_effect(ShotEffect::Sound { entity: v, sound: sound::WEAPON_HIT_VEHICLE }),
                EntityType::Nothing => {}
            }
        }
        px.hooks.shot_effect(ShotEffect::GlassHit { victim: v, pos: at });
        self.blow_up_explosive_thing(px.rng, v);
        out
    }
}
