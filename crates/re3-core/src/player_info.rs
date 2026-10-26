//! `CPlayerInfo`: the player's money, wasted/busted state and the other facts that outlive the ped.
//! The collision model for hydraulic cars and the car using it live in `Ents`.
use crate::entity::EntityRef;

/// `eWastedBustedState`.
pub mod wb_state {
    pub const PLAYING: i8 = 0;
    pub const WASTED: i8 = 1;
    pub const BUSTED: i8 = 2;
    pub const FAILED_CRITICAL_MISSION: i8 = 3;
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub remote_vehicle: Option<EntityRef>,
    pub player_name: String,
    pub money: i32,
    pub visible_money: i32,
    pub collected_packages: i32,
    pub total_packages: i32,
    pub last_bump_player_car_timer: u32,
    pub unused_taxi_timer: u32,
    pub unused_taxi_thing: bool,
    pub next_sex_frequency_update_time: u32,
    pub next_sex_money_update_time: u32,
    pub sex_frequency: i32,
    pub hooker: Option<EntityRef>,
    pub wb_state: i8,
    pub wb_time: u32,
    pub in_remote_mode: bool,
    pub time_lost_remote_car: u32,
    pub time_last_health_loss: u32,
    pub time_last_armour_loss: u32,
    pub time_tank_shot_gun: u32,
    pub upside_down_counter: i32,
    pub traffic_multiplier: i16,
    pub road_density: f32,
    pub previous_time_rewarded_for_explosion: u32,
    pub explosions_since_last_reward: i32,
    pub infinite_sprint: bool,
    pub fast_reload: bool,
    pub get_out_of_jail_free: bool,
    pub get_out_of_hospital_free: bool,
}

impl Default for PlayerInfo {
    /// `CPlayerInfo::Clear`.
    fn default() -> Self {
        Self {
            remote_vehicle: None,
            player_name: String::new(),
            money: 0,
            visible_money: 0,
            collected_packages: 0,
            total_packages: 3,
            last_bump_player_car_timer: 0,
            unused_taxi_timer: 0,
            unused_taxi_thing: false,
            next_sex_frequency_update_time: 0,
            next_sex_money_update_time: 0,
            sex_frequency: 0,
            hooker: None,
            wb_state: wb_state::PLAYING,
            wb_time: 0,
            in_remote_mode: false,
            time_lost_remote_car: 0,
            time_last_health_loss: 0,
            time_last_armour_loss: 0,
            time_tank_shot_gun: 0,
            upside_down_counter: 0,
            traffic_multiplier: 0,
            road_density: 1.0,
            previous_time_rewarded_for_explosion: 0,
            explosions_since_last_reward: 0,
            infinite_sprint: false,
            fast_reload: false,
            get_out_of_jail_free: false,
            get_out_of_hospital_free: false,
        }
    }
}

impl PlayerInfo {
    /// `CPlayerInfo::IsPlayerInRemoteMode`.
    pub fn is_in_remote_mode(&self) -> bool {
        self.remote_vehicle.is_some() || self.in_remote_mode
    }
}
