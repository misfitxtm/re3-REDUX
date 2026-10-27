//! Helicopters in the world: `CHeli::ProcessControl` and the manager (`UpdateHelis`, ...).
// The game's angle code uses 3.14 and 6.28 on purpose.
#![allow(clippy::approx_constant)]
use super::{Kinds, LineOpts, World};
use crate::entity::{status, Ent, Entity, EntityRef, EntityType, Kind};
use crate::general::Rng;
use crate::math::{atan_of_xy, cross, magnitude, normalise};
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::base::{VehicleData, VehicleSub};
use crate::vehicle::env::{CarEnv, CarEvent, CarSound, Particle, ParticleKind};
use crate::vehicle::heli::*;
use glam::{Vec2, Vec3};

impl World {
    fn heli_data(&self, r: EntityRef) -> Option<&HeliData> {
        match &self.get(r)?.kind {
            Kind::Vehicle(v) => match &v.sub {
                VehicleSub::Heli(h) => Some(h),
                _ => None,
            },
            _ => None,
        }
    }

    fn heli_data_mut(&mut self, r: EntityRef) -> Option<&mut HeliData> {
        match &mut self.get_mut(r)?.kind {
            Kind::Vehicle(v) => match &mut v.sub {
                VehicleSub::Heli(h) => Some(h),
                _ => None,
            },
            _ => None,
        }
    }

    /// `CHeli::CHeli` for `MI_CHOPPER` or `MI_ESCAPE`.
    pub fn new_heli(&mut self, model_index: i16, created_by: u8, rng: &mut Rng) -> Option<EntityRef> {
        let time_ms = self.ents.timer.time_in_ms;
        let info = self.ents.models.get(model_index as i32)?;
        let crate::modelinfo::Kind::Vehicle(mi_data) = &info.kind else { return None };
        let nh = new_heli(time_ms, created_by, mi_data.handling_id as usize, mi_data, rng);
        let mut base = Entity::new(EntityType::Vehicle, rng);
        base.model_index = model_index;
        base.status = status::HELI;
        self.insert(Ent::new(base, Some(nh.physical), Kind::Vehicle(Box::new(nh.vehicle))))
    }

