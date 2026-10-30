# dps150 — Requirements Register

Status: baseline, 2026-10-30. The index that connects the specification to tests and to the implementation plan. It does not define behaviour: each row is a one-line summary of a rule whose full text lives in the source section. **If a row and its source disagree, the source wins; fix the row.**

## 1. Conventions

- **ID** = area + number, never reused or renumbered. A rule that is removed after the baseline keeps its row with status `withdrawn`. Areas: `PRT` protocol and device layer, `CLI` command line, `LIM` limits and validation, `SAF` output safety, `LNK` link loss, `EDT` editing, `PRE` presets, `PRO` protection, `APP` app behaviour (connection, errors, threads, constants), `SIM` simulator, `CFG` configuration files, `UI` rendering.
- **Tag in the source.** The source section carries `{#ID}` on the line that states the rule (on each line when a rule is stated in more than one place), so `grep -rn '{#SAF-03}' spec/` finds it. A rule that lives in a table row has no inline tag; its row below names the section.
- **Status** copies the source tag: `D` decided, `P` proposed and not yet reviewed, `U` depends on unverified device behaviour (PROTOCOL.md §8), `V` verified on hardware (PROTOCOL.md), `—` untagged in the source (normative as written).
- **Verify** says where a test lives (DEVOPS.md §4): `L1` in-process unit/app test, `L2` pseudo-terminal test against the simulator, `L3` golden rendering test, `M` manual on hardware or real terminals (DESIGN.md §12).
- **Tests cite IDs.** A test name or comment contains the ID it verifies (for example `saf_03_output_on_asks_first`). Every row must end up with at least one test, or an explicit `M` and a checklist item.
- **Changing a rule** means: edit the source, edit the row, record a decision in spec/MAIN.md §9 if the behaviour changes, then update tests.

## 2. Protocol and device layer (PRT, CLI)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| PRT-01 | Send the connect frame `F1 C1 00 01 01 02` on every port open (and every reconnect) before anything else; without it the device is silent | MAIN §3, PROTOCOL §3, APP §2 | V | L1 L2 M |
| PRT-02 | Frame codec: checksum on encode and decode; resync on `0xF0`; tolerate partial frames at open and pushed frames interleaved with replies; match replies by type, not arrival order | PROTOCOL §2, MAIN §3 | V | L1 L2 |
| PRT-03 | SET frames get no reply; after a write show the user's value, then reconcile with the next dump and show `Device kept {value}` if it differs by more than the display resolution | APP §3 | D | L1 |
| PRT-04 | Values are sent at full precision and rounded for display only | DESIGN §4.3 | D | L1 |
| PRT-05 | On start the app changes no device setting; it connects, reads the full state and shows it; writes happen only on explicit user action | SAFETY §4.10, MAIN §3 | D | L1 L2 |
| PRT-06 | Poll `FF` at the Refresh rate (default 10 Hz); skip a tick while the previous reply is outstanding; pushed frames update the model whenever they arrive | APP §2 | — | L1 L2 |
| CLI-01 | Flags are `--port <path>`, `--help`, `--version` and nothing else in v1 | MAIN §6 | D | L1 |
| CLI-02 | Port resolution: `--port` wins; else with `auto_detect` match USB `2E3C:5740`; exactly one match is used, zero or several print the candidates to stderr and exit 1; with `auto_detect = false` the port comes from `--port` or config `port`, else exit 1 | MAIN §6 | D | L1 M |

## 3. Limits and validation (LIM)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| LIM-01 | Every value is checked on the host before sending; never send negative, NaN, infinite or unparsed values | SAFETY §1, PROTOCOL §7, MAIN §3 | D | L1 |
| LIM-02 | Set voltage is within 0 … min(live max `E2`, 30.00 V); set current within 0 … live max `E3` | SAFETY §1 | D | L1 |
| LIM-03 | OVP, OCP, OPP, OTP, LVP are within 0 … the device range for that limit (`E4`..`E8`) | SAFETY §1 | D | L1 |
| LIM-04 | Ordering: set V < OVP; set I < OCP; set V × set I < OPP; LVP < live input voltage. "Below" means at least one resolution step (10 mV, 1 mA, 0.1 W) except that a limit at its device maximum allows equality. The margin is one named constant [U] | SAFETY §1 | D, U | L1 |
| LIM-05 | The ordering rules hold on every screen and in both directions: main panel and Presets reject a typed value or recall and stop a cursor step just below the limit; the Protection screen shows `✗` and disables `⏎` | SAFETY §1, DESIGN §8.3 | D | L1 L3 |
| LIM-06 | If the device already violates an ordering rule at start or reconnect, change nothing, show the violation on the Protection screen, and refuse output ON with a notice | SAFETY §1 | D | L1 L2 |
| LIM-07 | If the live maximum drops below the voltage (or current) setpoint, never change the setpoint; show the persistent warning and refuse output ON; OFF and editing stay possible | SAFETY §1, DESIGN §10 | D | L1 L3 |
| LIM-08 | A typed out-of-range value is rejected with a reason, never clamped or replaced; cursor steps stop at the limit and name it in a notice | DESIGN §1.8, §7.1, §7.2 | D | L1 L3 |
| LIM-09 | Typed values are rounded to 10 mV (voltage) / 1 mA (current) before the range check | DESIGN §7.2 | D | L1 |

