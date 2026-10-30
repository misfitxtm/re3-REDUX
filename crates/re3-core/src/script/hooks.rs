//! What the script commands ask of the parts of the game that are not ported: camera, HUD, audio, streaming and so on.
/// `CamShakeNoPos` and the like: everything is a no-op until the game loop supplies it.
#[allow(unused_variables)]
pub trait ScriptHooks {
    /// `CamShakeNoPos(&TheCamera, strength)`.
    fn camera_shake(&mut self, strength: f32) {}
    /// `CPad::GetPad(0)->GetCrossJustDown()`: the button that skips a wait with a message on screen.
    fn cross_just_down(&mut self) -> bool {
        false
    }
    /// The messages on screen end now (their start time is set to 0).
    fn skip_current_messages(&mut self) {}
    /// `CMessages::ClearSmallMessagesOnly`.
    fn clear_small_messages_only(&mut self) {}
    /// `CStreaming::RequestSpecialModel(MI_PLAYER, "player", ...)` and `LoadAllRequestedModels`.
    fn load_player_model(&mut self) {}
    /// `CMessages::AddBigMessage(text, time, style)`.
    fn print_big(&mut self, key: &str, time: u32, style: i32) {}
    /// `CMessages::AddMessage(text, time, flags)`.
    fn print(&mut self, key: &str, time: u32, flags: i32) {}
    /// `CMessages::AddMessageJumpQ`.
    fn print_now(&mut self, key: &str, time: u32, flags: i32) {}
    /// `CMessages::AddMessageSoon`.
    fn print_soon(&mut self, key: &str, time: u32, flags: i32) {}
    /// `CMessages::ClearMessages`.
    fn clear_messages(&mut self) {}
    /// `TheCamera.IsSphereVisible`.
    fn is_sphere_visible(&mut self, pos: glam::Vec3, radius: f32) -> bool {
        false
    }
    /// `GetPadState(pad, button)`: the state of a pad control (0 when not pressed).
    fn pad_state(&mut self, pad: i32, button: i32) -> i16 {
        0
    }
    /// While the intro plays the mouse, enter or space also count as the start button.
    fn intro_skip_pressed(&mut self, pad: i32, button: i32) -> bool {
        false
    }
    /// `CPad::GetPad(pad)->GetHorn()`.
    fn horn_pressed(&mut self, pad: i32) -> bool {
        false
    }
    /// `CBulletInfo::TestForSniperBullet(x1, x2, y1, y2, z1, z2)`.
    fn test_for_sniper_bullet(&mut self, x1: f32, x2: f32, y1: f32, y2: f32, z1: f32, z2: f32) -> bool {
        false
    }
    /// `TheCamera.CamShake(strength, x, y, z)`.
    fn camera_shake_with_point(&mut self, strength: f32, at: glam::Vec3) {}
    /// `CUserDisplay::Pager.AddMessage(text, time, flag, priority)`.
    fn add_pager_message(&mut self, key: &str, a: i32, b: i32, c: i32) {}
    /// `TheCamera.TakeControl(entity, mode, transition, CAMCONTROL_SCRIPT)`.
    fn camera_take_control(&mut self, target: Option<crate::entity::EntityRef>, mode: i32, transition: i32) {}
    /// `TheCamera.TakeControlNoEntity(pos, transition, CAMCONTROL_SCRIPT)`.
    fn camera_take_control_no_entity(&mut self, pos: glam::Vec3, transition: i32) {}
    /// `TheCamera.Restore()`.
    fn camera_restore(&mut self) {}
    /// `TheCamera.SetCamPositionForFixedMode(pos, rotation)`.
    fn camera_set_fixed_position(&mut self, pos: glam::Vec3, rotation: glam::Vec3) {}
    /// `CPad::GetPad(pad)->StartShake(duration, frequency)`.
    fn pad_shake(&mut self, pad: i32, duration: i32, frequency: i32) {}
    /// `TheCamera.SetFadeColour(r, g, b)`.
    fn set_fade_colour(&mut self, r: i32, g: i32, b: i32) {}
    /// `TheCamera.Fade(seconds, direction)`.
    fn fade(&mut self, seconds: f32, direction: i32) {}
    /// `TheCamera.GetFading()`.
    fn is_fading(&mut self) -> bool {
        false
    }
    /// `CShadows::StoreShadowToBeRendered(type, pos, front, side, intensity, r, g, b)`.
    fn draw_shadow(&mut self, ty: i32, pos: glam::Vec3, front: (f32, f32), side: (f32, f32), rgba: [i32; 4]) {}
    /// `CRadar::SetEntityBlip(kind, handle, colour, display)`: the blip's handle.
    fn add_entity_blip(&mut self, kind: u8, entity: i32, colour: i32, display: i32) -> i32 {
        -1
    }
    /// `CRadar::SetCoordBlip(kind, pos, colour, display)`.
    fn add_coord_blip(&mut self, kind: u8, pos: glam::Vec3, colour: i32, display: i32) -> i32 {
        -1
    }
    /// `CRadar::ClearBlip`.
    fn clear_blip(&mut self, blip: i32) {}
    fn change_blip_colour(&mut self, blip: i32, colour: i32) {}
    fn change_blip_brightness(&mut self, blip: i32, brightness: i32) {}
    fn change_blip_scale(&mut self, blip: i32, scale: i32) {}
    fn change_blip_display(&mut self, blip: i32, display: i32) {}
    /// A sound at a place that plays once (`CreateOneShotScriptObject`).
    fn add_one_off_sound(&mut self, pos: glam::Vec3, id: u16) {}
    /// A sound at a place that plays until it is removed: the handle the script keeps.
    fn add_continuous_sound(&mut self, pos: glam::Vec3, id: u16) -> i32 {
        -1
    }
    /// `DMAudio.DestroyLoopingScriptObject`.
    fn remove_sound(&mut self, handle: i32) {}
    /// The parts of `CMissionCleanup::Process` that are not game logic: `TheCamera.Restore` and `SetWideScreenOff`, `DMAudio.ClearMissionAudio`,
    /// `CWeather::ReleaseWeather`, the streaming requests of the mission dropped, `CHud::SetHelpMessage(nil)` and the flashing item cleared.
    fn end_of_mission_housekeeping(&mut self) {}
    fn force_weather(&mut self, weather: i32) {}
    fn force_weather_now(&mut self, weather: i32) {}
    fn release_weather(&mut self) {}
    /// `CMessages::AddBigMessageWithNumber`, `AddMessageWithNumber` and `AddMessageJumpQWithNumber`.
    fn print_big_with_number(&mut self, key: &str, time: u32, style: i32, number: i32) {}
    fn print_with_number(&mut self, key: &str, time: u32, flags: i32, number: i32) {}
    fn print_now_with_number(&mut self, key: &str, time: u32, flags: i32, number: i32) {}
    /// `CGame::playingIntro`.
    fn playing_intro(&mut self) -> bool {
        false
    }
    /// The player's idle animation blended in by the camera script when it takes control away (`BlendAnimation(STD_IDLE, 1000)`).
    fn player_idle_for_camera_script(&mut self) {}
    /// `CUserDisplay::Pager.AddMessageWithNumber`.
    fn add_pager_message_with_number(&mut self, key: &str, number: i32, a: i32, b: i32, c: i32) {}
    /// `CMessages::AddBigMessageQ` and `AddBigMessageWithNumberQ`.
    fn print_big_queued(&mut self, key: &str, time: u32, style: i32) {}
    fn print_big_with_number_queued(&mut self, key: &str, time: u32, style: i32, number: i32) {}
    /// `CPad::GetPad(pad)->bApplyBrakes`.
    fn apply_brakes_to_players_car(&mut self, pad: i32, on: bool) {}
    /// `CHeli::ActivateHeli`.
    fn activate_heli(&mut self, on: bool) {}
    /// `CStreaming::RequestSpecialChar(slot, name, ...)` and `HasSpecialCharLoaded`.
    fn request_special_character(&mut self, slot: i32, name: &str) {}
    fn has_special_character_loaded(&mut self, slot: i32) -> bool {
        true
    }
    /// `CCutsceneMgr::SetCutsceneOffset`.
    fn set_cutscene_offset(&mut self, offset: glam::Vec3) {}
    /// `CStreaming::RequestModel(model, DEPENDENCY | NOFADE | SCRIPTOWNED)`, `HasModelLoaded` and `SetMissionDoesntRequireModel`.
    fn request_model_for_script(&mut self, model: i16) {}
    fn has_model_loaded_for_script(&mut self, model: i16) -> bool {
        true
    }
    fn mark_model_as_no_longer_needed(&mut self, model: i16) {}
    /// `CCoronas::RegisterCorona` for a script (id, position, size, type, flare, colour).
    fn draw_corona(&mut self, id: u32, pos: glam::Vec3, size: f32, ty: i32, flare: i32, rgb: [i32; 3]) {}
    /// `CPointLights::AddLight` with radius 12.
    fn draw_light(&mut self, pos: glam::Vec3, rgb: [f32; 3]) {}
    /// `CWeather::StoreWeatherState` and `RestoreWeatherState`.
    fn store_weather(&mut self) {}
    fn restore_weather(&mut self) {}
    /// `CStreaming::SetMissionDoesntRequireSpecialChar`.
    fn unload_special_character(&mut self, slot: i32) {}
    /// `CParticleObject::AddObject(type, pos, remove_if_far)`.
    fn add_particle_effect(&mut self, ty: i32, pos: glam::Vec3, remove_if_far: bool) {}
    /// `CRadar::SetBlipSprite`.
    fn set_blip_sprite(&mut self, blip: i32, sprite: i32) {}
    /// `CTheScripts::HighlightImportantAngledArea`: the four corners.
    fn highlight_important_angled_area(&mut self, id: u32, corners: [(f32, f32); 4], z: f32) {}
    /// One of the script's statistics changed.
    fn stat(&mut self, stat: ScriptStat) {}
    /// `CMessages::AddMessageWithNumber` and its jump-queue and soon versions (`kind` 0, 1, 2) with up to six numbers.
    fn print_with_numbers(&mut self, kind: u8, key: &str, time: u32, flags: i32, numbers: &[i32]) {}
    /// `CCutsceneMgr`: loading the data, making the objects and heads, animating them, running and clearing.
    fn load_cutscene(&mut self, name: &str) {}
    fn create_cutscene_object(&mut self, model: i32) -> i32 {
        -1
    }
    fn set_cutscene_anim(&mut self, name: &str, object: i32) {}
    fn start_cutscene(&mut self) {}
    fn cutscene_time_in_milliseconds(&mut self) -> i32 {
        0
    }
    fn has_cutscene_finished(&mut self) -> bool {
        true
    }
    fn clear_cutscene(&mut self) {}
    fn create_cutscene_head(&mut self, object: i32, model: i32) -> i32 {
        -1
    }
    fn set_cutscene_head_anim(&mut self, name: &str, head: i32) {}
    /// `CRecordDataForChase::StartChaseScene(speed)`, with every drawn model deleted first.
    fn start_chase_scene(&mut self, speed: f32) {}
    /// `CRecordDataForChase::CleanUpChaseScene`.
    fn stop_chase_scene(&mut self) {}
    /// `CPlane::CreateIncomingCesna`.
    fn start_drug_run(&mut self) {}
    /// `CPlane::HasCesnaLanded`.
    fn drug_run_completed(&mut self) -> bool {
        false
    }
    /// `CPlane::HasCesnaBeenDestroyed`.
    fn drug_plane_shot_down(&mut self) -> bool {
        false
    }
    /// `CPlane::CreateDropOffCesna`.
    fn start_drug_drop_off(&mut self) {}
    /// `CPlane::HasDropOffCesnaBeenShotDown`.
    fn drop_off_plane_shot_down(&mut self) -> bool {
        false
    }
    /// `CPlane::FindDropOffCesnaCoordinates`.
    fn find_drop_off_plane_coordinates(&mut self) -> glam::Vec3 {
        glam::Vec3::ZERO
    }
    /// `CPed::DeleteRwObject`: the ped's drawn model goes (for `UNDRESS_CHAR`).
    fn delete_ped_rw_object(&mut self, ped: i32) {}
    /// `SetModelIndex(model)` again: the ped's drawn model is made anew from the model now loaded (for `DRESS_CHAR`).
    fn recreate_ped_rw_object(&mut self, ped: i32) {}
    /// `CStreaming::RequestSpecialModel(slot, name, ...)`.
    fn request_special_model(&mut self, slot: i32, name: &str) {}
    /// `TheCamera.SetZoomValueFollowPedScript` or `SetZoomValueCamStringScript`, whichever the camera mode wants.
    fn set_camera_zoom(&mut self, zoom: i32) {}
    /// `CStats::IndustrialPassed`, `CommercialPassed` or `SuburbanPassed` (0, 1, 2); the first two also play a radio announcement.
    fn level_passed(&mut self, level: u8) {}
    /// `CMessages::AddBigMessageWithNumber` with up to six numbers.
    fn print_big_with_numbers(&mut self, key: &str, time: u32, style: i32, numbers: &[i32]) {}
    /// `TheCamera.SetMotionBlur(0, 0, 0, 0, type)`.
    fn set_motion_blur(&mut self, kind: i32) {}
    /// `CMessages::AddMessageWithString` (kind 0 queued, 1 now): a text with another text key in it.
    fn print_string_in_string(&mut self, kind: u8, key: &str, time: u32, flags: i32, inner_key: &str) {}
    /// `CStreaming::LoadAllRequestedModels(false)`.
    fn load_all_models_now(&mut self) {}
    /// `CTheScripts::ScriptSprites[slot].SetTexture(name)`, `LoadTxd` of a script texture dictionary and its removal.
    fn load_sprite(&mut self, slot: i32, name: &str) {}
    fn load_script_texture_dictionary(&mut self, name: &str) {}
    fn remove_script_texture_dictionary(&mut self) {}
    /// `DMAudio.PlayFrontEndTrack(STREAMED_SOUND_MISSION_COMPLETED + n - 1)` after switching to the front-end music mode.
    fn play_mission_passed_tune(&mut self, n: i32) {}
    /// `CParticleObject::AddObject(type, pos, target, size, ..., colour, true)` for a moving effect.
    fn add_moving_particle_effect(&mut self, ty: i32, pos: glam::Vec3, target: glam::Vec3, size: f32, life: i32, rgb: [i32; 3]) {}
    /// `C3dMarkers::PlaceMarkerSet` for the script's blue sphere.
    fn draw_sphere_marker(&mut self, id: u32, pos: glam::Vec3, radius: f32) {}
    /// `DMAudio.PlaySuspectLastSeen`.
    fn police_radio_message(&mut self, pos: glam::Vec3) {}
    /// `CRubbish::SetVisibility`.
    fn switch_rubbish(&mut self, on: bool) {}
    /// `CPad::SetDisablePlayerControls(PLAYERCONTROL_CUTSCENE)`.
    fn disable_player_controls_for_cutscene(&mut self) {}
    /// `CCutsceneMgr::StartCutsceneProcessing`.
    fn start_cutscene_processing(&mut self) {}
    /// The level change of `LOAD_COLLISION`: the island the player is on (streaming and collision are reloaded).
    fn load_collision(&mut self, level: i32) {}
    /// `CRecordDataForChase::RemoveCarFromChase`.
    fn remove_car_from_chase(&mut self, car: i32) {}
    /// `CGame::frenchGame` and `germanGame`.
    fn is_french_game(&mut self) -> bool {
        false
    }
    fn is_german_game(&mut self) -> bool {
        false
    }
    /// `DMAudio.ClearMissionAudio`.
    fn clear_mission_audio(&mut self) {}
    /// `TheCamera.SetNearClipScript`.
    fn set_near_clip(&mut self, clip: f32) {}
    /// `DMAudio.SetRadioChannel`.
    fn set_radio_channel(&mut self, channel: i32, position: i32) {}
    /// `CWeather::bScriptsForceRain`.
    fn force_rain(&mut self, on: bool) {}
    /// Whether measurements are shown in metres (`USE_MEASUREMENTS_IN_METERS`).
    fn are_measurements_in_metres(&mut self) -> bool {
        false
    }
    /// `CTheScripts::HighlightImportantArea` on the map: a rectangle (`z` is the marker height).
    fn highlight_important_area(&mut self, id: u32, x1: f32, y1: f32, x2: f32, y2: f32, z: f32) {}
    /// `CStreaming::ms_disableStreaming = !on`.
    fn switch_streaming(&mut self, on: bool) {}
    /// `CHeli::StartCatalinaFlyBy`.
    fn start_catalina_heli(&mut self) {}
    /// `CHeli::CatalinaTakeOff`.
    fn catalina_heli_take_off(&mut self) {}
    /// `CHeli::RemoveCatalinaHeli`.
    fn remove_catalina_heli(&mut self) {}
    /// `CHeli::HasCatalinaBeenShotDown`.
    fn catalina_heli_shot_down(&mut self) -> bool {
        false
    }
    /// `CHeli::MakeCatalinaHeliFlyAway`.
    fn catalina_heli_fly_away(&mut self) {}
    /// The vehicle handle of `CHeli::FindPointerToCatalinasHeli`, or -1.
    fn catalina_heli(&mut self) -> i32 {
        -1
    }
    /// Swaps the model of the building of `old` nearest to `pos` for `new` (and remembers it to undo).
    fn swap_nearest_building_model(&mut self, pos: glam::Vec3, radius: f32, old: i32, new: i32) {}
    /// `CWorld::bProcessCutsceneOnly = !on`.
    fn switch_world_processing(&mut self, on: bool) {}
    /// `CStreaming::ms_vehiclesLoaded`: the models of the cars in memory (-1 for an empty slot).
    fn loaded_vehicle_models(&mut self) -> Vec<i32> {
        vec![]
    }
    /// `CStreaming::LoadScene`.
    fn load_scene(&mut self, pos: glam::Vec3) {}
    /// `TheCamera.SetCameraDirectlyInFrontForFollowPed_CamOnAString`.
    fn set_camera_in_front_of_player(&mut self) {}
    /// `DMAudio.PreloadMissionAudio`.
    fn load_mission_audio(&mut self, name: &str) {}
    /// `DMAudio.GetMissionAudioLoadingStatus() == 1`.
    fn mission_audio_loaded(&mut self) -> bool {
        true
    }
    /// `DMAudio.PlayLoadedMissionAudio`.
    fn play_mission_audio(&mut self) {}
    /// `DMAudio.IsMissionAudioSampleFinished`.
    fn mission_audio_finished(&mut self) -> bool {
        true
    }
    /// `DMAudio.SetMissionAudioLocation`.
    fn set_mission_audio_position(&mut self, pos: glam::Vec3) {}
    /// `FrontEndMenuManager.m_bSaveMenuActive = true`.
    fn activate_save_menu(&mut self) {}
    /// `!FrontEndMenuManager.m_bMenuActive`.
    fn save_game_finished(&mut self) -> bool {
        true
    }
    /// `CMessages::ClearThisPrint`.
    fn clear_this_print(&mut self, key: &str) {}
    /// `CMessages::ClearThisBigPrint`.
    fn clear_this_big_print(&mut self, key: &str) {}
    /// `CHud::SetHelpMessage(text, false)`; `None` clears it.
    fn set_help_message(&mut self, key: Option<&str>) {}
    /// `CHud::m_ItemToFlash`.
    fn flash_hud_object(&mut self, item: i32) {}
    /// `CCredits::Start`.
    fn start_credits(&mut self) {}
    /// `CCredits::Stop`.
    fn stop_credits(&mut self) {}
    /// `CCredits::AreCreditsDone`.
    fn credits_finished(&mut self) -> bool {
        true
    }
    /// `CParticle::AddParticle(kind, pos, dir, nil, size, ...)`.
    fn create_single_particle(&mut self, kind: i32, pos: glam::Vec3, dir: glam::Vec3, size: f32) {}
    /// `CRecordDataForChase::TurnChaseCarIntoScriptCar`: the new car's handle, or -1.
    fn turn_chase_car_into_script_car(&mut self, index: i32) -> i32 {
        -1
    }
    /// `CSpecialParticleStuff::StartBoatFoamAnimation`.
    fn start_boat_foam_animation(&mut self) {}
    /// `CSpecialParticleStuff::UpdateBoatFoamAnimation`.
    fn update_boat_foam_animation(&mut self, matrix: &crate::math::Matrix) {}
    /// `TheCamera.m_bIgnoreFadingStuffForMusic = !fades`.
    fn set_music_does_fade(&mut self, fades: bool) {}
    /// `CGame::playingIntro` set by the script (with the streaming and music changes that come with it).
    fn set_intro_is_playing(&mut self, playing: bool) {}
    /// `DMAudio.ChangeMusicMode(MUSICMODE_CUTSCENE)` and the preload of the end of game tune.
    fn load_end_of_game_tune(&mut self) {}
    /// `DMAudio.PlayPreloadedCutSceneMusic`.
    fn play_end_of_game_tune(&mut self) {}
    /// `DMAudio.StopCutSceneMusic` and back to the game music.
    fn stop_end_of_game_tune(&mut self) {}
    /// `CVehicle::bAllDodosCheat || CVehicle::bCheat3`.
    fn car_cheats_active(&mut self) -> bool {
        false
    }
    /// `CPlane::FindDrugPlaneCoordinates`.
    fn find_drug_plane_coordinates(&mut self) -> glam::Vec3 {
        glam::Vec3::ZERO
    }
    /// Removes the particle objects inside the box.
    fn remove_particle_effects_in_area(&mut self, min: glam::Vec3, max: glam::Vec3) {}
    /// The level change of `LOAD_COLLISION_WITH_SCREEN`.
    fn load_collision_with_screen(&mut self, level: i32) {}
    /// `LoadSplash`.
    fn load_splash_screen(&mut self, name: &str) {}
    /// `CPad::GetPad(0)->SetEnablePlayerControls(PLAYERCONTROL_CAMERA)`.
    fn enable_player_control_camera(&mut self) {}
    /// The debug camera (`TheCamera.Cams[2]`): its source and front vector.
    fn debug_camera(&mut self) -> (glam::Vec3, glam::Vec3) {
        (glam::Vec3::ZERO, glam::Vec3::ZERO)
    }
    /// `TheCamera.SetParametersForScriptInterpolation(a, 50 - a, time)`.
    fn set_interpolation_parameters(&mut self, a: f32, time: i32) {}
    /// `CPed::nEnterCarRangeMultiplier`.
    fn set_enter_car_range_multiplier(&mut self, m: f32) {}
    /// `CCamera::m_bUseMouse3rdPerson`: the help texts about aiming with the pad are skipped when it is on.
    fn use_mouse_third_person(&mut self) -> bool {
        false
    }
}

/// The statistics the script feeds (`CStats`), one per command.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptStat {
    ProgressMade(i32),
    TotalProgress(i32),
    JumpDistance(f32),
    JumpHeight(f32),
    JumpFlips(i32),
    JumpSpins(i32),
    StuntJump(i32),
    UniqueJumpFound,
    UniqueJumpsTotal(i32),
    PassengerDroppedOffWithTaxi,
    MoneyMadeWithTaxi(i32),
    MissionGiven,
    MissionPassed(String),
    /// A statistic that has not been given its own variant: its name and the numbers it was given.
    Named(&'static str, Vec<i32>),
}

/// Nothing is shown or heard.
pub struct NoScriptHooks;
impl ScriptHooks for NoScriptHooks {}
