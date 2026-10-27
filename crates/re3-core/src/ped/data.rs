//! The `CPed` fields beyond the physical: state, objective, flags, weapons, timers.
use super::types::ped_type;
use crate::collision::StoredCollPoly;
use crate::entity::EntityRef;
use crate::weapons::info::weapon_type as wt;
use crate::weapons::weapon::Weapon;
use glam::{Vec2, Vec3};

pub const FEET_OFFSET: f32 = 1.04;
pub const CHECK_NEARBY_THINGS_MAX_DIST: f32 = 15.0;
pub const ENTER_CAR_MAX_DIST: f32 = 30.0;
/// `CAN_SEE_ENTITY_ANGLE_THRESHOLD`: 60 degrees.
pub const CAN_SEE_ENTITY_ANGLE_THRESHOLD: f32 = 60.0 * std::f32::consts::PI / 180.0;

pub const RANDOM_CHAR: u8 = 1;
pub const MISSION_CHAR: u8 = 2;

/// `PedState`.
pub mod state {
    pub const NONE: u8 = 0;
    pub const IDLE: u8 = 1;
    pub const LOOK_ENTITY: u8 = 2;
    pub const LOOK_HEADING: u8 = 3;
    pub const WANDER_RANGE: u8 = 4;
    pub const WANDER_PATH: u8 = 5;
    pub const SEEK_POS: u8 = 6;
    pub const SEEK_ENTITY: u8 = 7;
    pub const FLEE_POS: u8 = 8;
    pub const FLEE_ENTITY: u8 = 9;
    pub const PURSUE: u8 = 10;
    pub const FOLLOW_PATH: u8 = 11;
    pub const SNIPER_MODE: u8 = 12;
    pub const ROCKET_MODE: u8 = 13;
    pub const DUMMY: u8 = 14;
    pub const PAUSE: u8 = 15;
    pub const ATTACK: u8 = 16;
    pub const FIGHT: u8 = 17;
    pub const FACE_PHONE: u8 = 18;
    pub const MAKE_CALL: u8 = 19;
    pub const CHAT: u8 = 20;
    pub const MUG: u8 = 21;
    pub const AIM_GUN: u8 = 22;
    pub const AI_CONTROL: u8 = 23;
    pub const SEEK_CAR: u8 = 24;
    pub const SEEK_IN_BOAT: u8 = 25;
    pub const FOLLOW_ROUTE: u8 = 26;
    pub const CPR: u8 = 27;
    pub const SOLICIT: u8 = 28;
    pub const BUY_ICECREAM: u8 = 29;
    pub const INVESTIGATE: u8 = 30;
    pub const STEP_AWAY: u8 = 31;
    pub const ON_FIRE: u8 = 32;
    pub const UNKNOWN: u8 = 33;
    pub const STATES_NO_AI: u8 = 34;
    pub const JUMP: u8 = 35;
    pub const FALL: u8 = 36;
    pub const GETUP: u8 = 37;
    pub const STAGGER: u8 = 38;
    pub const DIVE_AWAY: u8 = 39;
    pub const STATES_NO_ST: u8 = 40;
    pub const ENTER_TRAIN: u8 = 41;
    pub const EXIT_TRAIN: u8 = 42;
    pub const ARREST_PLAYER: u8 = 43;
    pub const DRIVING: u8 = 44;
    pub const PASSENGER: u8 = 45;
    pub const TAXI_PASSENGER: u8 = 46;
    pub const OPEN_DOOR: u8 = 47;
    pub const DIE: u8 = 48;
    pub const DEAD: u8 = 49;
    pub const CARJACK: u8 = 50;
    pub const DRAG_FROM_CAR: u8 = 51;
    pub const ENTER_CAR: u8 = 52;
    pub const STEAL_CAR: u8 = 53;
    pub const EXIT_CAR: u8 = 54;
    pub const HANDS_UP: u8 = 55;
    pub const ARRESTED: u8 = 56;
}

/// `eMoveState`.
pub mod move_state {
    pub const NONE: u8 = 0;
    pub const STILL: u8 = 1;
    pub const WALK: u8 = 2;
    pub const RUN: u8 = 3;
    pub const SPRINT: u8 = 4;
}

