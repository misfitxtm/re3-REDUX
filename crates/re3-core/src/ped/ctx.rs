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
    /// `CTheScripts::IsPlayerOnAMission`.
    fn player_on_mission(&mut self) -> bool {
        false
    }
    /// `CCarCtrl::JoinCarWithRoadSystem`.
    fn join_car_with_road_system(&mut self, _veh: EntityRef) {}
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
    /// `DMAudio.IsMP3RadioChannelAvailable`.
    fn mp3_radio_available(&mut self) -> bool {
        false
    }
    /// A permanent dark blood-pool shadow where a bloody foot left the ground.
    fn add_blood_footprint(&mut self, _pos: Vec3, _forward: Vec3, _right: Vec3) {}
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
    /// `DMAudio.PlayFrontEndSound(sound, param)` for the sounds that carry a number (clock ticks, rampage time).
    fn front_end_sound_with(&mut self, _sound: u16, _param: i32) {}
    /// One more of a game statistic.
    fn stat(&mut self, _stat: StatEvent) {}
    /// `CTimer::SetTimeScale`.
    fn set_time_scale(&mut self, _scale: f32) {}
    /// `CCarCtrl::PossiblyRemoveVehicle`.
    fn possibly_remove_vehicle(&mut self, _veh: EntityRef) {}
    /// A visual, sound or stat effect of a shot.
    fn shot_effect(&mut self, _effect: ShotEffect) {}
    /// `TheCamera.Find3rdPersonCamTargetVector`: where the mouse-aimed shot starts and ends.
    fn cam_target_vector(&mut self, _range: f32, source: Vec3) -> (Vec3, Vec3) {
        (source, source)
    }
    /// `CGarages::IsPointWithinHideOutGarage`.
    fn point_in_hide_out_garage(&mut self, _pos: Vec3) -> bool {
        false
    }
    /// `CStreaming::RequestModel` for a model the generation code wants soon.
    fn request_model(&mut self, _model: i16) {}
    /// `CStreaming::SetModelIsDeletable`.
    fn set_model_is_deletable(&mut self, _model: i16) {}
    /// `TheCamera.SetWideScreenOn/Off`.
    fn camera_widescreen(&mut self, _on: bool) {}
    /// `CPad::SetEnablePlayerControls` / `SetDisablePlayerControls` for `PLAYERCONTROL_PHONE`.
    fn phone_controls(&mut self, _enabled: bool) {}
    /// `PlayOneShotScriptObject`.
    fn play_one_shot_script_object(&mut self, _sound: u16, _pos: Vec3) {}
    /// `CPad::SetEnablePlayerControls` / `SetDisablePlayerControls` for `PLAYERCONTROL_GARAGE`.
    fn garage_controls(&mut self, _enabled: bool) {}
    /// `DMAudio.PlayOneShot(hGarages, sound, 1.0)`.
    fn garage_sound(&mut self, _sound: u16) {}
    /// `CHud::SetHelpMessage(TheText.Get(key), false)`.
    fn help_message(&mut self, _key: &str) {}
    /// `CPad::GetPad(0)->Mode`: the control layout.
    fn pad_mode(&mut self) -> i16 {
        0
    }
    /// One puff of the respray: `CParticle::AddParticle(PARTICLE_GARAGEPAINT_SPRAY, ..., ms_vehicleColourTable[colour])`.
    fn garage_paint_spray(&mut self, _pos: Vec3, _colour: u8) {}
    /// `TheCamera.SetCameraDirectlyBehindForFollowPed_CamOnAString`.
    fn camera_directly_behind_ped(&mut self) {}
    /// `TheCamera.SetCamPositionForFixedMode` and `TakeControl(player car, MODE_FIXED, JUMP_CUT, SCRIPT)`.
    fn camera_fixed_for_pickup(&mut self, _pos: Vec3) {}
    /// `TheCamera.RestoreWithJumpCut`.
    fn camera_restore_with_jump_cut(&mut self) {}
    /// `CMessages::AddMessage(text, time, 0)`.
    fn add_message(&mut self, _text: &str, _time: u32) {}
    /// `TheCamera.m_CameraAverageSpeed`.
    fn camera_average_speed(&mut self) -> f32 {
        0.0
    }
    /// `TheCamera.GenerationDistMultiplier`.
    fn generation_dist_multiplier(&mut self) -> f32 {
        1.0
    }
    /// The camera's position while `CCarCtrl::bCarsGeneratedAroundCamera` is set.
    fn camera_generation_centre(&mut self) -> Option<Vec3> {
        None
    }
    /// The model's clump is loaded (`GetRwObject()`).
    fn model_loaded(&mut self, _model: i16) -> bool {
        true
    }
    /// A fresh skeleton for a ped of `model`; None leaves the ped without a clump.
    fn load_ped_clump(&mut self, _model: i16) -> Option<super::clump::PedClump> {
        None
    }
    /// `CCutsceneMgr::IsRunning`.
    fn cutscene_running(&mut self) -> bool {
        false
    }
    /// False while the camera is in a sniper mode or the player looks sideways or back (`CCam::Mode`, `LookingLeft/Right/Behind`).
    fn camera_allows_offscreen_despawn(&mut self) -> bool {
        true
    }
    /// `CTheScripts::ClearSpaceForMissionEntity`.
    fn clear_space_for_mission_entity(&mut self, _pos: Vec3, _entity: EntityRef) {}
    /// `CShadows::AddPermanentShadow` with outline texture `outline` (0..3) where a dead ped far away is removed.
    fn add_dead_ped_outline(&mut self, _pos: Vec3, _outline: u8, _angle: f32) {}
    /// The camera work when the remote-control car is gone.
    fn remote_mode_camera(&mut self, _step: super::player_info::RemoteCamera) {}
    /// `CStats::DistanceTravelledInVehicle` / `OnFoot`.
    fn add_distance_travelled(&mut self, _in_vehicle: bool, _distance: f32) {}
    /// The WASTED or BUSTED message and the stat (the kill frenzy has already been reset).
    fn player_wasted_or_busted(&mut self, _busted: bool) {}
    /// Pad controls off (and shaking stopped, replays off, the unsafe countdown reset) while safe, back on after.
    fn set_player_safe_controls(&mut self, _safe: bool) {}
    /// `!CGame::playingIntro && !CTheScripts::IsCountdownToMakePlayerUnsafeOn()`.
    fn can_make_player_unsafe(&mut self) -> bool {
        true
    }
    /// `CProjectileInfo::RemoveAllProjectiles`.
    fn remove_all_projectiles(&mut self) {}
    /// `TheCamera.TakeControl(car, MODE_BEHINDCAR, INTERPOLATION, CAMCONTROL_SCRIPT)`.
    fn camera_take_control_behind_car(&mut self, _veh: EntityRef) {}
    /// `CMoneyMessages::RegisterOne` for a wreck's reward.
    fn money_message(&mut self, _pos: Vec3, _amount: i32) {}
    /// `GetPedWalkLeftRight` and `GetPedWalkUpDown` of pad 0.
    fn ped_walk_axes(&mut self) -> (f32, f32) {
        (0.0, 0.0)
    }
}