    /// `CHeli::ProcessControl`.
    pub fn process_control_heli(&mut self, r: EntityRef, env: &mut CarEnv, rng: &mut Rng) {
        let Some(h0) = self.heli_data(r) else { return };
        let mut h = h0.clone();
        let timer = self.ents.timer.clone();
        let step = timer.time_step;
        let Some(e) = self.get(r) else { return };
        let (mut m, mut mv, random_seed) = (e.base.matrix, e.phys.as_ref().unwrap().move_speed, e.base.random_seed as u32);
        let player = env.player.clone();

        let mut target = Vec3::ZERO;
        let mut v_target_dist: Vec2;
        if h.heli_type == HeliType::Catalina && h.status != HeliStatus::ShotDown {
            let at = |t: [f32; 3]| Vec3::from(t);
            match h.path_state {
                0..=5 => {
                    target = at(CATALINA_TARGET[h.path_state as usize]);
                    if magnitude(target - m.pos) < 9.0 {
                        h.path_state += 1;
                    }
                }
                6 => {
                    target = at(CATALINA_TARGET[6]);
                    if m.pos.z <= 31.55 {
                        h.path_state = 7;
                        m.pos.z = 31.55;
                        mv = Vec3::ZERO;
                    }
                }
                7 => {
                    m.pos.z = 31.55;
                    target = m.pos;
                }
                8 => {
                    target = Vec3::new(m.pos.x, m.pos.y, 74.0);
                    if m.pos.z >= 40.0 {
                        self.ents.helis.path_point = 2;
                        h.path_state = 9;
                    }
                }
                9 => {
                    let pp = self.ents.helis.path_point as usize;
                    target = at(DAM_PATH[pp]);
                    if magnitude(target - m.pos) < 9.0 {
                        self.ents.helis.path_point += 1;
                        if self.ents.helis.path_point >= 6 {
                            h.path_state = 10;
                            self.ents.helis.path_point = 0;
                        }
                    }
                }
                10 => {
                    let pp = self.ents.helis.path_point as usize;
                    target = at(SHORT_PATH[pp]);
                    if magnitude(target - m.pos) < 9.0 {
                        self.ents.helis.path_point += 1;
                        if self.ents.helis.path_point >= 3 {
                            h.path_state = 9;
                            self.ents.helis.path_point = 1;
                        }
                    }
                }
                11 => {
                    let pp = self.ents.helis.path_point as usize;
                    target = at(LONG_PATH[pp]);
                    if magnitude(target - m.pos) < 9.0 {
                        self.ents.helis.path_point += 1;
                        if self.ents.helis.path_point >= 7 {
                            h.path_state = 9;
                            self.ents.helis.path_point = 0;
                        }
                    }
                }
                12 => target = Vec3::new(m.pos.x, m.pos.y, 200.0),
                _ => {}
            }
            let d = target - m.pos;
            v_target_dist = Vec2::new(d.x, d.y);
            h.target_z = target.z;
            if h.path_state == 6 {
                m.pos.x = m.pos.x * 0.99 + target.x * 0.01;
                m.pos.y = m.pos.y * 0.99 + target.y * 0.01;
            }
        } else {
            let d = player.coors - m.pos;
            v_target_dist = Vec2::new(d.x, d.y);
            h.target_z = player.coors.z;
            // flying away heads for (0, 0)
            if h.status == HeliStatus::FlyAway && m.pos.z > 20.0 {
                v_target_dist = Vec2::new(-m.pos.x, -m.pos.y);
            }
            let (ground_z, _) = self.find_ground_z_for_3d_coord(Vec3::new(m.pos.x, m.pos.y, 1000.0));
            match h.status {
                HeliStatus::Hover => h.target_z = ground_z.max(h.target_z) + 8.0,
                HeliStatus::ShotDown | HeliStatus::Hover2 => h.target_z = ground_z.max(h.target_z) + 8.0 + h.target_offset,
                _ => h.target_z = ground_z.max(h.target_z) + 12.0,
            }
            // move up if too low
            if m.pos.z - 2.0 < ground_z && h.status != HeliStatus::ShotDown {
                mv.z += step * 0.01;
            }
            mv.z = mv.z.clamp(-0.3, 0.3);
        }

        let target_dist = v_target_dist.length();
        match h.status {
            HeliStatus::Hover | HeliStatus::Hover2 => {
                let target_height = if h.heli_type == HeliType::Catalina { 8.0 } else { 40.0 - h.heli_id as f32 * 10.0 };
                if target_dist > target_height {
                    h.status = HeliStatus::ChasePlayer;
                }
                // falls through to the chase check, as the original does
                let target_height = if h.heli_type == HeliType::Catalina { 4.0 } else { 30.0 - h.heli_id as f32 * 7.5 };
                if target_dist < 1.0 || (target_dist < target_height && self.is_line_of_sight_clear(m.pos, player.coors, Kinds { buildings: true, ..Kinds::NONE }, LineOpts::default())) {
                    h.status = HeliStatus::Hover;
                }
            }
            HeliStatus::ChasePlayer => {
                let target_height = if h.heli_type == HeliType::Catalina { 4.0 } else { 30.0 - h.heli_id as f32 * 7.5 };
                if target_dist < 1.0 || (target_dist < target_height && self.is_line_of_sight_clear(m.pos, player.coors, Kinds { buildings: true, ..Kinds::NONE }, LineOpts::default())) {
                    h.status = HeliStatus::Hover;
                }
            }
            _ => {}
        }

        // xy speed
        let mut speed = if target_dist > 100.0 {
            1.0
        } else if target_dist > 75.0 {
            0.7
        } else {
            0.4
        };
        if matches!(h.status, HeliStatus::Hover | HeliStatus::Hover2 | HeliStatus::ShotDown) {
            speed = 0.0;
        }
        if target_dist != 0.0 {
            v_target_dist /= target_dist;
        } else {
            v_target_dist.x = 1.0;
        }
        let mut target_speed = v_target_dist * speed;

        if matches!(h.status, HeliStatus::Hover2 | HeliStatus::ShotDown) {
            let force = (timer.frame_counter.wrapping_add(random_seed)) & 8 != 0;
            let near = Kinds { buildings: true, ..Kinds::NONE };
            let side = if h.test_right { 1.0 } else { -1.0 };
            let free = force || self.test_sphere_against_world(m.pos + side * 4.0 * m.right, 2.0, Some(r), near, false).is_none();
            if free {
                let k = if h.status == HeliStatus::ShotDown { 0.15 } else { 0.05 };
                if h.status == HeliStatus::ShotDown {
                    h.target_offset -= step * 0.05;
                }
                if h.test_right {
                    target_speed.x -= -v_target_dist.x * k;
                    target_speed.y -= v_target_dist.y * k;
                } else {
                    target_speed.x += -v_target_dist.x * k;
                    target_speed.y += v_target_dist.y * k;
                }
            } else {
                h.test_right = !h.test_right;
                if h.status == HeliStatus::Hover2 {
                    h.target_offset += 5.0;
                } else {
                    h.target_offset -= 5.0;
                }
            }
            if h.target_offset > 30.0 {
                h.target_offset = 30.0;
            }
            if h.status == HeliStatus::ShotDown
                && force
                && (self.test_sphere_against_world(m.pos + 1.5 * m.forward, 2.0, Some(r), near, false).is_some() || self.test_sphere_against_world(m.pos - 1.5 * m.forward, 2.0, Some(r), near, false).is_some())
            {
                h.explosion_timer = timer.previous_time_in_ms;
            }
        } else if h.target_offset >= 2.0 {
            h.target_offset -= 2.0;
        }

        if h.heli_type == HeliType::Catalina && matches!(h.path_state, 9..=11) {
            let f = 0.997f32.powf(step);
            mv.x *= f;
            mv.y *= f;
        }

        let mut speed_dir = target_speed - Vec2::new(mv.x, mv.y);
        let speed_diff = speed_dir.length();
        if speed_diff != 0.0 {
            speed_dir /= speed_diff;
        } else {
            speed_dir.x = 1.0;
        }
        let speed_inc = step * 0.002;
        if speed_diff < speed_inc {
            mv.x = target_speed.x;
            mv.y = target_speed.y;
        } else {
            mv.x += speed_dir.x * speed_inc;
            mv.y += speed_dir.y * speed_inc;
        }
        m.pos.x += mv.x * step;
        m.pos.y += mv.y * step;

        // z target and speed
        if h.status == HeliStatus::FlyAway {
            h.target_z = 1000.0;
        }
        if (timer.time_in_ms.wrapping_add(800u32.wrapping_mul(h.heli_id as u32))) & 0x800 != 0 {
            h.target_z += 2.0;
        }
        h.target_z += h.heli_id as f32 * 5.0;
        let target_speed_z = (h.target_z - m.pos.z) * 0.01;
        let speed_diff_z = target_speed_z - mv.z;
        let mut speed_inc_z = step * 0.001;
        if h.status == HeliStatus::FlyAway {
            speed_inc_z *= 1.5;
        }
        if speed_diff_z.abs() < speed_inc_z {
            mv.z = target_speed_z;
        } else if speed_diff_z < 0.0 {
            mv.z -= speed_inc_z;
        } else {
            mv.z += speed_inc_z * 1.5;
        }
        m.pos.z += mv.z * step;

        // angular speed
        h.angular_speed *= 0.995f32.powf(step);
        let target_angular_speed = if target_dist < 8.0 {
            0.0
        } else {
            let mut diff = atan_of_xy(v_target_dist.x, v_target_dist.y) - h.rotation;
            while diff < -3.14 {
                diff += 6.28;
            }
            while diff > 3.14 {
                diff -= 6.28;
            }
            if diff.abs() > 0.4 {
                if diff < 0.0 { -0.2 } else { 0.2 }
            } else {
                0.0
            }
        };
        let ang_diff = target_angular_speed - h.angular_speed;
        let ang_inc = step * 0.0001;
        if ang_diff.abs() < ang_inc {
            h.angular_speed = target_angular_speed;
        } else if ang_diff < 0.0 {
            h.angular_speed -= ang_inc;
        } else {
            h.angular_speed += ang_inc;
        }
        h.rotation += h.angular_speed * step;

        // the orientation
        let up = normalise(Vec3::new(3.0 * mv.x, 3.0 * mv.y, 1.0));
        let fwd = Vec3::new(-h.rotation.cos(), -h.rotation.sin(), 0.0);
        let right = cross(up, fwd);
        let fwd = cross(up, right);
        m.right = right;
        m.forward = fwd;
        m.up = up;

        // search light and shooting
        let mut events = std::mem::take(&mut env.events);
        if h.status == HeliStatus::FlyAway || h.heli_type == HeliType::Catalina || player.no_rain_zone {
            h.search_light_intensity = 0.0;
        } else {
            let mut time_diff = timer.time_in_ms.wrapping_sub(h.search_light_timer) as i32;
            while time_diff > 1000 {
                for i in (1..6).rev() {
                    h.search_light_history_x[i] = h.search_light_history_x[i - 1];
                    h.search_light_history_y[i] = h.search_light_history_y[i - 1];
                }
                h.search_light_history_x[0] = player.coors.x + player.speed.x * 50.0 * (h.heli_id as f32 + 2.0);
                h.search_light_history_y[0] = player.coors.y + player.speed.y * 50.0 * (h.heli_id as f32 + 2.0);
                time_diff -= 1000;
                h.search_light_timer = h.search_light_timer.wrapping_add(1000);
            }
            let f1 = time_diff as f32 / 1000.0;
            let f2 = 1.0 - f1;
            let i = h.heli_id as usize + 2;
            h.search_light_x = h.search_light_history_x[i] * f2 + h.search_light_history_x[i - 1] * f1;
            h.search_light_y = h.search_light_history_y[i] * f2 + h.search_light_history_y[i - 1] * f1;

            let dist = (Vec2::new(h.search_light_x, h.search_light_y) - Vec2::new(m.pos.x, m.pos.y)).length();
            h.search_light_intensity = if dist > 60.0 {
                0.0
            } else if dist < 40.0 {
                1.0
            } else {
                1.0 - (40.0 - dist) / (60.0 - 40.0)
            };

            let dx = player.coors.x - h.search_light_x;
            let dy = player.coors.y - h.search_light_y;
            if h.search_light_intensity < 0.9 || dx * dx + dy * dy > 7.0 * 7.0 {
                h.shoot_timer = timer.time_in_ms;
            } else if timer.time_in_ms > h.police_shout_timer {
                events.push(CarEvent::PoliceHeliPlayerFound);
                h.police_shout_timer = timer.time_in_ms + 4500 + (rng.number() as u32 & 0xFFF);
            }
            if !player.replay_playing {
                let mut shoot_timeout = 1500i32;
                if h.heli_type == HeliType::Random {
                    shoot_timeout = match player.wanted_level {
                        0..=2 => 999_999,
                        3 => 10_000,
                        4 => 5_000,
                        5 => 3_500,
                        _ => 2_000,
                    };
                    if player.no_police_zone {
                        shoot_timeout /= 2;
                    }
                }
                if player.wanted_ignored {
                    h.shoot_timer = timer.time_in_ms;
                } else {
                    let due = h.shoot_timer.wrapping_add(shoot_timeout as u32);
                    if timer.time_in_ms > due && timer.previous_time_in_ms <= due {
                        if self.is_line_of_sight_clear(m.pos, player.coors, Kinds { buildings: true, ..Kinds::NONE }, LineOpts::default()) {
                            if h.status == HeliStatus::Hover2 {
                                h.status = HeliStatus::Hover;
                            }
                        } else {
                            h.shoot_timer = timer.time_in_ms;
                            if h.status == HeliStatus::Hover {
                                h.status = HeliStatus::Hover2;
                            }
                        }
                    }
                    let due = h.shoot_timer.wrapping_add(shoot_timeout as u32);
                    if timer.time_in_ms > due && timer.time_in_ms > h.last_shot_time.wrapping_add(200) {
                        let mut shot_target = player.coors;
                        shot_target.x += ((rng.number() & 0xFF) as i32 - 128) as f32 / 50.0;
                        shot_target.y += ((rng.number() & 0xFF) as i32 - 128) as f32 / 50.0;
                        let direction = normalise(player.coors - m.pos);
                        shot_target += 3.0 * direction;
                        let source = m.pos + 3.0 * direction;
                        events.push(CarEvent::FireInstantHit { source, target: shot_target, damage: 20 });
                        events.push(CarEvent::WeaponShotFired);
                        h.last_shot_time = timer.time_in_ms;
                    }
                }
            }
        }

        // Catalina's bombs
        if h.heli_type == HeliType::Catalina && h.path_state > 8 && (timer.time_in_ms >> 9) != (timer.previous_time_in_ms >> 9) {
            let mut bomb_pos = m.pos - 60.0 * mv;
            let dx = player.coors.x - bomb_pos.x;
            let dy = player.coors.y - bomb_pos.y;
            if dx * dx + dy * dy < 35.0 * 35.0 {
                let (ground_z, _) = self.find_ground_z_for_3d_coord(bomb_pos);
                let water_z = self.ents.water.height(bomb_pos.x, bomb_pos.y, bomb_pos.z, true).unwrap_or(0.0);
                if ground_z > water_z {
                    bomb_pos.z = ground_z + 2.0;
                    events.push(CarEvent::HeliExplosion { pos: bomb_pos, bomb: true, culprit: Some(r) });
                } else {
                    bomb_pos.z = water_z;
                    for _ in 0..16 {
                        let dir = Vec3::new(((rng.number() & 0xFF) as i32 - 127) as f32 * 0.001, ((rng.number() & 0xFF) as i32 - 127) as f32 * 0.001, 0.5);
                        events.push(CarEvent::Particle(Particle::new(ParticleKind::BoatSplash, bomb_pos, dir, 0.2)));
                    }
                }
            }
        }
        env.events = events;

        // write back, then refile
        if let Some(hd) = self.heli_data_mut(r) {
            *hd = h;
        }
        if let Some(e) = self.get_mut(r) {
            e.base.matrix = m;
            e.phys.as_mut().unwrap().move_speed = mv;
            e.base.flags.is_in_safe_position = true;
        }
        let _ = self.remove_and_add(r);
        let _ = VehicleData::default;
    }

