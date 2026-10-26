//! What every ped routine needs: the random generator, the outside systems, and quick accessors.
use super::data::PedData;
use crate::entity::{Ent, EntityRef, Kind};
use crate::general::Rng;
use crate::world::World;
use glam::Vec3;

/// The wanted level facts the ped code asks about the player.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WantedInfo {
    pub current_cops: i32,
    pub level: i32,
    pub ignored_by_cops: bool,
    pub ignored_by_everyone: bool,
}

/// What a `CEventList` entry points at (`eEventEntity`, already looked up in the pools).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventEntity {
    None,
    Ped(EntityRef),
    Vehicle(EntityRef),
    Object(EntityRef),
}

/// A found `CEventList` entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventInfo {
    pub index: usize,
    pub pos: Vec3,
    pub entity: EventEntity,
}

/// Systems the ped code calls into that live outside the world: sound, camera, pad, script, wanted level,
/// population, fire, particles. Every method has a harmless default.
pub trait PedHooks {
    /// `CPed::Say`.
    fn say(&mut self, _ped: EntityRef, _sound: u16) {}
    /// `DMAudio.PlayOneShot` on the ped.
    fn play_one_shot(&mut self, _ped: EntityRef, _sound: u16, _volume: f32) {}
    /// `CEventList::RegisterEvent(event, EVENT_ENTITY_PED, ped, nil, time)`; `event` is an `event_type`.
    fn register_event(&mut self, _event: u32, _ped: EntityRef, _time: u32) {}
    /// `CEventList::FindClosestEvent`.
    fn find_closest_event(&mut self, _event: u32, _pos: Vec3) -> Option<EventInfo> {
        None
    }
    /// `CEventList::ClearEvent`.
    fn clear_event(&mut self, _index: usize) {}
    /// `CTrafficLights::LightForPeds() == PED_LIGHTS_WALK`.
    fn lights_for_peds_walk(&mut self) -> bool {
        true
    }
    /// `CDarkel::FrenzyOnGoing`.
    fn frenzy_on_going(&mut self) -> bool {
        false
    }
    /// `CCullZones::GetWantedLevelDrop`.
    fn wanted_level_drop(&mut self) -> i32 {
        0
    }
    /// `DMAudio.ReportCrime`.
    fn report_crime(&mut self, _ty: u8, _pos: Vec3) {}
    /// `CPed::CreateDeadPedWeaponPickups` and `CreateDeadPedMoney`.
    fn create_dead_ped_pickups(&mut self, _ped: EntityRef) {}
    /// `gPhoneInfo.m_aPhones[id].m_vecPos`.
    fn phone_position(&mut self, _id: i16) -> Vec3 {
        Vec3::ZERO
    }
    /// `gPhoneInfo.m_aPhones[id].m_nState = PHONE_STATE_FREE`.
    fn free_phone(&mut self, _id: i16) {}
    /// `CTheScripts::IsPlayerOnAMission`.
    fn player_on_mission(&mut self) -> bool {
        false
    }
    /// `CCarCtrl::JoinCarWithRoadSystem`.
    fn join_car_with_road_system(&mut self, _veh: EntityRef) {}
    /// A parked car became a random one (`--NumParkedCars`, `++NumRandomCars`).
    fn car_became_random(&mut self, _veh: EntityRef) {}
    /// `AudioManager.PlayerJustLeftCar`.
    fn player_left_car(&mut self) {}
    /// `CEventList::RegisterEvent(EVENT_STEAL_CAR, EVENT_ENTITY_VEHICLE, veh, ped, 1500)`.
    fn register_steal_car(&mut self, _veh: EntityRef, _ped: EntityRef) {}
    /// `CPad::GetAccelerate() >= 255 || GetBrake() >= 255`.
    fn player_pressing_gas_or_brake(&mut self) -> bool {
        false
    }
    /// The target button or any stick or d-pad direction is held.
    fn player_pad_active(&mut self) -> bool {
        false
    }
    /// `CCarCtrl::NumLawEnforcerCars` changes with `CVehicle::ChangeLawEnforcerState`.
    fn law_enforcer_changed(&mut self, _veh: EntityRef, _enforcer: bool) {}
    /// `CPickups::GenerateNewOne(pos, MI_MONEY, PICKUP_MONEY, amount)`.
    fn generate_money_pickup(&mut self, _pos: Vec3, _amount: i32) {}
    /// `TheCamera.IsSphereVisible`.
    fn is_sphere_visible(&mut self, _pos: Vec3, _radius: f32) -> bool {
        false
    }
    /// `CParticle::AddParticle(PARTICLE_BLOOD, pos, dir, ...)`.
    fn add_blood(&mut self, _pos: Vec3, _dir: Vec3) {}
    /// `AddGunshell` (the shotgun casing).
    fn add_gunshell(&mut self, _pos: Vec3, _dir: glam::Vec2, _size: f32) {}
    /// `CDarkel::RegisterKillByPlayer` or `RegisterKillNotByPlayer`.
    fn register_kill(&mut self, _ped: EntityRef, _method: usize, _by_player: bool, _head_shot: bool) {}
    /// `CWorld::Players[PlayerInFocus].m_nTimeLastArmourLoss` or `m_nTimeLastHealthLoss` is set.
    fn player_lost_armour_or_health(&mut self, _armour: bool) {}
    /// A body part came off a ped: the renderer spawns the flying part, hides the frames and adds the particles.
    fn body_part_removed(&mut self, _ped: EntityRef, _node: usize, _direction: i8) {}
    /// `CStats::HeadsPopped++` and the effects of a head shot.
    fn head_shot_effect(&mut self, _ped: EntityRef, _pos: Vec3) {}
    /// `CPad::GetPad(0)->StartShake(duration, frequency)`.
    fn start_pad_shake(&mut self, _duration: u32, _frequency: u8) {}
    /// `CGame::noProstitutes`.
    fn no_prostitutes(&mut self) -> bool {
        false
    }
    /// `CGame::nastyGame`.
    fn nasty_game(&mut self) -> bool {
        true
    }
    /// The camera is in a first-person weapon mode (sniper or M16): `PlayerWeaponMode.Mode`.
    fn player_weapon_first_person(&mut self) -> bool {
        false
    }
    /// `TheCamera.Find3rdPersonQuickAimPitch`.
    fn quick_aim_pitch(&mut self) -> f32 {
        0.0
    }
    /// `CPad::GetPad(0)->ResetAverageWeapon`.
    fn reset_average_weapon(&mut self) {}
    /// `CGarages::IsPointWithinAnyGarage`.
    fn point_in_garage(&mut self, _pos: Vec3) -> bool {
        false
    }
    /// `CCullZones::NoPolice`.
    fn no_police_zone(&mut self) -> bool {
        false
    }
    /// `CCarAI::GetCarToGoToCoors`.
    fn car_goto_coors(&mut self, _veh: EntityRef, _pos: Vec3) {}
    /// `CCarAI::FindPoliceCarMissionForWantedLevel`.
    fn police_car_mission(&mut self) -> u8 {
        0
    }
    /// `CCarCtrl::RegisterVehicleOfInterest`.
    fn register_vehicle_of_interest(&mut self, _veh: EntityRef) {}
    /// `CCarCtrl::ChooseModel` for the zone at `pos`.
    fn choose_car_model(&mut self, _pos: Vec3) -> Option<i16> {
        None
    }
    /// `GetIsOnScreen`.
    fn is_on_screen(&mut self, _entity: EntityRef) -> bool {
        true
    }
    /// `TheCamera.ClearPlayerWeaponMode`.
    fn clear_player_weapon_mode(&mut self) {}
    /// `CCarCtrl::RemoveFromInterestingVehicleList`.
    fn remove_from_interesting_vehicle_list(&mut self, _veh: EntityRef) {}
    /// `CCarCtrl::RegisterVehicleOfInterest` and `AudioManager.PlayerJustGotInCar`.
    fn player_got_in_car(&mut self, _veh: EntityRef) {}
    /// `CCarCtrl::NumAmbulancesOnDuty` is lowered.
    fn ambulance_off_duty(&mut self) {}
    /// `CCarCtrl::NumFiretrucksOnDuty` is lowered.
    fn firetruck_off_duty(&mut self) {}
    /// `DMAudio.IsMP3RadioChannelAvailable`.
    fn mp3_radio_available(&mut self) -> bool {
        false
    }
    /// A permanent dark blood-pool shadow where a bloody foot left the ground.
    fn add_blood_footprint(&mut self, _pos: Vec3, _forward: Vec3, _right: Vec3) {}
    /// `gPhoneInfo.FindNearestFreePhone`: its id and where a ped stands to use it.
    fn nearest_free_phone(&mut self, _pos: Vec3) -> Option<(i16, Vec3)> {
        None
    }
    /// The phone is `PHONE_STATE_FREE`.
    fn phone_is_free(&mut self, _id: i16) -> bool {
        true
    }
    /// `PHONE_STATE_REPORTING_CRIME`.
    fn phone_start_reporting(&mut self, _id: i16) {}
    /// `gPhoneInfo.PhoneAtThisPosition`.
    fn phone_at_position(&mut self, _pos: Vec3) -> bool {
        false
    }
    /// `CPad::GetPad(0)->ArePlayerControlsDisabled`.
    fn player_controls_disabled(&mut self) -> bool {
        false
    }
    /// `Cams[ActiveCam].GetWeaponFirstPersonOn`.
    fn weapon_first_person_on(&mut self) -> bool {
        false
    }
    /// `Cams[0].Using3rdPersonMouseCam`.
    fn third_person_mouse_cam(&mut self) -> bool {
        false
    }
    /// `Cams[ActiveCam].m_fPlayerVelocity = speed`.
    fn set_player_velocity(&mut self, _speed: f32) {}
    /// A permanent (or, with `permanent` false, growing) blood pool under a ped.
    fn blood_pool(&mut self, _ped: EntityRef, _pos: Vec3, _size: f32, _permanent: bool) {}
    /// Where the camera is, for the distance tests that only decide on effects.
    fn camera_position(&mut self) -> Vec3 {
        Vec3::splat(1.0e9)
    }
    /// `TheCamera.Orientation`.
    fn camera_orientation(&mut self) -> f32 {
        0.0
    }
    /// `CPad::GetPad(0)->JumpJustDown`.
    fn jump_just_down(&mut self) -> bool {
        false
    }
    /// `CParticleObject::AddObject` and `CParticle::AddParticle` for a ped.
    fn ped_particle(&mut self, _kind: PedParticle, _pos: Vec3, _dir: Vec3, _size: f32) {}
    /// `CWeather::Rain <= 0.1f || CCullZones::CamNoRain() || CCullZones::PlayerNoRain()`.
    fn no_rain(&mut self) -> bool {
        true
    }
    /// The camera is in a first-person mode (sniper, rocket launcher, M16, first person, heli cannon) or a weapon first-person view.
    fn camera_first_person_weapon(&mut self) -> bool {
        false
    }
    /// The active camera is first person and looking left (`DirectionWasLooking == LOOKING_LEFT`).
    fn camera_first_person_looking_left(&mut self) -> bool {
        false
    }
    /// Position and forward direction of the road piece a roadblock sits on (`CRoadBlocks::RoadBlockObjects`).
    fn roadblock_road(&mut self, _node: i16) -> Option<(Vec3, glam::Vec2)> {
        None
    }
    /// `CRenderer::IsEntityCullZoneVisible`.
    fn cull_zone_visible(&mut self, _entity: EntityRef) -> bool {
        true
    }
    /// `CEventList::RegisterEvent(EVENT_FIRE, pos, time)`.
    fn register_fire_event(&mut self, _pos: Vec3, _time: u32) {}
    /// `CEventList::RegisterEvent(event, entity, culprit, time)` for a ped or vehicle that was set on fire.
    fn register_event_with_culprit(&mut self, _event: u32, _entity: EntityRef, _culprit: EntityRef, _time: u32) {}
    /// A fire's light and, for a fire on the ground, its glow on the road (`shadow` with the green and blue strength).
    fn fire_light(&mut self, _pos: Vec3, _red: f32, _green: f32, _shadow: bool, _shadow_strength: i32) {}
    /// Pad 0 this frame.
    fn pad(&mut self) -> PadState {
        PadState::default()
    }
    /// The camera this frame.
    fn camera(&mut self) -> CameraState {
        CameraState::default()
    }
    /// `TheCamera.SetNewPlayerWeaponMode(mode, 0, 0)`.
    fn set_new_player_weapon_mode(&mut self, _mode: u8) {}
    /// `TheCamera.UpdateAimingCoors`.
    fn update_aiming_coors(&mut self, _pos: Vec3) {}
    /// `TheCamera.PlayerExhaustion`.
    fn set_player_exhaustion(&mut self, _exhaustion: f32) {}
    /// `CWeaponEffects::MarkTarget(pos, 64, g, 0, 255, 0.8f)`.
    fn mark_target(&mut self, _pos: Vec3, _green: u8) {}
    /// `CWeaponEffects::ClearCrossHair`.
    fn clear_cross_hair(&mut self) {}
    /// `DMAudio.PlayFrontEndSound`.
    fn play_front_end_sound(&mut self, _sound: u16) {}
    /// `CTimer::SetTimeScale`.
    fn set_time_scale(&mut self, _scale: f32) {}
    /// `CCarCtrl::PossiblyRemoveVehicle`.
    fn possibly_remove_vehicle(&mut self, _veh: EntityRef) {}
    /// `GetPedWalkLeftRight` and `GetPedWalkUpDown` of pad 0.
    fn ped_walk_axes(&mut self) -> (f32, f32) {
        (0.0, 0.0)
    }
}