/// `eWaitState`.
pub mod wait_state {
    pub const FALSE: u8 = 0;
    pub const TRAFFIC_LIGHTS: u8 = 1;
    pub const CROSS_ROAD: u8 = 2;
    pub const CROSS_ROAD_LOOK: u8 = 3;
    pub const LOOK_PED: u8 = 4;
    pub const LOOK_SHOP: u8 = 5;
    pub const LOOK_ACCIDENT: u8 = 6;
    pub const FACEOFF_GANG: u8 = 7;
    pub const DOUBLEBACK: u8 = 8;
    pub const HITWALL: u8 = 9;
    pub const TURN180: u8 = 10;
    pub const SURPRISE: u8 = 11;
    pub const STUCK: u8 = 12;
    pub const LOOK_ABOUT: u8 = 13;
    pub const PLAYANIM_DUCK: u8 = 14;
    pub const PLAYANIM_COWER: u8 = 15;
    pub const PLAYANIM_TAXI: u8 = 16;
    pub const PLAYANIM_HANDSUP: u8 = 17;
    pub const PLAYANIM_HANDSCOWER: u8 = 18;
    pub const PLAYANIM_CHAT: u8 = 19;
    pub const FINISH_FLEE: u8 = 20;
}

/// `eObjective`.
pub mod objective {
    pub const NONE: u8 = 0;
    pub const WAIT_ON_FOOT: u8 = 1;
    pub const FLEE_ON_FOOT_TILL_SAFE: u8 = 2;
    pub const GUARD_SPOT: u8 = 3;
    pub const GUARD_AREA: u8 = 4;
    pub const WAIT_IN_CAR: u8 = 5;
    pub const WAIT_IN_CAR_THEN_GET_OUT: u8 = 6;
    pub const KILL_CHAR_ON_FOOT: u8 = 7;
    pub const KILL_CHAR_ANY_MEANS: u8 = 8;
    pub const FLEE_CHAR_ON_FOOT_TILL_SAFE: u8 = 9;
    pub const FLEE_CHAR_ON_FOOT_ALWAYS: u8 = 10;
    pub const GOTO_CHAR_ON_FOOT: u8 = 11;
    pub const FOLLOW_CHAR_IN_FORMATION: u8 = 12;
    pub const LEAVE_CAR: u8 = 13;
    pub const ENTER_CAR_AS_PASSENGER: u8 = 14;
    pub const ENTER_CAR_AS_DRIVER: u8 = 15;
    pub const FOLLOW_CAR_IN_CAR: u8 = 16;
    pub const FIRE_AT_OBJECT_FROM_VEHICLE: u8 = 17;
    pub const DESTROY_OBJECT: u8 = 18;
    pub const DESTROY_CAR: u8 = 19;
    pub const GOTO_AREA_ANY_MEANS: u8 = 20;
    pub const GOTO_AREA_ON_FOOT: u8 = 21;
    pub const RUN_TO_AREA: u8 = 22;
    pub const GOTO_AREA_IN_CAR: u8 = 23;
    pub const FOLLOW_CAR_ON_FOOT_WITH_OFFSET: u8 = 24;
    pub const GUARD_ATTACK: u8 = 25;
    pub const SET_LEADER: u8 = 26;
    pub const FOLLOW_ROUTE: u8 = 27;
    pub const SOLICIT_VEHICLE: u8 = 28;
    pub const HAIL_TAXI: u8 = 29;
    pub const CATCH_TRAIN: u8 = 30;
    pub const BUY_ICE_CREAM: u8 = 31;
    pub const STEAL_ANY_CAR: u8 = 32;
    pub const MUG_CHAR: u8 = 33;
    pub const FLEE_CAR: u8 = 34;
}

/// `eFormation`.
pub mod formation {
    pub const UNDEFINED: u8 = 0;
    pub const REAR: u8 = 1;
    pub const REAR_LEFT: u8 = 2;
    pub const REAR_RIGHT: u8 = 3;
    pub const FRONT_LEFT: u8 = 4;
    pub const FRONT_RIGHT: u8 = 5;
    pub const LEFT: u8 = 6;
    pub const RIGHT: u8 = 7;
    pub const FRONT: u8 = 8;
}

/// `PedRouteType`.
pub mod route_type {
    pub const STOP_WHEN_DONE: i16 = 1;
    pub const GO_BACKWARD_WHEN_DONE: i16 = 2;
    pub const GO_TO_START_WHEN_DONE: i16 = 3;
}

/// `ePedPieceTypes`.
pub mod piece {
    pub const TORSO: u8 = 0;
    pub const MID: u8 = 1;
    pub const LEFTARM: u8 = 2;
    pub const RIGHTARM: u8 = 3;
    pub const LEFTLEG: u8 = 4;
    pub const RIGHTLEG: u8 = 5;
    pub const HEAD: u8 = 6;
}

