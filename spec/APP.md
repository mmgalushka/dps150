# dps150 — App and Device-layer Behaviour

Status: v0.2, 2026-10-30, reviewed. Authority for how the app talks to the device and keeps its model: data binding, polling and update model, write read-back, configuration files, the connection state machine, frame and error handling, threading, and the register of unverified constants. §1–§4 were split from DESIGN.md v0.2; §5–§8 are new. The device it talks to (real or simulated) is described in PROTOCOL.md and [SIMULATOR.md](SIMULATOR.md). Wire-protocol facts are in [PROTOCOL.md](PROTOCOL.md); safety rules in [SAFETY.md](SAFETY.md); what the user sees in [DESIGN.md](DESIGN.md); scope and the decision log in [MAIN.md](MAIN.md). Requirement IDs: [REQUIREMENTS.md](REQUIREMENTS.md).

## 1. Data binding

Sources (see PROTOCOL.md §4 and §6):

| UI element | Wire source |
|---|---|
| Measured V, I, P | `C3` (3 floats V, I, P; order [U]); offsets 12/16/20 of the `FF` dump |
| Set V, set I | `C1`, `C2` |
| Input voltage | `C0` |
| Temperature | `C4` |
| Capacity, energy | `D9`, `DA` |
| Output ON/OFF | `DB` (write [U]) |
| Protection state | `DC` (mapping [U]) |
| Mode CV/CC | `DD` (1 = CV, 0 = CC [U]) |
| Live max V / I | `E2`, `E3` |
| OVP/OCP/OPP/OTP/LVP limits | `D1`..`D5` |
| Limit device ranges | `E4`..`E8` |
| Presets M1..M6 | `C5`..`D0` (V, I pairs) |
| Brightness, volume | `D6`, `D7` |
| Model, hardware, firmware | `DE`, `DF`, `E0` (read once at connect) |

## 2. Update model

