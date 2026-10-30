# dps150 — Implementation Plan

Status: baseline, 2026-10-30. How the specification gets built: order, gates and coverage. It defines no behaviour; rules live in the specification and are cited by requirement ID ([REQUIREMENTS.md](REQUIREMENTS.md)). Decisions behind this plan: [MAIN.md](MAIN.md) §9.

## 1. Approach

Delivery is in safety stages. Each stage is a working program, run on the simulator and on the real unit before the next one starts. Code that can write to the device does not exist until the read path is proven, and code that can switch the output does not exist until writes are proven.

| Stage | Milestones | Result | Gate to leave it |
|---|---|---|---|
| Foundations | M0, M1, H1, M2 | Workspace, CI, protocol, verified device facts, simulator | H1 results recorded in the specification |
| 1. Read-only monitor | M3, M4 | Live readings in the TUI; no SET can be sent | Monitors the unit for 10 minutes, survives a cable pull |
| 2. Setpoints and limits | M5 | Edit V, I, limits and presets; output stays OFF | Every write reads back correctly on the unit |
| 3. Output control | M6 | Output, prompts, trips, link-loss OFF, quit rules | Stage 3 checks on the unit, with a load |
| 4. Completion | M7, M8 | Settings, help, colour modes, release | DESIGN.md §12 passes; v1.0.0 tagged |

There is no public release before v1.0.0.

## 2. Implementation rules

- **Tests first and tests cite IDs**, as in REQUIREMENTS.md §1: `saf_03_output_on_asks_first`. A test that relies on a simulator [A] row has `assumed` in its name.
- **Numbers.** `protocol` defines integer types for thousandths of a unit (PRT-04). `f32` appears only in wire encoding and in measured readings. Encoding takes those integer types, so a negative or NaN setpoint cannot be expressed (LIM-01).
- **Two write checks.** `app` applies the rules of SAFETY.md §1; the device core repeats the range check independently (APP-12).
- **App logic is pure.** `app` is one function: state + event → new state + commands. `ui` is one function: model → cells. Neither touches a port, a clock or the terminal, so prompts, the jump guard and every screen test without I/O.
- **Golden tests** read `tests/golden/` directly with a small comparator in the `ui` test code, and report the first differing cell.
- **Unverified values** go in the `constants` module only (APP-10).
- **Hardware.** Read-only by default. No SET and no output enable without the user's explicit approval in that session. The memory of a previous approval does not count.
- **Git.** One branch and one pull request per milestone, CI green before merge. `.gitignore` is an allowlist: add rules in the commit that adds the files (DEVOPS.md §2.1).
- **Spec changes** follow REQUIREMENTS.md §1: source, register row, decision log, then tests. A gap in the specification is raised, not invented (DESIGN.md §11).

### Implementation choices

Settled here because they are conventional or follow from the specification; recorded so they are not reopened by accident.

| Topic | Choice | Why |
|---|---|---|
| Concurrency | Threads and channels, no async runtime | APP.md §7 needs one I/O thread; nothing else is concurrent |
| Terminal backend | `crossterm` | ratatui's default; covers macOS and Linux |
| Arguments | `clap` | Exits with status 2 on usage errors, as APP-06 requires |
| Config | `serde` and `toml`; the path is built by hand | CFG-01 fixes `~/.config` on macOS too, which directory crates do not |
| Signals | `signal-hook` | SIGTERM and SIGHUP must follow On exit (SAF-11) |
| Drawing | Fill the ratatui `Buffer` directly from computed rectangles | The goldens fix exact cells; a constraint solver would be fought, not used |
| Versions | Latest stable of each crate at M0, locked in `Cargo.lock`; MSRV equals the pinned toolchain | No library consumers |
| Hardware probe | `crates/protocol/examples/hw_probe.rs` | An example never ships in a release |

## 3. Milestones

Requirement lists show where each requirement is **finished**; earlier milestones may build part of it. §5 is the complete map.

