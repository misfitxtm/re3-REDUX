//! `CPlayerPed` state (filled in with the player controls).
use crate::entity::EntityRef;

#[derive(Debug, Clone)]
pub struct PlayerData {
    /// `m_fFPSMoveHeading`.
    pub fps_move_heading: f32,
    pub free_aim_active: bool,
    /// `m_walkAngle` kept between frames while the stick is centred.
    pub walk_angle: f32,
    /// `m_bCanBeDamaged`.
    pub can_be_damaged: bool,
    /// `m_nEvadeAmount` and `m_pEvadingFrom`: the dive the player is asked to make.
    pub evade_amount: u8,
    pub evading_from: Option<EntityRef>,
    /// `m_fMoveSpeed`.
    pub move_speed: f32,
    /// `m_bAdrenalineActive`.
    pub adrenaline_active: bool,
    /// `nPlayerInComboMove`.
    pub combo_move: u16,
    /// `m_bHaveTargetSelected`.
    pub have_target_selected: bool,
    /// `m_fAttackButtonCounter`.
    pub attack_button_counter: f32,
    /// `m_pArrestingCop`.
    pub arresting_cop: Option<EntityRef>,
    /// `m_bSpeedTimerFlag`.
    pub speed_timer_flag: bool,
    /// `m_pWanted`.
    pub wanted: crate::control::wanted::Wanted,
    pub current_stamina: f32,
    pub max_stamina: f32,
    pub stamina_progress: f32,
    /// `m_nAdrenalineTime`.
    pub adrenaline_time: u32,
    /// `m_nSelectedWepSlot`.
    pub selected_wep_slot: usize,
    /// `m_nSpeedTimer`.
    pub speed_timer: u32,
    pub has_lock_on_target: bool,
    /// `m_nTargettableObjects`.
    pub targettable_objects: [Option<EntityRef>; 4],
    /// `m_nHitAnimDelayTimer`: no new hit-by-gun animation before this time.
    pub hit_anim_delay_timer: u32,
}

impl Default for PlayerData {
    fn default() -> Self {
        Self { fps_move_heading: 0.0, free_aim_active: false, walk_angle: 0.0, can_be_damaged: true, evade_amount: 0, evading_from: None, move_speed: 0.0, adrenaline_active: false, combo_move: 0, have_target_selected: false, attack_button_counter: 0.0, arresting_cop: None, speed_timer_flag: false, wanted: Default::default(), current_stamina: 150.0, max_stamina: 150.0, stamina_progress: 0.0, adrenaline_time: 0, selected_wep_slot: 0, speed_timer: 0, has_lock_on_target: false, targettable_objects: [None; 4], hit_anim_delay_timer: 0 }
    }
}