- **Link open:** send the connect frame first (PROTOCOL.md §3); the device does not stream or answer GETs without it. It opens a session only and changes no setting. {#PRT-01}
- Poll the full dump (`FF`) at the Settings → Refresh rate (default 10 Hz). If the previous reply has not arrived when the next tick fires, skip the tick. Pushed frames (`C0`, `C3`, `C4`, `E2`, `E3`) update the model whenever they arrive. Match replies by type (PROTOCOL.md §2). A tick skipped because the previous reply is still outstanding **counts as a missed tick**; the link is lost after 3 consecutive missed ticks (DESIGN.md §6.7). {#PRT-06}
- The UI renders from a single model snapshot, redrawn on every model change or key press, and at least once per second. Coalesce redraws to at most ~30 per second.

## 3. Write confirmation

SET frames get no acknowledgement. After writing a value, show the user's value immediately; on the next dump, if the device value differs by more than the display resolution, replace the display with the device value and show the notice `Device kept {value}` (`wn`). {#PRT-03}

## 4. Configuration and files [D]

### 4.1 Config file

`$XDG_CONFIG_HOME/dps150/config.toml`, defaulting to `~/.config/dps150/config.toml` on macOS and Linux. Created on first change; missing keys use defaults. {#CFG-01}

```toml
theme    = "midnight-slate"   # only value in v1
colors   = "auto"             # auto | 24bit | 256 | 16 | none
output_on = "ask"             # ask | immediate
v_jump   = 5.0                # volts; 0 disables the voltage-jump confirmation
i_jump   = 2.0                # amps; 0 disables the current-jump confirmation
on_exit  = "leave"            # leave | off
refresh_hz = 10               # 10 | 5 | 2
auto_detect = true
reconnect   = true
port = ""                     # empty = auto-detect
```

### 4.2 Preset names

`presets.toml` in the same directory: `names = ["3V3 logic", "USB 5V", "9V pedal", "12V fan", "LED test", "24V rail"]`. Voltages and currents are *not* stored here; they live on the device. The rename field (DESIGN.md §8.1) writes this file. {#PRE-03} {#CFG-02}

## 5. Connection state machine [D]

The `device` layer owns the link. States and transitions:

| State | Entered when | Does | Leaves when |
|---|---|---|---|
| `Resolving` | program start | Picks the port (spec/MAIN.md §6) | a port is chosen → `Opening`; none or several → exit 1 |
| `Opening` | port chosen, or a reconnect attempt | Opens the port at 115200 8N1 | opened → `Handshake`; failed → exit 1 at start, `Lost` on a reconnect |
| `Handshake` | port opened | Sends the connect frame, then `GET DE`, `DF`, `E0` and `FF` | the `FF` reply arrives → `Online`; no reply (below) → exit 1 at start, `Lost` on a reconnect |
| `Online` | first full dump received | Polls and decodes (§2, §6) | link lost (SAFETY.md §3.1) or an I/O error → `Lost` |
| `Lost` | see above | Closes the port; the UI shows the link-lost state (DESIGN.md §6.7) | Reconnect = on: after 1 s → `Opening`; off: on `r` → `Opening` |

- **Start-up failures exit; they do not show a screen** (there is no start screen, DESIGN.md §11.2). The message goes to stderr and the exit status is 1: {#APP-01}
  - port cannot be opened: `dps150: cannot open {port}: {reason}` (permission denied adds the hint `Linux: add your user to the dialout group`)
  - no `FF` reply within 3 s of the connect frame (the frame is sent again once at 1.5 s): `dps150: device did not answer on {port}`
- **Reconnecting** repeats `Opening` and `Handshake` with the same limits per attempt; a failed attempt returns to `Lost` and waits for the next retry. Writes stay blocked until the first dump has arrived (SAFETY.md §3.2). {#APP-02}
- A reply to `GET FF` counts as the poll answer for a tick; pushed frames alone do not (§2). `Handshake` and `Online` use the same reader (§6).

## 6. Frame handling and errors [D]

- **Reader.** It resynchronises on `0xF0` (PROTOCOL.md §2): after the start byte it reads `command`, `type`, `len`, `len` data bytes and the checksum. Partial frames are kept until completed. {#APP-03}
- **Bad frames.** A checksum mismatch, or a known type with an unexpected length, drops that frame and resumes scanning at the byte after its start byte. Unknown types are ignored. None of these affects the UI; the device layer counts them for tests.
- **Stale partial frame.** Bytes of an incomplete frame are discarded after 200 ms without any new byte. {#APP-04}
- **I/O errors.** A read or write error on the port takes the link to `Lost` at once; it does not wait for three missed ticks. A failed SET is the same as any write error.
- **Config files.** A missing file means defaults. An unreadable or invalid `config.toml` or `presets.toml` means defaults for the session and the notice `config.toml: line 3: invalid value; using defaults` on the first screen; the invalid file is never overwritten, and a setting changed meanwhile applies to the session only, with the notice `Settings not saved: config.toml is invalid`. Unknown keys are ignored. A write failure shows `Could not save config.toml: {reason}` and the value applies for the session. {#APP-05}
- **Exit status.** 0 normal; 1 runtime failure (start-up failures above, `dps150: could not confirm output OFF`, SAFETY.md §4.9); 2 command-line usage error (unknown flag, bad value). Errors go to stderr; stdout carries only the exit lines of SAFETY.md §4.9. {#APP-06}
- **Panic.** The terminal is restored (DESIGN.md §2.1) and `dps150: internal error, output state unknown` goes to stderr before the process ends. Nothing is sent to the device on a panic. {#APP-07}

## 7. Threads and model ownership [D]

- **Device thread.** One thread owns the port. It reads, decodes and writes; the UI thread never blocks on I/O. {#APP-08}
- **Channels.** UI to device: typed commands (set voltage, set current, output on, output off, set limit, write preset, …). Device to UI: model updates, link-state changes and notices.
- **Model.** The UI/app thread owns one model snapshot; the renderer reads only that snapshot (§2).
- **Output OFF has priority.** An output-OFF command (`Space`, `Ctrl-X`, a prompt cancel) goes to the front of the queue, bypasses the 50 ms SET coalescing (DESIGN.md §7.1, `SAF-07`) and is never dropped or merged away. While the link is down it becomes the queued OFF of SAFETY.md §3.1. {#APP-09}

## 8. Unverified constants [U]

Every value that depends on device behaviour nobody has observed lives in one module, `constants`, one named item each, so a hardware session changes one line and no logic. The simulator uses the same values (SIMULATOR.md §6). {#APP-10}

| Constant | Assumed value | Verify (PROTOCOL.md §8) |
|---|---|---|
| `OUTPUT_ON` / `OUTPUT_OFF` written to `DB` | `01` / `00` | Output enable |
| `MODE_CV` / `MODE_CC` read from `DD` | `1` / `0` | `DD` mapping |
| `C3` field order | V, I, P | `C3` real values |
| Protection codes read from `DC` | `0` none, `1` OVP, `2` OCP, `3` OPP, `4` OTP, `5` LVP | Protection trip |
| Clearing a trip | write `DB = 00`, then re-read `DC` | Protection trip |
| `LIMIT_MARGIN` | one resolution step: 10 mV, 1 mA, 0.1 W; equality allowed at a limit's device maximum | Trip at equality (SAFETY.md §1) |
| Brightness / volume slider range | placeholder 5 steps | Ranges of `D6`, `D7` |

Timing constants (not device-dependent): `MISSED_TICKS` 3, `HANDSHAKE_TIMEOUT` 3 s (retry at 1.5 s), `RECONNECT_INTERVAL` 1 s, `STALE_FRAME` 200 ms, `SET_COALESCE` 50 ms, `QUIT_READBACK` 500 ms.
