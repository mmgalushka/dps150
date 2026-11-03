# dps150 — Specification

Status: baseline, 2026-10-30. This document is the central artifact of the project. Every design decision must be reflected here before it is implemented.

Companion documents: [PROTOCOL.md](PROTOCOL.md) (wire protocol, verified behaviour), [DESIGN.md](DESIGN.md) (UI layout, colour scheme, screens, interaction), [SAFETY.md](SAFETY.md) (limits, confirmations, link loss, safety rules), [APP.md](APP.md) (data binding, polling, configuration files, connection and error handling), [SIMULATOR.md](SIMULATOR.md) (the device simulator) and [DEVOPS.md](../DEVOPS.md) (toolchain, testing, CI, release, distribution). [REQUIREMENTS.md](REQUIREMENTS.md) is the register of testable rules (stable IDs `PRT-01`, `SAF-03`, …) that tests and the implementation plan cite; the rule text stays in the source sections, tagged `{#ID}`. spec/MAIN.md owns goals, scope, architecture and the decision log; it links to the companions instead of repeating them.

Legend for protocol/device facts: **[V]** verified on real hardware, **[U]** unverified (from published sources).

## 1. Goals and non-goals

### Goals
- Simplify control of the FNIRSI DPS-150 bench power supply from a terminal.
- Full-screen TUI as the primary interface.
- Single native binary, installable via Homebrew (`brew install mmgalushka/tap/dps150`) and as a `.deb` package (`sudo apt install ./dps150_<version>_<arch>.deb`). See §6.
- macOS and Linux are first-class; Windows is deferred (added later).

### Non-goals (v1)
- Sequences, scripted ramps and steps — a lab bench supply does not need a scripting engine.
- Logging/graph export (CSV, plots) — deferred.
- Scriptable CLI subcommands — deferred (to be revisited).
- Support for other FNIRSI models.

## 2. Device overview
- FNIRSI DPS-150: 0–30 V, 0–5 A, 150 W max. Resolution 10 mV / 1 mA. CV/CC modes. [U]
- Input 5–32 V DC (USB-C PD/QC or DC jack); 30 V output requires ~32 V input. [U] The UI shows the live maximum voltage, which follows the input (SAFETY.md §1).
- Control via micro-USB (USB serial). Official software is Windows-only. [U]
- Protections: OVP, OCP, OPP, OTP. [U]
- Test unit: connected to the author's Mac.

## 3. Protocol
The wire protocol is documented in [PROTOCOL.md](PROTOCOL.md), the source of truth for frame format, command and parameter tables, and verified behavior. It was verified on real hardware on 2026-10-28 (firmware V1.2). Do not duplicate its details here.

