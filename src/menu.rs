//! Follows the Shoutouts menu: redraws it after a page switch while it's open, and adds the switch
//! to its button guide.
//!
//! The menu's per-frame vtable entry (slot 59, called by `uGUI::update`) tells which menu is open
//! and on which tab. When a switch asks for it, the hook calls the menu's own `refreshPage` for the
//! Auto tab's current page, on the menu's own thread, so the rows show the new page at once. The
//! destructor's entry forgets the menu when the game frees it.
//!
//! The button guide hint goes on a line of its own under the last entry of the Auto tab's guides
//! ("Auto-translation On/Off"), as `<ICON PAD_R3> Cycle Through Pages (1/2)` after a line break in
//! that entry's text. Next to it in the same row, the guide squeezes both to fit its width, which
//! `HintOnOwnLine = false` goes back to. The game draws `<ICON …>` tags with the glyph of the controller in use, as in its own texts. With one page the hint is
//! greyed out with a grey text style, if the game has one of the names tried.

use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char},
    mem::transmute,
    panic,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed},
    },
};

use crate::{game, log, shout};

/// Frames, counted by the chat update, that the menu may go without updating and still count as
/// open. Covers the two updates running in a different order or on different threads.
const OPEN_FRAMES: u64 = 10;
/// Text style names that might be grey, tried in order. The game's own texts use names such as
/// `MOJI_YELLOW_DEFAULT`, `MOJI_LIGHTBLUE2_SELECTED` and `MOJI_ORENGE2_SELECTED2`.
const GREY_STYLES: &[&str] = &[
    "MOJI_GRAY_DEFAULT",
    "MOJI_GLAY_DEFAULT",
    "MOJI_GREY_DEFAULT",
    "MOJI_GRAY2_DEFAULT",
    "MOJI_GLAY2_DEFAULT",
    "MOJI_DARKGRAY_DEFAULT",
    "MOJI_GRAY_SELECTED",
    "MOJI_GLAY_SELECTED",
    "MOJI_GRAY_SELECTED2",
    "MOJI_GLAY_SELECTED2",
    "MOJI_GRAY2_SELECTED",
    "MOJI_GLAY2_SELECTED",
    "MOJI_GRAY",
    "MOJI_GLAY",
    "MOJI_DISABLE_DEFAULT",
    "MOJI_WHITE_DISABLE",
    "MOJI_BLACK_DEFAULT",
];

static BASE: AtomicUsize = AtomicUsize::new(0);
static STEP_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static DTOR_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
/// The menu last seen open on the Auto tab, or 0.
static MENU: AtomicUsize = AtomicUsize::new(0);
/// The frame that happened on.
static SEEN_FRAME: AtomicU64 = AtomicU64::new(0);
/// Counted by the chat update.
static FRAME: AtomicU64 = AtomicU64::new(0);
static REFRESH: AtomicBool = AtomicBool::new(false);
static HINT: OnceLock<Hint> = OnceLock::new();
static GUIDE: Mutex<Option<Guide>> = Mutex::new(None);

type Passthrough = unsafe extern "system" fn(usize, usize, usize, usize) -> usize;

/// What the guide hint says, from the config.
struct Hint {
    /// `<ICON …>` tag of the controller button, or the key name in brackets without one.
    button: String,
    text: String,
    /// What goes between the game's last guide text and the hint: a line break, or spaces.
    separator: &'static str,
}

/// The guide texts made so far. The game keeps pointers to them, so they live as long as the game.
#[derive(Default)]
struct Guide {
    /// By the game's text and the hint: the two together.
    texts: HashMap<(usize, String), CString>,
    /// The game's text behind each of ours.
    originals: HashMap<usize, usize>,
    /// The grey style found, once looked for.
    grey: Option<Option<&'static str>>,
}

pub fn configure(base: usize, icon: Option<&str>, key: &str, text: &str, own_line: bool) {
    BASE.store(base, Relaxed);
    let button = match icon {
        Some(icon) => format!("<ICON {icon}>"),
        None => format!("[{key}]"),
    };
    let separator = if own_line { "\r\n" } else { "   " };
    let _ = HINT.set(Hint { button, text: text.to_owned(), separator });
}

pub fn set_originals(step: usize, dtor: usize) {
    STEP_ORIGINAL.store(step, Relaxed);
    DTOR_ORIGINAL.store(dtor, Relaxed);
}

/// Called once a frame by the chat update.
pub fn next_frame() {
    FRAME.fetch_add(1, Relaxed);
}

/// Whether the Shoutouts menu is open on its Auto tab.
pub fn auto_tab_open() -> bool {
    MENU.load(Relaxed) != 0 && FRAME.load(Relaxed).saturating_sub(SEEN_FRAME.load(Relaxed)) <= OPEN_FRAMES
}

/// Redraws the Auto tab and its guide on the menu's next update, if it's open.
pub fn request_refresh() {
    if auto_tab_open() {
        REFRESH.store(true, Relaxed);
    }
}

/// Replaces the menu's vtable slot 59.
pub unsafe extern "system" fn step_hook(menu: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    // SAFETY: the vtable held this function, of this signature, before the swap.
    let result = unsafe {
        let original: Passthrough = transmute(STEP_ORIGINAL.load(Relaxed));
        original(menu, rdx, r8, r9)
    };
    let _ = panic::catch_unwind(|| {
        // SAFETY: the game passes its live uGUIFixedPhrase.
        unsafe { on_update(menu) }
    });
    result
}

