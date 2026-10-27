//! What a vehicle needs to know about the rest of the game, and what it asks of it.
//! The original reaches for globals (the pad, the player, particles, audio); here the caller
//! supplies the inputs and collects the requests as events.
use crate::entity::EntityRef;
use crate::vehicle::base::{PadState, VehicleEvent};
use glam::Vec3;

/// `CPlayerInfo` and `FindPlayer*` as the vehicles read them.
#[derive(Debug, Clone, Default)]
pub struct PlayerView {
    pub vehicle: Option<EntityRef>,
    pub ped: Option<EntityRef>,
    pub coors: Vec3,
    pub speed: Vec3,
    pub wanted_level: i32,
    /// `m_pRemoteVehicle`.
    pub remote_vehicle: Option<EntityRef>,
    /// `m_WBState == WBSTATE_PLAYING`.
    pub playing: bool,
    pub time_tank_shot_gun: u32,
    pub looking_forward_first_person: bool,
    /// The player's vehicle is a car, its alarm is on, and where it is (`ScanForCrimes`).
    pub vehicle_is_car: bool,
    pub vehicle_alarm_on: bool,
    pub vehicle_pos: Vec3,
    pub vehicle_move_speed: f32,
    /// The wanted level system as the helicopters read it.
    pub wanted_ignored: bool,
    pub num_helis_required: i32,
    pub no_rain_zone: bool,
    pub no_police_zone: bool,
    pub replay_playing: bool,
    pub chopper_loaded: bool,
    pub escape_loaded: bool,
    /// The player ped is entering (or carjacking) this vehicle (`PED_ENTER_CAR` + seek target).
    pub entering_vehicle: Option<EntityRef>,
    /// Position the game camera looks from (`GetGameCamPosition`).
    pub game_cam_pos: Vec3,
}

/// The vehicle cheats (`CVehicle::bCheat*`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Cheats {
    pub wheels_only: bool,
    pub all_dodos: bool,
    pub cheat3: bool,
    pub cheat5: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleKind {
    EngineSteam,
    EngineSmoke,
    EngineSmoke2,
    CarFlame,
    CarFlameSmoke,
    SparkSmall,
    CarCollisionDust,
    CarDebris,
    WheelDirt,
    WheelWater,
    WaterSpray,
    PedSplash,
    RubberSmoke,
    HeliDust,
    GunSmoke2,
    GunFlash,
    RainSplashBiggrow,
    CarSplash,
    BoatThrustJet,
    BoatWake,
    BoatSplash,
    HeliDebris,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub kind: ParticleKind,
    pub pos: Vec3,
    pub dir: Vec3,
    pub size: f32,
    pub colour: Option<[u8; 4]>,
    pub rotation_speed: f32,
    pub rotation: f32,
    pub frame: i32,
    pub life: i32,
}