### M0. Workspace and CI
- Install Rust in the sandbox with `rustup`. If it is blocked, the firewall needs `static.rust-lang.org`, `index.crates.io` and `static.crates.io` (a user step).
- Workspace layout of DEVOPS.md §2.1, `rust-toolchain.toml`, `deny.toml`, allowlist rules, `helper.sh` with the build, test and lint commands, `ci.yml`.
- **Spike:** open a pty with the `serialport` crate on Linux and macOS. This is the highest technical risk in the level 2 tests (§6).
- *Exit:* CI green on Linux and macOS; spike result written down.

### M1. Protocol
- Frame encode and decode, checksum, resync, partial frames, register table, `FF` dump decode, the integer value types.
- Requirements: PRT-02, APP-03, APP-04.
- *Exit:* all 345 frames of `tests/fixtures/capture-dev-to-host.bin` decode with no errors.

### H1. Hardware session (gate)
A supervised session with the user at the unit, driven by `hw_probe`. Every write step is shown and approved first. The steps below are the proposal and are reviewed together before the session starts.

**Equipment:** a power resistor of about 10 Ω rated 10 W or more on the output, nothing else connected, and the supply's current setpoint low. A multimeter is useful to compare `C3` with reality.

**Part A, read-only, no load needed:**
1. Poll `FF` at 10 Hz for 60 s, through the bridge and directly on the Mac: record reply latency, worst gap and missed replies. This validates or changes `LNK-01`.
2. Measure the real push rate (PROTOCOL.md §8).
3. Record the state of the unit, to restore it at the end.

**Part B, with the load, at low settings (for example 5 V, 0.2 A limit, at most 2 W in the resistor):**
1. Output enable: `DB = 01` and `DB = 00`; does it read back, and does the output really switch?
2. `C3` field order and scale against a multimeter; measurement resolution (DESIGN.md §11.6).
3. `DD` in CV and in CC; `D9` and `DA` accumulation.
4. Protection trips by lowering a threshold, not by raising power: OVP, OCP and OPP just below the operating point; OTP below room temperature; LVP above the input voltage. For each: the `DC` code, whether the output turns off, how it clears.
5. Trip at equality (`LIMIT_MARGIN`).
6. Brightness and volume ranges; SET to a read-only type.
7. Power cycle: which settings survive; is the unit silent until connect?
8. Pull the USB data cable with the output on, to a load that is safe to leave on.
9. Restore the state recorded in A3.

**Output:** PROTOCOL.md §8 items resolved to [V] or recorded as different; APP.md §8 constants and SIMULATOR.md §6 updated. A result that changes behaviour goes through the decision log. Anything that cannot be measured stays [U] behind its constant.

### M2. Simulator
- `SimDevice`, then `dps150-sim` with pty and stdin commands, built on H1's facts.
- Requirements: SIM-01 to SIM-12.
- *Exit:* the conformance test against the capture passes (SIM-12).

### M3. Device core (stage 1)
- The pure link state machine, then the thread and serial transport. In this stage it has no write path except the connect frame and GET.
- Requirements: PRT-01, PRT-06, LNK-01, LNK-06, APP-01, APP-02, APP-08, APP-10, APP-11.
- *Exit:* level 1 tests against `SimDevice` and level 2 against `dps150-sim` pass, including `unplug`, `close`, `corrupt`, `split` and `delay`.

### M4. Read-only monitor (stage 1)
- `dps150` binary, CLI and port resolution, config reading, model, terminal setup and restore, tier selection, fonts, header, strip, readouts and status for the main panel at all tiers.
- Requirements: CLI-01, CLI-02, PRT-05, LNK-02, CFG-01, APP-05, APP-06, APP-07, UI-01, UI-03, UI-06, UI-07, UI-10, UI-11, UI-12, UI-13, UI-14, UI-15, UI-17.
- *Exit:* the stage 1 gate. Main-panel goldens that need no editing state pass: `main-*-cv`, `main-*-cc`, `main-*-lost`.

