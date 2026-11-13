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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pages: DEFAULT_PAGES,
            switch_key: DEFAULT_KEY.to_owned(),
            controller_button: DEFAULT_BUTTON.to_owned(),
            guide_text: DEFAULT_GUIDE_TEXT.to_owned(),
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
            Some(key) => key,
            None => {
                problems.push(format!("config: SwitchKey {:?} isn't a key name, using {DEFAULT_KEY}", config.switch_key));
                config.switch_key = DEFAULT_KEY.to_owned();
                virtual_key(DEFAULT_KEY).unwrap_or(0x76)
            }
        };
        let button = match pad_button(&config.controller_button) {
            Some(button) => button,
            None => {
                problems.push(format!(
                    "config: ControllerButton {:?} isn't a button name, using {DEFAULT_BUTTON}",
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
        0x0001 => "PAD_KEY_U",
        0x0002 => "PAD_KEY_D",
        0x0004 => "PAD_KEY_L",
        0x0008 => "PAD_KEY_R",
        0x0010 => "OPTION",
        0x0020 => "PAD_TOUCH",
        0x0040 => "PAD_L3",
        0x0080 => "PAD_R3",
        0x0100 => "PAD_L1",
        0x0200 => "PAD_R1",
        0x1000 => "PAD_RD",
        0x2000 => "PAD_RR",
        0x4000 => "PAD_RL",
        0x8000 => "PAD_RU",
        _ => return None,
    })
}

/// The XInput button mask for a button name, 0 for "None".
fn pad_button(name: &str) -> Option<u16> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "none" => 0,
        "up" => 0x0001,
        "down" => 0x0002,
        "left" => 0x0004,
        "right" => 0x0008,
        "start" => 0x0010,
        "back" => 0x0020,
        "leftstick" => 0x0040,
        "rightstick" => 0x0080,
        "lb" => 0x0100,
        "rb" => 0x0200,
        "a" => 0x1000,
        "b" => 0x2000,
        "x" => 0x4000,
        "y" => 0x8000,
        _ => return None,
    })
}

/// The Windows virtual-key code for a key name, or for a code written as "0x76".
fn virtual_key(name: &str) -> Option<u16> {
    let name = name.trim();
    if let Some(hex) = name.strip_prefix("0x").or_else(|| name.strip_prefix("0X")) {
        return u16::from_str_radix(hex, 16).ok().filter(|&code| (1..=0xFE).contains(&code));
    }
    let upper = name.to_ascii_uppercase();
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()) {
        return (1..=24).contains(&n).then_some(0x70 + n - 1);
    }
    if let Some(n) = upper.strip_prefix("NUM").and_then(|n| n.parse::<u16>().ok()) {
        return (n <= 9).then_some(0x60 + n);
    }
    if upper.len() == 1 {
        let c = upper.as_bytes()[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u16);
        }
    }
    Some(match upper.as_str() {
        "INSERT" => 0x2D,
        "DELETE" => 0x2E,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        _ => return None,
    })
}
