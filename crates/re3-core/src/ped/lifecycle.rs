//! Making and unmaking a ped: `CPed::CPed`, `SetModelIndex` and `~CPed`.
use super::clump::PedClump;
use super::ctx::Px;
use super::data::*;
use crate::anim::tables_gta3::anim;
use crate::entity::physical::Physical;
use crate::entity::{Ent, Entity, EntityRef, EntityType, Kind};
use crate::vehicle::automobile::car_door_flag;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;
use glam::Vec3;

impl World {
    /// `new CPed(pedType)` followed by `SetModelIndex` when the model's skeleton is at hand.
    pub fn create_ped(&mut self, px: &mut Px, ped_type: usize, model: i16, clump: Option<PedClump>) -> Option<EntityRef> {
        let mut base = Entity::new(EntityType::Ped, px.rng);
        base.flags.uses_collision = true;
        base.flags.ped_physics = true;
        base.flags.use_collision_records = true;
        base.model_index = model;
        let rand_camera = px.rng.number() & 3 == 0;
        let fear_flags = self.ents.ped_types.threats(ped_type);
        let ped = Box::new(PedData::new(ped_type, fear_flags, rand_camera));
        let mass = 70.0;
        let phys = Physical { mass, turn_mass: 100.0, air_resistance: 0.4 / mass, elasticity: 0.05, ..Default::default() };
        let r = self.insert(Ent::new(base, Some(phys), Kind::Ped(ped)))?;
        self.ents.population.update_ped_count(ped_type, false);
        self.give_weapon(r, wt::UNARMED, 0);
        if let Some(clump) = clump {
            self.set_ped_model(r, clump);
        }
        Some(r)
    }

    /// The ped half of `CPed::SetModelIndex`: stats, turn rate and animation group come from the model.
    pub fn set_ped_model(&mut self, r: EntityRef, mut clump: PedClump) {
        let model = self.ent(r).base.model_index as i32;
        let (stat, group) = match self.ents.models.get(model).map(|m| &m.kind) {
            Some(crate::modelinfo::Kind::Ped(p)) => (p.ped_stat_type.max(0) as usize, p.anim_group),
            _ => (0, 0),
        };
        // the root's movement is read back as `anim_move_delta`
        clump.anim.velocity = Some(Vec3::ZERO);
        self.ped_mut(r).clump = Some(clump);
        self.set_ped_stats(r, stat);
        let rate = self.ents.ped_stats.0[stat].heading_change_rate;
        let pd = self.ped_mut(r);
        pd.heading_rate = rate;
        pd.anim_group = group;
        self.ped_add_anim(r, group as usize, anim::STD_IDLE);
    }

    /// `CPed::~CPed`: lets go of the vehicle, the fire and the neighbours' lists, then frees the slot.
    pub fn destroy_ped(&mut self, px: &mut Px, r: EntityRef) {
        if self.ped(r).in_vehicle {
            let (veh, door, state) = (self.ped(r).my_vehicle, self.ped(r).veh_door, self.ped(r).ped_state);
            if let Some(v) = veh.and_then(|v| self.try_veh_mut(v)) {
                if v.driver == Some(r) {
                    v.driver = None;
                } else {
                    v.remove_passenger(r);
                }
                if matches!(state, state::EXIT_CAR | state::DRAG_FROM_CAR) {
                    v.getting_out_flags &= !car_door_flag(door as usize);
                }
            }
            let pd = self.ped_mut(r);
            pd.in_vehicle = false;
            pd.my_vehicle = None;
        } else if self.ped(r).entering_car() {
            self.quit_entering_car(px, r);
        }
        if let Some(fire) = self.ped(r).fire {
            self.extinguish_fire_index(px, fire);
        }
        // the neighbours' lists can be out of date, so the ped takes itself out of them
        let near: Vec<EntityRef> = self.ped(r).near_peds.iter().flatten().copied().collect();
        let near: Vec<EntityRef> = near.into_iter().filter(|&n| self.ped_pointer_valid(n)).collect();
        for n in near {
            let Some(pd) = self.get_mut(n).and_then(|e| match &mut e.kind {
                Kind::Ped(p) => Some(p),
                _ => None,
            }) else {
                continue;
            };
            let kept: Vec<Option<EntityRef>> = pd.near_peds.iter().copied().filter(|&p| p != Some(r)).collect();
            pd.num_near_peds = pd.num_near_peds.saturating_sub((10 - kept.len()) as u16);
            for (i, slot) in pd.near_peds.iter_mut().enumerate() {
                *slot = kept.get(i).copied().flatten();
            }
        }
        let ty = self.ped(r).ped_type;
        self.ents.population.update_ped_count(ty, true);
        self.delete(r);
    }
}