impl Particle {
    pub fn new(kind: ParticleKind, pos: Vec3, dir: Vec3, size: f32) -> Self {
        Self { kind, pos, dir, size, colour: None, rotation_speed: 0.0, rotation: 0.0, frame: 0, life: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarSound {
    BombTick,
    BombTimedActivated,
    BombOnIgnitionActivated,
    Hydraulic1,
    Hydraulic2,
    Hydraulic3,
    CarJump,
    WaterFall,
    CarSplash,
    WindshieldCrack,
    /// Light index that broke.
    LightBreak(u8),
    DoorOpen(u8),
    DoorClose(u8),
    TankTurretRotate,
    PedBodycastHit,
    Splatter,
    AnnoyedDriver,
    CranePickup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplosionKind {
    Car,
    CarQuick,
    TankGrenade,
}

/// A request from a car to the rest of the game.
#[derive(Debug, Clone, PartialEq)]
pub enum CarEvent {
    Vehicle(VehicleEvent),
    Sound { sound: CarSound, volume: f32 },
    Particle(Particle),
    /// `CPad::StartShake(duration, frequency)`.
    PadShake { duration: f32, freq: u8 },
    Explosion { kind: ExplosionKind, pos: Vec3, culprit: Option<EntityRef> },
    StartFire { culprit: Option<EntityRef> },
    CamShake { amount: f32, pos: Vec3 },
    /// The occupants of an exploding car die; `register_kill` is for the kill counter.
    KillOccupants,
    Money(i32),
    TriggerMessage(&'static str),
    /// `CWaterCannons::UpdateOne`.
    WaterCannon { pos: Vec3, dir: Vec3 },
    /// `CRemote::TakeRemoteControlledCarFromPlayer`.
    TakeRemoteControl,
    /// `CPopulation`/`CGarages` queries answered by the caller are hooks, not events; this
    /// asks the player's ped to forget wanted-level drops.
    AwardMoneyForExplosion,
    /// Drive-by shooting step (`DoDriveByShootings`) for the driver.
    DriveBy { looking_left: bool, looking_right: bool },
    /// A part flies off (`SpawnFlyingComponent`).
    FlyingComponent { car: EntityRef, component: usize, group: u8 },
    MakeWayForSiren,
    /// A car stopped being on duty or a law enforcer: the counters of `CCarCtrl` drop.
    DutyEnded { ambulance: bool, firetruck: bool, law_enforcer: bool },
    /// `FireOneInstantHitRound`.
    FireInstantHit { source: Vec3, target: Vec3, damage: i32 },
    /// A helicopter part comes loose.
    HeliComponent { heli: EntityRef, component: usize },
    /// The player shot a helicopter down: kill counts, money, and the crime.
    HeliDestroyed { slot: usize, pos: Vec3 },
    /// `CExplosion::AddExplosion(EXPLOSION_HELI / HELI_BOMB)`.
    HeliExplosion { pos: Vec3, bomb: bool, culprit: Option<EntityRef> },
    PoliceHeliPlayerFound,
    /// `TheCamera.SetFadeColour` + a fade out and back in.
    CameraFlash { rgb: [u8; 3] },
    /// `RegisterCrime_Immediately`: the player shot down a Cessna.
    CrimeDestroyedCessna { pos: Vec3, id: i32 },
    PlaneOnGround,
    /// `StartShake_Train` at this position.
    TrainShake { x: f32, y: f32 },
    TrainDoor { opening: bool },
    TrainAnnouncement { arriving: bool, station: usize },
    WeaponShotFired,
    /// The Predator's fixed guns (`DoFixedMachineGuns`) for the weapon code to fire.
    FixedMachineGuns,
    /// The player's tank fired from `origin` along `dir` (60 units): the weapon code resolves the shot.
    TankFire { origin: Vec3, dir: Vec3, moving_fast: bool, move_speed: Vec3 },
    /// An occupant takes drowning damage.
    Drown { occupant: EntityRef, is_driver: bool, water_tight: bool, amount: f32 },
    /// `POBJECT_CAR_WATER_SPLASH`.
    CarWaterSplash { pos: Vec3, up_speed: f32, alpha: u8 },
    /// `POBJECT_SPLASHES_AROUND`.
    SplashesAround { pos: Vec3 },
}

/// The pad plus the history the horn logic reads.
#[derive(Debug, Clone, Default)]
pub struct Pad {
    pub state: PadState,
    pub hand_brake: bool,
    pub exit_vehicle: bool,
    pub horn_just_down: bool,
    pub car_gun_just_down: bool,
    pub car_gun_fired: bool,
    pub circle_just_down: bool,
    pub controls_disabled: bool,
    /// Horn button, newest last; `[len-1]` is this frame.
    pub horn_history: [bool; 16],
    /// Camera-driven look direction for drive-bys and weapon doors.
    pub looking_left: bool,
    pub looking_right: bool,
}

/// Everything outside the vehicle that `ProcessControl` consults.
#[derive(Debug, Default)]
pub struct CarEnv {
    pub pad: Pad,
    pub player: PlayerView,
    pub cheats: Cheats,
    pub events: Vec<CarEvent>,
    /// Time cycle light (ambient and directional RGB, 0..1), for particle colours.
    pub ambient: [f32; 3],
    pub directional: [f32; 3],
    pub camera_pos: Vec3,
    /// The camera is in top-down mode (`MODE_TOPDOWN` or Obbe in control).
    pub camera_topdown: bool,
    /// `TheCamera.GenerationDistMultiplier` (1.0 normally).
    pub generation_dist_multiplier: f32,
    /// `FindPlayerCentreOfWorld`.
    pub player_centre: Vec3,
}

/// Game systems the vehicle code calls into (car AI, path finding, zones). Defaults do nothing.
pub trait CarHooks {
    fn update_car_ai(&mut self, _car: &mut crate::entity::Ent) {}
    fn update_car_on_rails(&mut self, _car: &mut crate::entity::Ent) {}
    fn steer_ai_car_with_physics(&mut self, _car: &mut crate::entity::Ent) {}
    fn scan_for_ped_danger(&mut self, _car: &mut crate::entity::Ent) {}
    /// `CCarCtrl::MapCouldMoveInThisArea`.
    fn map_could_move_in_this_area(&mut self, _x: f32, _y: f32) -> bool {
        false
    }
    /// `CPopulation::IsPointInSafeZone`.
    fn is_point_in_safe_zone(&mut self, _pos: Vec3) -> bool {
        false
    }
    /// `HasCarStoppedBecauseOfLight` (needs the path nodes).
    fn car_stopped_because_of_light(&mut self, _car: &crate::entity::Ent) -> bool {
        false
    }
    /// `gFireManager.FindFurthestFire_NeverMindFireMen(pos, 10, 35)`.
    fn find_furthest_fire(&mut self, _pos: Vec3) -> Option<Vec3> {
        None
    }
    /// Whether an occupant may be told to leave a flipped car (not the player, not a mission ped,
    /// not following a leader who is in a vehicle).
    fn occupant_can_be_ordered_out(&mut self, _ped: EntityRef) -> bool {
        true
    }
    /// Whether the driver is a player-controlled ped who is not exiting or being dragged out.
    fn driver_controlled_by_player(&mut self, _ped: EntityRef) -> bool {
        false
    }
    /// The player is recording a chase (`CRecordDataForChase::IsRecording`).
    fn recording_chase(&mut self) -> bool {
        false
    }
    /// `CCarCtrl::NumParkedCars`.
    fn num_parked_cars(&mut self) -> i32 {
        0
    }
    /// Streaming: ask for a model and say whether it is loaded.
    fn request_model(&mut self, _model: i32) {}
    fn model_loaded(&mut self, _model: i32) -> bool {
        true
    }
    /// `CCarCtrl::SteerAIBoatWithPhysics`.
    fn steer_ai_boat_with_physics(&mut self, _boat: &mut crate::entity::Ent) {}
}

/// Hooks that do nothing.
pub struct NoHooks;

impl CarHooks for NoHooks {}