    fn remove_and_delete_heli(&mut self, i: usize) {
        if let Some(r) = self.ents.helis.helis[i].take() {
            self.delete(r);
        }
    }

    /// `CHeli::GenerateHeli`.
    pub fn generate_heli(&mut self, catalina: bool, env: &CarEnv, rng: &mut Rng) -> Option<EntityRef> {
        let model = if catalina { mi::ESCAPE } else { mi::CHOPPER };
        let r = self.new_heli(model, crate::vehicle::base::PERMANENT_VEHICLE, rng)?;
        let pos = if catalina {
            Vec3::new(-224.0, 201.0, 83.0)
        } else {
            let mut p = env.player.coors;
            let angle = (rng.number() & 0xFF) as f32 / 0x100 as f32 * 6.28;
            p.x += 250.0 * angle.sin();
            p.y += 250.0 * angle.cos();
            if !(-2000.0..=2000.0).contains(&p.x) || !(-2000.0..=2000.0).contains(&p.y) {
                p = env.player.coors;
                p.x -= 250.0 * angle.sin();
                p.y -= 250.0 * angle.cos();
            }
            p.z += 50.0;
            p
        };
        let used: Vec<i8> = self.ents.helis.helis.iter().flatten().filter_map(|&h| self.heli_data(h).map(|d| d.heli_id)).collect();
        let mut id = 0i8;
        while used.contains(&id) {
            id += 1;
        }
        let e = self.get_mut(r)?;
        e.base.matrix = crate::math::Matrix::from_translation(pos);
        if catalina {
            e.base.matrix = crate::math::Matrix::rotate_z_only(270.0 * std::f32::consts::PI / 180.0);
            e.base.matrix.pos = pos;
        }
        e.base.status = status::ABANDONED;
        if let Kind::Vehicle(v) = &mut e.kind {
            v.flags.is_locked = true;
        }
        if let Some(d) = self.heli_data_mut(r) {
            d.heli_id = id;
        }
        let _ = self.add(r);
        Some(r)
    }

