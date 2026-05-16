# mandleROT numpad manual

The hardware is a cheap 19-key USB numpad mounted on the rig **rotated 90°
counter-clockwise** relative to the printed PC numpad layout. All
references below are from the operator's point of view (the rotated frame).

The keypad doubles as the live-performance controller. The keyboard is
still wired (see `keymap.toml`) but the numpad is the hot path.

---

## Physical layout

```
   ┌──────┬──────┬──────┬──────┐
   │  −   │  +   │ Bksp │ Enter│             ← row 1 (top)
   ├──────┼──────┼──────┼──────┼──────┐
   │  ×   │  9   │  6   │  3   │   .  │      ← row 2
   ├──────┼──────┼──────┼──────┼──────┤
   │  /   │  8   │  5   │  2   │ 000  │      ← row 3
   ├──────┼──────┼──────┼──────┼──────┤
   │NumLk*│  7   │  4   │  1   │   0  │      ← row 4 (bottom)
   └──────┴──────┴──────┴──────┴──────┘
   * physical key exists but emits no event on this unit's firmware
```

The digit block (`9 6 3 / 8 5 2 / 7 4 1`) reads naturally as slots
**1-9 top-left to bottom-right** after the 90° rotation — the keymap
remaps the physical key codes accordingly. The two `0`-emitting keys
are different: the bottom-right `0` sends `KP0` once; the row-3 `000`
sends `KP0` three times in rapid succession and is treated as a
**held modifier**, not a slot key.

---

## Default mode (no modifier held)

```
   ┌──────┬──────┬──────┬──────┐
   │ xf−  │ xf+  │ Trig │ Lyr⇄ │
   ├──────┼──────┼──────┼──────┼──────┐
   │ scn◀ │Slot 1│Slot 2│Slot 3│ Mode▶│
   ├──────┼──────┼──────┼──────┼──────┤
   │ scn▶ │Slot 4│Slot 5│Slot 6│ 000  │
   ├──────┼──────┼──────┼──────┼──────┤
   │ dead │Slot 7│Slot 8│Slot 9│ Tap  │
   └──────┴──────┴──────┴──────┴──────┘
```

| Key      | Action            | Notes |
| -------- | ----------------- | ----- |
| `−`      | Xfade −           | Nudge A↔B blend toward A. |
| `+`      | Xfade +           | Nudge toward B. |
| `Bksp`   | Trigger           | One-shot pulse to `u_trigger`. |
| `Enter`  | Toggle active layer (A ↔ B) | |
| `×` (`*`)| Scene cycle −1 on ACTIVE layer | Previous scene. |
| `/`      | Scene cycle +1 on ACTIVE layer | Next scene. Pairs with `×` for one-finger scrubbing. |
| `.`      | Advance mode      | Param → Look → Scene → Param |
| `9`–`1`  | Slot 1-9 select / recall | What it does depends on the current mode. See below. |
| `0`      | Tap tempo         | Repeatedly tap to set BPM. |
| `000`    | Held modifier — no action when tapped alone | |
| `NumLk`  | (dead) — physical key emits no event | |

### Slot key behaviour by mode

| Mode    | Slot key action                                |
| ------- | ----------------------------------------------- |
| Scene   | Recall the scene bound to that slot.           |
| Param   | Select that param slot for ±/audio edits.      |
| Look    | Recall the saved Look stored in that slot.     |

Default-mode boot state is **Param** — slot keys edit the active layer's
param values straight away. Mode label is shown top-left on the SPI
status panel.

---

## With `000` held

`000` is a sticky modifier: tap it once and the next non-`0` keypress
within ~600 ms takes the `000` overlay. Hold it down (the firmware fires
the burst repeatedly) and the window keeps re-arming. Pressing `000`
alone does nothing.

```
   ┌──────┬──────┬──────┬──────┐
   │ Pm−  │ Pm+  │ Blnd │AudByp│
   ├──────┼──────┼──────┼──────┼──────┐
   │ scn◀'│ ·    │ ·    │ ·    │Sttngs│
   ├──────┼──────┼──────┼──────┼──────┤
   │ scn▶'│ ·    │ Rst  │ ·    │ 000  │
   ├──────┼──────┼──────┼──────┼──────┤
   │  ·   │ ·    │ ·    │ ·    │Freeze│
   └──────┴──────┴──────┴──────┴──────┘
```

| Combo             | Action                                       |
| ----------------- | -------------------------------------------- |
| `000` + `−`       | Param step − on selected param (PARAM mode)  |
| `000` + `+`       | Param step + on selected param (PARAM mode)  |
| `000` + `Bksp`    | Blend mode cycle (Mix → Add → Mult → …)      |
| `000` + `Enter`   | Toggle audio bypass                          |
| `000` + `×` (`*`) | Scene cycle −1 on the **inactive** layer     |
| `000` + `/`       | Scene cycle +1 on the **inactive** layer     |
| `000` + `.`       | Open Settings menu                           |
| `000` + `5`       | Reset all params on the active layer         |
| `000` + `0`       | Freeze toggle (latch current output frame)   |
| `000` + anything else | no-op                                    |

`000+−`/`000+=` are no-ops outside PARAM mode — unmodified `−`/`+` always
drive xfade, so the modifier is what gates param edits from the pad.

The PREV-other combo lets you scrub the inactive layer backwards without
first flipping which layer is active — pair with `Enter` (toggle layer)
to swap the cued scene onto screen.

---

## In menus (settings, slots, scene list, post-FX…)

When a menu screen is open, the keymap is bypassed: every key goes
straight to the top screen. The rotated numpad's centre cross is
translated into arrow keys before the screen sees them, so the
3×3 digit block reads as a natural d-pad from the operator's POV:

