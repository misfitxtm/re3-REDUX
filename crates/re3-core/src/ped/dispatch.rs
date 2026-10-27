//! The virtual `ProcessControl` of the `CPed` subclasses, and the ped-side answers to what the physics queued.
use super::ctx::Px;
use super::data::*;
use super::fight::local_direction;
use super::pending::Hit;
use crate::anim::tables_gta3::anim;
use crate::entity::EntityRef;
use crate::vehicle::damage::engine;
use crate::vehicle::env::CarEvent;
use crate::weapons::info::weapon_type as wt;
use crate::world::{World, WorldEvent};
use glam::{Vec2, Vec3};

impl World {
    /// `ProcessControl` of whichever subclass the ped is.
    pub fn process_ped_control(&mut self, px: &mut Px, r: EntityRef) {
        match self.ped(r).class {
            PedClass::Civilian => self.civilian_process_control(px, r),
            PedClass::Player => self.player_process_control(px, r),
            PedClass::Cop => self.cop_process_control(px, r),
            PedClass::Emergency => self.emergency_process_control(px, r),
        }
    }

    /// Does what the physics queued for peds and fires and hands back the rest (sound, glass, car control).
    pub fn handle_ped_events(&mut self, px: &mut Px, events: Vec<WorldEvent>) -> Vec<WorldEvent> {
        let mut rest = Vec::new();
        for ev in events {
            match ev {
                WorldEvent::PedFall { ped, by, damage, direction } => {
                    if self.try_ped(ped).is_some() {
                        self.inflict_damage(px, ped, Hit { by: Some(by), weapon: wt::FALL, damage, piece: piece::TORSO, direction });
                    }
                }
                WorldEvent::KillPedWithCar { ped, vehicle, impulse } => {
                    if self.try_ped(ped).is_some() && self.try_veh(vehicle).is_some() {
                        self.kill_ped_with_car(px, ped, vehicle, impulse);
                    }
                }
                WorldEvent::ExplosionHitPed { ped, culprit, damage, force_dir } => self.explosion_hits_ped(px, ped, culprit, damage, force_dir),
                WorldEvent::RemovePed(ped) => {
                    if self.try_ped(ped).is_some() {
                        self.remove_ped(px, ped);
                    }
                }
                WorldEvent::StartFire { target, culprit } => {
                    self.start_fire_on(px, target, culprit, 0.8, true);
                }
                WorldEvent::StartFireAt { pos, size } => self.start_fire_at(px, pos, size, false),
                WorldEvent::ExtinguishCarFire(v) => self.extinguish_car_fire(px, v),
                WorldEvent::Car(CarEvent::TakeRemoteControl) => self.take_remote_controlled_car_from_player(),
                WorldEvent::Car(CarEvent::DutyEnded { ambulance, firetruck, law_enforcer }) => {
                    let cc = &mut self.ents.car_ctrl;
                    cc.num_ambulances_on_duty -= ambulance as i32;
                    cc.num_firetrucks_on_duty -= firetruck as i32;
                    cc.num_law_enforcer_cars -= law_enforcer as i32;
                }
                other => rest.push(other),
            }
        }
        rest
    }

    /// The ped half of `CWorld::TriggerExplosion`: damage, the fall, and who gets blamed.
    fn explosion_hits_ped(&mut self, px: &mut Px, ped: EntityRef, culprit: Option<EntityRef>, damage: f32, force: Vec3) {
        let Some(pd) = self.try_ped(ped) else { return };
        let direction = local_direction(pd.rotation_cur, Vec2::new(-force.x, -force.y));
        self.inflict_damage(px, ped, Hit { by: culprit, weapon: wt::EXPLOSION, damage, piece: piece::TORSO, direction: direction as u8 });
        let Some(pd) = self.try_ped(ped) else { return };
        if pd.ped_state != state::DIE {
            self.set_fall(px, ped, 2000, anim::STD_HIGHIMPACT_FRONT + direction as u32, false);
        }
        let Some(by) = culprit.filter(|&c| self.is_ped(c)) else { return };
        let event = if self.ped(ped).ped_type == super::types::ped_type::COP { event_type::SHOOT_COP } else { event_type::SHOOT_PED };
        px.hooks.register_event_with_culprit(event, ped, by, 10000);
        self.register_threat_with_gang_peds(ped, Some(by));
    }

    /// `CVehicle::ExtinguishCarFire`.
    pub fn extinguish_car_fire(&mut self, px: &mut Px, r: EntityRef) {
        let Some(v) = self.try_veh_mut(r) else { return };
        v.health = v.health.max(300.0);
        let fire = v.car_fire;
        if let Some(i) = fire {
            self.extinguish_fire_index(px, i);
        }
        if let Some(car) = self.car_mut(r) {
            if car.damage.engine_status >= engine::ON_FIRE {
                car.damage.set_engine_status(engine::ON_FIRE as u32 - 10);
            }
            car.fire_blow_up_timer = 0.0;
        }
    }
}
