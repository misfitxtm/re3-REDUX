//! `Ped.cpp`: giving, selecting and clearing a ped's weapons.
use crate::entity::EntityRef;
use crate::weapons::info::weapon_type as wt;
use crate::weapons::weapon::WeaponState;
use crate::world::World;

impl World {
    /// `CPed::GiveWeapon`.
    pub fn give_weapon(&mut self, r: EntityRef, ty: usize, ammo: i32) -> usize {
        let infos = &self.ents.weapon_infos;
        let pd = match &mut self.ents.pools.get_mut(r).expect("ped").kind {
            crate::entity::Kind::Ped(p) => p,
            _ => panic!("not a ped"),
        };
        if pd.has_weapon(ty) {
            let w = &mut pd.weapons[ty];
            w.ammo_total = if w.ammo_total + ammo > 99999 { 99999 } else { w.ammo_total + ammo };
            w.reload(infos);
        } else {
            pd.weapons[ty].initialise(infos, ty, ammo);
            pd.max_weapon_type_allowed += 1;
        }
        if pd.weapons[ty].state == WeaponState::OutOfAmmo {
            pd.weapons[ty].state = WeaponState::Ready;
        }
        ty
    }

    /// `CPed::GetWeaponSlot`.
    pub fn get_weapon_slot(&self, r: EntityRef, ty: usize) -> Option<usize> {
        self.ped(r).has_weapon(ty).then_some(ty)
    }

    /// `CPed::SetCurrentWeapon`.
    pub fn set_current_weapon(&mut self, r: EntityRef, ty: usize) {
        if !self.ped(r).has_weapon(ty) {
            return;
        }
        self.remove_weapon_model(r);
        self.ped_mut(r).current_weapon = ty as u8;
        let model = self.ents.weapon_infos.get(ty).model_id;
        self.add_weapon_model(r, model);
    }

    /// `CPed::GrantAmmo`.
    pub fn grant_ammo(&mut self, r: EntityRef, ty: usize, ammo: i32) {
        let infos = &self.ents.weapon_infos;
        let pd = match &mut self.ents.pools.get_mut(r).expect("ped").kind {
            crate::entity::Kind::Ped(p) => p,
            _ => panic!("not a ped"),
        };
        if pd.has_weapon(ty) {
            pd.weapons[ty].ammo_total += ammo;
        } else {
            pd.weapons[ty].initialise(infos, ty, ammo);
            pd.max_weapon_type_allowed += 1;
        }
    }

    /// `CPed::SetAmmo`.
    pub fn set_ammo(&mut self, r: EntityRef, ty: usize, ammo: i32) {
        let infos = &self.ents.weapon_infos;
        let pd = match &mut self.ents.pools.get_mut(r).expect("ped").kind {
            crate::entity::Kind::Ped(p) => p,
            _ => panic!("not a ped"),
        };
        if pd.has_weapon(ty) {
            pd.weapons[ty].ammo_total = ammo;
        } else {
            pd.weapons[ty].initialise(infos, ty, ammo);
            pd.max_weapon_type_allowed += 1;
        }
    }

    /// `CPed::ClearWeapons`.
    pub fn clear_weapons(&mut self, r: EntityRef) {
        self.remove_weapon_model(r);
        let pd = self.ped_mut(r);
        pd.max_weapon_type_allowed = wt::BASEBALLBAT as u8;
        pd.current_weapon = wt::UNARMED as u8;
        let model = self.ents.weapon_infos.get(wt::UNARMED).model_id;
        self.add_weapon_model(r, model);
        for w in &mut self.ped_mut(r).weapons {
            w.ty = wt::UNARMED;
            w.state = WeaponState::Ready;
            w.ammo_in_clip = 0;
            w.ammo_total = 0;
            w.timer = 0;
        }
    }
}
