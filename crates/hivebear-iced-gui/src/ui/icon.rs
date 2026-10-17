//! Phosphor icon codepoints for `assets/Phosphor.ttf` (see PHOSPHOR-LICENSE.txt).

use iced::widget::{text, Text};
use iced::Font;

pub const FONT_BYTES: &[u8] = include_bytes!("../../assets/Phosphor.ttf");
pub const FONT: Font = Font::with_name("Phosphor");

/// An icon glyph as a text widget.
pub fn icon<'a>(glyph: &'a str) -> Text<'a> {
    text(glyph).font(FONT)
}

pub const ARROW_COUNTER_CLOCKWISE: &str = "\u{E038}";
pub const ARROW_LEFT: &str = "\u{E058}";
pub const ARROW_RIGHT: &str = "\u{E06C}";
pub const ARROWS_CLOCKWISE: &str = "\u{E094}";
pub const ARROW_SQUARE_OUT: &str = "\u{E5DE}";
pub const CARET_DOWN: &str = "\u{E136}";
pub const CARET_RIGHT: &str = "\u{E13A}";
pub const CHAT_CIRCLE: &str = "\u{E168}";
pub const CHECK: &str = "\u{E182}";
pub const CHECK_CIRCLE: &str = "\u{E184}";
pub const CLOUD: &str = "\u{E1AA}";
pub const COMMAND: &str = "\u{E1C4}";
pub const COPY: &str = "\u{E1CA}";
pub const CPU: &str = "\u{E610}";
pub const CROWN: &str = "\u{E614}";
pub const DOWNLOAD_SIMPLE: &str = "\u{E20C}";
pub const ENVELOPE: &str = "\u{E214}";
pub const EYE: &str = "\u{E220}";
pub const EYE_SLASH: &str = "\u{E224}";
pub const FINGERPRINT: &str = "\u{E23E}";
pub const FLOPPY_DISK: &str = "\u{E248}";
pub const GAUGE: &str = "\u{E628}";
pub const GEAR: &str = "\u{E270}";
pub const GRAPH: &str = "\u{EB58}";
pub const HARD_DRIVE: &str = "\u{E29E}";
pub const INFO: &str = "\u{E2CE}";
pub const KEY: &str = "\u{E2D6}";
pub const LIGHTNING: &str = "\u{E2DE}";
pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
pub const MEMORY: &str = "\u{E9C4}";
pub const MONITOR: &str = "\u{E32E}";
pub const NETWORK: &str = "\u{EDDE}";
pub const PACKAGE: &str = "\u{E390}";
pub const PAPER_PLANE_RIGHT: &str = "\u{E396}";
pub const PENCIL_SIMPLE: &str = "\u{E3B4}";
pub const PLAY: &str = "\u{E3D0}";
pub const PLUS: &str = "\u{E3D4}";
pub const POWER: &str = "\u{E3DA}";
pub const PULSE: &str = "\u{E000}";
pub const ROBOT: &str = "\u{E762}";
pub const SHARE_NETWORK: &str = "\u{E408}";
pub const SHIELD: &str = "\u{E40A}";
pub const SHIELD_CHECK: &str = "\u{E40C}";
pub const SIDEBAR: &str = "\u{EAB6}";
pub const SIDEBAR_SIMPLE: &str = "\u{EC24}";
pub const SLIDERS: &str = "\u{E432}";
pub const SPARKLE: &str = "\u{E6A2}";
pub const SQUARES_FOUR: &str = "\u{E464}";
pub const STOP: &str = "\u{E46C}";
pub const TRASH: &str = "\u{E4A6}";
pub const USER: &str = "\u{E4C2}";
pub const USER_CIRCLE: &str = "\u{E4C4}";
pub const USER_PLUS: &str = "\u{E4D0}";
pub const USERS: &str = "\u{E4D6}";
pub const WARNING_CIRCLE: &str = "\u{E4E2}";
pub const WIFI_HIGH: &str = "\u{E4EA}";
pub const WIFI_SLASH: &str = "\u{E4F2}";
pub const X: &str = "\u{E4F6}";
