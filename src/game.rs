//! Where the game keeps its automatic shoutouts, and the functions the plugin hooks or calls.
//!
//! `sUserData` (pointer at RVA 0x5013950) owns the three hunter save slots, 0x26CC00 bytes each,
//! at `+0xA8`, and the index of the current one at `+0xA0` (`sUserData::getCurrentSlot`, RVA
//! 0x1B8DBB0). Each slot is guarded by a critical section at `+8`, which the game's shoutout menu
//! holds while it changes a setting, and starts with the hunter's name at `+0x50`.
//!
//! A slot holds three arrays of 32 shoutouts, 0x84 bytes each: a 4-byte header, then the text as a
//! NUL-terminated UTF-8 string of up to 128 bytes. They're the manual ones at `+0x13D1E8`, the
//! custom ones at `+0x13E268` and the automatic ones at `+0x13F2E8`, of which the game uses 18. An
//! empty text means the game's default text for that situation. Which automatic shoutouts are on is
//! a bit mask at `+0x140370`, bit `n` for shoutout `n`.
//!
//! Every situation that sends an automatic shoutout calls `sChat::autoShoutout(sChat*, index)` (RVA
//! 0x1A52450), which checks the bit, then sends the text through `sChat::sendShoutout` (RVA
//! 0x1A525C0) as kind 4. That reads the text from the slot while it builds the message, so the text
//! in the slot when `autoShoutout` runs is the one that's sent.
//!
//! Addresses are for MonsterHunterWorld.exe 15.23.00 (PE timestamp 0x66D13CB6).

use std::ffi::{CStr, c_char};

use windows::Win32::System::Threading::{CRITICAL_SECTION, EnterCriticalSection, LeaveCriticalSection};

pub const TIMESTAMP: u32 = 0x66D1_3CB6;

/// `sChat::autoShoutout(sChat* this, u32 index)`.
pub const AUTO_SHOUTOUT_RVA: usize = 0x1A5_2450;
/// mov [rsp+8],rbx; mov [rsp+10h],rsi; push rdi; sub rsp,30h
pub const AUTO_SHOUTOUT_BYTES: [u8; 15] =
    [0x48, 0x89, 0x5C, 0x24, 0x08, 0x48, 0x89, 0x74, 0x24, 0x10, 0x57, 0x48, 0x83, 0xEC, 0x30];

/// `sChat`'s per-frame update, vtable slot 6. Runs on the game's main thread.
pub const CHAT_UPDATE_RVA: usize = 0x1A5_11F0;
/// mov [rsp+10h],rbx; mov [rsp+18h],rsi; push rbp; push rdi; push r12; push r14
pub const CHAT_UPDATE_BYTES: [u8; 16] =
    [0x48, 0x89, 0x5C, 0x24, 0x10, 0x48, 0x89, 0x74, 0x24, 0x18, 0x55, 0x57, 0x41, 0x54, 0x41, 0x56];

/// `sChat::addSystemLog(sChat* this, const char* text, float time, i32, u8)`: adds a line to the
/// player's own chat log. The game passes -1.0, -1 and 0 for the last three, as its script binding
/// `SetSystemChatLog` does. Not thread-safe, so only called from the chat update.
pub const SYSTEM_LOG_RVA: usize = 0x1A5_40D0;
pub type SystemLog = unsafe extern "system" fn(usize, *const c_char, f32, i32, u8);

/// `sChat`'s queue of system messages waiting for the chat log: up to 64 entries of 0x210 bytes,
/// all inline data after a shared vtable, with the count after them. The update moves the first
/// one to the log when the timer at `+0x60` runs out (elapsed time at `+0x68`, length at `+0x6C`),
/// then waits 1.5 s before the next. A message's category is at `+0x1A0` of its entry; adding one
/// with a category above 5 first drops the waiting ones with the same category.
pub const CHAT_QUEUE: usize = 0x2_0620;
pub const CHAT_QUEUE_COUNT: usize = 0x2_8A20;
pub const CHAT_QUEUE_MAX: usize = 64;
pub const QUEUE_ENTRY_SIZE: usize = 0x210;
pub const QUEUE_ENTRY_CATEGORY: usize = 0x1A0;
pub const CHAT_TIMER_ELAPSED: usize = 0x68;
pub const CHAT_TIMER_LENGTH: usize = 0x6C;
/// The category of this plugin's messages, one the game doesn't use.
pub const MESSAGE_CATEGORY: i32 = 0x4D41;