/// `PedNode`: the skeleton frames the ped code addresses.
pub mod node {
    pub const TORSO: usize = 0;
    pub const MID: usize = 1;
    pub const HEAD: usize = 2;
    pub const UPPERARML: usize = 3;
    pub const UPPERARMR: usize = 4;
    pub const HANDL: usize = 5;
    pub const HANDR: usize = 6;
    pub const UPPERLEGL: usize = 7;
    pub const UPPERLEGR: usize = 8;
    pub const FOOTL: usize = 9;
    pub const FOOTR: usize = 10;
    pub const LOWERLEGR: usize = 11;
    pub const MAX: usize = 12;
}

/// `PedLineUpPhase`.
pub mod line_up {
    pub const TO_CAR_START: u8 = 0;
    pub const TO_CAR_END: u8 = 1;
    pub const TO_CAR_2: u8 = 2;
}

/// `PedOnGroundState`.
pub mod on_ground {
    pub const NO_PED: u8 = 0;
    pub const IN_FRONT_OF_ATTACKER: u8 = 1;
    pub const ON_THE_FLOOR: u8 = 2;
    pub const DEAD_ON_THE_FLOOR: u8 = 3;
}

/// `eEventType`.
pub mod event_type {
    pub const NULL: u32 = 0;
    pub const ASSAULT: u32 = 1;
    pub const RUN_REDLIGHT: u32 = 2;
    pub const ASSAULT_POLICE: u32 = 3;
    pub const GUNSHOT: u32 = 4;
    pub const INJURED_PED: u32 = 5;
    pub const DEAD_PED: u32 = 6;
    pub const FIRE: u32 = 7;
    pub const STEAL_CAR: u32 = 8;
    pub const HIT_AND_RUN: u32 = 9;
    pub const HIT_AND_RUN_COP: u32 = 10;
    pub const SHOOT_PED: u32 = 11;
    pub const SHOOT_COP: u32 = 12;
    pub const EXPLOSION: u32 = 13;
    pub const PED_SET_ON_FIRE: u32 = 14;
    pub const COP_SET_ON_FIRE: u32 = 15;
    pub const CAR_SET_ON_FIRE: u32 = 16;
    pub const ASSAULT_NASTYWEAPON: u32 = 17;
    pub const ICECREAM: u32 = 18;
    pub const ATM: u32 = 19;
    pub const SHOPSTALL: u32 = 20;
}

/// `FightState`.
pub mod fight_state {
    pub const MOVE_FINISHED: i8 = -2;
    pub const JUST_ATTACKED: i8 = -1;
    pub const NO_MOVE: i8 = 0;
    pub const STATE_1: i8 = 1;
}

/// `EndFight` modes.
pub const ENDFIGHT_NORMAL: u8 = 0;
pub const ENDFIGHT_WITH_A_STEP: u8 = 1;
pub const ENDFIGHT_FAST: u8 = 2;

/// `PointBlankNecessity`.
pub mod point_blank {
    pub const NONE: u8 = 0;
    pub const FOR_WANTED_PED: u8 = 1;
    pub const FOR_SOMEONE_ELSE: u8 = 2;
}

/// The `b*` bit flags of `CPed`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)] // a bit-flag set
pub struct PedFlags {
    pub is_standing: bool,
    pub was_standing: bool,
    pub is_attacking: bool,
    pub is_pointing_gun_at: bool,
    pub is_looking: bool,
    pub keep_trying_to_look: bool,
    pub is_restoring_look: bool,
    pub is_aiming_gun: bool,
    pub is_restoring_gun: bool,
    pub can_point_gun_at_target: bool,
    pub is_talking: bool,
    pub is_in_the_air: bool,
    pub is_landing: bool,
    pub is_running: bool,
    pub hit_something_last_frame: bool,
    pub veh_enter_door_is_blocked: bool,
    pub can_ped_enter_seeked_car: bool,
    pub responds_to_threats: bool,
    pub render_ped_in_car: bool,
    pub changed_seat: bool,
    pub update_anim_heading: bool,
    pub body_part_just_came_off: bool,
    pub is_shooting: bool,
    pub find_new_node_after_state_restore: bool,
    pub has_a_camera: bool,
    pub gonna_investigate_event: bool,
    pub ped_is_bleeding: bool,
    pub stop_and_shoot: bool,
    pub is_ped_die_anim_playing: bool,
    pub use_ped_node_seek: bool,
    pub objective_completed: bool,
    pub script_objective_completed: bool,
    pub kinda_stay_in_same_place: bool,
    pub being_chased_by_police: bool,
    pub not_allowed_to_duck: bool,
    pub crouch_when_shooting: bool,
    pub is_ducking: bool,
    pub get_up_anim_started: bool,
    pub do_bloody_footprints: bool,
    pub flee_after_exiting_car: bool,
    pub wander_path_after_exiting_car: bool,
    pub is_leader: bool,
    pub dont_drag_me_out_car: bool,
    pub flag_f8: bool,
    pub will_be_quick_jacked: bool,
    pub cancel_entering_car: bool,
    pub obstacle_showed_up_during_kill_objective: bool,
    pub duck_and_cover: bool,
    pub still_on_valid_poly: bool,
    pub allow_medics_to_revive_me: bool,
    pub reset_walk_anims: bool,
    pub start_wander_path_on_foot: bool,
    pub on_boat: bool,
    pub bus_jacked: bool,
    pub gonna_kill_the_car_jacker: bool,
    pub fade_out: bool,
    pub knocked_up_into_air: bool,
    pub hit_steep_slope: bool,
    pub cull_extra_far_away: bool,
    pub clear_objective: bool,
    pub trying_to_reach_dry_land: bool,
    pub collided_with_my_vehicle: bool,
    pub rich_from_mugging: bool,
    pub chris_criminal: bool,
    pub shake_fist: bool,
    pub no_critical_hits: bool,
    pub veh_exit_will_be_instant: bool,
    pub has_already_been_recorded: bool,
    pub fallen_down: bool,
    pub flag_i40: bool,
    pub flag_i80: bool,
}

