# dps150 — Device Simulator

Status: baseline, 2026-10-30. Authority for the simulator: what it is for, how it is built, what it reproduces, what it only assumes, and how tests drive it. It models the device described in [PROTOCOL.md](PROTOCOL.md); app-side behaviour is in [APP.md](APP.md) and [SAFETY.md](SAFETY.md); the test levels are in [DEVOPS.md](../DEVOPS.md) §4. Requirement IDs `SIM-nn`: [REQUIREMENTS.md](REQUIREMENTS.md).

Labels: **[V]** reproduces behaviour verified on the real unit (PROTOCOL.md). **[A]** *assumed*: nobody has observed it on hardware; it is our choice, made so the app can be exercised. Every [A] behaviour is one named constant or one small function, is listed in §6, and any test that depends on it has `assumed` in its name. A hardware session replaces each [A] with a [V] or a fix.

## 1. Purpose

The simulator is a fake DPS-150 that speaks the real protocol. The app cannot tell it from the device. It is **not** needed by a user with a supply connected; it exists for development and tests: {#SIM-01}

1. **Automated tests.** CI has no supply attached. The simulator lets `device`, `app` and the whole program run in CI (DEVOPS.md §4, levels 1 and 2).
2. **Faults on demand.** Link loss, a protection trip, a device that keeps another value, a corrupted or split frame, an input-voltage sag: each is a command (§5) instead of a risky experiment on hardware.
3. **Development without the unit.** From the Linux sandbox, a laptop or a train. A pty needs no USB.
4. **Safety.** Output enable is unverified (PROTOCOL.md §8); no test run should switch a real supply.
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
- After the connect frame it pushes one frame every 200 ms, cycling `C3`, `C0`, `E2`, `E3`, `C4` (the order in `tests/fixtures/capture-dev-to-host.bin`), so each type repeats about every second.
- `GET` of a type in PROTOCOL.md §4 is answered with `F0 A1 <type> <len> <data> <ck>` at the next `advance` (well inside the observed ~100 ms). `GET FF` returns the 139-byte dump of PROTOCOL.md §6. The strings `DE`, `DF`, `E0` are ASCII without terminator. A `GET` of any other type gets no reply. {#SIM-06}

**SET** (`F1 B1 <type> <len> <data> <ck>`): the types marked W in PROTOCOL.md §4 are stored **verbatim and without any validation** (31.0 V, −1.0 V and 6.0 A are all stored and read back); no acknowledgement is sent. Floats are stored as binary32. {#SIM-07}

**Parsing.** The simulator accepts frames split at any byte boundary and several frames in one write. It skips bytes before `0xF1`.

## 4. Device model: assumed behaviour [A]

Everything about the output is assumed, because no one has switched the output on yet (PROTOCOL.md §8).

- **Load.** A resistive load `R` (default 10 Ω) or *open* (no load).
- **Output off:** `C3` = 0 V, 0 A, 0 W [V]. **Output on:** if `Vset ≤ Iset · R` the supply is in CV: `V = Vset`, `I = V / R`, `DD` = 1; otherwise in CC: `I = Iset`, `V = I · R`, `DD` = 0. `P = V · I`. With *open* load, `I = 0` and CV. The output voltage is limited to the live maximum `E2`: the setpoint above `E2` is stored but not delivered. {#SIM-08}
- **Live maximum.** `E2 = Vin − 0.2` (the capture shows 20.034 V for 20.234 V in, equal to binary32 precision), limited to 0..30 V. `E3` = 5.1 A [V].
- **Output enable.** `SET DB = 01` turns the output on, `SET DB = 00` off. While a protection is tripped, `DB = 01` is ignored. {#SIM-09}
- **Protections.** After every `advance`: trip OVP if `V > OVP`; OCP if `I > OCP`; OPP if `P > OPP`; OTP if `temperature > OTP`; LVP if `Vin < LVP`. On a trip the output turns off and `DC` takes the code. The code mapping is `1` OVP, `2` OCP, `3` OPP, `4` OTP, `5` LVP, `0` none (the same constant as in APP.md §8). A trip is cleared when the cause is gone and the host writes `DB = 00`. {#SIM-10}
- **Temperature** is constant (default 23.608 °C) unless set by a command. **Capacity and energy** (`D9`, `DA`) integrate `I·dt` and `P·dt` while the output is on and are not reset when it turns off.
- **Persistence.** Settings written by SET live until the process ends. The `powercycle` command resets everything to the initial state and the device is silent again until the next connect frame.

## 5. Control interface (fault injection)

The same operations exist as library methods on `SimDevice` and as one-line commands on the stdin of `dps150-sim`. The binary prints `ok` or `error: <reason>` for each command and ignores blank lines and lines starting with `#`, so a scenario can be piped from a file. {#SIM-11}

| Command | Effect |
|---|---|
| `load <ohms>` / `load open` | Set the resistive load / no load |
| `vin <volts>` | Set the input voltage (changes `C0` and `E2`); `vin 15` simulates a sag |
| `temp <celsius>` | Set the temperature (for OTP tests) |
| `trip ovp\|ocp\|opp\|otp\|lvp` | Force that protection to trip now |
| `keep <type> <value>` | The device silently keeps `<value>` for that type instead of the SET value (the "Device kept" case, DESIGN.md §10) |
| `unplug` / `plug` | `unplug`: the device goes silent and ignores input, the port stays open (link-loss by timeout). `plug`: it answers again, but only after a new connect frame, like a device that lost its session |
| `close` / `reopen` | `close` closes the pty (the app sees an I/O error, like a pulled cable); `reopen` opens a new pty and prints its path |
| `corrupt <n>` | The next `n` frames sent have a wrong checksum |
| `split <n>` | The next `n` frames are written one byte at a time with 20 ms between bytes |
| `delay <ms>` | Delay every reply by `<ms>` |
| `powercycle` | Reset to the initial state; silent until the next connect frame |
| `state` | Print the device state, one `name value` per line |
| `quit` | Exit |

## 6. Register of assumed behaviours

| Assumption | Section | Verify on hardware (PROTOCOL.md §8) | Tests that depend on it |
|---|---|---|---|
| `DB = 01` turns the output on, `DB = 00` off | §4 | Output enable | output on/off, confirmations, jump guard, quit rules |
| CV when `Vset ≤ Iset·R`, `DD` = 1 CV / 0 CC; `C3` order V, I, P | §4 | `C3` real values, `DD` | mode badges, CC display |
| `DC` codes 1..5 = OVP, OCP, OPP, OTP, LVP; cleared by `DB = 00` | §4 | Protection trip | trip display, tripped `Space` |
| Trip turns the output off and ignores `DB = 01` while tripped | §4 | Protection trip | trip display, SAFETY.md §3 |
| SET to a read-only type is ignored | §3 | SET to read-only (PROTOCOL.md §7.6) | none expected |
| Capacity and energy integrate while on, never reset | §4 | `D9`, `DA` | Status line only |
| Settings persist for the process lifetime | §4 | Power cycle | none |
| `E2 = Vin − 0.2` | §4 | already close to [V] | live-maximum tests |

## 7. Conformance and tests

- **Conformance with the capture.** A `SimDevice` configured with the captured raw values (`C0` = `9B DF A1 41`, `C4` = `A2 DC BC 41`, `E2` = `01 46 A0 41`, `E3` = `33 33 A3 40`, output off, set explicitly so that no arithmetic can differ) and fed the captured host bytes of `tests/fixtures/capture-host-to-dev.bin` (the connect frame and six `GET C0`) produces, for its first five push frames, exactly the first five frames of `tests/fixtures/capture-dev-to-host.bin` (`C3`, `C0`, `E2`, `E3`, `C4`), byte for byte, and one `C0` reply per `GET`. {#SIM-12}
- **Round trip.** Every frame the simulator emits decodes with the `protocol` crate and has a valid checksum; every frame the `protocol` crate encodes is accepted by the simulator.
- **Determinism.** With injected time, the same inputs always give the same outputs; no test sleeps.
- **Level 2.** A test starts `dps150-sim`, reads the path from the first stdout line, starts the app with `--port <path>`, and drives faults through the simulator's stdin.
- **Naming.** A test that relies on an [A] row of §6 has `assumed` in its name, so a hardware session can find and revisit them.