/// `uGUIFixedPhrase`, the Shoutouts menu with its Manual, Custom and Auto tabs. The plugin swaps
/// two entries of its vtable, which only this class uses: the destructor (slot 0) and slot 59,
/// which `uGUI::update` (RVA 0x2354370) calls every frame while the menu updates.
pub const FIXED_PHRASE_VTABLE_RVA: usize = 0x346_E038;
pub const FIXED_PHRASE_DTOR_SLOT: usize = 0;
pub const FIXED_PHRASE_DTOR_RVA: usize = 0x1E0_C0F0;
pub const FIXED_PHRASE_STEP_SLOT: usize = 59;
pub const FIXED_PHRASE_STEP_RVA: usize = 0x49_F570;
/// `uMhGUI`'s phase: 2 while the screen is open.
pub const GUI_PHASE: usize = 0x1D04;
pub const PHASE_OPEN: u32 = 2;
/// The menu's open tab, 2 for Auto.
pub const GUI_TAB: usize = 0x28F0;
pub const TAB_AUTO: u32 = 2;
/// The menu's row lists, the argument to `refreshPage`.
pub const GUI_LISTS: usize = 0x28F8;
/// The page shown on each tab, a u32 per tab.
pub const GUI_PAGES: usize = 0x3A90;
/// `refreshPage(lists, tab, page)`: reads every row of a page from the save again, texts and
/// switches, as the menu does when it opens or turns a page.
pub const REFRESH_PAGE_RVA: usize = 0x1E0_D110;
pub type RefreshPage = unsafe extern "system" fn(usize, u32, u32);

/// The menu's button guides, built once when the menu is made (RVA 0x1E0E850): lists of up to 16
/// entries of 0x18 bytes from `+0x30`, with the count at `+0x1B0`. An entry is a button icon, flags
/// (0x1F), a pointer to its text and two more dwords. The Auto tab shows the list at `+0x3440`, or
/// the one at `+0x3640` on a row that isn't unlocked yet; both end with "Auto-translation On/Off".
/// The guide is shown by copying the list, so a changed text pointer shows once it's set again.
pub const GUIDE_LISTS_AUTO: [usize; 2] = [0x3440, 0x3640];
pub const GUIDE_ENTRIES: usize = 0x30;
pub const GUIDE_COUNT: usize = 0x1B0;
pub const GUIDE_MAX: usize = 16;
pub const GUIDE_ENTRY_SIZE: usize = 0x18;
pub const GUIDE_ENTRY_TEXT: usize = 0x08;
/// `setAutoGuide(menu, _, row, menu)`: shows the Auto tab's guide for a row of the page (or -1),
/// as the menu does when the cursor moves.
pub const SET_AUTO_GUIDE_RVA: usize = 0x1E0_CC00;
pub type SetAutoGuide = unsafe extern "system" fn(usize, u32, i32, usize);
/// A row list's cursor row on its page.
pub const LIST_CURSOR_ROW: usize = 0x274;

/// Looks a text style up by name (`<STYL name>`), returning null if the game has none of that name;
/// the text parser then ignores the tag. The first argument is unused.
pub const STYLE_LOOKUP_RVA: usize = 0x234_0990;
pub type StyleLookup = unsafe extern "system" fn(usize, *const c_char) -> usize;

/// The static `MtDTI` of `sChat`. Its name is filled in by the game's static constructors, which
/// run after the DRM has decrypted the code.
const SCHAT_DTI_RVA: usize = 0x500_CE80;
const DTI_NAME: usize = 0x08;

