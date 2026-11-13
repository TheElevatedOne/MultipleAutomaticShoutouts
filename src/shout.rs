//! Picks a random page each time an automatic shoutout is sent, and turns the page the game's
//! menu shows when the switch key is pressed.
//!
//! The hook on `sChat::autoShoutout` keeps the pages in step with the game's save first, so a
//! change just made in the menu counts. It then picks one of the pages that have the situation
//! turned on. If that's not the page in the save, it puts the picked page's text and switch in the
//! save's entry, lets the game send it, and puts the save's own entry back. When no page has the
//! situation turned on, nothing is sent.
//!
//! The hook on `sChat`'s per-frame update reads the switch key and button, and every few seconds
//! copies the save's shoutouts into the shown page, so the file keeps up with what you change in
//! the menu. After a switch with the menu open, it asks the menu to redraw (see `menu`).
//!
//! A hunter is known by save slot and name. While the game starts, a slot can hold a placeholder
//! hunter before the save is loaded, so new hunters are only taken on when the game is clearly
//! playing one: when an automatic shoutout is sent, the switch is pressed, or the menu is open.
//!
//! Locks are always taken in the same order: the slot's critical section first, then the state.

use std::{
    ffi::CString,
    mem::transmute,
    panic::{self, AssertUnwindSafe},
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    game::{self, AUTO_COUNT, RawEntry, Slot},
    input::Input,
    log, menu,
    sets::{Hunter, Page, Store},
};

/// Update calls between two syncs of the save into the file, a few seconds.
const SYNC_INTERVAL: u32 = 150;

static AUTO_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static UPDATE_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static BASE: AtomicUsize = AtomicUsize::new(0);
static RNG: AtomicU64 = AtomicU64::new(0);
static STATE: Mutex<Option<State>> = Mutex::new(None);
/// The file's next contents, written by a background thread so the game never waits on the disk.
static PENDING: Mutex<Option<String>> = Mutex::new(None);
static SETS_PATH: OnceLock<PathBuf> = OnceLock::new();
/// Whether the update hook has run, to tell in the log whether the switch key can work.
static UPDATE_SEEN: AtomicBool = AtomicBool::new(false);

type AutoShoutout = unsafe extern "system" fn(usize, u32, usize, usize) -> usize;
type Update = unsafe extern "system" fn(usize, usize, usize, usize) -> usize;

struct State {
    pages: usize,
    input: Input,
    store: Store,
    dirty: bool,
    frames: u32,
}

pub fn configure(base: usize, pages: usize, input: Input, sets_path: PathBuf) {
    BASE.store(base, Relaxed);
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    RNG.store(seed | 1, Relaxed);

    let (store, problems, changed) = Store::load(&sets_path, pages);
    for problem in problems {
        log!("{problem}");
    }
    for hunter in &store.hunters {
        log!(
            "sets: slot {} ({}): {} pages, page {} in the menu",
            hunter.slot,
            hunter.name,
            hunter.pages.len(),
            hunter.shown + 1
        );
    }
    *lock(&STATE) = Some(State { pages, input, store, dirty: changed, frames: 0 });
    let _ = SETS_PATH.set(sets_path.clone());
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            flush(&sets_path);
        }
    });
}

/// Writes the file if it has changed. Also called when the game closes.
pub fn flush_at_exit() {
    if let Some(path) = SETS_PATH.get() {
        flush(path);
    }
}

fn flush(path: &PathBuf) {
    let Ok(mut pending) = PENDING.try_lock() else { return };
    let Some(text) = pending.take() else { return };
    drop(pending);
    // Written next to it and renamed over it, so a crash never leaves half a file.
    let temp = path.with_extension("toml.tmp");
    if std::fs::write(&temp, text).is_ok() && std::fs::rename(&temp, path).is_ok() {
        return;
    }
    log!("error: can't write {}", path.display());
}

pub fn set_auto_original(trampoline: usize) {
    AUTO_ORIGINAL.store(trampoline, Relaxed);
}

pub fn set_update_original(trampoline: usize) {
    UPDATE_ORIGINAL.store(trampoline, Relaxed);
}

