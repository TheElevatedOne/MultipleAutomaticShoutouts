//! Follows the Shoutouts menu: redraws it after a page switch while it's open, and adds the switch
//! to its button guide.
//!
//! The menu's per-frame vtable entry (slot 59, called by `uGUI::update`) tells which menu is open
//! and on which tab. When a switch asks for it, the hook calls the menu's own `refreshPage` for the
//! Auto tab's current page, on the menu's own thread, so the rows show the new page at once. The
//! destructor's entry forgets the menu when the game frees it.
//!
//! The button guide hint is an entry of its own at the end of both Auto tab guide lists, with the
//! button's guide id, so the game draws its icon in a key box and lays it out as its own entries.
//! A button without a known guide id (Back, or none) falls back to text after the last entry: its
//! `<ICON …>` tag or key name and the hint, after a line break with `HintOnOwnLine`. With one page the hint
//! is greyed out with a grey text style, if the game has one of the names tried.

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
    text: String,
    how: Placement,
}

/// How the hint goes into the guide.
enum Placement {
    /// As an entry of its own, which the game draws with the button's icon in a key box and lays
    /// out with its own entries.
    Entry { id: u32 },
    /// After the last entry's text, for a button without a known guide id: the `<ICON …>` tag of the
    /// button, or the key name in brackets without one, after a line break or spaces.
    Text { button: String, separator: &'static str },
}

/// The guide texts made so far. The game keeps pointers to them, so they live as long as the game.
#[derive(Default)]
struct Guide {
    /// By the game's text (0 for an entry of our own) and the hint: the text shown.
    texts: HashMap<(usize, String), CString>,
    /// Each of our texts, to the game's text behind it (0 for an entry of our own).
    originals: HashMap<usize, usize>,
    /// The grey style found, once looked for.
    grey: Option<Option<&'static str>>,
}

pub fn configure(base: usize, guide_id: Option<u32>, icon: Option<&str>, key: &str, text: &str, own_line: bool) {
    BASE.store(base, Relaxed);
    let how = match (guide_id, icon) {
        (Some(id), _) => Placement::Entry { id },
        (None, Some(icon)) => {
            Placement::Text { button: format!("<ICON {icon}>"), separator: if own_line { "\r\n" } else { "   " } }
        }
        (None, None) => Placement::Text { button: format!("[{key}]"), separator: if own_line { "\r\n" } else { "   " } },
    };
    let _ = HINT.set(Hint { text: text.to_owned(), how });
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

/// Puts the hint in both Auto tab guides. Returns whether a guide changed.
unsafe fn update_guide(menu: usize) -> bool {
    let Some(hint) = HINT.get() else { return false };
    let mut guide = GUIDE.lock().unwrap_or_else(|e| e.into_inner());
    let guide = guide.get_or_insert_with(Guide::default);
    let label = match shout::page_status() {
        shout::PageStatus::Page(shown, count) => format!("{} ({shown}/{count})", hint.text),
        shout::PageStatus::Unknown => hint.text.clone(),
        shout::PageStatus::Off => {
            let grey = *guide.grey.get_or_insert_with(find_grey_style);
            match grey {
                Some(style) => format!("<STYL {style}>{}</STYL>", hint.text),
                None => hint.text.clone(),
            }
        }
    };
    #[cfg(feature = "guide-ids")]
    {
        let page = match shout::page_status() {
            shout::PageStatus::Page(shown, _) => shown - 1,
            _ => 0,
        };
        let _ = &label;
        let mut changed = false;
        for list in game::GUIDE_LISTS_AUTO {
            // SAFETY: the guide lists lie inside the open menu.
            changed |= unsafe { put_id_chart(guide, menu + list, page) };
        }
        return changed;
    }
    #[allow(unreachable_code)]
    let mut changed = false;
    for list in game::GUIDE_LISTS_AUTO {
        // SAFETY: the guide lists lie inside the open menu.
        changed |= unsafe {
            match &hint.how {
                Placement::Entry { id } => put_entry(guide, menu + list, *id, &label),
                Placement::Text { button, separator } => {
                    put_text(guide, menu + list, &format!("{button} {label}"), separator)
                }
            }
        };
    }
    changed
}

/// Test build: after the game's own entries, 12 entries with guide ids `page * 12` onwards, each
/// labelled with its id. Returns whether the list changed.
#[cfg(feature = "guide-ids")]
unsafe fn put_id_chart(guide: &mut Guide, list: usize, page: usize) -> bool {
    const PER_PAGE: usize = 12;
    // SAFETY: the count is checked before entries are read; entries are written below the room.
    unsafe {
        let count_slot = (list + game::GUIDE_COUNT) as *mut usize;
        let count = *count_slot;
        if count > game::GUIDE_MAX {
            return false;
        }
        let entry = |i: usize| list + game::GUIDE_ENTRIES + i * game::GUIDE_ENTRY_SIZE;
        // The game's own entries end where ours start.
        let base = (0..count)
            .find(|&i| guide.originals.get(&*((entry(i) + game::GUIDE_ENTRY_TEXT) as *const usize)) == Some(&0))
            .unwrap_or(count);
        let ids = PER_PAGE.min(game::GUIDE_MAX - base);
        let mut changed = base + ids != count;
        for k in 0..ids {
            let id = (page * PER_PAGE + k) as u32;
            let label = format!("id 0x{id:02X}");
            let key = (0, label.clone());
            let ptr = match guide.texts.get(&key) {
                Some(text) => text.as_ptr() as usize,
                None => {
                    let Ok(text) = CString::new(label) else { return false };
                    let ptr = text.as_ptr() as usize;
                    guide.texts.insert(key, text);
                    guide.originals.insert(ptr, 0);
                    ptr
                }
            };
            let at = entry(base + k);
            if *((at + game::GUIDE_ENTRY_TEXT) as *const usize) != ptr || *((at + game::GUIDE_ENTRY_BUTTON) as *const u32) != id {
                *((at + game::GUIDE_ENTRY_BUTTON) as *mut u32) = id;
                *((at + game::GUIDE_ENTRY_FLAGS) as *mut u32) = game::GUIDE_FLAGS_ALL;
                *((at + game::GUIDE_ENTRY_TEXT) as *mut usize) = ptr;
                *((at + game::GUIDE_ENTRY_KIND) as *mut u64) = 0;
                changed = true;
            }
        }
        *count_slot = base + ids;
        changed
    }
}

/// Keeps an entry of our own at the end of a guide list. Returns whether the list changed.
unsafe fn put_entry(guide: &mut Guide, list: usize, id: u32, label: &str) -> bool {
    let key = (0, label.to_owned());
    let ours = match guide.texts.get(&key) {
        Some(text) => text.as_ptr() as usize,
        None => {
            let Ok(text) = CString::new(label) else { return false };
            let ptr = text.as_ptr() as usize;
            guide.texts.insert(key, text);
            guide.originals.insert(ptr, 0);
            ptr
        }
    };
    // SAFETY: the count is checked before an entry is read, and an entry is only added below the
    // list's room, filled in before the count takes it in.
    unsafe {
        let count_slot = (list + game::GUIDE_COUNT) as *mut usize;
        let count = *count_slot;
        if count > game::GUIDE_MAX {
            return false;
        }
        let entry = |i: usize| list + game::GUIDE_ENTRIES + i * game::GUIDE_ENTRY_SIZE;
        for i in 0..count {
            let text_slot = (entry(i) + game::GUIDE_ENTRY_TEXT) as *mut usize;
            if guide.originals.get(&*text_slot) == Some(&0) {
                if *text_slot == ours {
                    return false;
                }
                *text_slot = ours;
                return true;
            }
        }
        if count == game::GUIDE_MAX {
            return false;
        }
        let at = entry(count);
        *((at + game::GUIDE_ENTRY_BUTTON) as *mut u32) = id;
        *((at + game::GUIDE_ENTRY_FLAGS) as *mut u32) = game::GUIDE_FLAGS_ALL;
        *((at + game::GUIDE_ENTRY_TEXT) as *mut usize) = ours;
        *((at + game::GUIDE_ENTRY_KIND) as *mut u64) = 0;
        *count_slot = count + 1;
    }
    true
}

/// Puts the hint after the last entry's text of a guide list. Returns whether a text changed.
unsafe fn put_text(guide: &mut Guide, list: usize, suffix: &str, separator: &str) -> bool {
    // SAFETY: the count is checked before an entry is read.
    unsafe {
        let count = *((list + game::GUIDE_COUNT) as *const usize);
        if count == 0 || count > game::GUIDE_MAX {
            return false;
        }
        let text_slot =
            (list + game::GUIDE_ENTRIES + (count - 1) * game::GUIDE_ENTRY_SIZE + game::GUIDE_ENTRY_TEXT) as *mut usize;
        let current = *text_slot;
        if current == 0 {
            return false;
        }
        let original = guide.originals.get(&current).copied().unwrap_or(current);
        let key = (original, suffix.to_owned());
        let ours = match guide.texts.get(&key) {
            Some(text) => text.as_ptr() as usize,
            None => {
                let base = CStr::from_ptr(original as *const c_char).to_string_lossy();
                let Ok(text) = CString::new(format!("{base}{separator}{suffix}")) else { return false };
                let ptr = text.as_ptr() as usize;
                guide.texts.insert(key, text);
                guide.originals.insert(ptr, original);
                ptr
            }
        };
        if current == ours {
            return false;
        }
        *text_slot = ours;
    }
    true
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
