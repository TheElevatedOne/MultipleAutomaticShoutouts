//! The switch key and the controller button.
//!
//! The controller is read through XInput, which Steam Input and Proton also present controllers
//! as. `xinput1_4.dll` is loaded when the plugin starts rather than linked, so a system without it
//! only loses the button. Reading an empty controller slot is slow, so empty slots are checked
//! again only every 2 seconds.

use std::{
    mem::transmute,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        System::{
            LibraryLoader::{GetProcAddress, LoadLibraryW},
            Threading::GetCurrentProcessId,
        },
        UI::{
            Input::KeyboardAndMouse::GetAsyncKeyState,
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    },
    core::{s, w},
};

const ERROR_SUCCESS: u32 = 0;
const PADS: u32 = 4;
const RESCAN: Duration = Duration::from_secs(2);

/// `XINPUT_STATE`.
#[repr(C)]
#[derive(Default)]
struct PadState {
    packet: u32,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumbs: [i16; 4],
}

type GetState = unsafe extern "system" fn(u32, *mut PadState) -> u32;

pub struct Input {
    key: u16,
    button: u16,
    get_state: Option<GetState>,
    connected: [bool; PADS as usize],
    last_scan: Option<Instant>,
    key_was_down: bool,
    button_was_down: bool,
}

/// What was pressed this frame.
pub struct Pressed {
    pub key: bool,
    pub button: bool,
}

impl Input {
    /// `button` is an XInput button mask, 0 for none. Returns the input and whether XInput loaded.
    pub fn new(key: u16, button: u16) -> (Self, bool) {
        let get_state = if button == 0 { None } else { load_xinput() };
        let loaded = get_state.is_some();
        let input = Self {
            key,
            button,
            get_state,
            connected: [false; PADS as usize],
            last_scan: None,
            key_was_down: false,
            button_was_down: false,
        };
        (input, loaded)
    }

    /// Reads the key and the button; only presses while the game's window has focus count.
    pub fn poll(&mut self) -> Pressed {
        let focused = game_focused();
        // SAFETY: a plain query of the key's state.
        let key_down = focused && unsafe { GetAsyncKeyState(self.key as i32) } as u16 & 0x8000 != 0;
        let button_down = focused && self.button_down();
        let pressed = Pressed { key: key_down && !self.key_was_down, button: button_down && !self.button_was_down };
        self.key_was_down = key_down;
        self.button_was_down = button_down;
        pressed
    }

    fn button_down(&mut self) -> bool {
        let Some(get_state) = self.get_state else { return false };
        let rescan = self.last_scan.is_none_or(|at| at.elapsed() >= RESCAN);
        if rescan {
            self.last_scan = Some(Instant::now());
        }
        let mut down = false;
        for pad in 0..PADS {
            if !self.connected[pad as usize] && !rescan {
                continue;
            }
            let mut state = PadState::default();
            // SAFETY: XInputGetState with a valid slot and a state of its layout.
            let ok = unsafe { get_state(pad, &mut state) } == ERROR_SUCCESS;
            self.connected[pad as usize] = ok;
            down |= ok && state.buttons & self.button != 0;
        }
        down
    }
}

fn load_xinput() -> Option<GetState> {
    for name in [w!("xinput1_4.dll"), w!("xinput9_1_0.dll")] {
        // SAFETY: loads a system DLL and looks up a function of the declared signature.
        unsafe {
            let Ok(module) = LoadLibraryW(name) else { continue };
            if let Some(function) = GetProcAddress(module, s!("XInputGetState")) {
                return Some(transmute::<unsafe extern "system" fn() -> isize, GetState>(function));
            }
        }
    }
    None
}

fn game_focused() -> bool {
    // SAFETY: plain Win32 queries; the out parameter is a local.
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return false;
        }
        let mut process = 0;
        GetWindowThreadProcessId(window, Some(&mut process));
        process == GetCurrentProcessId()
    }
}