/// The effects a ped makes in or near water and on dry ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PedParticle {
    /// `POBJECT_PED_WATER_SPLASH` with its lifetime.
    WaterSplashObject(u32),
    /// `POBJECT_SPLASHES_AROUND` with its lifetime.
    SplashesAround(u32),
    RainSplashBigGrow,
    PedSplash,
    RubberSmoke,
    /// `particleProduceFootDust` / `particleProduceFootSplash` with the particle type.
    FootDust,
    CarFlame,
    CarFlameSmoke,
    FootSplash,
}

/// `CCam` modes the player code tells apart (`TheCamera.PlayerWeaponMode.Mode`).
pub mod cam_mode {
    pub const NONE: u8 = 0;
    pub const M16_1STPERSON: u8 = 1;
    pub const M16_1STPERSON_RUNABOUT: u8 = 2;
    pub const SNIPER: u8 = 3;
    pub const SNIPER_RUNABOUT: u8 = 4;
    pub const ROCKETLAUNCHER: u8 = 5;
    pub const ROCKETLAUNCHER_RUNABOUT: u8 = 6;
    pub const SYPHON: u8 = 7;
}

/// Pad 0 as the player code reads it this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PadState {
    pub walk_left_right: f32,
    pub walk_up_down: f32,
    pub sprint: bool,
    pub jump_just_down: bool,
    pub target: bool,
    pub target_just_down: bool,
    /// `GetWeapon()`: how hard the fire button is pressed.
    pub weapon: u8,
    pub weapon_just_down: bool,
    pub cycle_weapon_right_just_down: bool,
    pub cycle_weapon_left_just_down: bool,
    pub shift_target_left_just_down: bool,
    pub shift_target_right_just_down: bool,
    pub controls_disabled: bool,
    /// Accelerate, brake or steering is held.
    pub driving_input: bool,
    /// A left stick axis or d-pad direction went down this frame.
    pub direction_just_down: bool,
}