unsafe fn on_update(menu: usize) {
    if menu == 0 {
        return;
    }
    // SAFETY: fields of uMhGUI and uGUIFixedPhrase, which the menu is.
    let (phase, tab) = unsafe {
        (*((menu + game::GUI_PHASE) as *const u32), *((menu + game::GUI_TAB) as *const u32))
    };
    if phase != game::PHASE_OPEN {
        let _ = MENU.compare_exchange(menu, 0, Relaxed, Relaxed);
        return;
    }
    // SAFETY: the menu is open, so its guide lists are built.
    let guide_changed = unsafe { update_guide(menu) };
    if tab != game::TAB_AUTO {
        let _ = MENU.compare_exchange(menu, 0, Relaxed, Relaxed);
        return;
    }
    MENU.store(menu, Relaxed);
    SEEN_FRAME.store(FRAME.load(Relaxed), Relaxed);
    let refresh = REFRESH.swap(false, Relaxed);
    // SAFETY: the menu's own functions, called on its own update as its page turns are.
    unsafe {
        if refresh {
            let page = *((menu + game::GUI_PAGES + game::TAB_AUTO as usize * 4) as *const u32);
            let refresh_page: game::RefreshPage = transmute(BASE.load(Relaxed) + game::REFRESH_PAGE_RVA);
            refresh_page(menu + game::GUI_LISTS, game::TAB_AUTO, page);
        }
        if refresh || guide_changed {
            let list = *((menu + game::GUI_LISTS + game::TAB_AUTO as usize * 8) as *const usize);
            let row = if list == 0 { -1 } else { *((list + game::LIST_CURSOR_ROW) as *const i32) };
            let set_guide: game::SetAutoGuide = transmute(BASE.load(Relaxed) + game::SET_AUTO_GUIDE_RVA);
            set_guide(menu, 0, row, menu);
        }
    }
}

/// Puts the hint after the last entry of both Auto tab guides. Returns whether a text changed.
unsafe fn update_guide(menu: usize) -> bool {
    let Some(hint) = HINT.get() else { return false };
    let mut guide = GUIDE.lock().unwrap_or_else(|e| e.into_inner());
    let guide = guide.get_or_insert_with(Guide::default);
    let suffix = match shout::page_status() {
        shout::PageStatus::Page(shown, count) => format!("{} {} ({shown}/{count})", hint.button, hint.text),
        shout::PageStatus::Unknown => format!("{} {}", hint.button, hint.text),
        shout::PageStatus::Off => {
            let grey = *guide.grey.get_or_insert_with(find_grey_style);
            match grey {
                Some(style) => format!("<STYL {style}>{} {}</STYL>", hint.button, hint.text),
                None => format!("{} {}", hint.button, hint.text),
            }
        }
    };
    let mut changed = false;
    for list in game::GUIDE_LISTS_AUTO {
        let list = menu + list;
        // SAFETY: the guide list lies inside the menu; the count is checked before an entry is read.
        unsafe {
            let count = *((list + game::GUIDE_COUNT) as *const usize);
            if count == 0 || count > game::GUIDE_MAX {
                continue;
            }
            let text_slot = (list + game::GUIDE_ENTRIES + (count - 1) * game::GUIDE_ENTRY_SIZE + game::GUIDE_ENTRY_TEXT)
                as *mut usize;
            let current = *text_slot;
            if current == 0 {
                continue;
            }
            let original = guide.originals.get(&current).copied().unwrap_or(current);
            let key = (original, suffix.clone());
            let ours = match guide.texts.get(&key) {
                Some(text) => text.as_ptr() as usize,
                None => {
                    let base = CStr::from_ptr(original as *const c_char).to_string_lossy();
                    let Ok(text) = CString::new(format!("{base}{}{suffix}", hint.separator)) else { continue };
                    let ptr = text.as_ptr() as usize;
                    guide.texts.insert(key, text);
                    guide.originals.insert(ptr, original);
                    ptr
                }
            };
            if current != ours {
                *text_slot = ours;
                changed = true;
            }
        }
    }
    changed
}

/// The first grey text style the game has, if any.
fn find_grey_style() -> Option<&'static str> {
    // SAFETY: the game's style lookup, which only reads its style table and returns null for a
    // name it doesn't have.
    let lookup: game::StyleLookup = unsafe { transmute(BASE.load(Relaxed) + game::STYLE_LOOKUP_RVA) };
    let found = GREY_STYLES.iter().copied().find(|name| {
        let Ok(name) = CString::new(*name) else { return false };
        // SAFETY: as above, with a NUL-terminated name.
        unsafe { lookup(0, name.as_ptr()) != 0 }
    });
    match found {
        Some(style) => log!("guide: greyed out with the text style {style}"),
        None => log!("guide: the game has none of the grey text styles tried, so the hint stays white"),
    }
    found
}

/// Replaces the menu's destructor.
pub unsafe extern "system" fn dtor_hook(menu: usize, flags: usize, r8: usize, r9: usize) -> usize {
    let _ = MENU.compare_exchange(menu, 0, Relaxed, Relaxed);
    // SAFETY: the vtable held this function, of this signature, before the swap.
    unsafe {
        let original: Passthrough = transmute(DTOR_ORIGINAL.load(Relaxed));
        original(menu, flags, r8, r9)
    }
}