/// `FightMove` row (fistfite.dat plus the animation resolved).
#[derive(Debug, Clone, Copy, Default)]
pub struct FightMove {
    pub anim_id: u32,
    pub start_fire_time: f32,
    pub end_fire_time: f32,
    pub combo_follow_on_time: f32,
    pub strike_radius: f32,
    pub hit_level: u8,
    pub damage: u8,
    pub flags: u8,
}

/// `FightMoveHitLevel`.
pub mod hit_level {
    pub const NULL: u8 = 0;
    pub const GROUND: u8 = 1;
    pub const LOW: u8 = 2;
    pub const MEDIUM: u8 = 3;
    pub const HIGH: u8 = 4;
}

/// `PedFightMoves`.
pub mod fight_move {
    pub const NULL: u32 = 0;
    pub const STDPUNCH: u32 = 1;
    pub const IDLE: u32 = 2;
    pub const SHUFFLE_F: u32 = 3;
    pub const KNEE: u32 = 4;
    pub const HEADBUTT: u32 = 5;
    pub const PUNCHJAB: u32 = 6;
    pub const PUNCHHOOK: u32 = 7;
    pub const KICK: u32 = 8;
    pub const LONGKICK: u32 = 9;
    pub const ROUNDHOUSE: u32 = 10;
    pub const BODYBLOW: u32 = 11;
    pub const GROUNDKICK: u32 = 12;
    pub const HITFRONT: u32 = 13;
    pub const HITBACK: u32 = 14;
    pub const HITRIGHT: u32 = 15;
    pub const HITLEFT: u32 = 16;
    pub const HITBODY: u32 = 17;
    pub const HITCHEST: u32 = 18;
    pub const HITHEAD: u32 = 19;
    pub const HITBIGSTEP: u32 = 20;
    pub const HITONFLOOR: u32 = 21;
    pub const HITBEHIND: u32 = 22;
    pub const IDLE2NORM: u32 = 23;
    pub const NUM: usize = 24;
}

/// Which of the `CPed` subclasses the ped is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PedClass {
    #[default]
    Civilian,
    Player,
    Cop,
    Emergency,
}