    /// `CHeli::UpdateHelis`: spawns, retires and blows up the helicopters.
    pub fn update_helis(&mut self, env: &mut CarEnv, rng: &mut Rng) {
        let timer = self.ents.timer.clone();
        let mut num_required = if env.player.replay_playing { 0 } else { env.player.num_helis_required };
        if env.player.chopper_loaded && timer.time_in_ms > self.ents.helis.test_for_new_random_helis_timer {
            self.ents.helis.test_for_new_random_helis_timer = timer.time_in_ms + 15000;
            if (self.ents.helis.num_random_helis as i32) < num_required {
                self.ents.helis.num_random_helis += 1;
                if let Some(r) = self.generate_heli(false, env, rng) {
                    if let Some(d) = self.heli_data_mut(r) {
                        d.heli_type = HeliType::Random;
                    }
                    let slots = &mut self.ents.helis.helis;
                    if slots[HELI_RANDOM0].is_none() {
                        slots[HELI_RANDOM0] = Some(r);
                    } else if slots[HELI_RANDOM1].is_none() {
                        slots[HELI_RANDOM1] = Some(r);
                    }
                }
            }
        }
        // the script heli
        if self.ents.helis.script_heli_on {
            if env.player.chopper_loaded && self.ents.helis.helis[HELI_SCRIPT].is_none()
                && let Some(r) = self.generate_heli(false, env, rng) {
                    if let Some(d) = self.heli_data_mut(r) {
                        d.heli_type = HeliType::Script;
                    }
                    self.ents.helis.helis[HELI_SCRIPT] = Some(r);
                }
        } else if let Some(r) = self.ents.helis.helis[HELI_SCRIPT]
            && let Some(d) = self.heli_data_mut(r)
        {
            d.status = HeliStatus::FlyAway;
        }
        // Catalina's heli
        if self.ents.helis.catalina_heli_on {
            if env.player.escape_loaded && self.ents.helis.helis[HELI_CATALINA].is_none()
                && let Some(r) = self.generate_heli(true, env, rng) {
                    if let Some(d) = self.heli_data_mut(r) {
                        d.heli_type = HeliType::Catalina;
                    }
                    self.ents.helis.helis[HELI_CATALINA] = Some(r);
                }
        } else if let Some(r) = self.ents.helis.helis[HELI_CATALINA]
            && let Some(d) = self.heli_data_mut(r)
        {
            d.status = HeliStatus::FlyAway;
        }

        // drop the ones that left
        for i in 0..NUM_HELIS {
            let gone = self.ents.helis.helis[i].and_then(|r| self.heli_data(r).map(|d| d.status == HeliStatus::FlyAway).zip(self.get(r).map(|e| e.base.position().z)));
            if let Some((true, z)) = gone
                && z > 150.0
            {
                self.remove_and_delete_heli(i);
                if i != HELI_SCRIPT && i != HELI_CATALINA {
                    self.ents.helis.num_random_helis -= 1;
                }
            }
        }

        // explosions
        for i in 0..NUM_HELIS {
            let Some(r) = self.ents.helis.helis[i] else { continue };
            let Some(d) = self.heli_data(r) else { continue };
            if d.status != HeliStatus::ShotDown {
                continue;
            }
            let (explosion_timer, pos) = (d.explosion_timer, self.get(r).map(|e| e.base.position()).unwrap_or(Vec3::ZERO));
            if timer.time_in_ms > explosion_timer {
                // the second part of the explosion
                env.events.push(CarEvent::CamShake { amount: 0.7, pos });
                let colours: [[u8; 4]; 8] = [[0, 0, 0, 255], [224, 230, 238, 255], [0, 0, 0, 255], [0, 0, 0, 255], [66, 162, 252, 255], [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 255]];
                for _ in 0..40 {
                    let dir = Vec3::new(rng.range_f32(-2.0, 2.0), rng.range_f32(-2.0, 2.0), rng.range_f32(0.0, 2.0));
                    let mut rot = rng.range_i32(10, 30);
                    if rng.number() & 1 != 0 {
                        rot = -rot;
                    }
                    self.ents.helis.frame_gen += 1;
                    let fg = self.ents.helis.frame_gen;
                    let mut p = Particle::new(ParticleKind::HeliDebris, pos, dir, rng.range_f32(0.1, 1.0));
                    p.colour = Some(colours[(fg & 7) as usize]);
                    p.rotation_speed = rot as f32;
                    p.frame = fg & 3;
                    env.events.push(CarEvent::Particle(p));
                }
                env.events.push(CarEvent::HeliExplosion { pos, bomb: false, culprit: None });
                for c in [node::SKID_LEFT, node::SKID_RIGHT, node::TOPROTOR] {
                    self.spawn_heli_component(r, c, rng);
                }
                env.events.push(CarEvent::HeliDestroyed { slot: i, pos });
                self.remove_and_delete_heli(i);
                if i != HELI_SCRIPT && i != HELI_CATALINA {
                    self.ents.helis.num_random_helis -= 1;
                }
                if i == HELI_CATALINA {
                    self.ents.helis.catalina_has_been_shot_down = true;
                }
                self.ents.helis.test_for_new_random_helis_timer = timer.time_in_ms + 50000;
            } else if timer.time_in_ms + 7000 > explosion_timer {
                // the first part
                if timer.previous_time_in_ms + 7000 < explosion_timer {
                    self.spawn_heli_component(r, node::BACKROTOR, rng);
                    self.spawn_heli_component(r, node::TAIL, rng);
                    let fwd = self.get(r).map(|e| e.base.matrix.forward).unwrap_or(Vec3::Y);
                    if let Some(d) = self.heli_data_mut(r) {
                        d.angular_speed *= -2.5;
                    }
                    if let Some(e) = self.get_mut(r) {
                        e.base.flags.render_scorched = true;
                    }
                    env.events.push(CarEvent::CamShake { amount: 0.4, pos });
                    env.events.push(CarEvent::HeliExplosion { pos: pos - 2.5 * fwd, bomb: false, culprit: None });
                } else if let Some(d) = self.heli_data_mut(r) {
                    d.angular_speed *= 1.03;
                }
            }
        }

        // retire police helis that are no longer wanted
        for i in 0..2 {
            let Some(r) = self.ents.helis.helis[i] else { continue };
            if self.heli_data(r).is_some_and(|d| d.status != HeliStatus::FlyAway) {
                if num_required > 0 {
                    num_required -= 1;
                } else if let Some(d) = self.heli_data_mut(r) {
                    d.status = HeliStatus::FlyAway;
                }
            }
        }
        // all helis leave when the player is in a tunnel or under water
        if env.player.coors.z < -2.0 {
            for i in 0..NUM_HELIS {
                if let Some(r) = self.ents.helis.helis[i]
                    && let Some(d) = self.heli_data_mut(r)
                    && d.status != HeliStatus::ShotDown
                {
                    d.status = HeliStatus::FlyAway;
                }
            }
        }
    }

