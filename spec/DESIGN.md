# dps150 — UI Design

Status: baseline, 2026-10-30. Authority for everything the user sees: layout, colour, glyphs, copy and interaction of the TUI. Behaviour rules (limits, confirmations, link loss, safety) are in [SAFETY.md](SAFETY.md); data binding, polling and files are in [APP.md](APP.md). It is written to be implemented in any language and any TUI library. Wire-protocol facts live in [PROTOCOL.md](PROTOCOL.md); product scope, architecture and the decision log live in [MAIN.md](MAIN.md). This file does not repeat them; it refers to them.

## 0. How to read this document

- **Normative:** the tables, numbers, rules and the reference renderings in Appendix A. Where prose and a rendering disagree, the numbers in the tables win; report the disagreement.
- **Reference renderings** show the *characters and their positions only*. Colour is specified separately (§3, §5, §6, §8). They use the example reading in §0.2 and are meant to be used as golden files: a conforming implementation renders these exact characters at these exact sizes for these exact states. The files are in `tests/golden/`; Appendix A is their index. {#UI-02}
- Status tags on decisions:
  - **[D]** decided with the user and shown in a reviewed mockup.
  - **[P]** proposed by the designer to complete the design; not yet reviewed by the user. Implement it, but expect it to change.
  - **[U]** depends on device behaviour that is unverified (PROTOCOL.md §8). Implement behind a clearly named constant or feature check.
- Terms: *cell* = one terminal character position. *Row* / *column* count cells. "Inner width" of a box = box width − 4.
- Numbered list items are cited as §N.n: §1.5 is principle 5 of §1, §6.7 is subsection 7 of §6.

### 0.1 Design in one paragraph

The UI is a bench-instrument *Front Panel*: two large block-digit readouts (voltage, current), each showing the measured value above the set value. The set value is edited one digit at a time with a cursor, like an encoder knob. Everything else (mode, output, protections, power) sits in a status box below. Three secondary screens (Presets, Protection, Settings) keep the same visual language. The colour scheme is *Midnight Slate*: a cool dark blue-grey ground, one colour per measured quantity, one gold accent meaning "you can change this", and green/orange/red used only for state.

### 0.2 Example reading used by every rendering

Output ON, constant-voltage: set 5.00 V / 1.500 A; measured 4.998 V, 0.512 A, 2.56 W; input 20.25 V; temperature 24.3 °C; capacity 0.012 Ah; energy 0.060 Wh; live maximum 20.05 V / 5.10 A; limits OVP 30.00 V, OCP 5.10 A, OPP 150.0 W, OTP 80.0 °C, LVP 5.00 V; presets M1..M6 = 3.3/1.0, 5.0/1.0, 9.0/2.0, 12.0/2.0, 5.0/0.5, 24.0/2.0 (V/A), named `3V3 logic`, `USB 5V`, `9V pedal`, `12V fan`, `LED test`, `24V rail`. M6 is above the live maximum, so it is shown blocked (§8.1). Output-OFF and confirmation renderings use the same setpoints with measured values 0.000 V / 0.000 A / 0.00 W. The protection-trip renderings use an **OTP** trip, because an over-temperature trip is plausible at any setpoint. These are examples; they are *not* defaults to hard-code.

## 1. Principles

1. **Numbers first.** The measured and set values are the largest and brightest things on screen. Chrome (borders, labels, key hints) is low contrast.
2. **One colour, one meaning.** Voltage, current and power each have one colour wherever they appear. Gold means editable. Green/orange/red mean state only. Never decorate with state colours.
3. **Colour is never the only carrier.** Mode is a word in a badge, protection is a changed glyph plus a message, limiting is spelled out, focus is a `▸` plus a title. The UI must remain readable with colour off. {#UI-05}
4. **Never show stale or unknown numbers as live.** If the device stops answering, measured values are replaced by dashes and the output state is shown as unknown (`?`), never as OFF. {#LNK-02}
5. **Output OFF is always one key.** `Space` toggles output from every screen. Where `Space` is not a toggle (typed entry, prompts, the too-small screen) it cancels what is pending and can only turn the output OFF, never ON. `Ctrl-X` turns the output OFF in every mode, including text fields. {#SAF-01} {#SAF-02}
6. **Fluid above a floor.** Below 80×21 a fixed Compact layout (44×16 minimum). From 80×21 up, one fluid layout that stretches to fill the whole terminal, like a btop panel: boxes take a share of the width and height, bars lengthen, and the big digits step up to a larger font when the box allows (§2.2). No cap on size, no centring in the fluid layout. [D]
7. **Quiet.** No animation, blinking, spinners or mouse interaction in v1. [D] {#UI-13}
8. **No silent substitutions.** A value the user typed is either applied exactly or rejected with a reason; it is never silently replaced by a different value. Every prompt that can energise a load shows the voltage and current it will apply. {#LIM-08}

## 2. Terminal requirements and sizing

### 2.1 Requirements [D]

- UTF-8 terminal required. If the locale is not UTF-8, print one line to stderr ("dps150 needs a UTF-8 terminal") and exit with a non-zero status before entering the alternate screen. [D] {#UI-10}
- Every glyph in §4.1 is treated as **1 cell wide**. Some of them (○ ● ▸ ◂ ✓) are "ambiguous width" in East-Asian-locale terminals; those terminals are unsupported in v1.
- The application uses the alternate screen, hides the cursor, and restores both on exit and on panic/signal. [D] {#UI-11}

### 2.2 Layouts and sizing [D]

There are two layouts: **Compact** (fixed, the floor) and **Fluid** (stretches to fill the terminal like a btop panel). Fluid has two width bands, *Standard* and *Large*, that differ only in what is shown. The layout and band are chosen from the terminal size `cols × rows`, tested in this order: {#UI-01}

| Layout / band | Condition | Minimum size | Reference sizes (Appendix A) |
|---|---|---|---|
| Fluid, Large band | cols ≥ 100 and rows ≥ 21 | 100×21 | 100×21, 120×40 |
| Fluid, Standard band | cols ≥ 80 and rows ≥ 21 | 80×21 | 80×24 |
| Compact | cols ≥ 44 and rows ≥ 16 | 44×16 | 44×16 (main panel 44×14) |
| Too small | otherwise | — | 43×16 |

**Why rows ≥ 21.** The layout must fit the *tallest* screen of its kind, so that switching screens never changes it: Fluid is bound by Settings (21 rows), Compact by Presets and Protection (16 rows). An 80×20 terminal therefore uses the Compact layout, centred, rather than a Fluid layout that would clip the Settings screen. This is intended.

**Fluid frame.** {#UI-14}
1. The body fills the full terminal width and every row between the header (and live strip) and the key bar. The key bar is the last row. There are no side margins and no maximum size.
2. Header, live strip and key bar span the full width and are laid out for the actual width (§5.8, §5.9).
3. Content never stretches or is truncated to fill a box. What grows is boxes, bars, gaps and the digit font; spare room is left blank.

**Main panel in the Fluid layout.**
1. *Rows.* header 1 + readout boxes R + Status 4 + Memory (Large band 3, Standard band 1) + key bar 1. So R = rows − 9 (Large band) or rows − 7 (Standard band). The readout boxes take all the spare height: R is 12 at 100×21, 14 at 80×21, 17 at 80×24 and 31 at 120×40.
2. *Columns.* The left (VOLTAGE) readout box is `cols − ⌊cols/2⌋` wide and the right (CURRENT) box `⌊cols/2⌋`, so the left takes the odd cell. Status and Memory span the full width.
3. *Content block.* Each readout box holds a block of `2h + 4` lines, where `h` is the digit font height: `MEASURED`, `h` rows of digits, a blank line, the `SET …` line, `h` rows of digits, the cursor line (§6.2). The block is **centred vertically**: with `spare` = inner height − `2h + 4`, the top padding is `⌊spare/2⌋` and the bottom padding the rest. Left-aligned horizontally with a one-cell margin.
4. *Status line 2.* The power bar is 18 cells at 80 columns (Standard band) or 16 cells at 100 columns (Large band) and gains one cell per extra column; the other fields keep their width (§6.4).
5. *Large extras.* From 100 columns: the Memory box, capacity and energy, the full port path and the hardware version (§5.9, §6.4, §6.5). No further extras at wider sizes.

**Big-digit font.** {#UI-15} Fonts: F3 (3 rows), F5 (5 rows) and F7 (7 rows), defined in §4.2. Each readout box uses the **largest font that fits**. A font of height `h` fits when:
- the inner height ≥ `2h + 4`, and
- the inner width ≥ the widest line the box must show: voltage box `1 + w(DD.DDD) + 3` (margin, digits, `  V`); current box `1 + w(D.DDD) + 3 + 13` (margin, digits, `  A`, `   ◂ limiting`),

where `w(text)` is the width of the digits in the font plus one blank column between every two glyphs (`.` is one cell wide). Both boxes must fit, and both use the same font. The widths work out to:

| Font | Content lines | Inner size needed | Box needed | Fluid terminal needed |
|---|---|---|---|---|
| F3 | 10 | 34 × 10 | 38 × 12 | always met |
| F5 | 14 | 42 × 14 | 46 × 16 | cols ≥ 92 and rows ≥ 23 (Standard band) / 25 (Large band) |
| F7 | 18 | 50 × 18 | 54 × 20 | cols ≥ 108 and rows ≥ 29 (always the Large band) |

Examples: 100×21 and 80×24 use F3; 100×30 uses F5; 120×40 uses F7. The choice is re-evaluated on every resize and never clips a digit. Compact always uses F3. Beyond F7 the digits stay F7 and only the padding grows. Examples of Fluid main panels: `main-large-cv` (100×21, F3), `main-standard-cv` (80×24, F3), `main-cv-120x40` (F7).

**Secondary screens.** Stretching rules for Presets, Protection and Settings are in §8.5.

### 2.3 Placement

- Header row (§5.9), live strip (§5.10) and key bar (§5.8) span the **full terminal width**; their background colour fills every cell to the right edge. In Compact their content is laid out for the 44-cell design width and the rest is fill; in the Fluid layout it is laid out for the actual width (§5.8, §5.9).
- **Compact:** the body block has the design width (44) and is **centred horizontally**: left margin = ⌊(cols − 44) / 2⌋ cells, painted with the page background. **Fluid:** the body fills the terminal (§2.2); there is no margin. {#UI-03}
- Vertically, Compact is **top-aligned and contiguous**, rows below it painted with the page background. Fluid fills every row (§2.2).
- Resizing re-evaluates the layout, band and digit font immediately and redraws everything. All state (selection, edit in progress, prompts) is preserved while the terminal stays usable. When it becomes too small (§2.4), pending prompts and typed entry are cancelled; nothing is applied. [D] {#UI-12}

### 2.4 Too-small screen [D] {#SAF-10}

Clear the screen, then centre three lines:

1. `Terminal too small` (style `wn`).
2. `need 44×16, have {cols}×{rows}` (style `d`).
3. The output state: `output ON 3.30 V / 0.200 A` (style `wn`), `output OFF` (style `d`), or `output state unknown` (style `er`) when the link is down.

Keep reading input and keep polling. On this screen:

- `Space` and `Ctrl-X` **only turn the output OFF**. They never turn it on, because the confirmation could not be shown (SAFETY.md §2).
- `q` is ignored (the quit prompt, SAFETY.md §4.9, cannot be shown); `Ctrl-C` quits immediately with the exit behaviour of SAFETY.md §4.9.
- No other key does anything, and no other UI is drawn.

Reference: `toosmall-43x16`.

## 3. Colour system — "Midnight Slate"

### 3.1 Palette [D]

| Token | Hex | xterm-256 | 16-colour | Role |
|---|---|---|---|---|
| `bg` | `#0f1420` | 233 | terminal default | Page background (painted explicitly in 24-bit and 256 modes) |
| `bar` | `#182030` | 235 | 8 (bright black) | Header bar and live-strip background |
| `tx` | `#cfd6e4` | 188 | terminal default | Normal text |
| `dim` | `#66718a` | 60 | 8 (bright black) | Labels, hints, secondary text |
| `bd` | `#2c364b` | 237 | 8 (bright black) | Borders and unfilled bar track |
| `ti` | `#8ea2c8` | 110 | 7 (white) | Box titles; neutral bars (temperature, input) |
| `ch1` | `#4fd1c5` | 80 | 6 (cyan) | **Voltage** |
| `ch2` | `#6ea8fe` | 75 | 12 (bright blue) | **Current** |
| `pw` | `#b794f6` | 141 | 13 (bright magenta) | **Power** |
| `set` | `#f6c177` | 216 | 11 (bright yellow) | **Set value / editable** accent |
| `ok` | `#6fd08c` | 78 | 2 (green) | State: OK, ON, connected, running |
| `wn` | `#ff9e64` | 215 | 3 (yellow) | State: warning, limiting, CC |
| `er` | `#ff5c7a` | 204 | 9 (bright red) | State: trip, error, link lost |
| `key` | `#232c3f` | 236 | 8 (bright black) | Key-cap and inactive-badge background |
| `sel` | `#2d3b5e` | 238 | 4 (blue) | Selected row / typed-entry background |

The 256-colour column is the nearest entry in the xterm cube (levels 0, 95, 135, 175, 215, 255) and grey ramp (8 + 10·n), by Euclidean RGB distance, computed once; implementations may hard-code it.

### 3.2 Styles

Every piece of text is drawn with exactly one of these styles. A style is a foreground, an optional background, and attributes. Default cell: fg `tx`, bg `bg`.

| Style | fg | bg | Attributes | Used for |
|---|---|---|---|---|
| `tx` | tx | — | — | Normal text |
| `d` | dim | — | — | Labels, hints, secondary values |
| `bd` | bd | — | — | Box borders, empty part of bars |
| `bdf` | ti | — | — | Border of the focused readout box |
| `ti` | ti | — | bold | Box titles |
| `ch1` | ch1 | — | bold | Voltage digits, unit and bar |
| `ch2` | ch2 | — | bold | Current digits, unit and bar |
| `pw` | pw | — | bold | Power digits, text and bar |
| `set` | set | — | bold | Set-value digits, editable values, selection marker `▸`, title of the focused readout box |
| `cur` | set | — | — | Digit cursor `▔` |
| `ok` | ok | — | — | OK / running text, `✓`, `●` of a healthy state |
| `wn` | wn | — | bold | Warnings, "limiting", blocked values |
| `er` | er | — | bold | Trip and error text, tripped lamp |
| `k` | tx | key | — | Key cap |
| `sel` | tx | sel | bold | Selected row / typed-entry value |
| `hb` | tx | bar | bold | Brand label `FNIRSI` |
| `hd` | dim | bar | — | Header/strip secondary text and fill |
| `hok` | ok | bar | — | Connected label |
| `her` | er | bar | bold | Trip label, no-link label |
| `hv` | ch1 | bar | bold | Strip: voltage |
| `hi` | ch2 | bar | bold | Strip: current |
| `hp` | pw | bar | bold | Strip: power |
| `hscr` | set | bar | bold | Strip: screen name |
| `bcv` | bg | ch1 | bold | Badge: CV active |
| `bcc` | bg | wn | bold | Badge: CC active |
| `bon` | bg | ok | bold | Badge: output ON |
| `bun` | bg | wn | bold | Badge: output state unknown (`?`, link down) |
| `boff` | dim | key | — | Badge: inactive mode, output OFF, idle |
| `S`+x | as x | sel | as x | Variant of style x drawn on a selected row, for x in tx, d, bd, ti, ch1, ch2, pw, set, ok, wn, er |

### 3.3 Colour rules [D]

1. Measured and set values of one quantity use that quantity's colour (voltage `ch1`, current `ch2`, power `pw`) in the main panel, the live strip, and the load bars of the Protection screen. Set digits are the exception: they use `set` (gold) because they are editable.
2. `ok`, `wn`, `er` appear only for state. Never use them for emphasis or decoration. The power bar fill is `pw`, not a green→red gradient.
3. **Trip in the header keeps the header's background** (`bar`); only the text colour changes to `er` (style `her`). It is not a solid red block.
4. The active mode badge is filled (CV `ch1`, CC `wn`); the inactive one is `boff`.
5. Selected-row highlight spans exactly the inner width of its box (box width − 4), not the border.

### 3.4 Colour depth and fallbacks [D]

Detect once at start, in this order: {#UI-04}

1. `NO_COLOR` set to a non-empty value, or config `colors = "none"` → **none**.
2. Config `colors` set to `24bit`, `256` or `16` → that mode.
3. `COLORTERM` is `truecolor` or `24bit` → **24-bit** (use §3.1 hex).
4. `TERM` contains `256color` → **256** (use the xterm-256 column; paint `bg` explicitly).
5. Otherwise → **16** (use the 16-colour column; do **not** paint a background for `bg`, `tx` and `dim` text; assume a dark terminal).

**None mode:** emit no colour at all. Map styles to attributes: `ti`, `set`, `wn`, `er`, `ch1`, `ch2`, `pw` → bold; `sel`, `bcv`, `bcc`, `bon`, `bun` → reverse video (+ bold); `d`, `hd`, `bd`, `bdf`, `boff` → normal; everything else → normal. All meaning must still be readable because of §1.3.

## 4. Typography

### 4.1 Glyph inventory [D] {#UI-06}

| Purpose | Glyphs |
|---|---|
| Box (rounded) | `╭ ╮ ╰ ╯ ─ │` |
| Bar | `━` fill, `─` track |
| Big digits | `┏ ┓ ┗ ┛ ┃ ━ ┣ ┫ ╺ ╸ ╻ ╹` |
| Digit cursor | `▔` (U+2594) |
| Lamps | `○` armed, `●` tripped / connected |
| Markers | `▸` selected row, `◂ ▸` option arrows, `✓` valid, `✗` invalid / rule violated, `!` warning |
| Text | `· ° → ↑ ↓ ← ⏎ … – ×` |

### 4.2 Big-digit font [D]

Each glyph is 3 rows tall. Digit glyphs are 3 cells wide, `.` is 1 cell wide, `-` is 3 cells wide. **One blank column separates every pair of adjacent glyphs**, including either side of `.`. So `4.998` is 3+1+1+1+3+1+3+1+3 = 17 cells wide, `12.345` is 21.

```
 0     1     2     3     4     5     6     7     8     9     .     -
┏━┓   ╻   ╺━┓  ╺━┓  ╻ ╻  ┏━╸  ┏━╸  ╺━┓  ┏━┓  ┏━┓
┃ ┃   ┃   ┏━┛  ╺━┫  ┗━┫  ┗━┓  ┣━┓    ┃  ┣━┫  ┗━┫
┗━┛   ╹   ┗━╸  ╺━┛    ╹  ╺━┛  ┗━┛    ╹  ┗━┛  ╺━┛
```

Exact glyph rows (a `·` stands for a blank cell; do not draw it):

| Char | Row 1 | Row 2 | Row 3 |
|---|---|---|---|
| `0` | `┏━┓` | `┃·┃` | `┗━┛` |
| `1` | `·╻·` | `·┃·` | `·╹·` |
| `2` | `╺━┓` | `┏━┛` | `┗━╸` |
| `3` | `╺━┓` | `╺━┫` | `╺━┛` |
| `4` | `╻·╻` | `┗━┫` | `··╹` |
| `5` | `┏━╸` | `┗━┓` | `╺━┛` |
| `6` | `┏━╸` | `┣━┓` | `┗━┛` |
| `7` | `╺━┓` | `··┃` | `··╹` |
| `8` | `┏━┓` | `┣━┫` | `┗━┛` |
| `9` | `┏━┓` | `┗━┫` | `╺━┛` |
| `.` | `·` | `·` | `.` |
| `-` | `···` | `╺━╸` | `···` |

#### Larger fonts F5 and F7 [D]

F5 glyphs are 5 cells wide and 5 rows tall, F7 glyphs 7 wide and 7 tall. `.` is 1 cell wide and sits on the last row; one blank column separates adjacent glyphs, as in F3. All three fonts are drawn from the same seven segments, so F3 above is the output of the rule below (checked) and F5/F7 are its larger versions. {#UI-17}

- Segments: `a` top, `b` upper right, `c` lower right, `d` bottom, `e` lower left, `f` upper left, `g` middle. Digits: `0` abcdef · `1` bc · `2` abdeg · `3` abcdg · `4` bcfg · `5` acdfg · `6` acdefg · `7` abc · `8` abcdefg · `9` abcdfg · `-` g.
- The horizontal segments `a`, `g`, `d` lie on rows 1, ⌊h/2⌋ + 1 and h. A drawn horizontal segment is `━` between the two corner cells of its row. A drawn vertical segment is `┃` in the first (left segments `f`, `e`) or last (right segments `b`, `c`) column on the rows between the horizontal ones; upper segments `f`, `b` on the rows above the middle row, lower segments `e`, `c` below it.
- The six corner cells (top-left, top-right, middle-left, middle-right, bottom-left, bottom-right) take their character from the *arms* that meet there. Top-left: right arm if `a`, down arm if `f`. Top-right: left arm if `a`, down arm if `b`. Middle-left: right if `g`, up if `f`, down if `e`. Middle-right: left if `g`, up if `b`, down if `c`. Bottom-left: right if `d`, up if `e`. Bottom-right: left if `d`, up if `c`.
- Arms to character: right `╺` · left `╸` · up `╹` · down `╻` · right+down `┏` · left+down `┓` · right+up `┗` · left+up `┛` · right+up+down `┣` · left+up+down `┫` · up+down `┃` · no arms: blank.
- `1` is drawn as a single vertical in the middle column: `╻` on the first row, `┃` between, `╹` on the last row.

Reference: `font-f3`, `font-f5`, `font-f7` (the characters `0123456789.-` in each font).

### 4.3 Number formats [D unless noted] {#UI-07}

| Quantity | Format | Notes |
|---|---|---|
| Measured voltage (big) | `D.DDD` or `DD.DDD` | 3 decimals, no leading zero, left-aligned. **Compact tier only:** 2 decimals when ≥ 10 V (so it fits 17 cells). Measurement resolution on the wire is unverified [U]; if the device is found to report 10 mV steps, change to 2 decimals everywhere. |
| Measured current (big) | `D.DDD` | 3 decimals |
| Measured power (text) | `D.DD W` below 100 W, `DDD.D W` from 100 W | Use device power `C3[2]`, not V×I |
| Set voltage (big) | `DD.DD` | Leading zero kept so the cursor does not jump; range 00.00–30.00 |
| Set current (big) | `D.DDD` | Range 0.000–5.100 |
| Protection limits | OVP `DD.DD V`, OCP `D.DD A`, OPP `DDD.D W`, OTP `DD.D °C`, LVP `D.DD V` | Right-aligned in tables |
| Input voltage, temperature | `DD.DD V`, `DD.D °C` | |
| Capacity, energy | `D.DDD Ah`, `D.DDD Wh` | |
| Elapsed time | `m:ss`, or `h:mm:ss` from one hour | |
| Preset shorthand | Standard main panel: shortest decimal, e.g. `3.3/1`, `12/2`, `5/0.5` (`%g`). Large and all lists: fixed one decimal `3.3V/1.0A` | |

In edit boxes (Protection, Presets) the big digits keep a leading zero so the cursor does not jump: OVP `NN.NN`, OCP `NN.NN` (the tables show `D.DD A`), OPP `NNN.N`, OTP `NN.N`, LVP `NN.NN`.

Setpoints, limits and preset values are held as integers in thousandths of their unit (mV, mA, mW, m°C) and converted to the nearest binary32 only at the wire, so 3.299999952 from the device is 3300 mV and goes back as the same bits. That step is finer than the device resolution (10 mV, 1 mA), and no other rounding is applied to a value that is sent. Measured values stay floats and are rounded for display only. {#PRT-04}

## 5. Components

All components are described in cells. "Style" refers to §3.2.

### 5.1 Box

`box(width, title, lines, right?)`. Height = number of inner lines + 2.

- Top row: `╭─ ` + title + ` ` + `─`×fill + [` ` + right + ` ─`] + `╮`, where `fill = width − 3 − len(title) − 1 − 1 − (len(right) + 3 if right else 0)` and `fill ≥ 1`. Border characters style `bd`, title style `ti`, `right` text in whatever style the caller gives (usually `d`). If the title would not fit, truncate it with `…`.
- Body rows: `│` + one space + inner content padded with spaces to `width − 4` + one space + `│`. Padding spaces inherit the row's background (so a selected row stays highlighted to the edge of its inner width).
- Bottom row: `╰` + `─`×(width−2) + `╯`.
- Adjacent boxes touch with no gap (`╮╭`).

### 5.2 Bar

`bar(fraction, width, style)`: clamp fraction to 0..1; `n = round(fraction × width)`, but `n = 1` if fraction > 0 and rounds to 0. Draw `━`×n in `style`, then `─`×(width − n) in `bd`.

### 5.3 Digit cursor

A line under a big-digit row holding `▔` repeated 3 cells (the glyph width), style `cur`, placed under the digit being edited. It is drawn in the box's cursor row (the row after the big set digits), indented by the same one-space margin as the digits. Only the focused readout shows a cursor. Voltage digit order: tens, units, tenths, hundredths (skip `.`). Current digit order: units, tenths, hundredths, thousandths. Moving the cursor never wraps.

**Index systems (digits vs characters).** The cursor index counts digits only and skips the `.`: voltage `05.00` has digits 0–3 at character positions 0, 1, 3, 4. Compact plain-text offsets (§6.3) use the character position: offset = 4 + character position. Example: the units digit of `05.00` is digit 1, at character position 1, so the Compact cursor sits at offset 5.

### 5.4 Lamps

`○ NAME` in `d` for armed; `● NAME` in `er` for tripped; `? NAME` in `d` while the link is down (state unknown). Two spaces between lamps. Order: OVP, OCP, OPP, OTP, LVP.

### 5.5 Badges

Text surrounded by one space each side: ` CV `, ` CC `, ` ON `, ` OFF `, ` ? `, ` -- `. Styles per §3.2 (`bcv`, `bcc`, `bon`, `boff`, `bun`).

- **Mode badges (Standard, Large):** ` CV ` and ` CC `. Only one is active at a time; with output off, tripped or the link down, both are `boff`.
- **Mode badge (Compact):** a single badge: ` CV ` (`bcv`), ` CC ` (`bcc`), or ` -- ` (`boff`) when output is off, tripped or the link is down.
- **Output badge:** ` ON ` (`bon`), ` OFF ` (`boff`), or ` ? ` (`bun`) when the output state is unknown because the link is down (§6.7).

### 5.6 Key cap

` key ` (one space each side) in style `k`, followed by ` label` and two spaces in style `d`.

### 5.7 Selection marker and row

The selected row of a list starts with ` ▸ ` (style `Sset`), unselected rows with three spaces. The selected row is drawn entirely in `S`-variants on `sel` background over the box's inner width. Values on a selected row lose their quantity colours and use `Stx` (exception: set values keep `Sset`).

### 5.8 Key bar

One row, full terminal width, background `bg`. An *item* is a key cap (§5.6) plus its label; its width is `len(key) + len(label) + 5` (leading space, key, two spaces, label, two trailing spaces). The bar is built for the **layout width** `W`: the terminal width in the Fluid layout, 44 in Compact (§2.3); any remaining cells are padding. {#UI-08}

Algorithm:

1. Take the screen's items in priority order (§6.6, §8, §9). Compact screens have their own, shorter priority lists, given with each screen.
2. The **tail** is reserved first: `Esc back` on secondary screens (`Esc cancel` while an edit is active), then `? help` on every screen. Their widths are subtracted from `W`.
3. Add items in priority order while they still fit; stop at the first item that does not fit.
4. Append the tail. Pad with spaces to the full width.

So `Esc back` and `? help` are always visible, and lower-priority items drop first. Appendix A shows the result at each reference size; every bar there is generated by this algorithm. While typed entry is active, the bar is replaced by `⏎ apply`, `Esc cancel`, `Space off` and has no `? help` (`?` is not a typed character). When a transient message is active on non-main screens it replaces the key bar row (§5.12).

### 5.9 Header row

Full width, background `bar`. Left part, then fill (`hd`), then right part.

| Tier | Left | Right |
|---|---|---|
| Compact | ` FNIRSI ` (`hb`) ` DPS-150` (`hd`) | status label (below) |
| Standard | ` FNIRSI ` ` DPS-150 · fw V1.2` | status label + ` usbmodem13C85FCE40251 ` (`hd`, port basename) |
| Large | ` FNIRSI ` ` DPS-150 · fw V1.2 · hw V1.0` | status label + ` /dev/cu.usbmodem13C85FCE40251 ` (`hd`, full path) |

- The header variant follows the **layout and band** (Compact, Fluid Standard, Fluid Large), on every screen, including secondary screens.
- The Compact header never shows firmware or port, on any screen.
- Model and versions come from the device (APP.md §1). Status label: ` ● connected ` (`hok`); ` ● PROTECTION TRIPPED ` (`her`); ` ○ no link ` (`her`). [D]
- If the header would not fit the layout width, items are dropped in this order: the port, the firmware/hardware text. The status label is never dropped.

### 5.10 Live strip (secondary screens only)

One row below the header, full width, background `bar`: ` 4.998 V ` (`hv`) ` 0.512 A ` (`hi`) ` 2.56 W ` (`hp`), then the output state in `hd`/`hok`:

- cols ≥ 60: ` CV · ON`; when the output is off or tripped, ` OFF`; when the link is down, ` ?` in `er`.
- cols < 60 (Compact): ` ON` (`hok`) or ` OFF` (`hd`) or ` ?` (`her`), so the output state is visible on every screen at every size.

Fill with `hd`. The screen name in capitals, one space each side, in `hscr`, is right-aligned (`PRESETS`, `PROTECTION`, `SETTINGS`). While the link is down, the three measured values are replaced by `-.--- V`, `-.--- A`, `-.-- W`. The main panel has no strip.

### 5.11 Slider and option rows (Settings)

Slider: marker (3) + label, then a bar block right-aligned in the row: `━━`×n in `set` + `──`×(5−n) in `bd` + two spaces + n + two spaces, so the block ends two cells before the box edge; the gap between label and bar absorbs any extra width. Option: marker (3) + label padded to 12 + `◂ ` + value (`set`) + ` ▸`. Arrows are `d`, or `tx` on the selected row. Read-only rows: label in `d`, value right-aligned in `tx`, no marker.

### 5.12 Transient messages and prompts [D]

- **Main panel, Standard/Large:** line 2 of the Status box (the power line) is replaced. **Compact:** the info row under the Status box is replaced.
- **Secondary screens:** the key-bar row is replaced.
- **Fit rule.** A message, including its key caps, must fit the row it replaces: a line inside a box (`cols − 4` cells, at least 76 in the Fluid layout), the key-bar row (`cols`), or 43 cells in Compact (after the leading space). Messages are written to fit 76 cells, so they fit at every Fluid width. A message is never truncated and a key cap is never cut off. Compact has its own shorter text for every message; the full list, with lengths, is in §10. {#UI-09}
- A *notice* (a result, a clamp at a limit, an error) disappears after 3 s or on the next key press. A rejected typed value (§7.2) stays until the next key press.
- A *prompt* (confirmation) stays until answered. It is answered **only** with the keys it shows: {#SAF-05}
  - `y` = yes. `n` or `Esc` = no.
  - `⏎` is **ignored** in a prompt, so that the `⏎` that applied a typed value can never also confirm the prompt it triggers, and a double press is harmless.
  - `Space` (or `Ctrl-X`) cancels the prompt and, if the output is ON, turns it OFF. It never turns the output ON.
  - Any other key is ignored.
  - While a prompt is pending, the SET values still show the old setpoints, and nothing has been sent.
- A prompt is cancelled (nothing applied) when the link is lost or the terminal becomes too small. {#SAF-09}
- Styles: message text `wn` (or `er` for errors); key caps per §5.6, drawn as ` y  yes   Esc  no`.

### 5.13 Help overlay [D]

`?` opens a read-only overlay on every screen. It replaces the body (everything between the header and the key bar), keeps the header and the live strip, and closes with `?`, `Esc` or `q`. `Space` and `Ctrl-X` still work. The key bar shows ` Esc  close`.

Content, in this order:

1. **Keys.** Two columns. The left lists the editing keys of the current screen, the right lists screen navigation (`p s o ,`, `m 1-6`, `q`, `Space`, `Ctrl-X`). The text is taken from the screen's key table (§8, §9), so the overlay never disagrees with the bindings.
2. **Glossary**, only the lines that apply to the screen: `CV`, `CC`, the five protections and what a trip does.

Standard and Large use the layout in `help-main-standard` (a rounded box titled `Help · {screen}`). Compact uses one column with the same content and scrolls with `↑` `↓`; the scroll position is shown in the box title as `1/2`.

## 6. Main panel

### 6.1 Anatomy [D]

| Tier | Rows (top to bottom) |
|---|---|
| Fluid, Large band (≥ 100×21; 100×21, 120×40) | 1 header · R readout boxes (R = rows − 9; widths `cols − ⌊cols/2⌋` and `⌊cols/2⌋`) · 4 Status box (full width) · 3 Memory box (full width) · 1 key bar |
| Fluid, Standard band (≥ 80×21; 80×24) | 1 header · R readout boxes (R = rows − 7) · 4 Status box (full width) · 1 preset buttons · 1 key bar |
| Compact (44×14) | 1 header · 7 readout boxes (two, 22 wide) · 4 Status box (44) · 1 info row · 1 key bar |

Left box is **VOLTAGE**, right box is **CURRENT**. Body is horizontally centred (§2.3).

### 6.2 Readout box — Fluid layout

**Focus.** The focused box is marked on the box itself, not only by the thin digit cursor: its title is `▸ VOLTAGE` / `▸ CURRENT` in style `set`, and its border uses style `bdf` instead of `bd`. The unfocused box keeps the plain title (`ti`) and the `bd` border. With colour off the `▸` still marks the focus (§1.3). [D]

Box title as above (no `right` text). The content block has `2h + 4` lines for a font of height `h` (10 lines for F3), placed as in §2.2. The table shows F3; for F5 and F7 the digit rows repeat `h` times and the line numbers shift:

| # | Content | Style |
|---|---|---|
| 1 | `MEASURED` | `d` |
| 2–4 | One-space margin + big measured digits (`h` rows) | `ch1` (voltage) / `ch2` (current) |
| 4 (continued) | After the digits: 2 spaces + unit `V` / `A` | same as digits |
| 4 (continued) | Current box only, in CC mode: 3 more spaces + `◂ limiting` | `wn` |
| 5 | blank | |
| 6 | `SET  ←→ digit  ↑↓ value` | `d` |
| 7–9 | One-space margin + big set digits (`h` rows), format §4.3 | `set` |
| 10 | Digit cursor line (§5.3) in the focused box, otherwise blank | `cur` |

During typed entry (§7.2) rows 7–9 show the typed characters and row 10 shows `⏎ apply · Esc cancel` (`d`).

### 6.3 Readout box — Compact

Box width 22, titles `VOLTAGE (V)` / `CURRENT (A)`; the focused box gets the `▸` title and `bdf` border as in §6.2 (`▸ VOLTAGE (V)`). Five inner lines: big measured digits (3 rows; `ch1`/`ch2`, no unit), then `SET ` (`d`) + set value as **plain text** (`set`, format §4.3; current box in CC mode appends ` ◂ LIM` in `wn`), then the cursor line: spaces to align under the edited character (offset = 4 + character index, see §5.3) + `▔` (1 cell) in `cur`, focused box only.

**Typed entry (Compact):** the set text after `SET ` is replaced by the typed characters (style `sel`); the cursor line shows `⏎ set  Esc cancel` (`d`, 17 cells); the info row shows the range hint from §10. Reference: `main-compact-entry`.

### 6.4 Status box

Title `Status`. Two inner lines.

**Standard and Large, line 1:** `MODE ` (`d`) + CV badge + space + CC badge + `   OUTPUT ` (`d`) + output badge (ON / OFF / `?`, §5.5) + `   PROTECT ` (`d`) + five lamps (§5.4). Large only: after the lamps, two spaces + `all clear` (`ok`) when no protection is tripped and the link is up.

**Standard line 2:** `POWER ` (`d`) + power value `NN.NN W` (`pw`) + space + power bar (width 18 at 80 columns, plus one per extra column; fraction = P / 150 W, style `pw`) + ` of 150 W   Vin ` (`d`) + input voltage (`tx`) + `  T ` (`d`) + temperature (`tx`).

**Large line 2:** same but bar width 16 at 100 columns (plus one per extra column) and, after the temperature, `   Q ` (`d`) + capacity + `   E ` (`d`) + energy; the separators between fields are three spaces and `Temp ` replaces `T `. Exact text: see Appendix A.

**Compact:** line 1 = **mode badge** (` CV ` / ` CC ` / ` -- `, §5.5) + space + output badge + two spaces + power value (`pw`); line 2 = the five lamps (`○ OVP  ○ OCP …`, two spaces between lamps).

**Compact info row** (normal state): `Vin 20.25 V · 24.3 °C · 0.060 Wh` (`d` labels, `tx` values); transient messages, the trip text and the link-lost text replace it (§5.12, §6.7).

Which badge is lit: output ON and mode CV → CV lit; output ON and mode CC → CC lit; output OFF or tripped → both mode badges `boff` (Compact: ` -- `), output badge OFF; link down → mode badges `boff`, output badge `?`. [D]

### 6.5 Memory (preset) row

- **Standard:** one row below the Status box, no border: six buttons, each a key cap ` M1 ` plus ` 3.3/1 ` (`d`) and one space, then the hint `m 1-6 recall` (`d`) right-aligned to the right edge of the terminal. The button whose V and I equal the current setpoints exactly is drawn in `sel` instead of `d`. A button whose V or I exceeds the live maximum is drawn in `wn`, with a `!` in place of the key cap's leading space (`!M6`), so it is readable with colour off. No button is highlighted when none matches. [D]
- **Large:** a box `Memory` (width 100, right text `m 1-6 recall` in `d`) with one inner line of six buttons: ` M1 ` key cap + ` 3.3V/1.0A` + two spaces. The same match highlight (`sel`) and blocked styling (`wn`, `!M6`) apply.
- **Compact:** no preset row; presets are on the Presets screen and the `m` chord.
- The hint says `m 1-6`, never just `1-6`: bare digits start typed entry (§7.2).

### 6.6 Key bar priority (main panel)

`Space output`, `←→ digit`, `↑↓ value`, `⏎ type`, `Tab V/I`, `p presets`, `o protect`, `, settings`, `m recall`, then the reserved `? help` (§5.8). `q quit` is listed in help, not in the bar. [D]

At the reference widths (44, 80, 100) this gives: Compact `Space output · ←→ digit`; Standard `Space output · ←→ digit · ↑↓ value · ⏎ type · Tab V/I`; Large additionally `p presets · o protect`; at 120 columns `, settings` also fits (`main-cv-120x40`). `v` / `i` (type voltage / current directly, §7.2) are not in the bar: no extra item fits at 80 columns, and `⏎ type` already advertises typed entry; they are listed in the help overlay. The screen keys `p s o ,` and `m` that do not fit are listed in the help overlay (§5.13) and the Standard preset row carries the `m 1-6 recall` hint.

### 6.7 States

| State | Changes (everything else as the normal rendering) |
|---|---|
| Normal CV | Reference: `main-*-cv` |
| CC (current limiting) | Mode badge CC lit (Compact: the single mode badge reads ` CC `, style `bcc`); `◂ limiting` next to the current unit (Compact: ` ◂ LIM` after the set value); measured values as reported; Reference: `main-*-cc` [D] |
| Output OFF | OFF badge; both mode badges `boff` (Compact: ` -- `); measured values show `0.000` etc. as reported by the device; power `0.00 W`. Reference: `main-*-confirm-on` |
| Protection tripped | Header label ` ● PROTECTION TRIPPED ` (`her`); the tripped lamp `● NAME` (`er`); output OFF badge; Status line 2 (Compact: info row) replaced by `NAME tripped` (`er`) + `  output forced off · ` (`d`) + ` Space ` key cap + ` clear` (`d`). Compact text is the deliberate short form ` OTP tripped · output off ·  Space  clear` (§10). Reference: `main-*-trip` [D]. Which lamp, the value at trip, and what `Space` does are [U]: the protection-state byte mapping and the clearing procedure are unverified (PROTOCOL.md §8). Until verified, `Space` in the tripped state sends output-off and re-reads the protection state; if it is still tripped, show the notice `Still tripped` (§10). |
| Link lost | See below. Reference: `main-standard-lost`, `main-compact-lost` [D] |

**Link lost.** What the UI shows when the link is lost. When the link counts as lost, what is blocked, the OFF queue and reconnect are in SAFETY.md §3 (the device is powered from its own input, so a pulled data cable most likely leaves the output ON: the UI never claims it is off).

- Header label ` ○ no link ` (`her`).
- Measured big digits replaced by `-.---` (voltage, current) and power text by `-.--`. Set values stay (last known), the focus and the cursor stay.
- Mode badges `boff`; the output badge is ` ? ` (`bun`); the lamps are drawn `? NAME` in `d` (state unknown, not "armed").
- Status line 2 (Compact: info row): `Output state unknown, may still be ON` (`er`) + `  retrying every 1 s` (`d`); with Settings → Reconnect = off: `  reconnect off · r retry` (`d`).
- The same unknown-state rules apply to the live strip (§5.10) and the too-small screen (§2.4).

## 7. Editing model

### 7.1 Focus and cursor [D]

- Exactly one readout has focus: Voltage at start. `Tab` toggles focus. The focused readout shows the digit cursor and the `▸` title (§6.2). {#EDT-01}
- Initial cursor: voltage → units digit (index 1 of `DD.DD`); current → tenths (index 1 of `D.DDD` counting digits only: units, tenths, …). Each quantity remembers its last cursor position. [D]
- `←` / `→` move the cursor one digit; at either end they do nothing. {#EDT-02}
- `↑` / `↓` add / subtract the value of the digit under the cursor (voltage: 10, 1, 0.1, 0.01 V; current: 1, 0.1, 0.01, 0.001 A). The result is limited to the allowed range (SAFETY.md §1) and sent. At a limit the value stops and a notice names the limit (`At live maximum 20.05 V`, `At minimum 0.00 V`, `Limited by OVP 13.00 V`); steps are *knob* semantics and never reject. {#LIM-08} {#EDT-03}
- A held key repeats at the terminal's repeat rate; coalesce so that at most one SET frame per 50 ms is sent, always the latest value. A held key counts as **one action** for the jump guard (SAFETY.md §2): the walk stops at the guard threshold and further repeats are ignored until the prompt is answered. {#SAF-07}
- The displayed *set* value updates immediately (optimistic) and is reconciled with the device read-back (APP.md §3). {#EDT-04}
- Cursor index (digits only, skips `.`) and the Compact plain-text offset (counts the `.`) are two coordinate systems; §5.3 gives the conversion.

### 7.2 Typed entry [D]

- **Start.** Three ways, all equivalent once the entry is open: {#EDT-05}
  - `v` (or `V`) moves focus to the Voltage readout and starts typed entry with empty text; `i` (or `I`) does the same for Current. This is the quick path: `v` `5.25` `⏎` sets the voltage whatever the focus was. `Tab` is unchanged and still only toggles focus.
  - `⏎` on the focused readout starts typed entry with empty text.
  - Typing a digit or `.` on the main panel starts typed entry on the focused readout with that character already entered; digits never recall presets (that is `m` + digit, §8.1).
- **Display.** The big set digits are replaced by the characters typed so far (big font, style `set`, no leading-zero padding). Standard/Large: the cursor line shows `⏎ apply · Esc cancel` (`d`) and the message line shows `Typing a voltage. Range 0 – 20.05 V (live maximum)` (current: `Typing a current. Range 0 – 5.10 A`). Compact: see §6.3. The key bar shows `⏎ apply`, `Esc cancel`, `Space off` (§5.8). Reference: `main-standard-entry`, `main-compact-entry`.
- **Keys.** Digits and one `.`; `Backspace` deletes; `⏎` applies; `Esc` cancels. Max 5 characters. `Space` and `Ctrl-X` **cancel the entry and turn the output OFF** if it is ON (never ON). Any other key is ignored, including `v` and `i` (press `Esc` first to switch field). {#EDT-06}
- **On apply.** Parse as decimal. Empty input cancels. Round to 10 mV (voltage) / 1 mA (current), then check SAFETY.md §1: {#LIM-09}
  - **Out of range → rejected, not clamped.** The entry stays open, the typed text turns `er`, and the message line shows the reason: `35 V is above the live maximum 20.05 V`, `35 V is not below OVP 13.00 V …`, and so on (§10). The setpoint is unchanged. The next key press clears the message; the user can edit the text or `Esc`.
  - **In range →** the value is applied through the confirmations of SAFETY.md §2 (the `⏎` that applied it does not answer any prompt that follows, §5.12).
- Reference: `main-standard-reject`.

Ranges, limits, ordering rules and confirmations are in SAFETY.md §1 and §2.

## 8. Secondary screens

All secondary screens share this frame (Standard and Compact; Large uses Standard): header row (§5.9) · live strip (§5.10) · body · key bar (§5.8). `Esc` returns to the main panel (and cancels an active edit first). `Space` toggles output. `?` opens help (§5.13). Keys `p`, `o`, `,` on the main panel open Presets, Protection, Settings. Key bars in Appendix A are generated by §5.8 from the priority lists below.

### 8.1 Presets (`p`) [D]

Edits the six *device* slots M1..M6 (V and I only: PROTOCOL.md §4 `C5`..`D0`). Names are kept by the app (APP.md §4.2). There is no device recall command; **recall writes the V and I setpoints** (`C1`, `C2`) and never changes output state. [U] {#PRE-01}

**Recall.**

- From the main panel: the chord `m` then `1`..`6`. After `m` the message line shows `Recall which preset? 1-6, Esc cancels` and any other key cancels the chord. From the Presets screen: `⏎` on the selected slot. {#EDT-07}
- Output OFF: the values are written at once; notice `Recalled M5 · 5.00 V / 0.500 A`.
- Output ON: always the prompt `Recall M5 5.00 V / 0.500 A, output ON?` (SAFETY.md §2), whatever the size of the change. {#SAF-08}
- A slot whose V or I exceeds the live maximum, or breaks an ordering rule of SAFETY.md §1, is **blocked**: notice `M6 blocked: 24.00 V exceeds the live maximum 20.05 V`. It is never clamped. {#PRE-02}

**Standard** (80 wide): row 3 onward: left box `Presets` (48 wide, right text `device M1–M6`), right box `M{n} · {name}` (32 wide, right text `edit`), both 11 inner lines; below them a 80-wide box titled `M{k} · {name}` where k is the lowest-numbered blocked slot, one inner line.

Left box inner lines:

1. Column header (`d`): 3 spaces, `#` padded to 4, `name` padded to 11, `V` right-aligned in 8, `I` right-aligned in 8, `P max` right-aligned in 9.
2–7. One row per slot: marker (3) · `M{n}  ` · name padded to 11 · voltage `%6.2f V` · `  ` · current `%4.2f A` · `  ` · power `%5.1f W` (V×I). Styles: name `tx`, voltage `ch1` (or `wn` if above the live maximum), current `ch2`, power `pw`. Selected row per §5.7.
8. blank.
9. `Now ` (`d`) + `5.00 V` (`set`) + ` / ` (`d`) + `1.500 A` (`set`) + `  (no preset matches)` (`d`) — or `  (matches M{n})` when a slot matches.
10. `Recall sets V and I; output unchanged.` (`d`)
11. blank.

Right box inner lines: `VOLTAGE` (`d`) · big set voltage `NN.NN` (3 rows, `set`) · cursor line · `CURRENT` (`d`) · big set current `N.NNN` (3 rows) · cursor line (blank unless current is being edited) · validation line: `✓ ` (`ok`) + `within 20.05 V / 5.10 A` (`d`), or `! ` (`wn`) + reason when invalid.

The lower box content: if any slot is blocked: `! ` (`wn`) + `24.00 V exceeds the live maximum 20.05 V. Recall is blocked.`; otherwise `All presets are within the live limits.` (`d`) with title `Check`.

**Compact** (44 wide, 16 rows): header · strip · box `Presets` (6 inner lines: marker, `M{n} `, name padded to 9, `%5.2fV`, ` %4.2fA`) · box `M{n} · {name}` (2 inner lines: line 1 `V ` + ` NN.NN ` + ` V   I ` + ` N.NNN ` + ` A`, the focused value in `sel`, the other in `set`; line 2 the validation line) · one **status row** · key bar. The status row shows `Now 5.00 V / 1.500 A` (`d`/`set`) followed by `  ! M{k} above 20.05 V` (`wn`) when a slot is blocked or `  (= M{n})` when one matches. **In edit mode** line 2 of the edit box becomes the digit cursor line (`▔` under the edited digit, offset rule of §5.3) and the validation line moves to the status row. Reference: `presets-compact`.

**Keys.**

- List mode: `↑↓` select slot; `⏎` recall; `e` edit slot; `n` rename; `s` save the current setpoints into the slot; `Del` or `x` clear the slot; `Esc` back.
- Edit mode: `←→` digit, `↑↓` value, `Tab` V/I, `⏎` write slot to the device, `Esc` cancel.
- Key bar priority: `⏎ recall`, `e edit`, `n rename`, `s save`, `x clear`, `Space output`; tail `Esc back`, `? help`. Compact has its own priority list: `⏎ recall`, `s save`, `e edit`, `n rename`, `x clear`; at 44 columns it shows `⏎ recall`, `s save`, `Esc back`, `? help`; the rest are in help.
- `Del` exists on full keyboards; `x` is the alias that works on Mac laptops, which have no forward-delete key. The bar shows `x`.

**Rename (`n`).** The slot name in the right box title turns into an inline text field (style `sel`; the terminal cursor sits after the text). Printable characters, max 9 (so it fits the Compact name column), `Backspace` deletes, `⏎` saves to `presets.toml`, `Esc` cancels. Inside the field `Space` types a space; `Ctrl-X` still turns the output OFF (§1.5). The key bar shows `⏎ save`, `Esc cancel`. An empty name clears the name. {#PRE-04}

**Overwrite and clear.** `s` and `x`/`Del` ask first unless the slot already holds exactly the values (and, for clear, the default 5.00 V / 1.000 A and no name): {#SAF-12}

- `Overwrite M4?  12.00 V/2.000 A → 12.00 V/1.000 A` (Compact `Overwrite M4?`).
- `Clear M4 "12V fan" (12.00 V / 2.000 A)?` (Compact `Clear M4?`).

Answered per §5.12. `Del` resets the slot to 5.00 V / 1.000 A and clears its name.

References: `presets-standard`, `presets-compact`.

### 8.2 (not used)

Not used (sequences are out of scope, spec/MAIN.md §1). The number is kept so that references to §8.3 and §8.4 stay valid. The main panel key `s` is unassigned.

### 8.3 Protection (`o`) [D]

**Standard** (80 wide): box `Protection limits` (right text `trip turns output off`, 8 inner lines) above box `Selected {NAME}` / `Editing {NAME}` (4 inner lines).

Table inner lines: header (`d`): 3 spaces, `name` padded to 5, `limit` right-aligned in 8, 3 spaces, `device range` padded to 13, `live` padded to 15, `load`. Then one row per limit in order OVP, OCP, OPP, OTP, LVP: marker (3) · name + 2 spaces · limit right-aligned in 8 (`set`) · 3 spaces + device range padded to 13 (`d`; `tx` on the selected row) · live-label padded to 5 (`out`, `out`, `out`, `temp`, `in`; `d`) · live value right-aligned in 8 + 7 spaces (`tx`) · bar width 16 plus one per column beyond 80 (colour below) · ` NN%` (`NN` right-aligned in 3) + 3 spaces (`d`). Then blank, then `● ` (`ok`) + `no protection tripped` + `   last trip: none` (`d`).

When a stored limit already violates an ordering rule of SAFETY.md §1, that row's limit is drawn in `er` and the line `● no protection tripped …` is replaced by `✗ ` (`er`) + the violation, e.g. `Set voltage 13.00 V is not below OVP 13.00 V`. The app never edits the limit itself.

Load fraction and bar colour per row:

| Row | Fraction | Bar style |
|---|---|---|
| OVP | measured V / OVP | `ch1` |
| OCP | measured I / OCP | `ch2` |
| OPP | measured P / OPP | `pw` |
| OTP | temperature / OTP | `ti` |
| LVP | LVP / input voltage (approaches 100% as the input sags toward the limit) | `ti` |

**Mode is shown, not guessed.** The lower box has two states:

- *Selecting* (initial): title `Selected {NAME}`, right text `⏎ edit  r device default`. The digit cursor is not drawn. `←` `→` do nothing.
- *Editing*: title `Editing {NAME}`, right text `←→ digit  ↑↓ value  ⏎ apply  Esc cancel`; the key bar changes (§5.8).

Editing box (for the selected row): big digits of the limit value (3 rows, `set`; formats in §4.3), digit cursor line below (editing only); to the right of the digits, three info lines (`d` labels, 3 spaces after the digits): `Device range   0 – 5.10 A`; the related setpoint with its validation (`Current limit  1.500 A   ✓ OCP stays above it`); `Drawn now      0.512 A   margin 4.59 A`.

**Validation (host-enforced ordering, SAFETY.md §1) [D]:** OVP must be > set voltage; OCP > set current; OPP > set V × set I; LVP < live input voltage; each within its device range. An invalid candidate shows `✗ ` (`er`) + reason in the related-setpoint line and `⏎` is disabled. A valid one shows `✓ ` (`ok`). Related-setpoint line per limit: OVP `Voltage set`, OCP `Current limit`, OPP `Power now`/`Set power`, LVP `Input now`, OTP `Temperature now`.

**Compact** (44 wide, 16 rows): box `Protection` with one row per limit: marker · name + space · limit right-aligned in 8 (`set`) · two spaces · bar width 10 · ` NN%`; box `Selected {NAME}` / `Editing {NAME}` (3 inner lines): line 1 `I ` + ` NN.NN ` (`sel`) + ` A   ` + `✓ above limit 1.500 A`; line 2 the digit cursor line (editing only; blank when selecting); line 3 `range 0 – 5.10 A · drawn 0.512 A` (`d`); one row `● no protection tripped` (`ok`).

**Keys.**

- Selecting: `↑↓` select limit (initial row OVP); `⏎` start editing (cursor on the units digit); `r` load the device default as the candidate and start editing; `Esc` back. Typing a digit or `.` also starts editing in typed mode (below).
- Editing: `←→` digit; `↑↓` value; `⏎` applies and returns to *selecting* (disabled while invalid); `Esc` cancels. Digits and `.` switch to typed entry as in §7.2 (`⏎` applies directly; out-of-range text is rejected with the reason). `r` loads the device default as the candidate (does not write; `⏎` writes). Defaults: OVP 30.00 V, OCP 5.10 A, OPP 150.0 W, OTP 80.0 °C, LVP 5.00 V (observed on a fresh unit; PROTOCOL.md §4). A default that equals the device maximum is flagged in the validation line: `! default is the device maximum: no protection` (`wn`); it is still allowed.
- Key bar priority, selecting: `↑↓ limit`, `⏎ edit`, `r default`, `Space output`; tail `Esc back`, `? help`. Editing: `←→ digit`, `↑↓ value`, `⏎ apply`, `0-9 type`; tail `Esc cancel`, `? help`. Compact has its own editing priority: `⏎ apply`, `↑↓ value`, `←→ digit`; at 44 columns it shows `⏎ apply`.

References: `protection-standard`, `protection-compact` (both in the editing state).

### 8.4 Settings (`,`) [D]

**Standard** (80×21): a 2×2 grid of boxes, each 40 wide with 7 inner lines (stretching: §8.5): top row `Device` (right text `saved on device`) and `Safety`; bottom row `Display` and `Connection`. **Compact**: one box `Settings` (44 wide, 8 inner lines) listing, in order: Brightness, Volume, Output on, V jump over, I jump over, On exit, Refresh, Reconnect. Read-only rows (Model, Port, …) appear only in Standard.

| Group | Row | Type | Values (default first / current example) |
|---|---|---|---|
| Device | Brightness | slider | device range **[U]** — bars use 5 steps as a placeholder; written to type `D6` |
| Device | Volume | slider | same placeholder; type `D7` |
| Device | Model, Hardware, Firmware | read-only | from `DE`, `DF`, `E0` |
| Device | Limits | read-only | `30 V · 5.1 A · 150 W` (from `E4`, `E3`, `E6`) |
| Safety | Output on | option | `ask first` · `immediately` |
| Safety | V jump over | option | `5.0 V` · `1.0 V` · `2.0 V` · `10.0 V` · `off` |
| Safety | I jump over | option | `2.0 A` · `0.5 A` · `1.0 A` · `3.0 A` · `off` |
| Safety | On exit | option | `leave output` · `turn off` |
| Safety | (note) | text | `Out-of-range values are rejected;` / `steps stop at the limits. Always on.` (`d`, two lines) |
| Display | Theme | option | `Midnight Slate` (only theme in v1) |
| Display | Colours | option | `auto · 24-bit` (shows detection result) · `24-bit` · `256` · `16` · `off` |
| Display | Set precision | read-only | `10 mV / 1 mA` |
| Display | Refresh | option | `10 Hz` · `5 Hz` · `2 Hz` |
| Connection | Port | read-only | port basename |
| Connection | Baud | read-only | `115200` |
| Connection | Auto-detect | option | `on` · `off` |
| Connection | Reconnect | option | `on` · `off` |
| Connection | Link | read-only | `● connected` (`ok`) / `○ no link` (`er`) |

Row layout per §5.11. Editable rows are the only ones with a selection marker. Slider/option changes apply immediately; device settings (brightness, volume) are written to the device, app settings to the config file (APP.md §4.1).

**Keys:** `↑↓` move between editable rows (continues into the next group at the end of a group); `Tab` jumps to the next group; `←→` change value; `Space` output; `Esc` back. Key bar priority: `↑↓ item`, `←→ change`, `Tab next group`, `Space output`; tail `Esc back`, `? help`. Compact has its own priority list: `←→ change`, `↑↓ item`; at 44 columns it shows `←→ change`, `Esc back`, `? help`. References: `settings-standard`, `settings-compact`.

### 8.5 Fluid stretch of the secondary screens [D] {#UI-16}

Presets, Protection and Settings fill the terminal. Their frame is header · live strip · body · key bar, so the body is `rows − 3` rows tall (`B`) and `cols` wide. Everything in §2.2 about the frame applies. Inside boxes: text, lists and tables are **top-aligned** and spare rows are left blank at the bottom; big-digit blocks are **centred** vertically, as in §6.2, and use the largest font that fits (§2.2).

- **Presets.** The list box is `⌊0.6 × cols⌋` wide and the edit box the rest (48 + 32 at 80 columns); both are `B − 3` rows tall, and the `Check` box below them stays 3 rows. The extra width of the list goes to the name column (`name` width = 11 + inner width − 44). The edit box holds `2h + 5` lines (`VOLTAGE`, digits, cursor, `CURRENT`, digits, cursor, validation) and uses F5 when 1 + w(`NN.NN`) ≤ inner width and `2h + 5` ≤ inner height (so from 80×24), F7 when it fits likewise. References: `presets-standard` (80×24, F5), `presets-120x40` (F7).
- **Protection.** The limits box stays 10 rows; the edit box takes the other `B − 10` rows. The load bars are `16 + (cols − 80)` cells wide. In the edit box the digits sit at the left and the three info lines (widest 45 cells) start 3 cells to their right, on the first three digit rows; a font fits when 1 + w(`NN.NN`) + 3 + 45 ≤ inner width and `h + 1` ≤ inner height. References: `protection-standard` (80×24, F5), `protection-120x40` (F7).
- **Settings.** The 2×2 grid has columns `cols − ⌊cols/2⌋` and `⌊cols/2⌋` wide; the top row of boxes is `⌈B/2⌉` rows tall and the bottom row `⌊B/2⌋`. Read-only values stay right-aligned to the inner edge, the slider bar block stays right-aligned (§5.11), option rows stay left-aligned. References: `settings-standard` (80×24), `settings-120x40`.
- **Help overlay.** The box fills the body; text is top-aligned. Reference: `help-main-standard` (80×24).
- Compact screens do not stretch (§2.3).

## 9. Input map

### 9.1 Global

| Key | Action |
|---|---|
| `Space` | Toggle output. In typed entry, in a prompt and on the too-small screen it only cancels what is pending and turns the output OFF (never ON). Types a space in the preset-rename text field (use `Ctrl-X` there). With the link down it queues an OFF (§6.7) |
| `Ctrl-X` | Turn the output OFF, in every mode, including text fields |
| `?` | Help overlay (§5.13) |
| `q` | Quit from the main panel (asks if the output is ON, SAFETY.md §4.9); on other screens `q` acts like `Esc` |
| `Ctrl-C` | Quit immediately, no prompt, same exit behaviour as `q` (SAFETY.md §4.9) |
| `Esc` | Cancel the current edit or prompt; otherwise go back one screen |
| `y` / `n` | Answer a prompt (§5.12). `⏎` never answers a prompt |

### 9.2 Main panel

| Key | Action |
|---|---|
| `←` `→` | Move digit cursor |
| `↑` `↓` | Increase / decrease the digit under the cursor |
| `Tab` | Switch focus Voltage ↔ Current |
| `⏎` | Start typed entry for the focused value |
| `v` / `i` | Focus Voltage / Current and start typed entry (empty). `v` `5.25` `⏎` sets the voltage. `Tab` still only toggles focus |
| `0`..`9` `.` | Start typed entry with that character (never recalls a preset) |
| `m` then `1`..`6` | Recall preset M1..M6 (writes V and I, §8.1 rules; asks first when the output is ON) |
| `p` `o` `,` | Open Presets / Protection / Settings |
| `r` | Retry the connection once (only while the link is down and Reconnect = off) |

Screen-specific keys are in §8.1–§8.4.

## 10. Copy (fixed strings)

Screen names (strip): `PRESETS`, `PROTECTION`, `SETTINGS`. Box titles: `VOLTAGE`, `CURRENT`, `Status`, `Memory`, `Presets`, `Protection limits`, `Selected {NAME}`, `Editing {NAME}`, `Device`, `Safety`, `Display`, `Connection`, `Settings`, `Check`, `Help · {screen}`. Use `·` as the inline separator, `–` (en dash) in ranges, `×` for multiplication, `→` for before → after.

**One text per message.** The table lists every transient message once, in its Standard/Large form and its Compact form. Prompts are followed by the key caps shown in the Keys column. Lengths are in cells for the longest value (V up to `30.00`, I up to `5.100`, names up to 16 characters). The limit is 76 cells for the Fluid layout (its minimum inner width) and 43 for Compact, **including** the key caps (§5.12). Where Compact is `=`, the Standard text already fits.

| Message | Standard / Large | Compact | Keys | Cells S / C |
|---|---|---|---|---|
| Output ON prompt | `Turn output ON at 30.00 V / 5.100 A?` | `ON at 30.00 V / 5.100 A?` | `y` yes · `Esc` no | 55 / 43 |
| Voltage jump | `Set 15.00 V / 1.500 A (+10.00 V), output ON?` | `Set 15.00 V (+10.00 V)?` | `y` yes · `Esc` no | 63 / 42 |
| Voltage and current jump | `Set 15.00 V / 3.500 A (+10.00 V, +2.000 A), output ON?` | `Set 15.00 V, 3.500 A?` | `y` yes · `Esc` no | 73 / 40 |
| Current jump | `Set 5.00 V / 3.500 A (+2.000 A), output ON?` | `Set 3.500 A (+2.000 A)?` | `y` yes · `Esc` no | 62 / 42 |
| Recall with output ON | `Recall M4 12.00 V / 2.000 A, output ON?` | `Recall M4 12.00V/2.000A?` | `y` yes · `Esc` no | 58 / 43 |
| Overwrite slot | `Overwrite M4?  12.00 V/2.000 A → 12.00 V/1.000 A` | `Overwrite M4?` | `y` yes · `Esc` no | 67 / 32 |
| Clear slot | `Clear M4 "12V fan" (12.00 V / 2.000 A)?` | `Clear M4?` | `y` yes · `Esc` no | 58 / 28 |
| Quit, output ON | `Quit with output ON (12.00 V)?` | `Leave ON?` | `y` quit · `o` off+quit · `Esc` stay (Compact: `y` quit · `o` off · `Esc` stay) | 66 / 40 |
| Quit, link down | `Link down: cannot turn output off. Quit anyway?` | `Link down. Quit?` | `y` quit · `Esc` stay | 69 / 38 |
| Preset chord | `Recall which preset? 1-6, Esc cancels` | `Recall which? 1-6, Esc` |  | 37 / 22 |
| Recalled | `Recalled M5 · 5.00 V / 0.500 A` | `Recalled M5 · 5.00V/0.500A` |  | 30 / 26 |
| Recall blocked | `M6 blocked: 24.00 V exceeds the live maximum 20.05 V` | `M6 blocked: above 20.05 V` |  | 52 / 25 |
| Range hint, voltage | `Typing a voltage. Range 0 – 20.05 V (live maximum)` | `Voltage 0 – 20.05 V (live max)` |  | 50 / 30 |
| Range hint, current | `Typing a current. Range 0 – 5.10 A` | `Current 0 – 5.10 A (live max)` |  | 34 / 29 |
| Rejected: above live maximum (V) | `35.00 V is above the live maximum 20.05 V` | `35.00 V > 20.05 V live max` |  | 41 / 26 |
| Rejected: above live maximum (I) | `35.00 A is above the live maximum 5.10 A` | `35.00 A > 5.10 A live max` |  | 40 / 25 |
| Rejected: not below OVP | `35.00 V is not below OVP 13.00 V. Change OVP on the Protection screen (o)` | `35.00 V: not below OVP 13.00 V` |  | 73 / 30 |
| Rejected: not below OCP | `35.00 A is not below OCP 1.20 A. Change OCP on the Protection screen (o)` | `35.00 A: not below OCP 1.20 A` |  | 72 / 29 |
| Cannot turn ON (ordering) | `Cannot turn ON: set voltage is not below OVP (o)` | `ON blocked: V not below OVP` |  | 48 / 27 |
| Step at live maximum | `At live maximum 20.05 V` | = |  | 23 / 23 |
| Step at minimum | `At minimum 0.00 V` | = |  | 17 / 17 |
| Step at a protection limit | `Limited by OVP 13.00 V` | = |  | 22 / 22 |
| Set above live maximum (persistent) | `Set 20.00 V is above the live maximum 18.40 V. Lower it to turn output ON` | `V 20.00 above live max 18.40` |  | 73 / 28 |
| Cannot turn ON (live maximum) | `Cannot turn ON: set voltage is above the live maximum` | `ON blocked: V above live max` |  | 54 / 29 |
| Device kept another value | `Device kept 5.00 V` | = |  | 18 / 18 |
| Link lost | `Output state unknown, may still be ON  retrying every 1 s` | `Output unknown · retrying 1 s` |  | 57 / 29 |
| Link lost, Reconnect off | `Output state unknown, may still be ON  reconnect off · r retry` | `Output unknown · r retry` |  | 62 / 24 |
| OFF queued | `Off queued · use the front-panel button` | = |  | 39 / 39 |
| Write refused, link down | `Link down. Changes are not sent.` | `Link down: not sent` |  | 32 / 19 |
| Link restored | `Link restored · output ON at 12.00 V / 1.000 A` | `Restored · ON 12.00 V / 1.000 A` |  | 46 / 31 |
| Link restored, OFF sent | `Link restored · queued OFF sent, output OFF` | `Restored · OFF sent` |  | 43 / 19 |
| Protection tripped | `OTP tripped  output forced off ·` | `OTP tripped · output off ·` | `Space` clear | 47 / 41 |
| Still tripped | `Still tripped. Check the cause, then try again.` | `Still tripped` |  | 47 / 13 |

Placeholders: `{V}` `{I}` setpoint values, `{ΔV}` `{ΔI}` signed changes, `{x}` the typed text, `{max}` the limit named in the message.

## 11. Not yet designed

The implementation agent must not invent these silently; ask or stub with a plain message.

1. *(Not used; numbering kept.)*
2. **"No device found" start screen** and port-selection list. v1 behaviour is decided: print the candidates or "no device found" to stderr and exit 1 (spec/MAIN.md §6).
3. *(Not used; numbering kept.)*
4. *(Not used; secondary-screen stretching is §8.5.)*
5. *(Not used; the Fluid main panel and the fonts F5 and F7 are in §2.2 and §4.2. Fonts larger than F7 are not planned.)*
6. **Measured-voltage decimals** (3 vs 2) once the device resolution is verified (§4.3).
7. **Help overlay text for Compact** beyond the scrolling rule in §5.13, and for screens other than the main panel (their content is derived from the key tables, only `help-main-standard` is a golden rendering).
8. Anything marked [U]: output-enable write, protection state mapping and clearing, CV/CC value mapping, brightness and volume ranges, persistence across power cycles, what the output does when the USB cable is pulled, and whether the device trips when the setpoint equals the limit (SAFETY.md §1).

## 12. Acceptance checklist

An implementation conforms when all of the following hold:

1. For each name in Appendix A, rendering the corresponding screen/state at the stated reference size produces exactly those characters (trailing spaces ignored), including the digit font the size selects. At other sizes the invariants of item 3 hold instead.
2. Colours: spot-check each style in §3.2 against its use in §6 and §8 in a 24-bit terminal; repeat in 256 and 16 modes; repeat with `NO_COLOR=1` and confirm every state (including focus, blocked presets and unknown output) is still distinguishable.
3. Layout switching and stretching: at 120×30 and 100×21 the Fluid layout with Large extras; at 99×30 and 80×21 Fluid without them; 80×20 → Compact; 44×16 → Compact; 43×16 and 44×15 → Too small. For every size in a sweep from 44×16 to 250×80 and every screen and state: no overlap, no clipped digit or text, set and measured values and output state visible, key bar complete (`Esc back` and `? help` present), Fluid fills the terminal with no margin, Compact stays centred. Digit fonts: 80×24 and 100×21 use F3, 100×30 uses F5, 120×40 uses F7; the font never changes the position of anything outside the readout boxes.
4. Editing: ↑/↓ at each cursor position changes the value by the stated step; values stop at 0, at the live maximum and just below OVP/OCP; `Tab` moves focus and the `▸` title.
5. Typed entry: typing `5` on the main panel starts entry (it does **not** recall M5); typing `35` with a 20.05 V live maximum is rejected and the setpoint is unchanged; `m` `5` recalls M5.
6. Prompts: with a prompt showing, `⏎` and unrelated keys do nothing; two quick `⏎` after typing `12` never change the output; `y` confirms; `Space` cancels and turns the output OFF.
7. Jump guard: with the output ON at 5.00 V, seven `↑` on the units digit stop before 12.00 V and show the prompt when the sum passes 5.0 V; a recall with the output ON always prompts; every jump/recall/ON prompt shows V and I.
8. Safety: output ON asks by default; a preset above the live maximum is blocked; setpoint ≥ OVP or OCP is rejected on the main panel; the output cannot be turned ON while the ordering is violated.
9. Link loss: killing the serial link shows dashes and `?` for the output within 3 missed ticks; `Space` queues OFF; reconnecting sends the queued OFF first and shows the restore notice; no write is sent while the link is down.
10. `Space` toggles output on every screen, cancels (and turns OFF) in typed entry and prompts, and only turns OFF on the too-small screen. `Ctrl-X` turns OFF everywhere, including text fields.
11. *(Not used; numbering kept.)*
12. Quit: `q` with the output ON asks; `Ctrl-C` does not; the stdout line of SAFETY.md §4.9 is printed for each case.
13. Every Compact message of §10 fits 43 cells; no key cap is truncated at 44 columns.
14. No glyph outside §4.1 appears.

## Appendix A — Reference renderings

The renderings are plain-text files in `tests/golden/`, one per name (`tests/golden/<name>.txt`), used directly as golden-test inputs. They show characters and positions only; colour is per the tables above. Each file is exactly the stated size including blank rows; trailing spaces are not significant. Key bars are generated by §5.8 at the reference width; messages come from §10. Where a file and a table disagree, the table wins (§0): report it and fix the file. To change a rendering, edit its file and keep this index in step. Sizes are terminal sizes, except `main-compact-*` (44×14) and `settings-compact` (44×13): those are the top-aligned body block of a 44×16 terminal, the Compact floor, so a test renders at 44×16, compares the file's rows from the top and expects the remaining rows blank. `main-large-*` is the Large band at its 100×21 minimum, `*-standard` the Standard band at 80×24, `*-120x40` the Large band at 120×40.

| Name | Size | Screen and state |
|---|---|---|
| `main-large-cv` | 100×21 | Main panel, Large, output ON, constant voltage |
| `main-large-cc` | 100×21 | Main panel, Large, current limiting (CC) |
| `main-large-trip` | 100×21 | Main panel, Large, protection tripped (OTP) |
| `main-cv-120x40` | 120×40 | Main panel, Fluid Large band, F7 digits, output ON, constant voltage |
| `main-cc-120x40` | 120×40 | Main panel, Fluid Large band, F7 digits, current limiting (CC) |
| `main-trip-120x40` | 120×40 | Main panel, Fluid Large band, F7 digits, protection tripped (OTP) |
| `main-standard-cc` | 80×24 | Main panel, Standard, current limiting (CC) |
| `main-standard-confirm-jump` | 80×24 | Main panel, Standard, large voltage jump prompt |
| `main-standard-confirm-on` | 80×24 | Main panel, Standard, "turn output on" prompt (output still OFF) |
| `main-standard-cv` | 80×24 | Main panel, Standard, output ON, constant voltage |
| `main-standard-entry` | 80×24 | Main panel, Standard, typed entry on the voltage set value |
| `main-standard-lost` | 80×24 | Main panel, Standard, link lost (output state unknown) |
| `main-standard-reject` | 80×24 | Main panel, Standard, typed value rejected (35 V above the live maximum) |
| `main-standard-trip` | 80×24 | Main panel, Standard, protection tripped (OTP) |
| `main-compact-cc` | 44×14 | Main panel, Compact, current limiting (CC) |
| `main-compact-confirm-jump` | 44×14 | Main panel, Compact, large voltage jump prompt |
| `main-compact-confirm-on` | 44×14 | Main panel, Compact, "turn output on" prompt (output still OFF) |
| `main-compact-cv` | 44×14 | Main panel, Compact, output ON, constant voltage |
| `main-compact-entry` | 44×14 | Main panel, Compact, typed entry on the voltage set value |
| `main-compact-lost` | 44×14 | Main panel, Compact, link lost (output state unknown) |
| `main-compact-trip` | 44×14 | Main panel, Compact, protection tripped (OTP) |
| `presets-120x40` | 120×40 | Presets screen, Fluid Large band (edit box F7) |
| `presets-compact` | 44×16 | Presets screen, Compact |
| `presets-standard` | 80×24 | Presets screen, Standard |
| `protection-120x40` | 120×40 | Protection screen, Fluid Large band (editing OCP, F7) |
| `protection-compact` | 44×16 | Protection screen, Compact (editing OCP) |
| `protection-standard` | 80×24 | Protection screen, Standard (editing OCP) |
| `settings-120x40` | 120×40 | Settings screen, Fluid Large band |
| `settings-compact` | 44×13 | Settings screen, Compact |
| `settings-standard` | 80×24 | Settings screen, Standard |
| `help-main-standard` | 80×24 | Help overlay over the main panel, Standard |
| `toosmall-43x16` | 43×16 | Too-small screen (output ON) |
| `font-f3` | 45×3 | Digit font F3: `0123456789.-` |
| `font-f5` | 67×5 | Digit font F5: `0123456789.-` |
| `font-f7` | 89×7 | Digit font F7: `0123456789.-` |

## Appendix B — Decision log (design)

Baseline 2026-10-30. Product-level decisions are in [MAIN.md](MAIN.md) §9; this log keeps the choices that shaped the look of the UI.

| Decision | Rationale |
|---|---|
| Layout is *Front Panel* (two big readouts, digit-cursor editing), chosen among five proposals (Dashboard, Front Panel, Scope, Meters, Workspace) | User choice; reads from across the bench, edits like a knob |
| Colour scheme is *Midnight Slate*, chosen among five (Midnight Slate, Amber Phosphor, Bench Light, Graphite & Signal, Scope Classic) | User choice |
| Brand label in the header is `FNIRSI` (manufacturer), model after it | User request |
| Power bar fill uses the power colour, not a green→red gradient | User request: consistency with the power digits |
| Trip label in the header keeps the header background; only the text becomes red | User request |
| Three secondary screens (Presets, Protection, Settings) in the same style, reached by `p o ,`; header and live strip persist; `Space` toggles output everywhere | User approved direction |
| Settings shows a read-only "Set precision 10 mV / 1 mA" instead of an editable decimals row | An editable "2 / 3" row contradicted the main panel (measured values show 3 decimals) |
| Preset recall is the chord `m` + digit; bare digits start typed entry | A typed `5` that recalled M5 would energise a load with no prompt |
| Prompts are answered with `y`/`n`/`Esc`; `⏎` never confirms; `Space` and `Ctrl-X` cancel and turn OFF | The `⏎` that applied a typed value must not also confirm the follow-up prompt |
| Output state is *unknown* (`?`) when the link is down; `Space` queues an OFF | The UI must not claim OFF while the supply is probably still ON |
| Layout is a fixed Compact floor plus one Fluid layout that fills the terminal (btop-style), digits step up F3 → F5 → F7 | User wants a proportionally growing interface; Fluid details in §2.2, §8.5 |
| Key bars are generated by the §5.8 algorithm with `Esc back` and `? help` reserved | Keeps key bars correct at every width |