/// The ped's own record on top of entity and physical.
#[derive(Debug, Clone)]
pub struct PedData {
    pub class: PedClass,
    pub coll_poly: StoredCollPoly,
    pub collision_speed: f32,
    pub flags: PedFlags,
    pub char_created_by: u8,
    pub objective: u8,
    pub prev_objective: u8,
    pub ped_in_objective: Option<EntityRef>,
    pub car_in_objective: Option<EntityRef>,
    pub next_route_point_pos: Vec3,
    pub leader: Option<EntityRef>,
    pub ped_formation: u8,
    pub fear_flags: u32,
    pub threat_entity: Option<EntityRef>,
    pub event_or_threat: Vec2,
    pub event_type: u32,
    pub event_entity: Option<EntityRef>,
    pub angle_to_event: f32,
    pub anim_group: u32,
    pub vehicle_anim: Option<i32>,
    /// `CVisibilityPlugins::GetClumpAlpha`: 0 is invisible; the renderer fades it.
    pub clump_alpha: u8,
    pub anim_move_delta: Vec2,
    pub offset_seek: Vec3,
    pub ik: super::ik::PedIk,
    pub action_x: f32,
    pub action_y: f32,
    pub ped_state_timer: u32,
    pub ped_state: u8,
    pub last_ped_state: u8,
    pub move_state: u8,
    pub stored_move_state: i32,
    pub prev_move_state: i32,
    pub wait_state: u8,
    pub wait_timer: u32,
    pub path_node_states: [Vec2; 10],
    pub num_path_nodes: u16,
    pub cur_path_node: i16,
    pub path_dir: i8,
    pub last_path_node: Option<usize>,
    pub next_path_node: Option<usize>,
    pub health: f32,
    pub armour: f32,
    pub route_last_point: i16,
    pub route_start_point: u16,
    pub route_points_passed: i16,
    pub route_type: i16,
    pub route_points_being_passed: i16,
    pub moved: Vec2,
    pub rotation_cur: f32,
    pub rotation_dest: f32,
    pub heading_rate: f32,
    pub veh_door: u16,
    pub walk_around_type: i16,
    pub current_phys_surface: Option<EntityRef>,
    pub offset_from_phys_surface: Vec3,
    pub cur_surface: Option<EntityRef>,
    pub seek_pos: Vec3,
    pub seek_target: Option<EntityRef>,
    pub my_vehicle: Option<EntityRef>,
    pub in_vehicle: bool,
    pub distance_to_count_seek_done: f32,
    pub running_to_phone: bool,
    pub phone_id: i16,
    pub crime_to_report_on_phone: u32,
    pub phone_talk_timer: u32,
    pub last_accident: Option<usize>,
    pub ped_type: usize,
    pub ped_stats: usize,
    pub flee_from_pos_x: f32,
    pub flee_from_pos_y: f32,
    pub flee_from: Option<EntityRef>,
    pub flee_timer: u32,
    pub colliding_entity_while_fleeing: Option<EntityRef>,
    pub colliding_thing_timer: u32,
    pub colliding_entity: Option<EntityRef>,
    pub wander_range_bounds: Option<[f32; 4]>,
    pub weapons: [Weapon; wt::TOTAL_INVENTORY],
    pub stored_weapon: usize,
    pub current_weapon: u8,
    pub max_weapon_type_allowed: u8,
    pub wep_skills: u8,
    pub wep_accuracy: u8,
    pub point_gun_at: Option<EntityRef>,
    pub hit_last_pos: Vec3,
    pub cur_fight_move: u32,
    pub fight_button_pressure: u8,
    pub fight_state: i8,
    pub take_a_step_after_attack: bool,
    pub fire: Option<usize>,
    pub look_target: Option<EntityRef>,
    pub look_direction: f32,
    pub wep_model_id: i32,
    pub leave_car_timer: u32,
    pub get_up_timer: u32,
    pub look_timer: u32,
    pub chat_timer: u32,
    pub attack_timer: u32,
    pub shoot_timer: u32,
    pub car_jack_timer: u32,
    pub objective_timer: u32,
    pub duck_timer: u32,
    pub duck_and_cover_timer: u32,
    pub bloody_footprint_count_or_death_time: u32,
    pub panic_counter: u8,
    pub dead_bleeding: bool,
    pub body_part_bleeding: i8,
    pub near_peds: [Option<EntityRef>; 10],
    pub num_near_peds: u16,
    pub last_wep_dam: i8,
    pub last_sound_start: u32,
    pub sound_start: u32,
    pub last_queued_sound: u16,
    pub queued_sound: u16,
    pub seek_pos_ex: Vec3,
    pub distance_to_count_seek_done_ex: f32,
    /// `CPlayerPed` and the special peds hang more state here.
    pub player: Option<Box<super::player::PlayerData>>,
    /// `CCopPed` state.
    pub cop: Option<Box<super::cop::CopData>>,
    /// `CEmergencyPed` state.
    pub emergency: Option<Box<super::emergency::EmergencyData>>,
    pub clump: Option<super::clump::PedClump>,
}

pub const NO_SOUND: u16 = crate::audio::sound::NO_SOUND;
pub const NO_WEAPON: usize = wt::UNIDENTIFIED;

