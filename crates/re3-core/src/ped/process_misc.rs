//! `Ped.cpp` and friends: the parts of `CPed::ProcessControl` that stand on their own: buoyancy,
//! talking, bloody tyres and footsteps.
use super::ctx::{PedParticle, Px};
use super::data::*;
use super::pending::Hit;
use crate::anim::flags as aflags;
use crate::audio::sound;
use crate::entity::physical::GRAVITY;
use crate::entity::EntityRef;
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, World, VEHICLES};
use glam::{Vec2, Vec3};

/// `CommentWaitTime`: fixed delay, random extra after it, random extra before the same comment repeats, fixed delay before that, per ped sound from `PED_DEATH` on.
pub(super) const COMMENT_WAIT_TIME: [(u32, i32, i32, u32); 39] = [
    (500, 800, 500, 2), (500, 800, 500, 2), (500, 800, 500, 2), (500, 800, 500, 2), (100, 2, 100, 2),
    (700, 500, 1000, 500), (700, 500, 1000, 500), (5000, 2000, 15000, 3000), (5000, 2000, 15000, 3000), (5000, 2000, 15000, 3000),
    (6000, 6000, 6000, 6000), (1000, 1000, 2000, 2000), (1000, 500, 2000, 1500), (1000, 500, 2000, 1500), (800, 200, 1000, 500),
    (800, 200, 1000, 500), (800, 400, 2000, 1000), (800, 400, 2000, 1000), (400, 300, 2000, 1000), (2000, 1000, 2500, 1500),
    (200, 200, 200, 200), (6000, 3000, 5000, 6000), (6000, 3000, 9000, 5000), (6000, 3000, 9000, 5000), (6000, 3000, 9000, 5000),
    (400, 300, 4000, 1000), (400, 300, 4000, 1000), (400, 300, 4000, 1000), (1000, 500, 3000, 1000), (1000, 500, 1000, 1000),
    (3000, 2000, 3000, 2000), (1000, 500, 3000, 6000), (1000, 500, 2000, 4000), (1000, 500, 2000, 5000), (1000, 500, 3000, 2000),
    (1600, 1000, 2000, 2000), (3000, 2000, 5000, 3000), (1000, 1000, 1000, 1000), (1000, 1000, 5000, 5000),
];