/// The camera facts the player code asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraState {
    pub orientation: f32,
    /// `PlayerWeaponMode.Mode`, one of [`cam_mode`].
    pub player_weapon_mode: u8,
    /// The active cam is `MODE_FOLLOWPED` and the player looks behind.
    pub looking_behind: bool,
    /// `Cams[ActiveCam].Front.Heading()`.
    pub front_heading: f32,
    /// `Cams[0].Using3rdPersonMouseCam()`.
    pub using_3rd_person_mouse_cam: bool,
    /// `CCamera::bFreeCam`.
    pub free_cam: bool,
    /// `CCamera::m_bUseMouse3rdPerson`.
    pub use_mouse_3rd_person: bool,
}

/// No outside systems.
pub struct NoPedHooks;
impl PedHooks for NoPedHooks {}

pub struct Px<'a> {
    pub rng: &'a mut Rng,
    pub hooks: &'a mut dyn PedHooks,
}

impl World {
    /// The ped data of `r`. Panics if `r` isn't a live ped (or is lent out).
    pub fn ped(&self, r: EntityRef) -> &PedData {
        match &self.get(r).expect("ped").kind {
            Kind::Ped(p) => p,
            _ => panic!("not a ped"),
        }
    }

    pub fn ped_mut(&mut self, r: EntityRef) -> &mut PedData {
        match &mut self.get_mut(r).expect("ped").kind {
            Kind::Ped(p) => p,
            _ => panic!("not a ped"),
        }
    }

