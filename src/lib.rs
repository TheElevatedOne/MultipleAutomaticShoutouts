//! Monster Hunter: World plugin for Stracker's Loader. Keeps several pages of automatic shoutouts
//! and sends, each time, the shoutout of one of the pages that have it turned on, at random.
#![allow(non_snake_case)]

mod config;
mod detour;
mod game;
mod input;
mod log;
mod menu;
mod sets;
mod shout;

use std::{
    ffi::c_void,
    panic,
    path::PathBuf,
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::HMODULE,
    System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW},
};

use config::Config;

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;
/// How long to wait for the game to finish unpacking and set up its type info.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);

/// # Safety
/// Only the Windows loader calls this.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            let dll_path = module_path(module);
            // The loader lock is held here, so the work happens on a thread of its own.
            std::thread::spawn(move || run(dll_path));
        }
        // The background writer may not get another turn, so the last change is written here.
        DLL_PROCESS_DETACH => shout::flush_at_exit(),
        _ => {}
    }
    1
}

fn run(dll_path: PathBuf) {
    log::init(&dll_path.with_extension("log"));
    panic::set_hook(Box::new(|info| log!("error: {info}")));
    log!("MultipleAutomaticShoutouts {}", env!("CARGO_PKG_VERSION"));

    let (config, key, button, problems) = Config::load(&dll_path.with_extension("toml"));
    for problem in problems {
        log!("{problem}");
    }
    // SAFETY: a null name returns the game's own module.
    let Ok(module) = (unsafe { GetModuleHandleW(None) }) else {
        log!("error: can't find the game module");
        return;
    };
    let base = module.0 as usize;
    let timestamp = pe_timestamp(base);
    if timestamp != game::TIMESTAMP {
        log!(
            "error: unsupported game version (exe timestamp {timestamp:#X}, this build needs {:#X}). Nothing was changed.",
            game::TIMESTAMP
        );
        return;
    }

    // The code is still encrypted until the game's DRM stub has run, and the type info is filled
    // in by static constructors, which run after it, so waiting for the type info covers both.
    let start = Instant::now();
    while !game::type_info_ready(base) {
        if start.elapsed() > STARTUP_TIMEOUT {
            log!("error: the game's type info never showed up. Nothing was changed.");
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    log!("game ready after {:.1}s", start.elapsed().as_secs_f32());

    menu::configure(base, config::guide_id(button), config::icon_name(button), &config.switch_key, &config.guide_text, config.hint_on_own_line);
    if config.pages <= 1 {
        // Shoutouts are left to the game; only the menu's hint shows, greyed out.
        log!("Pages is 1, so shoutouts are left to the game and the menu's hint is greyed out");
        hook_menu(base);
        return;
    }

    let (input, xinput) = input::Input::new(key, button);
    if button != 0 && !xinput {
        log!("error: controller: XInput couldn't be loaded, so {} won't work", config.controller_button);
    }
    shout::configure(base, config.pages as usize, input, dll_path.with_extension("sets.toml"));

    let target = base + game::AUTO_SHOUTOUT_RVA;
    let Some(trampoline) = hook(
        "automatic shoutouts",
        target,
        &game::AUTO_SHOUTOUT_BYTES,
        shout::auto_shoutout_hook as *const () as usize,
    ) else {
        return;
    };
    shout::set_auto_original(trampoline);
    log!("automatic shoutouts: hooked, {} pages", config.pages);

    let target = base + game::CHAT_UPDATE_RVA;
    if let Some(trampoline) =
        hook("switch key", target, &game::CHAT_UPDATE_BYTES, shout::update_hook as *const () as usize)
    {
        shout::set_update_original(trampoline);
        log!(
            "switch key: hooked, {} turns the page in the menu, and {} does while the Auto tab is open",
            config.switch_key,
            config.controller_button
        );
        hook_menu(base);
        // Only to tell in the log whether the key can work; nothing waits for it.
        let start = Instant::now();
        while !shout::update_seen() && start.elapsed() < STARTUP_TIMEOUT {
            std::thread::sleep(Duration::from_millis(200));
        }
        if !shout::update_seen() {
            log!("error: switch key: the chat update never ran, so the key won't work");
        }
    }
}

/// Swaps the Shoutouts menu's two vtable entries, so a switch can redraw the open menu and its
/// button guide shows the switch.
fn hook_menu(base: usize) {
    let vtable = base + game::FIXED_PHRASE_VTABLE_RVA;
    let step_slot = vtable + game::FIXED_PHRASE_STEP_SLOT * 8;
    let dtor_slot = vtable + game::FIXED_PHRASE_DTOR_SLOT * 8;
    // The destructor first, so the menu is never remembered without being forgotten when freed.
    // SAFETY: entries of the menu's vtable, replaced by functions of the same signature.
    let swapped = unsafe {
        detour::swap_pointer(dtor_slot, base + game::FIXED_PHRASE_DTOR_RVA, menu::dtor_hook as *const () as usize)
            .and_then(|dtor| {
                menu::set_originals(base + game::FIXED_PHRASE_STEP_RVA, dtor);
                detour::swap_pointer(
                    step_slot,
                    base + game::FIXED_PHRASE_STEP_RVA,
                    menu::step_hook as *const () as usize,
                )
                .inspect_err(|_| {
                    let _ = detour::swap_pointer(dtor_slot, menu::dtor_hook as *const () as usize, dtor);
                })
            })
    };
    match swapped {
        Ok(_) => log!("shoutouts menu: hooked, its button guide shows the switch"),
        Err(e) => log!("error: shoutouts menu: couldn't hook it ({e:?}), so reopen the menu after a switch"),
    }
}

/// Installs one hook and logs why when it can't. Returns the address that runs the original.
fn hook(what: &str, target: usize, expected: &[u8], hook: usize) -> Option<usize> {
    // SAFETY: the target is a function in the game image, and the hook has its signature.
    match unsafe { detour::install(target, expected, hook) } {
        Ok(trampoline) => Some(trampoline),
        Err(detour::Error::Mismatch(found)) => {
            if found[0] == 0xE9 || (found[0] == 0xFF && found[1] == 0x25) {
                log!("error: {what}: another plugin already hooks this function. {what} unchanged.");
            } else {
                log!("error: {what}: the function doesn't match (found {found:02X?}). {what} unchanged.");
            }
            None
        }
        Err(e) => {
            log!("error: {what}: couldn't install the hook: {e:?}. {what} unchanged.");
            None
        }
    }
}

fn pe_timestamp(base: usize) -> u32 {
    // SAFETY: the PE headers of a loaded module are mapped and readable.
    unsafe {
        let e_lfanew = *((base + 0x3C) as *const u32) as usize;
        *((base + e_lfanew + 8) as *const u32)
    }
}

fn module_path(module: HMODULE) -> PathBuf {
    let mut buffer = [0u16; 1024];
    // SAFETY: the buffer is valid and the module is this DLL, which is loaded.
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
    PathBuf::from(String::from_utf16_lossy(&buffer[..len]))
}