/// The game statistics (`CStats`) the code counts up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatEvent {
    PeopleKilledByPlayer,
    PeopleKilledByOthers,
    CarsExploded,
    /// `PedsKilledOfThisType[type]++`.
    PedKilledOfType(usize),
    HeadsPopped,
    KillsSinceLastCheckpoint,
    /// `AnotherKillFrenzyPassed`.
    KillFrenzyPassed,
    CheckPointReachedSuccessfully,
    KgsOfExplosivesUsed(i32),
    CarsCrushed,
}

/// What a shot asks the renderer, audio and stats for. The random numbers the original draws for them are drawn before.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShotEffect {
    /// The light and the flash particles at the muzzle; `shooter_velocity` is set when the shooter moves.
    MuzzleFlash { weapon: usize, pos: Vec3, ahead: glam::Vec2, shooter_velocity: Option<Vec3> },
    GunSmoke { weapon: usize, pos: Vec3, ahead: glam::Vec2, speed: f32 },
    GunShell { pos: Vec3, velocity: Vec3, size: f32, rotation: f32, first: bool },
    Trace { from: Vec3, to: Vec3 },
    Sparks { pos: Vec3, dir: Vec3, count: u8 },
    BulletHitSmoke { pos: Vec3 },
    BloodSmall { pos: Vec3, dir: Vec3, count: u8 },
    /// `CGlass::WasGlassHitByBullet`.
    GlassHit { victim: EntityRef, pos: Vec3 },
    /// `DMAudio.PlayOneShot` on an entity's audio.
    Sound { entity: EntityRef, sound: u16 },
    /// `PlayOneShotScriptObject`.
    ScriptSound { sound: u16, pos: Vec3 },
    InstantHitFiredByPlayer,
    InstantHitHitByPlayer,
    RoundFiredByPlayer,
    GrenadeThrown,
    /// Five fireballs from a flamethrower, each slower than the last.
    Flames { pos: Vec3, dir: Vec3 },
    /// `CCoronas::bSmallMoon` flips when the sniper aims at it.
    SniperMoon,
    /// The first-person camera kicks back (`Beta` and `Alpha` change by these).
    CameraKick { beta: f32, alpha: f32 },
    /// `CamShakeNoPos`.
    CamShake(f32),
    /// A rocket's smoke trail puff.
    RocketSmoke { pos: Vec3 },
    /// `CPad::StartShake_Distance`.
    PadShake { duration: u32, frequency: u8, pos: Vec3 },
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
    /// `GetExitVehicle`: the enter/exit key is held.
    pub exit_vehicle: bool,
    pub exit_vehicle_just_down: bool,
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
    /// The active cam's `Source`, `Front` and `Up`.
    pub source: Vec3,
    pub front: Vec3,
    pub up: Vec3,
    /// The active cam mode is one of the first-person weapon modes (M16, sniper, rocket launcher, with their runabouts).
    pub weapon_first_person: bool,
    /// The active cam mode is the heli cannon's first person view.
    pub heli_cannon_first_person: bool,
    /// `GetLookingLRBFirstPerson`: the first-person camera looks left, right or behind.
    pub looking_lrb_first_person: bool,
    /// The active cam looks left or right (`LookingLeft`, `LookingRight`).
    pub looking_left_or_right: bool,
    /// `TheCamera.GetLookDirection() == 0`.
    pub look_direction_zero: bool,
    /// `TheCamera.GetForward()`, and `CamFrontXNorm` / `CamFrontYNorm` (the camera's front on the ground, normalised).
    pub forward: Vec3,
    pub front_norm: glam::Vec2,
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
