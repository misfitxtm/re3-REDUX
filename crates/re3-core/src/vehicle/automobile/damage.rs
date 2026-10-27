//! Collision damage, wrecking and the damage effects of cars (`VehicleDamage`, `BlowUpCar`, ...).
//! Parts that fly off, sounds and particles are returned as events.
// The per-wheel loops index several parallel arrays, as the original does.
#![allow(clippy::needless_range_loop)]
use super::{door_id, node, piece, Parts};
use crate::entity::EntityRef;
use crate::general::Rng;
use crate::math::{dot, magnitude};
use crate::modelinfo::indices_gta3::mi;
use crate::surface;
use crate::vehicle::base::CarLock;
use crate::vehicle::damage::{door_status, engine, Component, BUMPER_FRONT, BUMPER_REAR};
use crate::vehicle::env::{CarEvent, CarSound, ExplosionKind, Particle, ParticleKind, PlayerView};
use crate::vehicle::handling::HandlingData;
use glam::Vec3;

/// `ePanels` that are not bumpers.
pub mod panel {
    pub const FRONT_LEFT: u8 = 0;
    pub const FRONT_RIGHT: u8 = 1;
    pub const REAR_LEFT: u8 = 2;
    pub const REAR_RIGHT: u8 = 3;
    pub const WINDSCREEN: u8 = 4;
}

/// What the car needs to know about whatever it collided with.
#[derive(Debug, Clone, Copy, Default)]
pub struct DamageSource {
    pub is_building: bool,
    /// `GetStatus()` of the damaging entity.
    pub status: u8,
}

pub struct DamageCtx<'a> {
    pub player: &'a PlayerView,
    /// `m_pDamageEntity` details, if any.
    pub source: Option<DamageSource>,
    /// `GetIsOnScreen()`, from the renderer.
    pub on_screen: bool,
    pub model_index: i16,
    pub player_vehicle_move_speed: f32,
    pub timer: &'a crate::timer::Timer,
    pub rng: &'a mut Rng,
    pub events: &'a mut Vec<CarEvent>,
}

