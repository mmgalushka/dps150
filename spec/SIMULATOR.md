# dps150 — Device Simulator

Status: baseline, 2026-10-30; H1 Part B results applied 2026-11-02. Authority for the simulator: what it is for, how it is built, what it reproduces, what it only assumes, and how tests drive it. It models the device described in [PROTOCOL.md](PROTOCOL.md); app-side behaviour is in [APP.md](APP.md) and [SAFETY.md](SAFETY.md); the test levels are in [DEVOPS.md](../DEVOPS.md) §4. Requirement IDs `SIM-nn`: [REQUIREMENTS.md](REQUIREMENTS.md).

Labels: **[V]** reproduces behaviour verified on the real unit (PROTOCOL.md). **[A]** *assumed*: nobody has observed it on hardware; it is our choice, made so the app can be exercised. Every [A] behaviour is one named constant or one small function, is listed in §6, and any test that depends on it has `assumed` in its name. A hardware session replaces each [A] with a [V] or a fix.

## 1. Purpose

The simulator is a fake DPS-150 that speaks the real protocol. The app cannot tell it from the device. It is **not** needed by a user with a supply connected; it exists for development and tests: {#SIM-01}

1. **Automated tests.** CI has no supply attached. The simulator lets `device`, `app` and the whole program run in CI (DEVOPS.md §4, levels 1 and 2).
2. **Faults on demand.** Link loss, a protection trip, a device that keeps another value, a corrupted or split frame, an input-voltage sag: each is a command (§5) instead of a risky experiment on hardware.
3. **Development without the unit.** From the Linux sandbox, a laptop or a train. A pty needs no USB.
4. **Safety.** No test run should switch a real supply, and the tests of unsafe conditions (trips, cable pull) run against the simulator, not on the unit.
5. **Demo.** `dps150 --port <pty>` against the simulator gives a working UI for screenshots and trying the keys.

It does not replace the manual hardware acceptance (DESIGN.md §12). It does not model USB enumeration (so auto-detection by VID/PID is not covered), baud rate, DTR or RTS.

## 2. Structure

- **Library crate `simulator`.** A pure state machine, `SimDevice`, with no I/O and no clock of its own: `feed(bytes)` takes what the host wrote, `advance(dt)` moves simulated time forward, `take_output()` returns the bytes the device would send. Time is injected, so tests are deterministic and need no sleeping. {#SIM-02}
- **In-process use.** Tests pipe bytes directly between `SimDevice` and the pure device core (APP.md §7) under one injected clock (test level 1). No pty, thread or trait is involved.
- **Binary `dps150-sim`.** A thin real-time wrapper: it opens a pseudo-terminal, prints its path on the first stdout line (`dps150-sim: listening on /dev/pts/5`), calls `advance` from the system clock, copies bytes between the pty and `SimDevice`, and reads control commands from stdin (§5). The app runs against it with `dps150 --port <path>` (test level 2; the real `serialport` code path). {#SIM-03}
- Framing uses the `protocol` crate. A codec bug would therefore appear on both sides; the conformance test in §7 compares against raw captured bytes to catch exactly that.

## 3. Device model: reproduced behaviour [V]

**Initial state** is the state observed on the test unit (PROTOCOL.md §4): set 3.3 V / 2.0 A; presets M1..M6 all 5.0 V / 1.0 A; OVP 30.0 V, OCP 5.1 A, OPP 150.0 W, OTP 80.0 °C, LVP 5.0 V; brightness 3, volume 1; output off; model `DPS-150`, hardware `V1.0`, firmware `V1.2`; input 20.234 V and temperature 23.608 °C (both from the capture); range limits `E4..E8` = 30 V, 5.1 A, 150 W, 99 °C, 30 V. {#SIM-04}

**Session.**
- The device is **silent until it receives the connect frame** `F1 C1 00 01 01 02` (no stream, no reply to GET). {#SIM-05}
- After the connect frame it pushes one frame every 125 ms, cycling `C3`, `C0`, `E2`, `E3`, `C4` (the order in `tests/fixtures/capture-dev-to-host.bin`), so each type repeats about every 0.6 s (the real unit sends about 8 frames/s, PROTOCOL.md §5).
- `GET` of a type in PROTOCOL.md §4 is answered with `F0 A1 <type> <len> <data> <ck>` at the next `advance` (the real unit answers in under 1 ms, but about 4 % of replies take 90 to 125 ms, PROTOCOL.md §5; the `delay` command reproduces that). `GET FF` returns the 139-byte dump of PROTOCOL.md §6. The strings `DE`, `DF`, `E0` are ASCII without terminator. A `GET` of any other type gets no reply. {#SIM-06}

**SET** (`F1 B1 <type> <len> <data> <ck>`): the types marked W in PROTOCOL.md §4 are stored **verbatim and without any validation** (31.0 V, −1.0 V and 6.0 A are all stored and read back); no acknowledgement is sent. Floats are stored as binary32. {#SIM-07}

**SET to a read-only type** (`C3`, `DE`, `E0`, and all types not marked W in PROTOCOL.md §4) is ignored without any change. [V 2026-11-02] {#SIM-13}

**Dropped SET.** A SET is discarded when another frame arrives within `SET_DROP_WINDOW` after its last byte (PROTOCOL.md §7.7). The real window is between 0 and 5 ms; the simulator uses 2 ms [A: the length only]. A device layer that polls right after a SET therefore fails its tests, as it fails on the unit. {#SIM-14}

**Session lock.** The connect frame locks the panel; the close frame `F1 C1 00 01 00 01` unlocks it and makes the device silent again; `unplug` also unlocks. `state` prints `locked`. [V] {#SIM-15}

**Parsing.** The simulator accepts frames split at any byte boundary and several frames in one write. It skips bytes before `0xF1`.

## 4. Device model: assumed behaviour [A]

The output switch and protections were observed on 2026-11-02 (PROTOCOL.md §4, §7); what remains assumed is marked [A].

- **Load.** A resistive load `R` (default 10 Ω) or *open* (no load).
- **Output off:** `C3` = 0 V, 0 A, 0 W [V]. **Output on:** if `Vset ≤ Iset · R` the supply is in CV: `V = Vset`, `I = V / R`, `DD` = 1; otherwise in CC: `I = Iset`, `V = I · R`, `DD` = 0 [V]. `P = V · I`. With *open* load, `I = 0` and CV [A]. The output voltage is limited to the live maximum `E2`: the setpoint above `E2` is stored but not delivered [A]. The voltage ramps up in about 1 s and decays in about 0.5 s [V]. {#SIM-08}
- **Live maximum.** `E2 = Vin − 0.2` (the capture shows 20.034 V for 20.234 V in, equal to binary32 precision), limited to 0..30 V. `E3` = 5.1 A [V].
- **Output enable.** `SET DB = 01` turns the output on, `SET DB = 00` off [V]. `DB = 01` while a protection is latched **clears it and turns the output on** [V]; `DB = 00` does not clear it. {#SIM-09}
- **Protections.** After every `advance`: trip OVP if `V > OVP`; OCP if `I > OCP`; OPP if `P > OPP`; OTP if `temperature > OTP`; LVP if `Vin < LVP` [V]. The comparison is strict: equality does not trip [V]. On a trip the output turns off and `DC` takes the code `1` OVP, `2` OCP, `3` OPP, `4` OTP, `5` LVP, `0` none (the same constant as in APP.md §8) [V]. OVP, OCP, OPP and OTP are **latched**: they stay after the cause is gone and after the threshold is restored, until `DB = 01`. LVP is **not** latched and clears when `Vin ≥ LVP` [V]. OTP and LVP trip even with the output off [V]. The short beep and the panel icon are not modelled. {#SIM-10}
- **Temperature** is constant (default 23.608 °C) unless set by a command. **Capacity and energy** (`D9`, `DA`) integrate `I·dt` and `P·dt` while the output is on, are not reset when it turns off, and reset on `powercycle` [V].
- **Persistence.** Settings written by SET live until the process ends. The `powercycle` command resets the counters and the session and keeps the settings (setpoints survived a power cycle on the unit [V]); the device is silent again until the next connect frame. Brightness and volume keep only the low 4 bits of the written value [V].

## 5. Control interface (fault injection)

The same operations exist as library methods on `SimDevice` and as one-line commands on the stdin of `dps150-sim`. The binary prints `ok` or `error: <reason>` for each command and ignores blank lines and lines starting with `#`, so a scenario can be piped from a file. {#SIM-11}

| Command | Effect |
|---|---|
| `load <ohms>` / `load open` | Set the resistive load / no load |
| `vin <volts>` | Set the input voltage (changes `C0` and `E2`); `vin 15` simulates a sag |
| `temp <celsius>` | Set the temperature (for OTP tests) |
| `trip ovp\|ocp\|opp\|otp\|lvp` | Force that protection to trip now |
| `keep <type> <value>` | The device silently keeps `<value>` for that type instead of the SET value (the "Device kept" case, DESIGN.md §10) |
| `unplug` / `plug` | `unplug`: the device goes silent and ignores input, the port stays open (link-loss by timeout); **the output state is kept** (a pulled cable leaves the output ON, PROTOCOL.md §7.10). `plug`: it answers again, but only after a new connect frame, like a device that lost its session |
| `close` / `reopen` | `close` closes the pty (the app sees an I/O error, like a pulled cable); `reopen` opens a new pty and prints its path |
| `corrupt <n>` | The next `n` frames sent have a wrong checksum |
| `split <n>` | The next `n` frames are written one byte at a time with 20 ms between bytes |
| `delay <ms>` | Delay every reply by `<ms>` |
| `powercycle` | Reset counters and session, keep settings, output off; silent until the next connect frame |
| `state` | Print the device state, one `name value` per line |
| `quit` | Exit |

## 6. Register of assumed behaviours

| Assumption | Section | Status | Tests that depend on it |
|---|---|---|---|
| `DB` switching, `DD` CV/CC, `C3` order, `DC` codes, latching, clear by `DB = 01`, strict comparison | §4 | Verified 2026-11-02 | none need `assumed` any more |
| Open load gives `I = 0`, CV | §4 | Assumed | open-load tests |
| Output voltage limited to `E2` | §4 | Assumed | live-maximum tests |
| `SET_DROP_WINDOW` is 2 ms | §3 | Length assumed (0 to 5 ms observed) | write-gap tests |
| `E2 = Vin − 0.2` | §4 | Already close to [V] | live-maximum tests |
| Trip beep and panel icon, panel display during a session | §4 | Not modelled | none |

## 7. Conformance and tests

- **Conformance with the capture.** A `SimDevice` configured with the captured raw values (`C0` = `9B DF A1 41`, `C4` = `A2 DC BC 41`, `E2` = `01 46 A0 41`, `E3` = `33 33 A3 40`, output off, set explicitly so that no arithmetic can differ) and fed the captured host bytes of `tests/fixtures/capture-host-to-dev.bin` (the connect frame and six `GET C0`) produces, for its first five push frames, exactly the first five frames of `tests/fixtures/capture-dev-to-host.bin` (`C3`, `C0`, `E2`, `E3`, `C4`), byte for byte, and one `C0` reply per `GET`. {#SIM-12}
- **Round trip.** Every frame the simulator emits decodes with the `protocol` crate and has a valid checksum; every frame the `protocol` crate encodes is accepted by the simulator.
- **Determinism.** With injected time, the same inputs always give the same outputs; no test sleeps.
- **Level 2.** A test starts `dps150-sim`, reads the path from the first stdout line, starts the app with `--port <path>`, and drives faults through the simulator's stdin.
- **Naming.** A test that relies on an [A] row of §6 has `assumed` in its name, so a hardware session can find and revisit them.
