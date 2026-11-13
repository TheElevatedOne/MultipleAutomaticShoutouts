//! `MultipleAutomaticShoutouts.toml`, next to the DLL. Written with the defaults if it doesn't exist.

use std::{fs, path::Path};

use serde::Deserialize;

const DEFAULT_CONFIG: &str = include_str!("../MultipleAutomaticShoutouts.toml");
pub const MAX_PAGES: u32 = 20;
const DEFAULT_PAGES: u32 = 5;
const DEFAULT_KEY: &str = "F7";
const DEFAULT_BUTTON: &str = "RightStick";
const DEFAULT_GUIDE_TEXT: &str = "Cycle Through Pages";

#[derive(Deserialize)]
#[serde(default, rename_all = "PascalCase", deny_unknown_fields)]
pub struct Config {
    /// Called `Copies` before 0.0.3.
    #[serde(alias = "Copies")]
    pub pages: u32,
    pub switch_key: String,
    pub controller_button: String,
    pub guide_text: String,
    pub hint_on_own_line: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pages: DEFAULT_PAGES,
            switch_key: DEFAULT_KEY.to_owned(),
            controller_button: DEFAULT_BUTTON.to_owned(),
            guide_text: DEFAULT_GUIDE_TEXT.to_owned(),
            hint_on_own_line: true,
        }
    }
}

impl Config {
    /// Returns the config, the virtual-key code of the switch key, the XInput mask of the
    /// controller button (0 for none), and the problems to log, because the log isn't open yet.
    pub fn load(path: &Path) -> (Self, u16, u16, Vec<String>) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => {
                let _ = fs::write(path, DEFAULT_CONFIG);
                DEFAULT_CONFIG.to_owned()
            }
        };
        let mut problems = Vec::new();
        let mut config: Self = match toml::from_str(&text) {
            Ok(config) => config,
            Err(e) => {
                problems.push(format!("config error, using the defaults: {e}"));
                Self::default()
            }
        };
        if !(1..=MAX_PAGES).contains(&config.pages) {
            let given = config.pages;
            config.pages = given.clamp(1, MAX_PAGES);
            problems.push(format!("config: Pages {given} is outside 1 to {MAX_PAGES}, using {}", config.pages));
        }
        // The text goes into the game's markup, where < starts a tag.
        config.guide_text = config.guide_text.replace(['<', '>'], "");
        let key = match virtual_key(&config.switch_key) {
            Ok(key) => key,
            Err(why) => {
                problems.push(format!("config: SwitchKey {:?} {why}, using {DEFAULT_KEY}", config.switch_key));
                config.switch_key = DEFAULT_KEY.to_owned();
                virtual_key(DEFAULT_KEY).unwrap_or(0x76)
            }
        };
        let button = match pad_button(&config.controller_button) {
            Ok(button) => button,
            Err(why) => {
                problems.push(format!(
                    "config: ControllerButton {:?} {why}, using {DEFAULT_BUTTON}",
                    config.controller_button
                ));
                config.controller_button = DEFAULT_BUTTON.to_owned();
                pad_button(DEFAULT_BUTTON).unwrap_or(0)
            }
        };
        (config, key, button, problems)
    }
}

/// The game's `<ICON …>` name for a controller button mask, as its own texts use them.
pub fn icon_name(button: u16) -> Option<&'static str> {
    Some(match button {
        0x0020 => "PAD_TOUCH",
        0x0040 => "PAD_L3",
        0x0080 => "PAD_R3",
        _ => return None,
    })
}

/// Controller buttons with a use on the Shoutouts menu's Auto tab, from the game's button guide:
/// A confirms, B closes, X turns a shoutout on or off, Y edits it, LB and RB change tabs, the
/// D-pad moves the cursor and turns the menu's own pages, and Start closes the whole menu.
const PAD_TAKEN: &[&str] = &["a", "b", "x", "y", "lb", "rb", "start", "up", "down", "left", "right"];

/// The XInput button mask for a button name, 0 for "None".
fn pad_button(name: &str) -> Result<u16, &'static str> {
    let name = name.trim().to_ascii_lowercase();
    match name.as_str() {
        "none" => Ok(0),
        "back" => Ok(0x0020),
        "leftstick" => Ok(0x0040),
        "rightstick" => Ok(0x0080),
        _ if PAD_TAKEN.contains(&name.as_str()) => {
            Err("already does something in the Shoutouts menu; use RightStick, LeftStick, Back or None")
        }
        _ => Err("isn't a button name; use RightStick, LeftStick, Back or None"),
    }
}

/// Keys the switch key can't be: the Shoutouts menu's own keys (Esc, Enter, Space, the arrows, and
/// on the Auto tab N, R, C, Q, E, F and WASD), and every key that types or edits text, since the
/// key also works while you type a shoutout. Mouse buttons and modifiers are out too.
fn key_taken(code: u16) -> bool {
    matches!(code,
        0x01..=0x06            // mouse buttons
        | 0x08 | 0x09 | 0x0D   // Backspace, Tab, Enter
        | 0x10..=0x12          // Shift, Ctrl, Alt
        | 0x1B | 0x20          // Esc, Space
        | 0x23..=0x28          // End, Home, arrows
        | 0x2E                 // Delete
        | 0x30..=0x39          // digits
        | 0x41..=0x5A          // letters
        | 0x5B | 0x5C          // Windows keys
        | 0x60..=0x6F          // numpad
        | 0xA0..=0xA5          // left and right Shift, Ctrl, Alt
        | 0xBA..=0xC0          // punctuation
        | 0xDB..=0xDF          // punctuation
        | 0xE2                 // the extra < > key
    )
}

const KEY_HELP: &str = "use F1 to F24, Insert, PageUp, PageDown or a free key code";

/// The Windows virtual-key code for a key name, or for a code written as "0x76".
fn virtual_key(name: &str) -> Result<u16, String> {
    let name = name.trim();
    let upper = name.to_ascii_uppercase();
    let code = if let Some(hex) = upper.strip_prefix("0X") {
        u16::from_str_radix(hex, 16).ok().filter(|&code| (1..=0xFE).contains(&code))
    } else if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()) {
        (1..=24).contains(&n).then_some(0x70 + n - 1)
    } else {
        match upper.as_str() {
            "INSERT" => Some(0x2D),
            "PAGEUP" => Some(0x21),
            "PAGEDOWN" => Some(0x22),
            // Names 0.0.4 accepted, refused below with the reason.
            "DELETE" => Some(0x2E),
            "HOME" => Some(0x24),
            "END" => Some(0x23),
            _ if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphanumeric() => Some(upper.as_bytes()[0] as u16),
            _ if upper.strip_prefix("NUM").is_some_and(|n| n.parse::<u8>().is_ok_and(|n| n <= 9)) => Some(0x60),
            _ => None,
        }
    };
    match code {
        None => Err(format!("isn't a key name; {KEY_HELP}")),
        Some(code) if key_taken(code) => {
            Err(format!("is used in the Shoutouts menu or for typing a shoutout; {KEY_HELP}"))
        }
        Some(code) => Ok(code),
    }
}