## 4. Output safety (SAF)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| SAF-01 | `Space` toggles the output on every screen; where it is not a plain toggle (typed entry, prompts, too-small screen) it cancels what is pending and can only turn the output OFF | DESIGN §1.5, §9.1 | D | L1 M |
| SAF-02 | `Ctrl-X` turns the output OFF in every mode, including text fields | DESIGN §1.5, §9.1 | D | L1 |
| SAF-03 | Turning the output ON asks first by default and the prompt shows the V and I it will apply; "Output on = immediately" skips the prompt but not the ordering rules | SAFETY §2 | D | L1 L3 |
| SAF-04 | Turning the output OFF never asks | SAFETY §2 | D | L1 |
| SAF-05 | Prompts are answered only with `y`, `n` or `Esc`; `⏎` and other keys are ignored; `Space` / `Ctrl-X` cancel the prompt and turn the output OFF if it is ON; nothing is sent while a prompt is pending | DESIGN §5.12 | D | L1 |
| SAF-06 | Jump guard (output ON): a voltage or current change that puts the setpoint beyond the threshold (default 5.0 V / 2.0 A) from its reference is held and prompted; the reference is the setpoint at last output-ON or last confirmed prompt; the window is cumulative; the prompt shows V and I | SAFETY §2 | D | L1 |
| SAF-07 | A held key counts as one action for the jump guard; at most one SET per 50 ms, always the latest value | DESIGN §7.1 | D | L1 |
| SAF-08 | Preset recall with the output ON always asks, whatever the size of the change | SAFETY §2, DESIGN §8.1 | D | L1 |
| SAF-09 | A pending prompt or typed entry is cancelled, applying nothing, when the link is lost or the terminal becomes too small | DESIGN §5.12, §2.3 | D | L1 |
| SAF-10 | Too-small screen: shows the output state; `Space` / `Ctrl-X` only turn the output OFF; `q` is ignored; `Ctrl-C` quits | DESIGN §2.4 | D | L1 L3 |
| SAF-11 | Quit rules: `q` asks when it would leave the output ON; On exit = leave (default) or turn off with a 500 ms read-back; `Ctrl-C`/`SIGTERM`/`SIGHUP` quit without a prompt and follow the setting; the stdout line is printed last; exit status 1 if OFF cannot be confirmed | SAFETY §4.9 | D | L1 L2 M |
| SAF-12 | Overwriting or clearing a preset slot asks first, unless the slot already holds exactly those values | DESIGN §8.1 | D | L1 |

## 5. Link loss (LNK)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| LNK-01 | The link is lost after 3 consecutive missed poll ticks (a skipped tick counts) | SAFETY §3 | D | L1 L2 |
| LNK-02 | Never show stale or unknown numbers as live: measured values become dashes and the output state is shown as unknown (`?`), never OFF | SAFETY §3 | D | L1 L3 |
| LNK-03 | While the link is down nothing that writes to the device is sent; edits, recalls and settings writes are refused with the notice | SAFETY §3 | D | L1 L2 |
| LNK-04 | `Space` / `Ctrl-X` while down queue an output-OFF, sent as the first frame after reconnect; pressing `Space` again clears the queue | SAFETY §3 | D | L1 L2 |
| LNK-05 | Typed entry and prompts are cancelled on link loss | SAFETY §3 | D | L1 |
| LNK-06 | Reconnect: with Reconnect = on retry every 1 s; with off retry once per `r` | SAFETY §3 | D | L1 L2 |
| LNK-07 | On reconnect: connect frame, full state read, queued OFF first, restore notice; writes stay blocked and the screen shows device values until the first full dump has arrived | SAFETY §3 | D | L1 L2 |