```
   ┌──────┬──────┬──────┬──────┐
   │  Up  │ Down │ Esc  │ OK   │     (− = Up, + = Down on row 1)
   ├──────┼──────┼──────┼──────┼──────┐
   │  ·   │  ·   │  Up  │  ·   │  ·   │
   ├──────┼──────┼──────┼──────┼──────┤
   │  ·   │ Left │  ·   │ Right│ 000  │
   ├──────┼──────┼──────┼──────┼──────┤
   │  ·   │  ·   │ Down │  ·   │  ·   │
   └──────┴──────┴──────┴──────┴──────┘
```

| Key on pad | In menus      | Notes |
| ---------- | ------------- | ----- |
| `−`        | Cursor Up     | settings & list screens |
| `+`        | Cursor Down   | settings & list screens |
| `Bksp`     | Back (pop)    | also leaves the menu stack |
| `Enter`    | Confirm       | open / commit selection |
| `6` (top-centre digit) | Up    | numpad d-pad |
| `4` (bottom-centre digit) | Down | numpad d-pad |
| `8` (left-centre digit) | Left | page / value − |
| `2` (right-centre digit) | Right | page / value + |
| corners `1 3 7 9` | (per screen) | slot screen ignores; some screens use as digit jumps |
| `0`        | Clear / no-op | e.g. clear slot binding on the Slots screen |

The double-tap Esc / Backspace PANIC and the `−`+`+`+`Enter` numpad
chord still close the menu stack on top of firing the safe-fallback.

---

## Emergency chord

Press `−` **and** `+` **and** `Enter` together (any order, all three
held within 400 ms) → **PANIC**: both layers → Safe Fallback (SMPTE
bars), xfade → 0.5, audio bypass ON, mode → Scene. The unconditional
escape hatch — works even with menus open.

Keyboard-side equivalent: double-tap `Esc` or double-tap `Backspace`
within 400 ms.

---

## Firmware quirks worth knowing

- **NumLock auto-wrap**. The firmware wraps every press of the
  leftmost-column digit keys (`7`, `4`, `1`, `0` and the row-3 `000`)
  with `NumLock-press → key → NumLock-press`, so the digit reports its
  number scancode regardless of OS NumLock state. The keymap defers
  every NumLock press by 250 ms and drops any whose ±window contains a
  digit press — you should never see a stray scene-cycle from pressing
  a digit.

- **Corner `NumLock` key is dead on this unit.** Pressing it emits no
  evdev event at all (the firmware reserves NumLock signalling for the
  auto-wrap pattern). The keymap leaves that slot unbound.

- **`000` ≠ `0`**. The bottom-right `0` (row 4) and the row-3 `000` key
  both emit `KEY_KP0`. The `000` key fires **three KP0 events within
  ~50 ms**; `src/input/evdev_src.rs` buffers KP0 presses for 120 ms and
  classifies any triple in that window as a `000` tap (sets the sticky
  modifier). A single `0` press eventually flushes as TapTempo after
  the burst window expires — i.e. there's a ~120 ms latency on `0`
  taps that's tolerable for BPM use.

- **No physical Shift on the numpad.** The keyboard's `Shift+−` /
  `Shift+=` routes a param's audio band (PARAM mode only). Numpad
  audio routing is set via the Settings menu opened with `000+.`.

---

## Quick reference card

```
DEFAULT                           WITH 000 HELD
┌─────┬─────┬─────┬─────┐         ┌─────┬─────┬─────┬─────┐
│ xf− │ xf+ │Trig │Lyr⇄ │         │ pm− │ pm+ │Blnd │AudBy│
├─────┼─────┼─────┼─────┼─────┐   ├─────┼─────┼─────┼─────┼─────┐
│scn◀ │  1  │  2  │  3  │Mode▶│   │scn◀'│  ·  │  ·  │  ·  │Sttng│
├─────┼─────┼─────┼─────┼─────┤   ├─────┼─────┼─────┼─────┼─────┤
│scn▶ │  4  │  5  │  6  │ 000 │   │scn▶'│  ·  │Reset│  ·  │ 000 │
├─────┼─────┼─────┼─────┼─────┤   ├─────┼─────┼─────┼─────┼─────┤
│dead │  7  │  8  │  9  │ Tap │   │  ·  │  ·  │  ·  │  ·  │Freez│
└─────┴─────┴─────┴─────┴─────┘   └─────┴─────┴─────┴─────┴─────┘

IN MENUS (rotated-numpad → arrows)
┌─────┬─────┬─────┬─────┐
│ Up  │Down │Back │ OK  │      − / + cursor, Bksp pop, Enter confirm
├─────┼─────┼─────┼─────┼─────┐
│  ·  │  ·  │ Up  │  ·  │  ·  │  centre cross of digit block
├─────┼─────┼─────┼─────┼─────┤  remaps to arrows:
│  ·  │Left │  ·  │Right│ 000 │   6=Up  4=Down  8=Left  2=Right
├─────┼─────┼─────┼─────┼─────┤
│  ·  │  ·  │Down │  ·  │ Clr │
└─────┴─────┴─────┴─────┴─────┘
```

`scn◀` / `scn▶` cycle the **active** layer's previous / next scene; the
prime forms `scn◀'` / `scn▶'` cycle the **inactive** layer. Pair with
`Enter` (toggle layer) to swap which side is on screen. `pm−` / `pm+`
are PARAM-mode-only — outside PARAM mode the `000`+`±` chord is a no-op
and unmodified `−`/`+` still drive xfade.