impl Parts<'_> {
    /// `SetPanelDamage` etc. need the node to exist; a missing part sends a flying component.
    fn node_present(&self, component: usize) -> bool {
        self.car.is_component_present(component)
    }

    pub fn set_panel_damage(&mut self, component: usize, panel: u8, no_flying: bool, events: &mut Vec<CarEvent>) {
        let status = self.car.damage.panel_status(panel);
        if !self.node_present(component) {
            return;
        }
        if status == 1 {
            self.veh.flags.is_damaged = true;
        } else if status == 3 {
            self.veh.flags.is_damaged = true;
            if !no_flying {
                events.push(CarEvent::FlyingComponent { car: self.me, component, group: 5 });
            }
        }
    }

    pub fn set_bumper_damage(&mut self, component: usize, panel: u8, no_flying: bool, events: &mut Vec<CarEvent>) {
        let status = self.car.damage.panel_status(panel);
        if !self.node_present(component) {
            return;
        }
        if status == 1 {
            self.veh.flags.is_damaged = true;
        } else if status == 3 {
            self.veh.flags.is_damaged = true;
            if !no_flying {
                events.push(CarEvent::FlyingComponent { car: self.me, component, group: 0 });
            }
        }
    }

    /// `SetDoorDamage`; `noswing_boot` is the handling flag.
    pub fn set_door_damage(&mut self, component: usize, door: usize, no_flying: bool, handling: &HandlingData, events: &mut Vec<CarEvent>) {
        let mut status = self.car.damage.door_status(door);
        if !self.node_present(component) {
            return;
        }
        if door == door_id::BOOT && status == door_status::SWINGING && handling.flags & crate::vehicle::handling::flags::NOSWING_BOOT != 0 {
            self.car.damage.set_door_status(door_id::BOOT, door_status::MISSING);
            status = door_status::MISSING;
        }
        match status {
            door_status::SMASHED => self.veh.flags.is_damaged = true,
            door_status::MISSING => {
                self.veh.flags.is_damaged = true;
                if !no_flying {
                    let group = match door {
                        door_id::BONNET => 3,
                        door_id::BOOT => 4,
                        _ => 2,
                    };
                    events.push(CarEvent::FlyingComponent { car: self.me, component, group });
                }
            }
            _ => {}
        }
    }

    /// `SetupDamageAfterLoad`: re-applies the visible damage of every part the model has.
    pub fn setup_damage_after_load(&mut self, handling: &HandlingData, events: &mut Vec<CarEvent>) {
        let present = |p: &Self, n| p.node_present(n);
        if present(self, node::BUMP_FRONT) {
            self.set_bumper_damage(node::BUMP_FRONT, BUMPER_FRONT, false, events);
        }
        for (n, d) in [
            (node::BONNET, door_id::BONNET),
            (node::BOOT, door_id::BOOT),
            (node::DOOR_LF, door_id::FRONT_LEFT),
            (node::DOOR_RF, door_id::FRONT_RIGHT),
            (node::DOOR_LR, door_id::REAR_LEFT),
            (node::DOOR_RR, door_id::REAR_RIGHT),
        ] {
            if present(self, n) {
                self.set_door_damage(n, d, false, handling, events);
            }
        }
        if present(self, node::BUMP_REAR) {
            self.set_bumper_damage(node::BUMP_REAR, BUMPER_REAR, false, events);
        }
        for (n, p) in [(node::WING_LF, panel::FRONT_LEFT), (node::WING_RF, panel::FRONT_RIGHT), (node::WING_LR, panel::REAR_LEFT), (node::WING_RR, panel::REAR_RIGHT)] {
            if present(self, n) {
                self.set_panel_damage(n, p, false, events);
            }
        }
    }

    /// `RemoveBonnetInPedCollision`: a swinging bonnet that is open far enough flies off.
    pub fn remove_bonnet_in_ped_collision(&mut self, events: &mut Vec<CarEvent>) -> bool {
        let d = &self.car.doors[door_id::BONNET];
        if self.car.damage.door_status(door_id::BONNET) == door_status::SWINGING && d.angle_when_open() * 0.4 < d.angle {
            events.push(CarEvent::FlyingComponent { car: self.me, component: node::BONNET, group: 3 });
            self.veh.flags.is_damaged = true;
            self.car.damage.set_door_status(door_id::BONNET, door_status::MISSING);
            return true;
        }
        false
    }

    /// `dmgDrawCarCollidingParticles`.
    pub fn colliding_particles(&self, pos: Vec3, amount: f32, ctx: &mut DamageCtx, colour: [u8; 4]) {
        if !ctx.on_screen {
            return;
        }
        let n = (amount as i32) / 20;
        for _ in 0..((n + 4) & 0x1F) {
            let dir = Vec3::new(ctx.rng.range_f32(-0.1, 0.1), ctx.rng.range_f32(-0.1, 0.1), 0.006);
            ctx.events.push(CarEvent::Particle(Particle::new(ParticleKind::SparkSmall, pos, dir, 0.0)));
        }
        for _ in 0..n + 2 {
            let p = Vec3::new(ctx.rng.range_f32(-1.2, 1.2) + pos.x, ctx.rng.range_f32(-1.2, 1.2) + pos.y, pos.z);
            ctx.events.push(CarEvent::Particle(Particle::new(ParticleKind::CarCollisionDust, p, Vec3::ZERO, 0.5)));
        }
        let n = (amount as i32) / 50 + 1;
        for _ in 0..n {
            let dir = Vec3::new(ctx.rng.range_f32(-0.25, 0.25), ctx.rng.range_f32(-0.25, 0.25), ctx.rng.range_f32(0.1, 0.25));
            let size = ctx.rng.range_f32(0.02, 0.08);
            let rotation_speed = ctx.rng.range_i32(-40, 40) as f32;
            let frame = ctx.rng.range_i32(0, 4);
            let mut p = Particle::new(ParticleKind::CarDebris, pos, dir, size);
            p.colour = Some(colour);
            p.rotation_speed = rotation_speed;
            p.frame = frame;
            ctx.events.push(CarEvent::Particle(p));
        }
    }

    /// `VehicleDamage`: turns the last collision (`impulse` 0) or a given hit into damage.
    pub fn vehicle_damage(&mut self, handling: &HandlingData, impulse: f32, damaged_piece: u16, colour: [u8; 4], ctx: &mut DamageCtx) {
        let (mut impulse, mut damaged_piece, mut damage_multiplier) = (impulse, damaged_piece, 0.2f32);
        if impulse == 0.0 {
            impulse = self.phys.damage_impulse;
            damaged_piece = self.phys.damage_piece_type as u16;
            damage_multiplier = 1.0;
        }
        if !self.veh.flags.can_be_damaged {
            return;
        }
        let me_is_player_vehicle = ctx.player.vehicle == Some(self.me);
        let step = ctx.timer.time_step;

        // damage a flipped car
        if self.base.matrix.up.z < 0.0 && !me_is_player_vehicle {
            if self.car.flags.not_damaged_upside_down || self.base.status == crate::entity::status::PLAYER_REMOTE || self.phys.flags.is_in_water {
                return;
            }
            self.veh.health -= 4.0 * step;
        }

        if impulse > 25.0 && self.base.status != crate::entity::status::WRECKED {
            let damage_entity = self.phys.damage_entity;
            if self.veh.flags.is_law_enforcer
                && ctx.player.vehicle.is_some()
                && ctx.player.vehicle == damage_entity
                && self.base.status != crate::entity::status::ABANDONED
                && ctx.player_vehicle_move_speed >= magnitude(self.phys.move_speed)
                && ctx.player_vehicle_move_speed > 0.1
            {
                ctx.events.push(CarEvent::Vehicle(crate::vehicle::base::VehicleEvent::WantedLevelNoDrop(1)));
            }

            if self.base.status == crate::entity::status::PLAYER && impulse > 50.0 {
                let freq = (0.4 * impulse * 2000.0 / self.phys.mass + 100.0).min(250.0) as u8;
                ctx.events.push(CarEvent::PadShake { duration: (40000 / freq as i32) as f32, freq });
            }

            if self.base.flags.only_damaged_by_player && damage_entity != ctx.player.ped && damage_entity != ctx.player.vehicle {
                return;
            }
            if self.base.flags.collision_proof {
                return;
            }
            if let Some(src) = ctx.source
                && src.is_building
                && dot(self.phys.damage_normal, self.base.matrix.up) > 0.6
            {
                return;
            }

            let old_lights: [u8; 4] = std::array::from_fn(|i| self.car.damage.light_status(i as u8));

            if self.base.matrix.up.z > 0.0 || crate::math::magnitude_squared(self.phys.move_speed) > 0.1 {
                let mult = if self.car.flags.more_resistant_to_damage { 0.5 } else { 4.0 };
                let cdm = handling.collision_damage_multiplier;
                let mut double_money = false;
                let door_lock_ok = matches!(self.veh.door_lock, CarLock::NotUsed | CarLock::Unlocked);
                use piece::*;

                // bumper then, if it is gone, the panel behind it; the original falls through between cases
                let mut bonnet = false;
                let mut boot = false;
                match damaged_piece {
                    BUMP_FRONT => {
                        self.damage_part(node::BUMP_FRONT, impulse, ctx, colour);
                        if self.car.damage.apply_damage(Component::BumperFront, impulse * mult, ctx.rng) {
                            self.set_bumper_damage(node::BUMP_FRONT, BUMPER_FRONT, false, ctx.events);
                            double_money = true;
                        }
                        bonnet = self.node_present(node::BONNET) && self.car.damage.panel_status(BUMPER_FRONT) == 3;
                    }
                    BONNET => bonnet = true,
                    BUMP_REAR => {
                        self.damage_part(node::BUMP_REAR, impulse, ctx, colour);
                        if self.car.damage.apply_damage(Component::BumperRear, impulse * mult, ctx.rng) {
                            self.set_bumper_damage(node::BUMP_REAR, BUMPER_REAR, false, ctx.events);
                            double_money = true;
                        }
                        boot = self.node_present(node::BOOT) && self.car.damage.panel_status(BUMPER_REAR) == 3;
                    }
                    BOOT => boot = true,
                    DOOR_LF | DOOR_RF | DOOR_LR | DOOR_RR => {
                        let (n, comp, d) = match damaged_piece {
                            DOOR_LF => (node::DOOR_LF, Component::DoorFrontLeft, door_id::FRONT_LEFT),
                            DOOR_RF => (node::DOOR_RF, Component::DoorFrontRight, door_id::FRONT_RIGHT),
                            DOOR_LR => (node::DOOR_LR, Component::DoorRearLeft, door_id::REAR_LEFT),
                            _ => (node::DOOR_RR, Component::DoorRearRight, door_id::REAR_RIGHT),
                        };
                        self.damage_part(n, impulse, ctx, colour);
                        if door_lock_ok && self.car.damage.apply_damage(comp, impulse * mult, ctx.rng) {
                            self.set_door_damage(n, d, false, handling, ctx.events);
                            double_money = true;
                        }
                    }
                    WING_LF | WING_RF | WING_LR | WING_RR => {
                        let (n, comp, p) = match damaged_piece {
                            WING_LF => (node::WING_LF, Component::PanelFrontLeft, panel::FRONT_LEFT),
                            WING_RF => (node::WING_RF, Component::PanelFrontRight, panel::FRONT_RIGHT),
                            WING_LR => (node::WING_LR, Component::PanelRearLeft, panel::REAR_LEFT),
                            _ => (node::WING_RR, Component::PanelRearRight, panel::REAR_RIGHT),
                        };
                        self.damage_part(n, impulse, ctx, colour);
                        if self.car.damage.apply_damage(comp, impulse * mult, ctx.rng) {
                            self.set_panel_damage(n, p, false, ctx.events);
                            double_money = true;
                        }
                    }
                    #[allow(clippy::collapsible_match)]
                    WINDSCREEN => {
                        if self.car.damage.apply_damage(Component::PanelWindscreen, impulse * mult, ctx.rng) {
                            let old = self.car.damage.panel_status(panel::WINDSCREEN);
                            self.set_panel_damage(node::WINDSCREEN, panel::WINDSCREEN, false, ctx.events);
                            if old != self.car.damage.panel_status(panel::WINDSCREEN) {
                                ctx.events.push(CarEvent::Sound { sound: CarSound::WindshieldCrack, volume: 0.0 });
                                double_money = true;
                            }
                        }
                    }
                    _ => {}
                }
                if bonnet {
                    self.damage_part(node::BONNET, impulse, ctx, colour);
                    if ctx.model_index != mi::DODO && self.car.damage.apply_damage(Component::DoorBonnet, impulse * mult, ctx.rng) {
                        self.set_door_damage(node::BONNET, door_id::BONNET, false, handling, ctx.events);
                        double_money = true;
                    }
                }
                if boot {
                    self.damage_part(node::BOOT, impulse, ctx, colour);
                    if self.car.damage.apply_damage(Component::DoorBoot, impulse * mult, ctx.rng) {
                        self.set_door_damage(node::BOOT, door_id::BOOT, false, handling, ctx.events);
                        double_money = true;
                    }
                }
                let _ = cdm;

                if damage_entity.is_some() && damage_entity == ctx.player.vehicle && impulse > 10.0 {
                    let money = (((if double_money { 2.0 } else { 1.0 }) * impulse * handling.monetary_value as f32 / 1_000_000.0) as i32).min(40);
                    if money > 2 {
                        ctx.events.push(CarEvent::Money(money));
                    }
                }
            }

            let mut damage = (impulse - 25.0) * handling.collision_damage_multiplier * 0.6 * damage_multiplier;
            if ctx.model_index == mi::SECURICA && ctx.source.is_some_and(|s| s.status == crate::entity::status::PLAYER) {
                damage *= 7.0;
            }
            if damage > 0.0 {
                let old_health = self.veh.health as i32;
                if me_is_player_vehicle {
                    self.veh.health -= if self.veh.flags.take_less_damage { damage / 6.0 } else { damage / 2.0 };
                } else {
                    if damage > 35.0 && self.veh.driver.is_some() {
                        ctx.events.push(CarEvent::Sound { sound: CarSound::AnnoyedDriver, volume: 0.0 });
                    }
                    self.veh.health -= if self.veh.flags.take_less_damage { damage / 12.0 } else { damage / 4.0 };
                }
                if self.veh.health <= 0.0 && old_health > 0 {
                    self.veh.health = 1.0;
                }
            }

            // a light broke
            for (i, &old) in old_lights.iter().enumerate() {
                if old != 1 && self.car.damage.light_status(i as u8) == 1 {
                    ctx.events.push(CarEvent::Sound { sound: CarSound::LightBreak(i as u8), volume: i as f32 });
                    break;
                }
            }
        }

        if self.veh.health < 250.0 {
            if self.car.damage.engine_status < engine::ON_FIRE {
                self.car.damage.set_engine_status(engine::ON_FIRE as u32);
                self.car.fire_blow_up_timer = 0.0;
                self.car.set_on_fire_entity = self.phys.damage_entity;
            }
        } else if ctx.model_index == mi::BFINJECT {
            if self.veh.health < 400.0 {
                self.car.damage.set_engine_status(200);
            } else if self.veh.health < 600.0 {
                self.car.damage.set_engine_status(100);
            }
        }
    }

    /// The impact particles at a damaged part's node.
    fn damage_part(&self, component: usize, impulse: f32, ctx: &mut DamageCtx, colour: [u8; 4]) {
        // a missing node leaves the position at the origin, as in the original
        let pos = self.component_world_position(component).unwrap_or(Vec3::ZERO);
        self.colliding_particles(pos, impulse, ctx, colour);
    }

    /// `BlowUpCar`: wrecks the car. Returns false if it cannot be damaged.
    pub fn blow_up_car(&mut self, culprit: Option<EntityRef>, handling: &HandlingData, model_index: i16, now_ms: u32, events: &mut Vec<CarEvent>) -> bool {
        if !self.veh.flags.can_be_damaged {
            return false;
        }
        // the explosion pushes the vehicle up
        self.phys.move_speed.z += 0.13;
        self.base.status = crate::entity::status::WRECKED;
        self.base.flags.render_scorched = true;
        self.veh.time_of_death = now_ms;
        self.car.damage.wreck(&mut Rng::default());

        if model_index != mi::RCBANDIT {
            self.set_bumper_damage(node::BUMP_FRONT, BUMPER_FRONT, false, events);
            self.set_bumper_damage(node::BUMP_REAR, BUMPER_REAR, false, events);
            for (n, d) in [
                (node::BONNET, door_id::BONNET),
                (node::BOOT, door_id::BOOT),
                (node::DOOR_LF, door_id::FRONT_LEFT),
                (node::DOOR_RF, door_id::FRONT_RIGHT),
                (node::DOOR_LR, door_id::REAR_LEFT),
                (node::DOOR_RR, door_id::REAR_RIGHT),
            ] {
                self.set_door_damage(n, d, false, handling, events);
            }
            if self.node_present(node::WHEEL_LF) {
                events.push(CarEvent::FlyingComponent { car: self.me, component: node::WHEEL_LF, group: 1 });
            }
        }
        self.veh.health = 0.0;
        self.veh.bomb_timer = 0;
        self.car.bomb_type = super::BombType::None;

        events.push(CarEvent::CamShake { amount: 0.7, pos: self.base.position() });
        events.push(CarEvent::KillOccupants);

        self.veh.flags.engine_on = false;
        self.veh.flags.lights_on = false;
        self.veh.siren_or_alarm = false;
        self.car.flags.taxi_light = false;
        let (ambulance, firetruck) = (self.veh.flags.is_ambulance_on_duty, self.veh.flags.is_fire_truck_on_duty);
        self.veh.flags.is_ambulance_on_duty = false;
        self.veh.flags.is_fire_truck_on_duty = false;
        let law_enforcer = self.veh.change_law_enforcer_state(false).is_some();
        events.push(CarEvent::DutyEnded { ambulance, firetruck, law_enforcer });

        events.push(CarEvent::StartFire { culprit });
        let kind = if model_index == mi::RCBANDIT { ExplosionKind::CarQuick } else { ExplosionKind::Car };
        events.push(CarEvent::Explosion { kind, pos: self.base.position(), culprit });
        true
    }

    /// `AddDamagedVehicleParticles`: steam and smoke from a damaged engine.
    pub fn damaged_vehicle_particles(&self, model_index: i16, headlights: Vec3, player_first_person: bool, events: &mut Vec<CarEvent>) {
        if player_first_person {
            return;
        }
        let engine_status = self.car.damage.engine_status;
        if engine_status < engine::STEAM1 {
            return;
        }
        let fwd_speed = dot(self.phys.move_speed, self.base.matrix.forward) * 180.0;
        let mut direction = 0.5 * self.phys.move_speed;
        let mut pos = headlights + self.engine_offset();
        if model_index == mi::BFINJECT {
            pos = Vec3::new(0.3, -1.5, -0.1);
        }
        pos = self.base.matrix.transform_point(pos);
        pos.z += 0.15;
        let mut add = |kind, dir: Vec3, size: f32| events.push(CarEvent::Particle(Particle::new(kind, pos, dir, size)));
        if engine_status < engine::STEAM2 {
            if fwd_speed < 90.0 {
                direction.z += 0.05;
                add(ParticleKind::EngineSteam, direction, 0.1);
            }
        } else if engine_status < engine::SMOKE {
            if fwd_speed < 90.0 {
                add(ParticleKind::EngineSteam, direction, 0.0);
            }
        } else if engine_status < engine::ON_FIRE {
            if fwd_speed < 90.0 {
                add(ParticleKind::EngineSteam, direction, 0.0);
                add(ParticleKind::EngineSmoke, 0.3 * direction, 0.0);
            }
        } else if self.veh.health > 250.0 && fwd_speed < 90.0 {
            add(ParticleKind::EngineSmoke2, 0.2 * direction, 0.0);
        }
    }

    /// Where damage smoke comes out: at the bonnet edge, or at the engine once the bonnet is gone.
    pub fn engine_offset(&self) -> Vec3 {
        match self.car.damage.door_status(door_id::BONNET) {
            door_status::OK | door_status::SMASHED => Vec3::new(-0.1, -0.1, 0.0),
            _ => Vec3::new(-0.5, -0.3, 0.0),
        }
    }

    /// `AddWheelDirtAndWater`: dirt or spray under a wheel. Returns whether other effects may play too.
    pub fn wheel_dirt_and_water(&self, c: &crate::collision::ColPoint, below_effect_speed: bool, wet_roads: f32, frame_counter: u32, rng: &mut Rng, events: &mut Vec<CarEvent>) -> bool {
        if !below_effect_speed {
            return false;
        }
        let mut dirt = |colour: [u8; 4], size: (f32, f32), events: &mut Vec<CarEvent>| {
            let mut dir = Vec3::new(-0.05 * self.phys.move_speed.x, -0.05 * self.phys.move_speed.y, 0.0);
            for _ in 0..4 {
                dir.z = rng.range_f32(0.03, 0.06);
                let mut p = Particle::new(ParticleKind::WheelDirt, c.point, dir, rng.range_f32(size.0, size.1));
                p.colour = Some(colour);
                events.push(CarEvent::Particle(p));
            }
        };
        match c.surface_b {
            surface::GRASS => {
                dirt([8, 24, 8, 255], (0.02, 0.1), events);
                false
            }
            surface::GRAVEL => {
                dirt([64, 64, 64, 255], (0.02, 0.06), events);
                true
            }
            surface::MUD_DRY => {
                dirt([64, 32, 16, 255], (0.02, 0.06), events);
                false
            }
            _ => {
                if wet_roads > 0.01 && frame_counter & 1 != 0 {
                    let size = rng.range_f32(0.1, 0.5);
                    let mut p = Particle::new(ParticleKind::WaterSpray, c.point + Vec3::new(0.0, 0.0, 0.5), Vec3::Z, size);
                    p.colour = Some([48, 48, 64, 0]);
                    events.push(CarEvent::Particle(p));
                    false
                } else {
                    true
                }
            }
        }
    }
}
