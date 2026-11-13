//! The pages of each hunter's automatic shoutouts, kept in `MultipleAutomaticShoutouts.sets.toml`
//! next to the DLL.
//!
//! The game's save holds one page, the one its Automatic Shoutouts menu shows (`Shown`). The file
//! holds all of them, and the plugin copies the menu's changes into the file as you make them.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::game::{AUTO_COUNT, ENTRY_SIZE, RawEntry, Slot, TEXT_SIZE};

const HEADER: &str = "\
# Written by MultipleAutomaticShoutouts. One [[Hunter]] per hunter, by save slot (0 to 2) and name,
# and one [[Hunter.Page]] per page. Each page lists its 18 automatic shoutouts in the order of the
# game's menu: On says whether it's turned on, and Text is what's sent (empty means the game's
# default text, longer than 127 bytes is cut). Shown is the page, counted from 0, that's in the
# game's save and menu. Edit this file only while the game is closed.

";

/// One automatic shoutout's text. The game stores its length in bytes before it, as the menu's
/// setter (RVA 0x1E0DBA0) does, so only the text is kept.
#[derive(Clone, PartialEq)]
pub struct Entry {
    pub text: String,
}

impl Entry {
    fn from_raw(raw: &RawEntry) -> Self {
        let text = &raw[4..];
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        Self { text: String::from_utf8_lossy(&text[..end]).into_owned() }
    }

    pub fn to_raw(&self) -> RawEntry {
        let mut raw = [0u8; ENTRY_SIZE];
        let text = truncate(&self.text, TEXT_SIZE - 1);
        raw[..4].copy_from_slice(&(text.len() as u32).to_le_bytes());
        raw[4..4 + text.len()].copy_from_slice(text.as_bytes());
        raw
    }
}

/// The longest start of `text` that fits in `max` bytes without splitting a character.
fn truncate(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// One page of all automatic shoutouts. Bit `n` of `enabled` is shoutout `n`'s switch.
#[derive(Clone, PartialEq)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub enabled: u32,
}

pub const ENABLED_BITS: u32 = (1 << AUTO_COUNT) - 1;

impl Page {
    /// The page in the game's slot. The caller holds the slot's lock.
    pub fn read(slot: &Slot) -> Self {
        let entries = (0..AUTO_COUNT).map(|i| Entry::from_raw(&slot.entry(i))).collect();
        Self { entries, enabled: slot.enabled() & ENABLED_BITS }
    }

    /// Puts this page in the game's slot, leaving the mask bits the game doesn't use alone. The
    /// caller holds the slot's lock.
    pub fn write(&self, slot: &Slot) {
        for (i, entry) in self.entries.iter().enumerate() {
            slot.set_entry(i, &entry.to_raw());
        }
        slot.set_enabled((slot.enabled() & !ENABLED_BITS) | self.enabled);
    }

    pub fn is_on(&self, index: usize) -> bool {
        self.enabled & (1 << index) != 0
    }
}

pub struct Hunter {
    pub slot: u32,
    pub name: String,
    pub shown: usize,
    pub pages: Vec<Page>,
}

impl Hunter {
    /// A hunter whose pages all start as the shoutouts it has now.
    pub fn new(slot: u32, name: String, live: Page, count: usize) -> Self {
        Self { slot, name, shown: 0, pages: vec![live; count] }
    }

    /// Makes the number of pages `count`, keeping the shown one. Returns whether anything changed.
    fn fit(&mut self, count: usize) -> bool {
        let before = (self.pages.len(), self.shown);
        if self.shown >= self.pages.len() {
            self.shown = 0;
        }
        if self.pages.len() < count {
            let shown = self.pages[self.shown].clone();
            self.pages.resize(count, shown);
        } else if self.pages.len() > count {
            if self.shown >= count {
                self.pages.swap(self.shown, count - 1);
                self.shown = count - 1;
            }
            self.pages.truncate(count);
        }
        before != (self.pages.len(), self.shown)
    }
}

#[derive(Default)]
pub struct Store {
    pub hunters: Vec<Hunter>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
struct File {
    hunter: Vec<FileHunter>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FileHunter {
    slot: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    shown: usize,
    /// Called `Copy` before 0.0.3.
    #[serde(default, alias = "Copy")]
    page: Vec<FilePage>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default, rename_all = "PascalCase")]
struct FilePage {
    on: Vec<bool>,
    text: Vec<String>,
}

impl Store {
    /// Reads the file, fitting every hunter to `count` pages. Returns the problems to log, and
    /// whether the store differs from the file and should be written.
    pub fn load(path: &Path, count: usize) -> (Self, Vec<String>, bool) {
        let mut problems = Vec::new();
        let Ok(text) = fs::read_to_string(path) else { return (Self::default(), problems, false) };
        let file: File = match toml::from_str(&text) {
            Ok(file) => file,
            Err(e) => {
                let bad = path.with_extension("toml.bad");
                let _ = fs::copy(path, &bad);
                problems.push(format!(
                    "error: {} can't be read, starting over. The old file is kept as {}: {e}",
                    file_name(path),
                    file_name(&bad)
                ));
                return (Self::default(), problems, true);
            }
        };
        let mut store = Self::default();
        let mut changed = false;
        for hunter in file.hunter {
            if hunter.page.is_empty()
                || store.hunters.iter().any(|h| h.slot == hunter.slot && h.name == hunter.name)
            {
                problems.push(format!(
                    "sets: skipped a second or empty [[Hunter]] for slot {} ({})",
                    hunter.slot, hunter.name
                ));
                changed = true;
                continue;
            }
            let pages = hunter
                .page
                .into_iter()
                .map(|page| {
                    let entries = (0..AUTO_COUNT)
                        .map(|i| Entry { text: page.text.get(i).cloned().unwrap_or_default() })
                        .collect();
                    let enabled = (0..AUTO_COUNT)
                        .filter(|&i| page.on.get(i).copied().unwrap_or(false))
                        .fold(0, |mask, i| mask | 1 << i);
                    Page { entries, enabled }
                })
                .collect();
            let mut hunter = Hunter { slot: hunter.slot, name: hunter.name, shown: hunter.shown, pages };
            changed |= hunter.fit(count);
            store.hunters.push(hunter);
        }
        (store, problems, changed)
    }

    pub fn to_toml(&self) -> String {
        let file = File {
            hunter: self
                .hunters
                .iter()
                .map(|hunter| FileHunter {
                    slot: hunter.slot,
                    name: hunter.name.clone(),
                    shown: hunter.shown,
                    page: hunter
                        .pages
                        .iter()
                        .map(|page| FilePage {
                            on: (0..AUTO_COUNT).map(|i| page.is_on(i)).collect(),
                            text: page.entries.iter().map(|e| e.text.clone()).collect(),
                        })
                        .collect(),
                })
                .collect(),
        };
        let body = toml::to_string_pretty(&file).unwrap_or_default();
        format!("{HEADER}{body}")
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