    pub fn try_ped(&self, r: EntityRef) -> Option<&PedData> {
        match &self.get(r)?.kind {
            Kind::Ped(p) => Some(p),
            _ => None,
        }
    }

    pub fn is_ped(&self, r: EntityRef) -> bool {
        self.try_ped(r).is_some()
    }

    /// The vehicle data of `r`, if it is a live vehicle.
    pub fn try_veh(&self, r: EntityRef) -> Option<&crate::vehicle::base::VehicleData> {
        match &self.get(r)?.kind {
            Kind::Vehicle(v) => Some(v),
            _ => None,
        }
    }

    pub fn try_veh_mut(&mut self, r: EntityRef) -> Option<&mut crate::vehicle::base::VehicleData> {
        match &mut self.get_mut(r)?.kind {
            Kind::Vehicle(v) => Some(v),
            _ => None,
        }
    }

    /// The car data of `r`, if it is a car.
    pub fn car_ref(&self, r: EntityRef) -> Option<&crate::vehicle::automobile::AutomobileData> {
        match &self.try_veh(r)?.sub {
            crate::vehicle::base::VehicleSub::Car(c) => Some(c),
            _ => None,
        }
    }

    pub fn car_mut(&mut self, r: EntityRef) -> Option<&mut crate::vehicle::automobile::AutomobileData> {
        match &mut self.try_veh_mut(r)?.sub {
            crate::vehicle::base::VehicleSub::Car(c) => Some(c),
            _ => None,
        }
    }