    /// `CHeli::SpawnFlyingComponent`.
    pub fn spawn_heli_component(&mut self, r: EntityRef, component: usize, rng: &mut Rng) -> Option<EntityRef> {
        use crate::entity::object::CreatedBy;
        use crate::entity::physical::{Physical, GRAVITY};
        let d = self.heli_data(r)?;
        let node_pos = d.node_positions.get(component).copied().flatten()?;
        let e = self.get(r)?;
        let (m, model, mv, ts) = (e.base.matrix, e.base.model_index, e.phys.as_ref()?.move_speed, e.phys.as_ref()?.turn_speed);
        let mut o = Entity::new(EntityType::Object, rng);
        o.model_index = mi::CAR_WHEEL;
        o.matrix = m;
        o.matrix.pos = m.transform_point(node_pos);
        o.flags.render_scorched = true;
        o.flags.is_static = false;
        let mut phys = Physical { mass: 10.0, turn_mass: 25.0, air_resistance: 0.99, elasticity: 0.1, buoyancy: 10.0 * GRAVITY / 0.75, ..Default::default() };
        let mut data = crate::entity::ObjectData::new();
        data.created_by = CreatedBy::Temp;
        let now = self.ents.timer.time_in_ms;
        data.end_of_life_time = if component == node::TOPROTOR { now + 1000 } else { now + 3000 };
        data.ref_model_index = model;
        phys.move_speed = mv;
        phys.move_speed.z = if phys.move_speed.z > 0.0 { 0.3 } else { 0.0 };
        phys.turn_speed = ts * 2.0;
        match component {
            node::BACKROTOR => phys.turn_speed.x = 0.5,
            node::TOPROTOR | node::TOPKNOT => phys.turn_speed.z = 0.5,
            _ => phys.turn_speed.y = 0.5,
        }
        self.ents.models.add_ref(model as i32);
        let or = self.insert(Ent::new(o, Some(phys), Kind::Object(data)))?;
        let _ = self.add(or);
        Some(or)
    }