impl PedData {
    /// `CPed::CPed(pedType)` (the part that doesn't touch the world). `rand_camera` is `GetRandomNumber() & 3 == 0`.
    pub fn new(ped_type: usize, fear_flags: u32, rand_camera: bool) -> Self {
        let flags = PedFlags { responds_to_threats: true, render_ped_in_car: true, allow_medics_to_revive_me: true, has_a_camera: rand_camera, ..Default::default() };
        let mut d = Self {
            class: PedClass::Civilian,
            coll_poly: StoredCollPoly::default(),
            collision_speed: 0.0,
            flags,
            char_created_by: RANDOM_CHAR,
            objective: objective::NONE,
            prev_objective: objective::NONE,
            ped_in_objective: None,
            car_in_objective: None,
            next_route_point_pos: Vec3::ZERO,
            leader: None,
            ped_formation: formation::UNDEFINED,
            fear_flags,
            threat_entity: None,
            event_or_threat: Vec2::ZERO,
            event_type: 0,
            event_entity: None,
            angle_to_event: 0.0,
            anim_group: 0,
            vehicle_anim: None,
            clump_alpha: 255,
            anim_move_delta: Vec2::ZERO,
            offset_seek: Vec3::ZERO,
            ik: Default::default(),
            action_x: 0.0,
            action_y: 0.0,
            ped_state_timer: 0,
            ped_state: state::IDLE,
            last_ped_state: state::NONE,
            move_state: move_state::STILL,
            stored_move_state: move_state::NONE as i32,
            prev_move_state: move_state::NONE as i32,
            wait_state: wait_state::FALSE,
            wait_timer: 0,
            path_node_states: [Vec2::ZERO; 10],
            num_path_nodes: 0,
            cur_path_node: 0,
            path_dir: 0,
            last_path_node: None,
            next_path_node: None,
            health: 100.0,
            armour: 0.0,
            route_last_point: -1,
            route_start_point: 0,
            route_points_passed: 0,
            route_type: 0,
            route_points_being_passed: 0,
            moved: Vec2::ZERO,
            rotation_cur: 0.0,
            rotation_dest: 0.0,
            heading_rate: 15.0,
            veh_door: crate::vehicle::automobile::node::DOOR_LF as u16,
            walk_around_type: 0,
            current_phys_surface: None,
            offset_from_phys_surface: Vec3::ZERO,
            cur_surface: None,
            seek_pos: Vec3::ZERO,
            seek_target: None,
            my_vehicle: None,
            in_vehicle: false,
            distance_to_count_seek_done: 1.0,
            running_to_phone: false,
            phone_id: -1,
            crime_to_report_on_phone: 0,
            phone_talk_timer: 0,
            last_accident: None,
            ped_type,
            ped_stats: 0,
            flee_from_pos_x: 0.0,
            flee_from_pos_y: 0.0,
            flee_from: None,
            flee_timer: 0,
            colliding_entity_while_fleeing: None,
            colliding_thing_timer: 0,
            colliding_entity: None,
            wander_range_bounds: None,
            weapons: std::array::from_fn(|_| Weapon::default()),
            stored_weapon: wt::UNIDENTIFIED,
            current_weapon: wt::UNARMED as u8,
            max_weapon_type_allowed: wt::UNARMED as u8,
            wep_skills: 0,
            wep_accuracy: 60,
            point_gun_at: None,
            hit_last_pos: Vec3::ZERO,
            cur_fight_move: fight_move::NULL,
            fight_button_pressure: 0,
            fight_state: 0,
            take_a_step_after_attack: false,
            fire: None,
            look_target: None,
            look_direction: 0.0,
            wep_model_id: -1,
            leave_car_timer: 0,
            get_up_timer: 0,
            look_timer: 0,
            chat_timer: 0,
            attack_timer: 0,
            shoot_timer: 0,
            car_jack_timer: 0,
            objective_timer: 0,
            duck_timer: 0,
            duck_and_cover_timer: 0,
            bloody_footprint_count_or_death_time: 0,
            panic_counter: 0,
            dead_bleeding: false,
            body_part_bleeding: -1,
            near_peds: [None; 10],
            num_near_peds: 0,
            last_wep_dam: -1,
            last_sound_start: 0,
            sound_start: 0,
            last_queued_sound: NO_SOUND,
            queued_sound: NO_SOUND,
            seek_pos_ex: Vec3::ZERO,
            distance_to_count_seek_done_ex: 0.0,
            player: None,
            cop: None,
            emergency: None,
            clump: None,
        };
        for w in &mut d.weapons {
            w.ty = wt::UNARMED;
        }
        d
    }

    /// `IsPlayer`: any of the four player types.
    pub fn is_player(&self) -> bool {
        self.ped_type <= ped_type::PLAYER4
    }

    /// `IsGangMember`.
    pub fn is_gang_member(&self) -> bool {
        (ped_type::GANG1..=ped_type::GANG9).contains(&self.ped_type)
    }