    /// `IsNotInWreckedVehicle`.
    pub fn is_not_in_wrecked_vehicle(&self, r: EntityRef) -> bool {
        self.ped(r).my_vehicle.is_some_and(|v| self.get(v).is_some_and(|e| e.base.status != crate::entity::status::WRECKED))
    }

    pub fn is_vehicle_ref(&self, r: EntityRef) -> bool {
        matches!(self.get(r).map(|e| &e.kind), Some(Kind::Vehicle(_)))
    }

    pub fn epos(&self, r: EntityRef) -> Vec3 {
        self.get(r).map_or(Vec3::ZERO, |e| e.base.position())
    }

    pub fn now(&self) -> u32 {
        self.ents.timer.time_in_ms
    }

    pub fn time_step(&self) -> f32 {
        self.ents.timer.time_step
    }

    /// `CPed::IsPointerValid`: in the world, or the player.
    pub fn ped_pointer_valid(&self, r: EntityRef) -> bool {
        self.get(r).is_some_and(|e| !e.entry_info.is_empty()) || self.ents.player_ped == Some(r)
    }

    pub fn ent(&self, r: EntityRef) -> &Ent {
        self.get(r).expect("entity")
    }
}

use crate::anim::{AnimClump, Association};

impl World {
    pub fn ped_clump(&self, r: EntityRef) -> Option<&super::clump::PedClump> {
        self.try_ped(r)?.clump.as_ref()
    }

