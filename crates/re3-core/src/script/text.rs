//! The text the script puts on the screen itself (`intro_text_line`), set up by the `SET_TEXT_*` commands and drawn by `DISPLAY_TEXT`.

/// `intro_text_line`: the style of one line and the key of what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct IntroTextLine {
    pub scale_x: f32,
    pub scale_y: f32,
    pub colour: [u8; 4],
    pub justify: bool,
    pub centered: bool,
    pub background: bool,
    pub background_only: bool,
    pub wrap_x: f32,
    pub centre_size: f32,
    pub background_colour: [u8; 4],
    pub text_proportional: bool,
    pub text_before_fade: bool,
    pub right_justify: bool,
    pub font: i32,
    pub at_x: f32,
    pub at_y: f32,
    /// The text key; the renderer looks it up.
    pub text: String,
    /// The numbers put into the text where it has a `~1~` and so on.
    pub numbers: Vec<i32>,
}

impl Default for IntroTextLine {
    /// `intro_text_line::Reset`.
    fn default() -> Self {
        Self {
            scale_x: 0.48,
            scale_y: 1.12,
            colour: [225, 225, 225, 255],
            justify: false,
            centered: false,
            background: false,
            background_only: false,
            wrap_x: 182.0,
            centre_size: 640.0,
            background_colour: [128, 128, 128, 128],
            text_proportional: true,
            text_before_fade: false,
            right_justify: false,
            font: 2,
            at_x: 0.0,
            at_y: 0.0,
            text: String::new(),
            numbers: vec![],
        }
    }
}

pub const MAX_NUM_INTRO_TEXT_LINES: usize = 2;

/// `intro_script_rectangle`: a coloured box or sprite the script draws this frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct IntroRectangle {
    pub used: bool,
    pub before_fade: bool,
    /// The script sprite (`ScriptSprites`), or -1 for a plain box.
    pub texture_id: i32,
    pub rect: [f32; 4],
    pub colour: [u8; 4],
}

pub const MAX_NUM_INTRO_RECTANGLES: usize = 16;