## 6. Editing (EDT)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| EDT-01 | Exactly one readout has focus (Voltage at start); `Tab` toggles it | DESIGN §7.1 | D | L1 L3 |
| EDT-02 | `←` `→` move the digit cursor one digit, no wrap; each quantity remembers its cursor | DESIGN §7.1, §5.3 | D | L1 L3 |
| EDT-03 | `↑` `↓` add or subtract the value of the digit under the cursor (V: 10, 1, 0.1, 0.01; I: 1, 0.1, 0.01, 0.001), limited per LIM-02..05, and send | DESIGN §7.1 | D | L1 |
| EDT-04 | The displayed set value updates immediately and is reconciled with the device read-back (PRT-03) | DESIGN §7.1 | D | L1 |
| EDT-05 | Typed entry starts with `⏎`, `v` / `i`, or a digit or `.` on the main panel; a bare digit never recalls a preset | DESIGN §7.2, §9.2 | D | L1 L3 |
| EDT-06 | Typed entry accepts digits and one `.`, max 5 characters; `Backspace` deletes; `⏎` applies; `Esc` cancels; other keys are ignored | DESIGN §7.2 | D | L1 |
| EDT-07 | `m` then `1`..`6` recalls a preset; any other key cancels the chord | DESIGN §8.1, §9.2 | D | L1 |

## 7. Presets and protection (PRE, PRO)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| PRE-01 | Presets are the six device slots M1..M6 (V, I); recall writes the V and I setpoints and never changes the output state | DESIGN §8.1, MAIN §9 | D, U | L1 L2 |
| PRE-02 | A slot above the live maximum or breaking an ordering rule is blocked on recall with a notice, never clamped | DESIGN §8.1 | D | L1 L3 |
| PRE-03 | Slot names are stored by the app in `presets.toml`, at most 9 characters | DESIGN §8.1, APP §4.2 | D | L1 |
| PRE-04 | Rename is an inline text field: `⏎` saves, `Esc` cancels, `Space` types a space | DESIGN §8.1 | D | L1 |
| PRO-01 | A tripped protection shows the header label, the tripped lamp, output OFF and the `NAME tripped` message. Which lamp and the value at trip depend on the unverified `DC` mapping | DESIGN §6.7 | D, U | L3 M |
| PRO-02 | `Space` in the tripped state sends output-off and re-reads the protection state; if still tripped show `Still tripped` | DESIGN §6.7 | D, U | L1 M |

## 8. Configuration (CFG)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| CFG-01 | Config is `$XDG_CONFIG_HOME/dps150/config.toml` (default `~/.config/dps150/`), created on first change; missing keys use defaults | APP §4.1 | D | L1 |
| CFG-02 | Slot names live in `presets.toml` next to it; V and I are never stored there | APP §4.2 | D | L1 |

## 9. Rendering (UI)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| UI-01 | Layout selection by size: Fluid with Large extras ≥ 100×21, Fluid ≥ 80×21, Compact ≥ 44×16, otherwise too small; tested in that order | DESIGN §2.2 | D | L1 L3 |
| UI-02 | Each reference rendering in `tests/golden/` (indexed in DESIGN Appendix A) is reproduced exactly (characters and positions) at its reference size and state | DESIGN §0, §12.1 | D | L3 |
| UI-03 | Compact: body centred, stack top-aligned. Fluid: body fills the terminal with no margin. Header, strip and key bar span the full width in both | DESIGN §2.3 | D | L3 |
| UI-04 | Colour depth is detected once, in the order of DESIGN §3.4 (`NO_COLOR`, config, `COLORTERM`, `TERM`) | DESIGN §3.4 | D | L1 |
| UI-05 | Colour is never the only carrier of meaning; with colour off every state remains distinguishable | DESIGN §1.3, §3.4 | D | L3 M |
| UI-06 | Only the glyphs in §4.1 are used, each 1 cell wide | DESIGN §4.1, §2.1 | D | L3 |
| UI-07 | Number formats and rounding follow DESIGN §4.3 | DESIGN §4.3 | D | L1 L3 |
| UI-08 | The key bar is built by the §5.8 algorithm for the tier's design width; `Esc back` and `? help` are always visible | DESIGN §5.8 | — | L3 |
| UI-09 | Every message in DESIGN §10 fits its row (76 / 96 / 43 cells) including key caps, and is never truncated | DESIGN §5.12, §10 | D | L1 |
| UI-10 | A non-UTF-8 locale prints one line to stderr and exits non-zero before entering the alternate screen | DESIGN §2.1 | D | L1 |
| UI-11 | The alternate screen is entered and the cursor hidden; both are restored on exit, panic and signal | DESIGN §2.1 | D | L2 M |
| UI-12 | Resize re-evaluates the tier immediately and keeps all state | DESIGN §2.3 | D | L1 L3 |
| UI-13 | No animation, blinking, spinners or mouse interaction in v1 | DESIGN §1.7 | D | M |
| UI-14 | Fluid frame and main panel: fills the whole terminal with no margin and no cap; readout boxes take the spare height with the content block centred; the left box takes the odd cell; power bar gains a cell per extra column; invariants hold over the size sweep 44×16 … 250×80 | DESIGN §2.2, §12.3 | D | L3 |
| UI-15 | Big digits use the largest of F3/F5/F7 that fits both readout boxes (inner height ≥ 2h+4, inner width ≥ 34/42/50); re-evaluated on resize; Compact uses F3. 100×21 and 80×24 → F3, 100×30 → F5, 120×40 → F7 | DESIGN §2.2 | D | L1 L3 |
| UI-16 | Secondary screens stretch per DESIGN §8.5: lists and text top-aligned, digit blocks centred with the largest font that fits, load bars gain a cell per column, Settings grid halves the body | DESIGN §8.5 | D | L3 |
| UI-17 | Fonts F3, F5, F7 are generated by the segment and corner rule of DESIGN §4.2 and match the files `tests/golden/font-f3.txt`, `font-f5.txt` and `font-f7.txt` | DESIGN §4.2 | D | L1 L3 |