Design consequences for this project:
- Transport is USB CDC-ACM serial at 115200 8N1; the port is `/dev/cu.usbmodem*` on macOS (use `cu.`, not `tty.`).
- The client sends the connect frame (`F1 C1 00 01 01 02`) on every port open; without it the device stays silent (PROTOCOL.md §3). {#PRT-01}
- Once connected, the device pushes a few status frames at about 1 Hz on its own; the `device` layer polls the `FF` full dump for a coherent snapshot. The parser must tolerate pushed frames interleaved with replies and partial frames at port open.
- SET frames get no reply, and the device does not validate values. The host must check every setpoint and protection threshold against the device limits before sending (SAFETY.md §1: typed values are rejected, cursor steps stop at the limit, nothing is silently clamped), and never send negative or non-finite values.
- Setpoint and protection writes persist in device settings; the app must restore or deliberately own them.
- Still unverified (see PROTOCOL.md §8): output enable, protection trip behavior, persistence across power cycles.

## 4. Features (v1)
1. **Live monitoring**: measured voltage, current, power, CV/CC mode, output state, input voltage, temperature.
2. **Setpoints and output**: set V and I with a digit cursor (DESIGN.md §7) and direct numeric entry; output on/off; protection limits.
3. **Presets**: the six device slots M1–M6 (V, I) with names kept by the app; recall writes the setpoints.

The complete UI design (layout, colour scheme, screens, key bindings, formats) is in [DESIGN.md](DESIGN.md), the source of truth for everything the user sees. The safety and behaviour rules are in [SAFETY.md](SAFETY.md); data binding, polling and configuration files are in [APP.md](APP.md). It records layout and colour decisions; remaining UI gaps are listed in DESIGN.md §11.

## 5. Architecture
Rust workspace, layered (directory layout: [DEVOPS.md](../DEVOPS.md) §2.1):
- `protocol`: pure encode/decode/checksum, unit-tested.
- `device`: a pure link state machine (handshake, polling, typed state, write queue) and a thin thread with the serial transport (APP.md §7), so a simulator can substitute at the byte level.
- `app`: state, commands, presets.
- `ui`: ratatui rendering and input. Implements [DESIGN.md](DESIGN.md) exactly: layouts, components, colour styles, screens and key map. The layout stretches to fill the terminal above a fixed Compact floor (DESIGN.md §2.2). Colour-depth detection and the style table (DESIGN.md §3) are `ui` responsibilities.
- `simulator`: fake DPS-150 for development and CI: a library with a pure state machine plus the thin binary `dps150-sim` that exposes it as a pty and takes fault-injection commands (SIMULATOR.md). Unverified device behaviour is labelled *assumed* in it.

## 6. Platforms and packaging
- Stack: Rust, ratatui, `serialport` crate. [Decided]
- Name: `dps150` (package, binary, repository `mmgalushka/dps150`, public). [Decided]
- macOS and Linux (Homebrew tap, `.deb`); Windows deferred. GitHub-only infrastructure for now. Details, targets and install commands: [DEVOPS.md](../DEVOPS.md).
- Serial device naming and permissions: DEVOPS.md §8.

### Command line and port selection {#CLI-01} {#CLI-02}
`dps150 [--port <path>] [--help] [--version]`. No other flags in v1.
- `--port <path>` always wins and skips auto-detection.
- Otherwise, with config `auto_detect = true` (default), enumerate serial ports and keep those with USB `2E3C:5740` (PROTOCOL.md §1). Exactly one match: use it. Zero or several: print the candidates (or "no device found") to stderr and exit with status 1. There is no start screen or port picker in v1 (DESIGN.md §11.2).
- With `auto_detect = false`, the port comes from `--port` or the config `port` key; if neither is set, exit with status 1.

## 7. Testing
- Unit tests for protocol; integration tests against the simulator; manual validation on real hardware.
- UI: render each reference rendering in `tests/golden/` (indexed in [DESIGN.md](DESIGN.md) Appendix A) at its reference size to an in-memory cell buffer and compare characters against it (golden tests); at all other sizes run invariant tests over a size sweep (no overlap or clipping, key values visible); run the acceptance checklist in DESIGN.md §12 by hand on real terminals.
- Test levels, the pty simulator and CI: [DEVOPS.md](../DEVOPS.md).

## 8. Open questions
Product and design questions are all decided (§9). What remains is unverified device behaviour and pending review of design proposals:
- **Unverified device behaviour**: output enable, trips, persistence and the cable pull were verified in H1 Part B (2026-11-02). What remains is listed in PROTOCOL.md §8 (flags `D8`/`E1`, panel ranges, the 2.8 V meter reading).
- **Design proposals not yet reviewed**: none; no `[P]` item remains in the specification.
- **Simulator flag.** The simulator is a separate binary that prints a pty path (DEVOPS.md §4); `dps150` has no simulator mode.
- Later reviews (crates.io, apt repository, Windows, non-interactive CLI): see §1 non-goals and DEVOPS.md §9.

## 9. Decision log

Baseline 2026-10-30. Earlier entries (including superseded and withdrawn ones) are folded into the decisions below; the design-level log is DESIGN.md Appendix B. New decisions are appended with their date.

| Date | Decision | Rationale |
|---|---|---|
| baseline | Product is the FNIRSI DPS-150; full-screen TUI; macOS and Linux first, Windows deferred | User choice (the user wrote DSP-150/DPS-160 in places; DPS-150 confirmed by context) |
| baseline | v1 scope: live monitoring, set V/I and output, device presets M1–M6, protection limits, settings. Sequences (scripted ramps and steps) are out of scope | A lab bench supply does not need a scripting engine; it also removes the feature that depended most on unverified output-enable behaviour |
| baseline | Rust + ratatui + `serialport`; name `dps150`, repository `mmgalushka/dps150` (public) | Single binary, best TUI library, native brew/apt packaging; crates.io and Debian names were free |
| baseline | Protocol facts live in PROTOCOL.md, dev/CI/distribution in DEVOPS.md, UI in DESIGN.md, rules in SAFETY.md, app behaviour in APP.md; MAIN keeps goals, architecture and decisions only | Each changes independently (hardware verification, tooling, design); avoids duplicated, drifting text |
| baseline | Host-side checking of every SET value; out-of-range values are rejected or stopped at the limit, never silently clamped (SAFETY.md §1) | The device accepts out-of-range values verbatim (PROTOCOL.md §7) |
| baseline | The client always sends the connect frame `F1 C1 00 01 01 02` on open; device USB ID is 2E3C:5740 | The device is silent until connect (PROTOCOL.md §3); fixtures in `tests/fixtures/` |
| baseline | Port selection: auto-detect by USB `2E3C:5740`; one match is used, zero or several exit 1 with the candidates listed; `--port <path>` overrides; v1 flags are `--port`, `--help`, `--version`; no picker UI | The VID/PID is shared by the generic Artery AT32 CDC port, so ambiguity is an error, not a guess |
| baseline | Start-up failures exit 1 with a stderr message and there is no start screen; broken config files give defaults and a notice, never an overwrite (APP.md §5–§8) | Keeps the UI free of pre-connection states |
| baseline | Files are TOML under `~/.config/dps150/` (`config.toml`, `presets.toml`); presets are the device slots M1–M6 (V, I), names kept by the app, recall writes the V and I setpoints | No device recall command exists (PROTOCOL.md §8) |
| baseline | Safety defaults accepted (SAFETY.md §2): confirm output ON, cumulative jump guard, recall with output ON asks, typed values rejected not clamped, setpoint/limit ordering enforced in both directions | User accepted the proposal |
| baseline | Link loss: output shown as unknown, no writes, queued OFF sent first on reconnect (SAFETY.md §3) | A pulled data cable most likely leaves the output ON |
| baseline | Default "On exit" is *leave output ON*; `q` asks when the output is ON; Ctrl-C/SIGTERM follow the setting without a prompt | Quitting the UI must not cut power to a connected load unexpectedly |
| baseline | When the live maximum falls below the setpoint the app warns and refuses output ON; it never rewrites the setpoint | Keeps "writes only on explicit user action" (SAFETY.md §1, §4.10) |
| baseline | UI: "Front Panel" layout (two big readouts, digit-cursor editing), "Midnight Slate" colours; btop-style scaling: fixed Compact floor (44×16) plus one Fluid layout from 80×21 with no size cap; big digits F3 → F5 → F7; no graphs, mouse or animation in v1 | User choice among five layouts and five colour schemes (DESIGN.md Appendix B); Standard and Large are width bands of Fluid |
| baseline | Simulator: library `SimDevice` (pure state machine, injected time) plus binary `dps150-sim` (pty and stdin fault injection); behaviour not observed on hardware is labelled *assumed* in one register. No `dps150` simulator mode | Serves tests, fault injection, development without the unit and demos; the pty covers the real serial path |
| baseline | GitHub-only infrastructure; no hardware-in-the-loop CI; crates.io, apt repository, Windows and a non-interactive CLI are deferred (DEVOPS.md §9); goal wording uses the tap and local-`.deb` install commands | One platform, no extra accounts; unverified output-enable makes an unattended runner unsafe; bare `brew install dps150` needs homebrew-core |
| baseline | A root `helper.sh` is the planned single entry point for development tasks (DEVOPS.md §4.0) | Removes the need to remember cargo, simulator and bridge invocations; CI does not depend on it |
| 2026-10-30 | Setpoints, limits and presets are integer thousandths of their unit in the app; binary32 only at the wire; PRT-04 reworded | Steps and limit comparisons are exact, with no float error near safety limits; a read value written back keeps its bits |
| 2026-10-30 | The device layer is a pure state machine with injected time plus a thin I/O thread (APP-11); it holds a second, independent range check on every SET (APP-12) | Link and timing rules test deterministically; a defect in the app rules still cannot put an out-of-range value on the wire of a device that accepts anything |
| 2026-10-30 | Hardware session with a resistive load comes right after the `protocol` crate, before the simulator and any output code (PLAN.md §3) | Nobody has seen the output switch; finding wrong assumptions early avoids building safety logic and tests on them |
| 2026-10-30 | Delivery in safety stages (read-only monitor, setpoints and limits, output control, completion), each run on the real unit; the first public release is complete v1.0.0 (PLAN.md) | Code that can write to the device does not exist until the read path is proven; no partial tool is published |
| 2026-11-01 | H1 Part A: `LNK-01` stays at 3 missed ticks; the simulator pushes one frame every 125 ms, not 200 ms (SIM-05) | Measured on the unit, directly and through the bridge: about 8 push frames/s; about 4 % of replies take 90 to 125 ms, worst gap 225 ms and longest missed run 1 in 60 s (PROTOCOL.md §5) |
| 2026-11-02 | H1 Part B: after every SET the app sends nothing for `SET_GAP` (100 ms), polling included (SAF-15); the simulator drops a SET followed too soon by another frame (SIM-14) | On the unit a SET followed immediately by a GET was lost 8 of 9 times; with 5 ms or more of quiet it never was (PROTOCOL.md §7.7) |
| 2026-11-02 | Clearing a trip is turning the output ON: same ordering rules and ON confirmation, `DB = 01` only for a deliberate ON, never `DB = 00` to clear, no automatic retry (SAF-13). LVP is not latched | A host `DB = 01` (like the panel button) clears the latched trip and energizes the output in one step; there is no clear-without-ON (PROTOCOL.md §7.9) |
| 2026-11-02 | On normal quit the app sends the session close frame after the quit output-off and its read-back (SAF-14) | The connect frame locks the front panel until the close frame or a cable pull (PROTOCOL.md §7.8) |
| 2026-11-02 | A pulled data cable leaves the output ON and releases the panel lock; `LNK` wording changed from "most likely" to verified; the simulator `unplug` keeps the output state | Verified on the unit with a 10 Ω load (PROTOCOL.md §7.10) |
| 2026-11-02 | `LIMIT_MARGIN` stays as a host policy although the device does not trip at equality | Verified: OVP equal to the output voltage did not trip (strict comparison) |
| 2026-11-03 | M4: where a golden rendering and the prose of DESIGN.md disagreed, the prose was changed to match the golden: `all clear` follows the lamps after four spaces, the Compact unknown output badge is 5 cells wide, the Standard hint keeps one blank cell at the edge, and the Standard header shows the port without `cu.` or `tty.` | The goldens are the reviewed renderings and later milestones test against them; the prose was the looser of the two |
| 2026-11-03 | M5 is built in two parts, M5a (write path, limit rules, main-panel editing) and M5b (Presets and Protection screens, `presets.toml`) | Together they are larger than any other milestone; the write path and the limit rules are the safety-critical half and deserve their own review and their own hardware check |
| 2026-11-03 | Until M6, a setpoint change while the output is ON is held back with `Output is ON: changes are not sent yet` | The jump guard (SAF-06) is an M6 rule; without it a held key could walk the voltage on a live load. Typing, range checks and rejections still work |
| 2026-11-03 | The rejection for a value that breaks OPP has no row in DESIGN.md §10; one was added, marked [P] | LIM-04 requires the rule; the text follows the OVP and OCP rows and fits their cell limits |
| 2026-11-03 | `Space` and `Ctrl-X` in typed entry cancel the entry; the "and turn the output OFF" half arrives with the output milestone | There is no output write yet (PLAN.md §1) |
| 2026-11-03 | M5b: a recall while the output is ON is held back like any setpoint change, until M6 adds the recall prompt (SAF-08) | Same reason as M5a: the confirmations belong to the output milestone |
| 2026-11-03 | M5b: OTP below the measured temperature is an invalid candidate on the Protection screen | The unit trips at once even with the output off, and the host cannot clear a trip without turning the output on (PROTOCOL.md §7.9) |
| 2026-11-03 | M5b: the texts and labels that DESIGN.md left open for the Protection screen and the new notices were added to DESIGN.md §10, marked [P] | Raised, not invented silently (DESIGN.md §11) |

