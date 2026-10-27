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
    /// `CTheScripts::HighlightImportantArea` on the map: a rectangle (`z` is the marker height).
    fn highlight_important_area(&mut self, id: u32, x1: f32, y1: f32, x2: f32, y2: f32, z: f32) {}
}

/// Nothing is shown or heard.
pub struct NoScriptHooks;
impl ScriptHooks for NoScriptHooks {}