pub fn update_seen() -> bool {
    UPDATE_SEEN.load(Relaxed)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the menu's button guide should say about pages.
pub enum PageStatus {
    /// `Pages = 1`: the hint is greyed out.
    Off,
    /// No hunter is known yet, so no page number.
    Unknown,
    /// The page shown, counted from 1, and the number of pages.
    Page(usize, usize),
}

/// The page status for the current hunter, for the menu's button guide. Runs on the menu's update,
/// which may not be the chat update's thread; it only reads the state.
pub fn page_status() -> PageStatus {
    // Not configured with `Pages = 1`.
    let base = BASE.load(Relaxed);
    if base == 0 {
        return PageStatus::Off;
    }
    let Some(slot) = game::current_slot(base) else { return PageStatus::Unknown };
    let name = slot.name();
    let state = lock(&STATE);
    let Some(state) = state.as_ref() else { return PageStatus::Off };
    match state.store.hunters.iter().find(|h| h.slot == slot.index && h.name == name) {
        Some(hunter) => PageStatus::Page(hunter.shown + 1, hunter.pages.len()),
        None => PageStatus::Unknown,
    }
}

/// xorshift64*: a number below `n`.
fn random_below(n: usize) -> usize {
    let mut x = RNG.load(Relaxed);
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    RNG.store(x, Relaxed);
    (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as usize % n
}

/// Copies the save's shoutouts into the shown page of the hunter in `slot`, and returns it.
/// `None` while the slot holds no hunter, or one not seen before when `take_on` is false. The
/// caller holds the slot's lock.
fn sync<'a>(state: &'a mut State, slot: &Slot, take_on: bool) -> Option<&'a mut Hunter> {
    let name = slot.name();
    if name.is_empty() {
        return None;
    }
    let live = Page::read(slot);
    let at = state.store.hunters.iter().position(|h| h.slot == slot.index && h.name == name);
    let at = match at {
        Some(at) => at,
        None if take_on => {
            let hunter = Hunter::new(slot.index, name, live.clone(), state.pages);
            log!(
                "sets: slot {} ({}): {} pages of its automatic shoutouts made from the save",
                hunter.slot,
                hunter.name,
                state.pages
            );
            state.dirty = true;
            state.store.hunters.push(hunter);
            state.store.hunters.len() - 1
        }
        None => return None,
    };
    let hunter = &mut state.store.hunters[at];
    if hunter.pages[hunter.shown] != live {
        hunter.pages[hunter.shown] = live;
        state.dirty = true;
    }
    Some(hunter)
}

/// What the auto shoutout hook does around the game's function.
enum Plan {
    /// Send the save's own entry.
    Pass,
    /// No page has this situation turned on.
    Skip,
    /// Another page's entry was put in the save. Holds the save's entry and switch to put back,
    /// and what was written, to leave the entry alone if the game changed it in between.
    Swapped { slot: Slot, saved: RawEntry, was_on: bool, written: RawEntry },
}

fn plan(index: usize) -> Plan {
    let Some(slot) = game::current_slot(BASE.load(Relaxed)) else { return Plan::Pass };
    slot.locked(|| {
        let mut state = lock(&STATE);
        let Some(state) = state.as_mut() else { return Plan::Pass };
        let Some(hunter) = sync(state, &slot, true) else { return Plan::Pass };
        let on: Vec<usize> = (0..hunter.pages.len()).filter(|&p| hunter.pages[p].is_on(index)).collect();
        if on.is_empty() {
            return Plan::Skip;
        }
        let pick = on[random_below(on.len())];
        if pick == hunter.shown {
            return Plan::Pass;
        }
        let saved = slot.entry(index);
        let written = hunter.pages[pick].entries[index].to_raw();
        let was_on = slot.enabled() & (1 << index) != 0;
        slot.set_entry(index, &written);
        slot.set_enabled(slot.enabled() | 1 << index);
        Plan::Swapped { slot, saved, was_on, written }
    })
}

/// Replaces `sChat::autoShoutout`. The two spare arguments pass whatever else is in the registers.
pub unsafe extern "system" fn auto_shoutout_hook(chat: usize, index: u32, r8: usize, r9: usize) -> usize {
    // SAFETY: set before the hook was installed, with the hooked function's signature.
    let original: AutoShoutout = unsafe { transmute(AUTO_ORIGINAL.load(Relaxed)) };
    if index as usize >= AUTO_COUNT {
        // SAFETY: the game's own call, passed on unchanged.
        return unsafe { original(chat, index, r8, r9) };
    }
    let plan = panic::catch_unwind(|| plan(index as usize)).unwrap_or(Plan::Pass);
    match plan {
        Plan::Skip => 0,
        // SAFETY: as above.
        Plan::Pass => unsafe { original(chat, index, r8, r9) },
        Plan::Swapped { slot, saved, was_on, written } => {
            // SAFETY: as above.
            let result = unsafe { original(chat, index, r8, r9) };
            slot.locked(|| {
                if slot.entry(index as usize) == written {
                    slot.set_entry(index as usize, &saved);
                }
                let bit = 1 << index;
                slot.set_enabled(if was_on { slot.enabled() | bit } else { slot.enabled() & !bit });
            });
            result
        }
    }
}