    pub fn ped_clump_mut(&mut self, r: EntityRef) -> Option<&mut super::clump::PedClump> {
        match &mut self.get_mut(r)?.kind {
            Kind::Ped(p) => p.clump.as_mut(),
            _ => None,
        }
    }

    /// `RpAnimBlendClumpGetAssociation(clump, animId)`.
    pub fn ped_assoc(&self, r: EntityRef, anim: u32) -> Option<&Association> {
        self.ped_clump(r)?.anim.association(anim as i32)
    }

    pub fn ped_assoc_mut(&mut self, r: EntityRef, anim: u32) -> Option<&mut Association> {
        self.ped_clump_mut(r)?.anim.association_mut(anim as i32)
    }

    /// `RpAnimBlendClumpGetFirstAssociation(clump, mask)`.
    pub fn ped_first_assoc(&self, r: EntityRef, mask: u32) -> Option<&Association> {
        self.ped_clump(r)?.anim.first_association(mask)
    }

    /// `CAnimManager::BlendAnimation`; the index of the association in the clump.
    pub fn ped_blend_anim(&mut self, r: EntityRef, group: usize, anim: u32, delta: f32) -> Option<usize> {
        let Self { ents, .. } = self;
        let clump = match &mut ents.pools.get_mut(r)?.kind {
            Kind::Ped(p) => p.clump.as_mut()?,
            _ => return None,
        };
        ents.anim.blend_animation(&mut clump.anim, group, anim as usize, delta)
    }

    /// `CAnimManager::AddAnimation`.
    pub fn ped_add_anim(&mut self, r: EntityRef, group: usize, anim: u32) -> Option<usize> {
        let Self { ents, .. } = self;
        let clump = match &mut ents.pools.get_mut(r)?.kind {
            Kind::Ped(p) => p.clump.as_mut()?,
            _ => return None,
        };
        ents.anim.add_animation(&mut clump.anim, group, anim as usize)
    }

    /// The association at `index` of the ped's clump.
    pub fn ped_assoc_at(&mut self, r: EntityRef, index: usize) -> Option<&mut Association> {
        self.ped_clump_mut(r)?.anim.associations.get_mut(index)
    }

    /// `delete RpAnimBlendClumpGetAssociation(clump, anim)`.
    pub fn ped_remove_assoc(&mut self, r: EntityRef, anim: u32) {
        if let Some(c) = self.ped_clump_mut(r) {
            c.anim.associations.retain(|a| a.anim_id != anim as i32);
        }
    }

    /// `RpAnimBlendClumpSetBlendDeltas`.
    pub fn ped_set_blend_deltas(&mut self, r: EntityRef, mask: u32, delta: f32) {
        if let Some(c) = self.ped_clump_mut(r) {
            c.anim.set_blend_deltas(mask, delta);
        }
    }

    /// The ped's animation clump, for code that edits several associations.
    pub fn ped_anim_mut(&mut self, r: EntityRef) -> Option<&mut AnimClump> {
        self.ped_clump_mut(r).map(|c| &mut c.anim)
    }
}

#[cfg(test)]
impl World {
    /// A mutable entity record, for tests.
    pub(crate) fn ent_mut_for_test(&mut self, r: EntityRef) -> &mut Ent {
        self.get_mut(r).expect("entity")
    }
}