const USER_DATA_RVA: usize = 0x501_3950;
const USER_DATA_SLOT_INDEX: usize = 0xA0;
const USER_DATA_SLOTS: usize = 0xA8;
const SLOT_SIZE: usize = 0x26_CC00;
const SLOT_COUNT: i32 = 3;
const SLOT_LOCK: usize = 0x08;
const SLOT_NAME: usize = 0x50;
const NAME_LEN: usize = 0x40;
const AUTO_SHOUTOUTS: usize = 0x13_F2E8;
const AUTO_ENABLED: usize = 0x14_0370;

/// Automatic shoutouts the game uses, one per situation.
pub const AUTO_COUNT: usize = 18;
pub const ENTRY_SIZE: usize = 0x84;
/// The text's room in an entry, including its terminating NUL.
pub const TEXT_SIZE: usize = ENTRY_SIZE - 4;
pub type RawEntry = [u8; ENTRY_SIZE];

/// Whether the game's static constructors have filled in the `sChat` type info yet.
pub fn type_info_ready(base: usize) -> bool {
    // SAFETY: the address lies in the game's .data section, which is mapped and readable.
    let name = unsafe { *((base + SCHAT_DTI_RVA + DTI_NAME) as *const *const c_char) };
    // SAFETY: once set, the field points at the class name, a static string.
    !name.is_null() && unsafe { CStr::from_ptr(name) }.to_bytes() == b"sChat"
}

/// The current hunter save slot.
#[derive(Clone, Copy)]
pub struct Slot {
    pub index: u32,
    address: usize,
}

/// The current save slot, if the game has set up its user data.
pub fn current_slot(base: usize) -> Option<Slot> {
    // SAFETY: the pointer lies in the game's .data section; sUserData lives as long as the game.
    unsafe {
        let user_data = *((base + USER_DATA_RVA) as *const usize);
        if user_data == 0 {
            return None;
        }
        let slots = *((user_data + USER_DATA_SLOTS) as *const usize);
        let index = *((user_data + USER_DATA_SLOT_INDEX) as *const i32);
        if slots == 0 || !(0..SLOT_COUNT).contains(&index) {
            return None;
        }
        Some(Slot { index: index as u32, address: slots + index as usize * SLOT_SIZE })
    }
}

impl Slot {
    /// Runs `f` while holding the slot's critical section, the one the game's menu takes.
    pub fn locked<R>(&self, f: impl FnOnce() -> R) -> R {
        let lock = (self.address + SLOT_LOCK) as *mut CRITICAL_SECTION;
        struct Guard(*mut CRITICAL_SECTION);
        impl Drop for Guard {
            fn drop(&mut self) {
                // SAFETY: entered below by this thread.
                unsafe { LeaveCriticalSection(self.0) };
            }
        }
        // SAFETY: the game initialises the section when it creates the slot, and never frees it.
        unsafe { EnterCriticalSection(lock) };
        let _guard = Guard(lock);
        f()
    }

    /// The hunter's name. Empty while the slot holds no hunter.
    pub fn name(&self) -> String {
        // SAFETY: the name field lies inside the slot.
        let bytes = unsafe { std::slice::from_raw_parts((self.address + SLOT_NAME) as *const u8, NAME_LEN) };
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    }

    fn entry_ptr(&self, index: usize) -> *mut RawEntry {
        assert!(index < AUTO_COUNT);
        (self.address + AUTO_SHOUTOUTS + index * ENTRY_SIZE) as *mut RawEntry
    }

    pub fn entry(&self, index: usize) -> RawEntry {
        // SAFETY: the entry lies inside the slot, and is plain bytes.
        unsafe { self.entry_ptr(index).read_unaligned() }
    }

    pub fn set_entry(&self, index: usize, entry: &RawEntry) {
        // SAFETY: as above. Callers hold the slot's lock.
        unsafe { self.entry_ptr(index).write_unaligned(*entry) }
    }

    pub fn enabled(&self) -> u32 {
        // SAFETY: the mask lies inside the slot.
        unsafe { ((self.address + AUTO_ENABLED) as *const u32).read_unaligned() }
    }

    pub fn set_enabled(&self, mask: u32) {
        // SAFETY: as above. Callers hold the slot's lock.
        unsafe { ((self.address + AUTO_ENABLED) as *mut u32).write_unaligned(mask) }
    }
}