    /// `CHeli::TestRocketCollision`.
    pub fn test_heli_rocket_collision(&mut self, rocket: Vec3, rng: &mut Rng) -> bool {
        let now = self.ents.timer.time_in_ms;
        let mut hit = false;
        for i in 0..NUM_HELIS {
            let Some(r) = self.ents.helis.helis[i] else { continue };
            let Some(e) = self.get(r) else { continue };
            if !e.base.flags.explosion_proof && crate::math::magnitude_squared(rocket - e.base.position()) < 8.0 * 8.0 {
                let spin = if rng.true_false() { 0.05 } else { -0.05 };
                if let Some(d) = self.heli_data_mut(r) {
                    d.angular_speed = spin;
                    d.status = HeliStatus::ShotDown;
                    d.explosion_timer = now + 10000;
                }
                hit = true;
            }
        }
        hit
    }

    /// `CHeli::TestBulletCollision`: the bullet's end position if it hits a heli.
    pub fn test_heli_bullet_collision(&mut self, line0: Vec3, line1: Vec3, damage: u32, rng: &mut Rng) -> Option<Vec3> {
        let now = self.ents.timer.time_in_ms;
        let mut out = None;
        for i in 0..NUM_HELIS {
            let Some(r) = self.ents.helis.helis[i] else { continue };
            let Some(e) = self.get(r) else { continue };
            if e.base.flags.bullet_proof || crate::collision::dist_to_line(line0, line1, e.base.position()) >= 5.0 {
                continue;
            }
            let dist_to_heli = magnitude(e.base.position() - line0);
            let line = line1 - line0;
            let length = magnitude(line);
            out = Some(line0 + line * (1.0f32.max(dist_to_heli - 5.0) / length));
            let Some(d) = self.heli_data_mut(r) else { continue };
            d.bullet_damage += damage;
            if (d.heli_type == HeliType::Catalina && d.bullet_damage > 400) || (d.heli_type != HeliType::Catalina && d.bullet_damage > 700) {
                // the spin is only drawn once the heli is shot down
                d.angular_speed = if rng.true_false() { 0.05 } else { -0.05 };
                d.status = HeliStatus::ShotDown;
                d.explosion_timer = now + 10000;
            }
        }
        out
    }

    pub fn start_catalina_fly_by(&mut self) {
        self.ents.helis.catalina_heli_on = true;
        self.ents.helis.catalina_has_been_shot_down = false;
    }

    pub fn remove_catalina_heli(&mut self) {
        self.ents.helis.catalina_heli_on = false;
        self.remove_and_delete_heli(HELI_CATALINA);
    }

    pub fn catalina_take_off(&mut self) {
        if let Some(r) = self.ents.helis.helis[HELI_CATALINA]
            && let Some(d) = self.heli_data_mut(r)
        {
            d.path_state = 8;
        }
    }

    pub fn make_catalina_heli_fly_away(&mut self) {
        if let Some(r) = self.ents.helis.helis[HELI_CATALINA]
            && let Some(d) = self.heli_data_mut(r)
        {
            d.path_state = 12;
        }
    }
}

#[allow(dead_code)]
const _: Option<CarSound> = None;
