# Changelog

## 0.0.5

- Changed: `SwitchKey` and `ControllerButton` can no longer be a key or button the Shoutouts menu's Auto tab
  already uses, such as Esc, N, R, C, Q, E, F, WASD, B, X, Y, LB/RB, RT, Start or the D-pad, nor a key that types
  or edits text, which the switch also caught while you typed a shoutout. `SwitchKey` takes F1–F24, Insert, PageUp
  and PageDown; `ControllerButton` takes RightStick, LeftStick and Back. A config naming another falls back to the
  default and the log says why.
- Added: `HintOnOwnLine` (`true` by default). With `false` the hint follows "Auto-translation On/Off" on the same
  line, as in 0.0.3, in case its own line doesn't show right.
- Fixed: while the game sent a shoutout from another page, a save-to-file copy or a page turn on another thread
  could take that one text for the shown page's. Both now wait until the game is done.

## 0.0.4

- Changed: the button guide hint has a line of its own under "Auto-translation On/Off". Before, both shared one
  line and the guide squeezed them to fit.

## 0.0.3

- Added: the Auto tab's button guide shows the switch after "Auto-translation On/Off", with the game's own icon
  for the controller button, as in `<R3 icon> Cycle Through Pages (2/5)`. The page number follows each switch.
  The text is set with `GuideText`.
- Added: with `Pages = 1` the hint is greyed out, and the game sends its own shoutouts.
- Changed: copies are now called pages. The config key is `Pages` (`Copies` still works), the pages file has
  `[[Hunter.Page]]` (files with `[[Hunter.Copy]]` are still read), and the chat log line reads
  `Automatic Shoutouts: 2/5 Pages`.
- Fixed: a second hunter in the same save slot was dropped from the pages file when it was read.

## 0.0.2

- Changed: switching copies with the Shoutouts menu open now redraws its Auto tab at once. Before, it kept showing
  the old copy until you reopened it.
- Added: `ControllerButton` (right stick by default) switches the copy while the menu's Auto tab is open.
- Changed: the chat log line after a switch shows on the next frame instead of waiting behind the game's own
  notices, and a newer press replaces an older line still waiting instead of stacking.
- Fixed: while the game starts, the save slot briefly holds a placeholder hunter ("PLAYER 1"), and the plugin
  replaced your hunter's copies with copies of it. Each hunter is now kept by slot and name, and new ones are only
  taken on once you're playing.

## 0.0.1

- Added: several copies of each automatic shoutout (`Copies`, 5 by default). Each time a situation comes up, one
  of the copies that have it turned on is sent at random; with every copy off, nothing is sent.
- Added: `SwitchKey` (<kbd>F7</kbd> by default) switches which copy the game's Automatic Shoutouts menu shows and
  edits, and says which one in the chat log.
- Added: each hunter's copies are kept in `MultipleAutomaticShoutouts.sets.toml`, which follows your changes in
  the menu. New hunters start with copies of their current shoutouts.