/// Replaces `sChat`'s per-frame update.
pub unsafe extern "system" fn update_hook(chat: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    // SAFETY: set before the hook was installed, with the hooked function's signature.
    let result = unsafe {
        let original: Update = transmute(UPDATE_ORIGINAL.load(Relaxed));
        original(chat, rdx, r8, r9)
    };
    UPDATE_SEEN.store(true, Relaxed);
    menu::next_frame();
    if let Ok(Some(message)) = panic::catch_unwind(AssertUnwindSafe(tick)) {
        let _ = panic::catch_unwind(|| system_log(chat, &message));
    }
    result
}

/// Reads the switch key and button and syncs now and then. Returns a chat log line to show.
fn tick() -> Option<String> {
    let menu_open = menu::auto_tab_open();
    let (pressed, sync_due) = {
        let mut state = lock(&STATE);
        let state = state.as_mut()?;
        let input = state.input.poll();
        // The button has other uses outside the menu, such as the camera.
        let pressed = input.key || (input.button && menu_open);
        state.frames += 1;
        let sync_due = state.frames >= SYNC_INTERVAL;
        if sync_due {
            state.frames = 0;
        }
        (pressed, sync_due)
    };
    let mut message = None;
    if pressed || sync_due {
        if let Some(slot) = game::current_slot(BASE.load(Relaxed)) {
            message = slot.locked(|| {
                let mut state = lock(&STATE);
                let state = state.as_mut()?;
                let Some(hunter) = sync(state, &slot, pressed || menu_open) else {
                    return pressed.then(|| "Automatic Shoutouts: load a hunter first".to_owned());
                };
                if !pressed {
                    return None;
                }
                hunter.shown = (hunter.shown + 1) % hunter.pages.len();
                hunter.pages[hunter.shown].write(&slot);
                let (shown, count) = (hunter.shown + 1, hunter.pages.len());
                log!("slot {}: the menu now shows page {shown} of {count}", slot.index);
                state.dirty = true;
                Some(format!("Automatic Shoutouts: {shown}/{count} Pages"))
            });
            if pressed && menu_open {
                menu::request_refresh();
            }
        }
    }
    let mut state = lock(&STATE);
    if let Some(state) = state.as_mut() {
        if state.dirty {
            state.dirty = false;
            *lock(&PENDING) = Some(state.store.to_toml());
        }
    }
    message
}

/// Adds `text` to the chat log at once. The game's queue releases one message every 1.5 s, so
/// behind its own notices a line would wait several seconds. The message goes in with this
/// plugin's category, which drops a line from an earlier press still waiting, then moves to the
/// front of the queue, and the queue's timer is set to run out on the next update.
fn system_log(chat: usize, text: &str) {
    let Ok(text) = CString::new(text) else { return };
    // SAFETY: runs inside sChat's update with the live sChat, as the game's own calls do. The
    // queue entries are inline data after a vtable all of them share, so moving them is a copy.
    unsafe {
        let add: game::SystemLog = transmute(BASE.load(Relaxed) + game::SYSTEM_LOG_RVA);
        add(chat, text.as_ptr(), -1.0, game::MESSAGE_CATEGORY, 0);

        let count = *((chat + game::CHAT_QUEUE_COUNT) as *const usize);
        if count == 0 || count > game::CHAT_QUEUE_MAX {
            return;
        }
        let queue = (chat + game::CHAT_QUEUE) as *mut u8;
        let last = queue.add((count - 1) * game::QUEUE_ENTRY_SIZE);
        if *(last.add(game::QUEUE_ENTRY_CATEGORY) as *const i32) != game::MESSAGE_CATEGORY {
            return;
        }
        if count > 1 {
            let mut entry = [0u8; game::QUEUE_ENTRY_SIZE];
            std::ptr::copy_nonoverlapping(last, entry.as_mut_ptr(), game::QUEUE_ENTRY_SIZE);
            std::ptr::copy(queue, queue.add(game::QUEUE_ENTRY_SIZE), (count - 1) * game::QUEUE_ENTRY_SIZE);
            std::ptr::copy_nonoverlapping(entry.as_ptr(), queue, game::QUEUE_ENTRY_SIZE);
        }
        let length = *((chat + game::CHAT_TIMER_LENGTH) as *const f32);
        *((chat + game::CHAT_TIMER_ELAPSED) as *mut f32) = length;
    }
}
