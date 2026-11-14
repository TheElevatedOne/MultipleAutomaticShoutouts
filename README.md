# MultipleAutomaticShoutouts

![Static Badge](https://img.shields.io/badge/CLAUDE-VIBECODED?style=for-the-badge&logo=claude&label=VIBECODED&labelColor=%23333333&color=%23de7356)

A Monster Hunter: World plugin for Stracker's Loader that gives the Automatic Shoutouts menu several pages and,
each time a situation comes up, sends the shoutout of a random page that has it turned on. Only you need it; the
other hunters see ordinary shoutouts.

> [!WARNING]
> The plugin changes your save's shoutout texts while it runs (it puts the page you pick in the save) and writes
> the other pages to a file next to it. Back up your save first, like with any plugin that touches it.

## What it does

With `Pages = 5`, the Automatic Shoutouts menu has 5 pages, each with its own text and switch for every situation
("when a quest starts", "when you faint", …). Say you write 4 different texts for "when you faint" and turn it off
on the fifth page. Each time you faint, one of the 4 is sent at random, then another random one of the 4 the next
time, and so on.

- **Edited in the game's own menu.** The Auto tab of **Communication → Shoutouts** shows one page at a time. Press
  <kbd>F7</kbd>, or click the right stick on the Auto tab, to turn to the next page. The list redraws at once.
- **Shown in the menu's button guide** as an entry like the game's own: the right stick's key box, as the game
  draws it for your controller, then `Cycle Through Pages (2/5)`. With `Pages = 1` the hint is greyed out.
- **A line in the chat log** also says the page, such as `Automatic Shoutouts: 2/5 Pages`.
- **A situation turned off on every page sends nothing**, as with the game's own switch.
- **An empty text sends the game's default text**, as the game does.
- **Pages are kept per hunter** in `MultipleAutomaticShoutouts.sets.toml`.
- **New hunters start with 5 copies of what they have now**, so nothing changes until you edit a page.

## Requirements

- `MonsterHunterWorld.exe` version 15.23.00 (PE timestamp `0x66D13CB6`). On any other version the plugin logs
  the mismatch and changes nothing.
- [Stracker's Loader][loader].
- A CRC bypass plugin, such as `!CRCBypass.dll`. The plugin changes 31 bytes of the game's code in two functions
  and two entries of the Shoutouts menu's vtable, which the game's integrity check would otherwise catch.

## Install

1. Download `MultipleAutomaticShoutouts-<version>.zip` from [Releases][releases] and unpack it into the
   `Monster Hunter World` folder, or copy `MultipleAutomaticShoutouts.dll` and `MultipleAutomaticShoutouts.toml` to
   `Monster Hunter World/nativePC/plugins/` yourself, or run `make install`.
2. Start the game. The plugin writes `MultipleAutomaticShoutouts.log` next to the DLL.

A working start looks like this:

```text
MultipleAutomaticShoutouts 0.0.5
game ready after 0.4s
automatic shoutouts: hooked, 5 pages
switch key: hooked, F7 turns the page in the menu, and RightStick does while the Auto tab is open
shoutouts menu: hooked, its button guide shows the switch
sets: slot 0 (Hunter): 5 pages of its automatic shoutouts made from the save
```

## Editing the pages

1. Open **Communication → Shoutouts** and go to the Auto tab.
2. Press <kbd>F7</kbd> or click the right stick until the button guide shows the page you want, for example
   `Cycle Through Pages (2/5)`. The list shows that page's texts and switches at once.
3. Change texts and switches as usual. The plugin copies your changes into its file within a few seconds.
4. Repeat for the other pages.

The page you leave showing is the one in your save, and also what the game uses without the plugin. The key also
works with the menu closed; the button only works on the Auto tab, because the right stick moves the camera in the
field.

## Configuration

The file is `MultipleAutomaticShoutouts.toml`, next to the DLL. The plugin creates it with the defaults if it's
missing, and reads it once at game start.

| Key | Default | Description |
|---|---|---|
| `Pages` | `5` | Pages of automatic shoutouts, from 1 to 20. With `1` the game sends its own shoutouts and the guide hint is greyed out. Called `Copies` before 0.0.3, which still works |
| `SwitchKey` | `"F7"` | Turns the page the menu shows, while the game window has focus. `F1`–`F24`, `Insert`, `PageUp`, `PageDown`, or a virtual-key code such as `"0x76"`. See [Keys and buttons left out](#keys-and-buttons-left-out) |
| `ControllerButton` | `"RightStick"` | Does the same while the Shoutouts menu is open on its Auto tab, and is the icon in the button guide. `RightStick`, `LeftStick`, `Back`, or `None` (the guide then shows `[F7]`). Xbox names, see [below](#controller-button-names) |
| `GuideText` | `"Cycle Through Pages"` | The hint's text in the button guide, followed by the page, such as `(2/5)` |
| `HintOnOwnLine` | `true` | Only for `ControllerButton = "None"`, whose hint is text after "Auto-translation On/Off": puts it on a line of its own. With `false` it follows on the same line |

### Controller button names

`ControllerButton` uses the Xbox controller's names. PlayStation and Nintendo controllers label the same buttons
differently:

| `ControllerButton` | Xbox | PlayStation | Nintendo Switch |
|---|---|---|---|
| `RightStick` | Right stick click | R3 | Right stick click |
| `LeftStick` | Left stick click | L3 | Left stick click |
| `Back` | View | Create / Share | − (Minus) |

The button guide shows the game's icon for your controller either way.

### Keys and buttons left out

The switch can't be a key or button the Shoutouts menu's Auto tab already uses, as its button guide shows them:

| Input | Keyboard | Controller (Xbox / PlayStation / Nintendo) |
|---|---|---|
| Back, close the menu | Esc, right click | B / Circle / A, and Menu / Options / + |
| Shoutout On/Off | N | X / Square / Y |
| Edit Shoutout | R | Y / Triangle / X |
| Auto-translation On/Off | C | RT / R2 / ZR |
| Change tab | Q, E | LB, RB / L1, R1 / L, R |
| Confirm | F, Enter, Space | A / Cross / B |
| Move the cursor, turn the menu's pages | WASD, arrows, mouse wheel | D-pad, left stick |

The key also works while you type a shoutout, so letters, digits, the numpad, punctuation, Backspace, Tab, Delete,
Home and End are left out too, as are Shift, Ctrl, Alt and the mouse buttons. A config naming one of them falls
back to the default, and the log says why.

When you lower `Pages`, the pages past the new number are dropped, except the one in the menu, which moves to the
last place. When you raise it, the new pages start as copies of the one in the menu.

### The pages file

`MultipleAutomaticShoutouts.sets.toml` holds every hunter's pages. The plugin writes it; you can also edit it
while the game is closed.

```toml
[[Hunter]]
Slot = 0          # save slot, 0 to 2
Name = "Hunter"   # a hunter with another name in the slot gets its own [[Hunter]]
Shown = 1         # the page in the save and menu, counted from 0

[[Hunter.Page]]
On = [true, true, false, ...]                 # 18 switches, in the menu's order
Text = ["Let's go!", "", "Sorry!", ...]       # 18 texts; empty sends the default text
```

Texts longer than 127 bytes are cut. Files from 0.0.2 with `[[Hunter.Copy]]` are read as pages. If the file can't
be read, the plugin keeps it as `MultipleAutomaticShoutouts.sets.toml.bad` and starts over from the save.

## How it works

The game keeps three hunter save slots of 0x26CC00 bytes in `sUserData` (pointer at RVA `0x5013950`; slots at
`+0xA8`, current index at `+0xA0`). A slot has 32 automatic shoutout entries at `+0x13F2E8`, 0x84 bytes each: the
text's length, then the text, up to 127 bytes and a NUL. The game uses the first 18. Which ones are on is a bit
mask at `+0x140370`. The slot's critical section at `+8` guards both, and the menu holds it while it changes them.

Every situation sends its shoutout through `sChat::autoShoutout(this, index)` (RVA `0x1A52450`), which checks the
bit and builds the message from the slot's text. The plugin hooks it and:

1. copies the slot's 18 entries and mask into the shown page, so a change just made in the menu counts,
2. picks one of the pages that have this situation on, at random, and
3. if that's not the shown page, puts its text and switch in the slot, calls the game's function, and puts the
   slot's own entry back. Until then, the plugin neither copies the slot nor turns the page.

It also hooks `sChat`'s per-frame update (vtable slot 6, RVA `0x1A511F0`). There it reads the switch key and the
controller (XInput), swaps the shown page's entries in the slot for the next page's, and posts the chat log line
through `sChat::addSystemLog` (RVA `0x1A540D0`), as the game's `SetSystemChatLog` script function does. Every few
seconds it copies the slot into the shown page and, when that changed, writes the file from a background thread.

The game queues system messages at `sChat+0x20620` and moves one to the chat log every 1.5 s, so behind its own
notices a line would wait several seconds. The plugin's line has its own category (adding one drops a line from an
earlier press that's still waiting), moves to the front of the queue, and the queue's timer at `+0x68` is set to
its length at `+0x6C`, so the next update shows it.

The Shoutouts menu is `uGUIFixedPhrase`. The plugin replaces two entries of its vtable (RVA `0x346E038`): slot 59,
which `uGUI::update` calls every frame, and the destructor. Slot 59 tells whether the menu is open (phase 2 at
`+0x1D04`) on the Auto tab (2 at `+0x28F0`). After a switch it calls the menu's `refreshPage(menu+0x28F8, tab,
page)` (RVA `0x1E0D110`) with the Auto tab's page from `+0x3A98`, the function the menu uses to fill a page when it
opens or turns, on the menu's own update.

The menu builds its button guides once (RVA `0x1E0E850`) as lists of up to 16 0x18-byte entries at `+0x30`, with
the count at `+0x1B0`. An entry holds the button's guide id (u32), the inputs it shows for (u32, `0x1F`), a text
pointer, and a u32 kind: 0 for an ordinary entry, 2 for the Manual tab's "Recipient: …" entry, whose text the game
fills in itself. The Auto tab uses the list at `+0x3440`, or `+0x3640` on a row that isn't unlocked yet. The
plugin adds an ordinary entry to both with the button's guide id and the hint's text, and the game lays it out
with its other entries. The ids were read off the game's own guide with a test build (`cargo build --features
guide-ids`), which lists every id with its icon: `0x0B` is the right stick click (Tab on the keyboard), `0x0A`
the left stick click (Shift), `0x11` View (Ctrl). With `None`, the hint goes after the last entry's text instead,
as `[F7]` and the text, after a line break or spaces.
After a switch it shows the guide again through the menu's `0x1E0CC00`, as a cursor move does. With one page the
hint is wrapped in a grey `<STYL …>`: the plugin asks the game's style lookup (RVA `0x2340990`) for a list of
likely grey style names and uses the first it has. The game ignores a style it doesn't have.

## Known issues

- The grey style is found by trying names the game might use. If it has none of them, the log says so and the hint
  stays white with one page.
- With the keyboard, the hint shows the game's key for the controller button (Tab for the right stick click), but
  the switch key is <kbd>F7</kbd> unless you changed `SwitchKey`.
- The controller button is read through XInput. If Steam Input hides your controller from XInput, use the key, or
  tell me and the button can be read from the game's own input instead.
- Pressing the switch while a text is being typed in the menu turns the page underneath it, so the text you confirm
  goes to the new page.
- The game appears to send automatic shoutouts only when you're in a session with other hunters, so you won't see
  them, or this plugin's choices, while playing alone.
- The plugin knows a hunter by save slot and name. Deleting a hunter and making one with the same name in the same
  slot keeps the old hunter's pages. A hunter with another name in that slot gets its own pages, and the old ones
  stay in the file.
- If the game saved to disk in the moment a picked page is in the slot, that one text would be saved in place of
  the shown page's. The window is a single function call.

## Building

The Makefile uses the Rust and MinGW toolchain in `~/workdir/elden/ERBetterLod/.toolchain`. Run
`make toolchain` there first if it's missing, or point `TOOLCHAIN` at another copy.

```sh
make                # builds target/x86_64-pc-windows-gnu/release/MultipleAutomaticShoutouts.dll
make install        # copies the DLL and config to the game's nativePC/plugins
make install PLUGINS_DIR=/path/to/nativePC/plugins
make uninstall      # keeps MultipleAutomaticShoutouts.sets.toml
```

`make install` keeps a config you already have.

[loader]: https://www.nexusmods.com/monsterhunterworld/mods/1982
[releases]: https://github.com/TheElevatedOne/MultipleAutomaticShoutouts/releases