    /// `IsPedInControl`.
    pub fn is_ped_in_control(&self) -> bool {
        self.ped_state <= state::STATES_NO_AI && !self.flags.is_in_the_air && !self.flags.is_landing && self.health > 0.0
    }

    /// `IsPedShootable`.
    pub fn is_ped_shootable(&self) -> bool {
        self.ped_state <= state::STATES_NO_ST
    }

    /// `CanPedReturnToState`.
    pub fn can_ped_return_to_state(&self) -> bool {
        self.ped_state <= state::STATES_NO_AI
            && !matches!(self.ped_state, state::AIM_GUN | state::ATTACK | state::FIGHT | state::STEP_AWAY | state::SNIPER_MODE | state::LOOK_ENTITY)
    }

    /// `CanSetPedState`.
    pub fn can_set_ped_state(&self) -> bool {
        !self.dying_or_dead() && self.ped_state != state::ARRESTED && !self.entering_car() && self.ped_state != state::STEAL_CAR
    }

    /// `CanStrafeOrMouseControl` (the free camera is not ported).
    pub fn can_strafe_or_mouse_control(&self) -> bool {
        matches!(self.ped_state, state::NONE | state::IDLE | state::FLEE_POS | state::FLEE_ENTITY | state::ATTACK | state::FIGHT | state::AIM_GUN | state::JUMP)
    }

    /// `CanBeDeleted`.
    pub fn can_be_deleted(&self) -> bool {
        !self.in_vehicle && self.char_created_by != MISSION_CHAR
    }

    /// `IsTemporaryObjective`.
    pub fn is_temporary_objective(o: u8) -> bool {
        matches!(o, objective::LEAVE_CAR | objective::SET_LEADER | objective::ENTER_CAR_AS_DRIVER | objective::ENTER_CAR_AS_PASSENGER)
    }

    /// `SetObjectiveTimer` against the current time.
    pub fn set_objective_timer(&mut self, time: i32, now: u32) {
        if time == 0 {
            self.objective_timer = 0;
        } else if now > self.objective_timer {
            self.objective_timer = now + time as u32;
        }
    }

    /// `SetStoredObjective`.
    pub fn set_stored_objective(&mut self) {
        if self.objective == self.prev_objective {
            return;
        }
        match self.objective {
            objective::FLEE_ON_FOOT_TILL_SAFE
            | objective::KILL_CHAR_ON_FOOT
            | objective::KILL_CHAR_ANY_MEANS
            | objective::FLEE_CHAR_ON_FOOT_TILL_SAFE
            | objective::FLEE_CHAR_ON_FOOT_ALWAYS
            | objective::GOTO_CHAR_ON_FOOT
            | objective::FOLLOW_CHAR_IN_FORMATION
            | objective::LEAVE_CAR
            | objective::ENTER_CAR_AS_PASSENGER
            | objective::ENTER_CAR_AS_DRIVER
            | objective::GOTO_AREA_ON_FOOT
            | objective::RUN_TO_AREA => {}
            _ => self.prev_objective = self.objective,
        }
    }

    /// `ForceStoredObjective`.
    pub fn force_stored_objective(&mut self, new: u8) {
        if new != objective::ENTER_CAR_AS_DRIVER && new != objective::ENTER_CAR_AS_PASSENGER {
            self.prev_objective = self.objective;
            return;
        }
        match self.objective {
            objective::FLEE_ON_FOOT_TILL_SAFE
            | objective::KILL_CHAR_ON_FOOT
            | objective::FLEE_CHAR_ON_FOOT_TILL_SAFE
            | objective::FLEE_CHAR_ON_FOOT_ALWAYS
            | objective::GOTO_CHAR_ON_FOOT
            | objective::ENTER_CAR_AS_PASSENGER
            | objective::ENTER_CAR_AS_DRIVER
            | objective::GOTO_AREA_ON_FOOT
            | objective::RUN_TO_AREA => {}
            _ => self.prev_objective = self.objective,
        }
    }

    /// `SetMoveState`.
    pub fn set_move_state(&mut self, s: u8) {
        self.move_state = s;
    }

    /// `SetStoredState`.
    pub fn set_stored_state(&mut self) {
        if self.last_ped_state != state::NONE || !self.can_ped_return_to_state() {
            return;
        }
        if self.ped_state == state::WANDER_PATH {
            self.flags.find_new_node_after_state_restore = true;
            if self.move_state == move_state::NONE || self.move_state == move_state::STILL {
                self.move_state = move_state::WALK;
            }
        }
        self.last_ped_state = self.ped_state;
        if self.move_state as i32 >= self.prev_move_state {
            self.prev_move_state = self.move_state as i32;
        }
    }