impl World {
    /// `CPed::ProcessBuoyancy`.
    pub fn process_buoyancy(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).in_vehicle {
            return;
        }
        let step = self.time_step();
        let now = self.now();
        let level = if self.ped(r).ped_state == state::DEAD { 1.5 } else { 1.3 };
        let (matrix, touching, mass) = {
            let e = self.ent(r);
            (e.base.matrix, e.base.flags.touching_water, e.phys.as_ref().map_or(1.0, |p| p.mass))
        };
        let in_water = self.col_model_of(r).and_then(|col| self.ents.buoyancy.process(self.ents.water.as_ref(), &matrix, &col, None, touching, GRAVITY * mass * level, step));
        if let Some((_, impulse)) = in_water {
            self.get_mut(r).unwrap().base.flags.touching_water = true;
            let pos = self.epos(r);
            let boat_below = self
                .process_vertical_line(pos, pos.z - 3.0, Kinds { vehicles: true, ..Kinds::NONE }, false, None)
                .is_some_and(|h| self.try_veh(h.entity).is_some_and(|v| v.is_boat()));
            if boat_below {
                self.phys_mut(r).flags.is_in_water = false;
                return;
            }
            self.phys_mut(r).flags.is_in_water = true;
            self.phys_mut(r).apply_move_force(impulse);
            if self.ped(r).dying_or_dead() {
                return;
            }
            if self.ped(r).flags.trying_to_reach_dry_land && impulse.z / mass > GRAVITY * 0.4 * step {
                self.ped_mut(r).flags.trying_to_reach_dry_land = false;
                let mut pos = self.epos(r);
                if self.place_ped_on_dry_land(r) {
                    if self.ped(r).health > 20.0 {
                        self.inflict_damage(px, r, Hit { by: None, weapon: wt::DROWNING, damage: 15.0, piece: piece::TORSO, direction: 0 });
                    }
                    if self.ped(r).flags.is_in_the_air {
                        self.ped_set_blend_deltas(r, aflags::PARTIAL, -1000.0);
                        self.ped_mut(r).flags.is_in_the_air = false;
                    }
                    pos.z -= 0.8;
                    px.hooks.ped_particle(PedParticle::WaterSplashObject(50), pos, Vec3::ZERO, 0.0);
                    self.phys_mut(r).move_speed = Vec3::ZERO;
                    self.ped_mut(r).ped_state = state::IDLE;
                    return;
                }
            }
            let mut speed_mult = 0.0_f32;
            if impulse.z / mass > GRAVITY * 0.75 * step || self.ents.buoyancy.water_level() > self.epos(r).z {
                speed_mult = 0.9_f32.powf(step);
                self.phys_mut(r).move_speed *= speed_mult;
                self.ped_mut(r).flags.is_standing = false;
                self.inflict_damage(px, r, Hit { by: None, weapon: wt::DROWNING, damage: 3.0 * step, piece: piece::TORSO, direction: 0 });
            }
            if impulse.z / mass > GRAVITY * 0.25 * step {
                if speed_mult == 0.0 {
                    speed_mult = 0.9_f32.powf(step);
                }
                let p = self.phys_mut(r);
                p.move_speed.x *= speed_mult;
                p.move_speed.y *= speed_mult;
                if p.move_speed.z >= -0.1 {
                    if p.move_speed.z < -0.04 {
                        p.move_speed.z = -0.02;
                    }
                } else {
                    p.move_speed.z = -0.01;
                    let speed = p.move_speed;
                    px.hooks.play_one_shot(r, sound::SPLASH, 0.0);
                    let mut ahead = 2.2 * speed + self.epos(r);
                    if let Some(h) = self.ents.water.height(ahead.x, ahead.y, ahead.z, false) {
                        ahead.z = h;
                    }
                    px.hooks.ped_particle(PedParticle::WaterSplashObject(200), ahead, Vec3::new(0.0, 0.0, 0.1), 0.0);
                    self.ents.ped_splash_timers = [now + 80, now + 100];
                }
            }
        } else {
            self.get_mut(r).unwrap().base.flags.touching_water = false;
        }
        self.ped_water_circles_and_drops(px, r, now);
    }

    /// The tail of `CPed::ProcessBuoyancy`: the delayed circles and raindrops after a splash.
    fn ped_water_circles_and_drops(&mut self, px: &mut Px, r: EntityRef, now: u32) {
        let [drops, circles] = self.ents.ped_splash_timers;
        let surface = |w: &World| {
            let mut pos = w.epos(r);
            if let Some(h) = w.ents.water.height(pos.x, pos.y, pos.z, false) {
                pos.z = h;
            }
            pos
        };
        if circles != 0 && now >= circles {
            let mut pos = surface(self);
            if pos.z != 0.0 {
                self.ents.ped_splash_timers[1] = 0;
                for _ in 0..4 {
                    pos.x += px.rng.range_f32(-0.75, 0.75);
                    pos.y += px.rng.range_f32(-0.75, 0.75);
                    px.hooks.ped_particle(PedParticle::RainSplashBigGrow, pos, Vec3::ZERO, 0.0);
                }
            }
        }
        if drops != 0 && now >= drops {
            let mut pos = surface(self);
            if pos.z >= 0.0 {
                pos.z += 0.25;
                self.ents.ped_splash_timers[0] = 0;
                px.hooks.ped_particle(PedParticle::SplashesAround(1500), pos, Vec3::ZERO, 4.5);
            }
        }
    }

    /// `CPed::ServiceTalkingWhenDead`.
    pub fn service_talking_when_dead(&self, r: EntityRef) -> bool {
        self.ped(r).queued_sound == sound::PED_DEATH
    }

    /// `CPed::ServiceTalking`.
    pub fn service_talking(&mut self, px: &mut Px, r: EntityRef) {
        let now = self.now();
        if self.ped(r).flags.body_part_just_came_off && self.ped(r).body_part_bleeding == node::HEAD as i8 {
            return;
        }
        let model = self.ent(r).base.model_index;
        let bomber = self.ents.models.get(model as i32).is_some_and(|m| m.name.eq_ignore_ascii_case("bomber"));
        let pd = self.ped_mut(r);
        if bomber {
            pd.queued_sound = sound::PED_BOMBER;
        } else if pd.ped_state == state::ON_FIRE {
            pd.queued_sound = sound::PED_BURNING;
        }
        if pd.queued_sound == NO_SOUND {
            return;
        }
        if pd.queued_sound == sound::PED_DEATH {
            pd.sound_start = now.wrapping_sub(1);
        }
        if now > pd.sound_start {
            let queued = pd.queued_sound;
            let (fixed, random, ..) = COMMENT_WAIT_TIME[(queued - sound::PED_DEATH) as usize];
            let extra = px.rng.range_i32(0, random) as u32;
            px.hooks.play_one_shot(r, queued, 1.0);
            let pd = self.ped_mut(r);
            pd.last_sound_start = now;
            pd.sound_start = fixed + now + extra;
            pd.last_queued_sound = queued;
            pd.queued_sound = NO_SOUND;
        }
    }

    /// `CPed::DeadPedMakesTyresBloody`.
    pub fn dead_ped_makes_tyres_bloody(&mut self, px: &mut Px, r: EntityRef) {
        let pos = self.epos(r);
        let g = self.grid;
        let last = g.num - 1;
        let (x0, y0) = (g.index_x(pos.x - 2.0).max(0), g.index_y(pos.y - 2.0).max(0));
        let (x1, y1) = (g.index_x(pos.x + 2.0).min(last), g.index_y(pos.y + 2.0).min(last));
        let mut cars: Vec<EntityRef> = vec![];
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(sector) = self.sector(x, y) else { continue };
                for &v in sector.lists[VEHICLES].iter().chain(sector.lists[VEHICLES + 1].iter()) {
                    if !cars.contains(&v) {
                        cars.push(v);
                    }
                }
            }
        }
        let nasty = px.hooks.nasty_game();
        for veh in cars {
            let Some(vd) = self.try_veh(veh) else { continue };
            if !vd.is_car() {
                continue;
            }
            let vpos = self.epos(veh);
            let fast = self.ent(veh).phys.as_ref().is_some_and(|p| p.move_speed.truncate().length_squared() > 0.05);
            if (pos.x - vpos.x).abs() >= 10.0 || (pos.y - vpos.y).abs() >= 10.0 || !fast {
                continue;
            }
            let Some(col) = self.col_model_of(veh) else { continue };
            let (min, max) = (col.bounding_box.min, col.bounding_box.max);
            for wheel in 0..4 {
                let (Some(car), Some(vd)) = (self.car_ref(veh), self.try_veh(veh)) else { break };
                if car.wheel_skidmark_bloody[wheel] || vd.suspension_spring_ratio[wheel] >= 1.0 {
                    continue;
                }
                let offset = match wheel {
                    0 => Vec3::new(-max.x, max.y, 0.0),
                    1 => Vec3::new(-max.x, min.y, 0.0),
                    2 => Vec3::new(max.x, max.y, 0.0),
                    _ => Vec3::new(max.x, min.y, 0.0),
                };
                let wheel_pos = self.ent(veh).base.matrix.transform_point(offset);
                if (wheel_pos.z - pos.z).abs() >= 2.0 || (wheel_pos - pos).truncate().length_squared() >= 1.0 {
                    continue;
                }
                if nasty {
                    if let Some(car) = self.car_mut(veh) {
                        car.wheel_skidmark_bloody[wheel] = true;
                    }
                    px.hooks.play_one_shot(veh, sound::SPLATTER, 0.0);
                }
                let lift = Vec3::new(0.0, 0.0, 50.0);
                let matrix = self.ent(veh).base.matrix;
                let com = vpos;
                if let Some(p) = self.get_mut(veh).and_then(|e| e.phys.as_mut()) {
                    p.apply_move_force(lift);
                    p.apply_turn_force(&matrix, lift, wheel_pos - com);
                }
                if self.ents.player_ped.and_then(|p| self.try_ped(p)).and_then(|d| d.my_vehicle.filter(|_| d.in_vehicle)) == Some(veh) {
                    px.hooks.start_pad_shake(300, 70);
                }
            }
        }
    }

    /// `CPed::PlayFootSteps`.
    pub fn play_foot_steps(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped_mut(r);
        if pd.flags.do_bloody_footprints && pd.bloody_footprint_count_or_death_time > 0 && pd.bloody_footprint_count_or_death_time < 300 {
            pd.bloody_footprint_count_or_death_time -= 1;
            if pd.bloody_footprint_count_or_death_time == 0 {
                pd.flags.do_bloody_footprints = false;
            }
        }
        if !pd.flags.is_standing {
            return;
        }
        let Some(clump) = self.ped_clump(r) else { return };
        let (mut walk_run, mut walk_blend, mut idle_blend) = (None, 0.0_f32, 0.0_f32);
        for a in &clump.anim.associations {
            if a.flags & aflags::WALK != 0 {
                walk_run = Some((a.current_time, a.time_step, a.hierarchy.total_length));
                walk_blend += a.blend_amount;
            } else if a.flags & aflags::NOWALK == 0 {
                idle_blend += a.blend_amount;
            }
        }
        if let Some((time, time_step, length)) = walk_run.filter(|_| walk_blend > 0.5 && idle_blend < 1.0) {
            let step_start = 1.0 / 15.0;
            let step_end = length / 2.0 + step_start;
            let part = if time >= step_start && time - time_step < step_start {
                1
            } else if time >= step_end && time - time_step < step_end {
                2
            } else {
                0
            };
            if part != 0 {
                px.hooks.play_one_shot(r, if part == 1 { sound::STEP_START } else { sound::STEP_END }, 1.0);
                let foot = if part == 1 { node::FOOTL } else { node::FOOTR };
                let m = self.ent(r).base.matrix;
                let local = self.ped_clump(r).map_or(Vec3::ZERO, |c| c.skeleton.component_position(c.nodes[foot]));
                let mut foot_pos = m.transform_point(local);
                foot_pos.z -= 0.1;
                foot_pos += 0.2 * m.forward;
                if self.ped(r).flags.do_bloody_footprints {
                    px.hooks.add_blood_footprint(foot_pos, m.forward * 0.26, m.right * 0.14);
                    let pd = self.ped_mut(r);
                    if pd.bloody_footprint_count_or_death_time <= 20 {
                        pd.bloody_footprint_count_or_death_time = 0;
                        pd.flags.do_bloody_footprints = false;
                    } else {
                        pd.bloody_footprint_count_or_death_time -= 20;
                    }
                }
                if px.hooks.no_rain() {
                    if self.ped(r).is_player() {
                        px.hooks.ped_particle(PedParticle::FootDust, foot_pos, Vec3::ZERO, 0.0);
                    }
                } else if part == 2 {
                    px.hooks.ped_particle(PedParticle::FootSplash, foot_pos, Vec3::ZERO, 0.15);
                }
            }
        }
        let touched = self.ent(r).phys.as_ref().map_or(0, |p| p.surface_touched);
        if touched == crate::surface::WATER {
            let (speed, m) = (self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed), self.ent(r).base.matrix);
            let ped_speed = Vec2::new(speed.x, speed.y).length();
            if ped_speed > 0.13 && self.ents.timer.frame_counter.is_multiple_of(2) {
                let size = (ped_speed * 2.0).clamp(0.25, 0.75);
                let mut pos = self.epos(r) + m.forward * 0.3;
                pos.z -= 1.2;
                let mut dir = speed * -0.75;
                dir.z = px.rng.range_f32(0.01, 0.03);
                px.hooks.ped_particle(PedParticle::PedSplash, pos, dir, 0.8 * size);
                dir.z = px.rng.range_f32(0.03, 0.05);
                px.hooks.ped_particle(PedParticle::RubberSmoke, pos, dir, size);
            }
        }
    }

    /// The ped's physical state (it always has one).
    pub(super) fn phys_mut(&mut self, r: EntityRef) -> &mut crate::entity::physical::Physical {
        self.get_mut(r).and_then(|e| e.phys.as_mut()).expect("ped physical")
    }
}