## 10. App behaviour (APP)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| APP-01 | Start-up failures print a message to stderr and exit 1: port cannot be opened (with a `dialout` hint on permission denied); no `FF` reply within 3 s of the connect frame (frame resent once at 1.5 s). There is no start screen | APP §5 | D | L1 L2 |
| APP-02 | Reconnect repeats open and handshake per attempt; a failed attempt returns to Lost; writes stay blocked until the first full dump | APP §5 | D | L1 L2 |
| APP-03 | The reader resynchronises on `0xF0`, keeps partial frames, drops frames with a bad checksum or unexpected length and resumes after their start byte, and ignores unknown types, without UI effect | APP §6 | D | L1 |
| APP-04 | An incomplete frame is discarded after 200 ms without a new byte | APP §6 | D | L1 |
| APP-05 | Invalid or unreadable config files: defaults for the session plus a notice; the invalid file is never overwritten; save failures give a notice and apply to the session | APP §6 | D | L1 |
| APP-06 | Exit status 0 normal, 1 runtime failure, 2 usage error; errors on stderr, only the exit lines of SAFETY §4.9 on stdout | APP §6 | D | L1 L2 |
| APP-07 | On panic the terminal is restored, `dps150: internal error, output state unknown` goes to stderr, and nothing is sent to the device | APP §6 | D | L1 |
| APP-08 | Device I/O runs on its own thread; the UI thread never blocks on I/O | APP §7 | D | L1 |
| APP-09 | An output-OFF command goes first in the queue, bypasses SET coalescing and is never dropped or merged away | APP §7 | D | L1 |
| APP-10 | Every value that depends on unobserved device behaviour is a named item in one `constants` module and is listed in APP §8 | APP §8 | D | L1 |

## 11. Simulator (SIM)

| ID | Requirement | Source | Status | Verify |
|---|---|---|---|---|
| SIM-01 | A simulator exists for tests, fault injection, development without the unit and demos; users with a supply never need it | SIMULATOR §1 | D | — |
| SIM-02 | Library crate `simulator`: a pure state machine (`feed`, `advance`, `take_output`) with injected time and no I/O | SIMULATOR §2 | D | L1 |
| SIM-03 | Binary `dps150-sim` opens a pty, prints its path on the first stdout line, runs in real time and reads control commands from stdin | SIMULATOR §2 | D | L2 |
| SIM-04 | Initial state equals the state observed on the test unit | SIMULATOR §3 | V | L1 |
| SIM-05 | Silent until the connect frame; then pushes `C3`, `C0`, `E2`, `E3`, `C4` every 200 ms each | SIMULATOR §3 | V | L1 L2 |
| SIM-06 | `GET` of a known type is answered (`FF` is the 139-byte dump); unknown types get no reply | SIMULATOR §3 | V | L1 |
| SIM-07 | `SET` of writable types is stored verbatim without validation and without an acknowledgement | SIMULATOR §3 | V | L1 |
| SIM-08 | Output model: resistive load, CV/CC, power, output limited to the live maximum | SIMULATOR §4 | U | L1 |
| SIM-09 | `DB` turns the output on and off; ignored while a protection is tripped | SIMULATOR §4 | U | L1 |
| SIM-10 | Protections trip on their conditions, set `DC` codes 1..5, switch the output off, and clear on `DB = 00` once the cause is gone | SIMULATOR §4 | U | L1 |
| SIM-11 | Fault injection through library methods and stdin commands: `load`, `vin`, `temp`, `trip`, `keep`, `unplug`/`plug`, `close`/`reopen`, `corrupt`, `split`, `delay`, `powercycle`, `state`, `quit` | SIMULATOR §5 | D | L2 |
| SIM-12 | Conformance: configured with the captured raw values and fed the captured host bytes, the simulator reproduces the first five frames of the captured device stream byte for byte and answers each `GET C0` | SIMULATOR §7 | V | L1 |