    /// `ClearAimFlag` for the ped's own flags.
    pub fn clear_aim_flag(&mut self) {
        if self.flags.is_aiming_gun {
            self.flags.is_aiming_gun = false;
            self.flags.is_restoring_gun = true;
            self.ik.flags &= !super::ik::ik_flag::AIMS_WITH_ARM;
            self.look_timer = 0;
        }
    }

    /// `SetAimFlag(angle)`; `can_aim_with_arm` from the weapon's flags.
    pub fn set_aim_flag_angle(&mut self, angle: f32, can_aim_with_arm: bool) {
        self.flags.is_aiming_gun = true;
        self.flags.is_restoring_gun = false;
        self.look_direction = angle;
        self.look_timer = 0;
        self.look_target = None;
        self.seek_target = None;
        if can_aim_with_arm {
            self.ik.flags |= super::ik::ik_flag::AIMS_WITH_ARM;
        } else {
            self.ik.flags &= !super::ik::ik_flag::AIMS_WITH_ARM;
        }
    }

    /// `SetAimFlag(entity)`.
    pub fn set_aim_flag_entity(&mut self, to: EntityRef) {
        self.flags.is_aiming_gun = true;
        self.flags.is_restoring_gun = false;
        self.look_target = Some(to);
        self.seek_target = Some(to);
        self.look_timer = 0;
    }

    /// `SetLookFlag(direction)`.
    pub fn set_look_flag_direction(&mut self, direction: f32, keep_trying: bool, now: u32) {
        if self.look_timer < now {
            self.flags.is_looking = true;
            self.flags.is_restoring_look = false;
            self.look_target = None;
            self.look_direction = direction;
            self.look_timer = 0;
            self.flags.keep_trying_to_look = keep_trying;
            if self.ped_state != state::DRIVING {
                self.ik.flags &= !super::ik::ik_flag::LOOKAROUND_HEAD_ONLY;
            }
        }
    }

    /// `SetLookFlag(entity)`.
    pub fn set_look_flag_entity(&mut self, target: EntityRef, keep_trying: bool, now: u32) {
        if self.look_timer < now {
            self.flags.is_looking = true;
            self.flags.is_restoring_look = false;
            self.look_target = Some(target);
            self.look_direction = 999999.0;
            self.look_timer = 0;
            self.flags.keep_trying_to_look = keep_trying;
            if self.ped_state != state::DRIVING {
                self.ik.flags &= !super::ik::ik_flag::LOOKAROUND_HEAD_ONLY;
            }
        }
    }

    /// `CanSeeEntity`'s angle test given the angle from the target to the ped.
    pub fn can_see_angle(&self, needed_angle: f32, threshold: f32) -> bool {
        use std::f32::consts::TAU;
        let mut needed = needed_angle;
        if needed < 0.0 {
            needed += TAU;
        } else if needed > TAU {
            needed -= TAU;
        }
        let mut ours = self.rotation_cur;
        if ours < 0.0 {
            ours += TAU;
        } else if ours > TAU {
            ours -= TAU;
        }
        let turn = (needed - ours).abs();
        turn < threshold || TAU - threshold < turn
    }

    pub fn dead(&self) -> bool {
        self.ped_state == state::DEAD
    }

    pub fn dying(&self) -> bool {
        self.ped_state == state::DIE
    }

    pub fn dying_or_dead(&self) -> bool {
        self.ped_state == state::DIE || self.ped_state == state::DEAD
    }

    pub fn on_ground(&self) -> bool {
        self.ped_state == state::FALL || self.ped_state == state::DIE || self.ped_state == state::DEAD
    }

    pub fn on_ground_or_getting_up(&self) -> bool {
        self.on_ground() || self.ped_state == state::GETUP
    }

    pub fn driving(&self) -> bool {
        self.ped_state == state::DRIVING
    }

    pub fn entering_car(&self) -> bool {
        self.ped_state == state::ENTER_CAR || self.ped_state == state::CARJACK
    }

    /// `UseGroundColModel`.
    pub fn use_ground_col_model(&self) -> bool {
        matches!(self.ped_state, state::FALL | state::DIVE_AWAY | state::DIE | state::DEAD)
    }

    pub fn has_weapon(&self, t: usize) -> bool {
        self.weapons[t].ty == t
    }

    pub fn in_vehicle_now(&self) -> bool {
        self.in_vehicle && self.my_vehicle.is_some()
    }
}

impl Default for PedData {
    fn default() -> Self {
        Self::new(ped_type::CIVMALE, 0, false)
    }
}