### M5. Setpoints and limits (stage 2)
- Limit rules, digit cursor, typed entry, write reconciliation, coalescing, Protection and Presets screens. Output writes are still absent from the code.
- Requirements: PRT-03, PRT-04, LIM-01 to LIM-09, LNK-03, EDT-01 to EDT-07, PRE-01 to PRE-04, CFG-02, SAF-12, APP-12.
- *Exit:* the stage 2 gate. Before and after a session on the unit, a dump comparison shows only the intended changes.

### M6. Output control (stage 3)
- Output ON and OFF, prompts, jump guard, recall prompt, too-small rules, OFF priority, tripped state, queued OFF and reconnect, quit, signals.
- Requirements: SAF-01 to SAF-11, LNK-04, LNK-05, LNK-07, PRO-01, PRO-02, APP-09.
- *Exit:* the stage 3 gate, with the user at the unit and the H1 load: confirm, jump guard, one real trip, a cable pull with queued OFF, each quit path.

### M7. Completion (stage 4)
- Settings and Help screens, config writing, colour modes, secondary-screen stretching, the remaining goldens, the size sweep.
- Requirements: UI-02, UI-04, UI-05, UI-08, UI-09, UI-16.
- Before starting: agree the Compact and secondary-screen help text (DESIGN.md §11.7).
- *Exit:* every golden passes; the sweep 44×16 … 250×80 is clean.

### M8. Release (stage 4)
- `release.yml`, cargo-dist, cargo-deb, `dps150-sim` excluded, `.deb` post-install note, `README.md` usage, `CONTRIBUTING.md` command list.
- User steps: create `mmgalushka/homebrew-tap` and add the tap token as a repository secret.
- Dry run on a pre-release tag, install on a clean Mac and a clean Debian or Ubuntu. Run DESIGN.md §12 by hand, including on Linux with the real unit, which is untested so far.
- *Exit:* v1.0.0 tagged.

## 4. Dependencies

```
M0 → M1 → H1 → M2 → M3 → M4 → M5 → M6 → M7 → M8
```

Only H1 depends on the user being present, plus the stage gates and the M8 steps. M2 onward can use the [A] behaviours if H1 is delayed, at the price of revisiting them; M6 must not be merged before H1 Part B.

## 5. Requirement coverage

| Area | Milestone |
|---|---|
| PRT | 01, 06 M3 · 02 M1 · 03, 04 M5 · 05 M4 |
| CLI | 01, 02 M4 |
| LIM | 01–09 M5 |
| SAF | 01–11 M6 · 12 M5 |
| LNK | 01, 06 M3 · 02 M4 · 03 M5 · 04, 05, 07 M6 |
| EDT | 01–07 M5 |
| PRE | 01–04 M5 |
| PRO | 01, 02 M6 |
| CFG | 01 M4 · 02 M5 |
| UI | 01, 03, 06, 07, 10–15, 17 M4 · 02, 04, 05, 08, 09, 16 M7 |
| APP | 01, 02, 08, 10, 11 M3 · 03, 04 M1 · 05, 06, 07 M4 · 09 M6 · 12 M5 |
| SIM | 01–12 M2 |

## 6. Risks

| Risk | Effect | Response |
|---|---|---|
| `serialport` may not open a pty on macOS | Level 2 tests fail on macOS | Found in the M0 spike. Fall back to running level 2 on Linux only in CI; the Mac still tests the real port by hand |
| 300 ms link-loss window is too tight for the real device or the bridge | False link loss, which closes the port and cancels prompts | Measured in H1 Part A. If needed, change `MISSED_TICKS` or the tick accounting through the decision log |
| H1 finds output or trip behaviour unlike the assumptions | Safety rules may need new behaviour | That is why H1 comes first; changes go through the specification before M6 |
| No resistive load available when H1 is due | M6 blocked | M2 to M5 can continue on [A] behaviours (§4) |
| Goldens and tables disagree | UI work stalls | The table wins (DESIGN.md §0); fix the file and report it |
| Linux has not been tried with the unit | Port names or permissions wrong in DEVOPS.md §8 | Checked in M8 |
