# FNIRSI DPS-150 — Serial Protocol

Status: baseline, 2026-10-30. Derived by interrogating a real unit on 2026-10-28 (firmware `V1.2`, hardware `V1.0`, model string `DPS-150`). This is the reference for the `protocol` and `device` crates (see spec/MAIN.md §3, §5).

Legend: **[V]** verified on hardware, **[U]** unverified (from published sources or inferred).

## 1. Transport

| Item | Value |
|---|---|
| Interface | USB CDC-ACM serial (micro-USB) [V] |
| macOS node | `/dev/cu.usbmodem<serial>` (here `/dev/cu.usbmodem13C85FCE40251`; use `cu.`, not `tty.`) [V] |
| Baud | 115200 (8N1) [V — works; as CDC-ACM the rate is probably ignored] |
| Byte order | Little-endian; floats are IEEE-754 binary32 [V] |
| USB VID/PID | `0x2E3C` / `0x5740` (decimal 11836 / 22336); vendor "Artery", product "AT32 Virtual Com Port" (AT32 MCU, generic CDC) [V 2026-10-29] |

## 2. Frame format [V]

```
byte 0   start     0xF1 host→device, 0xF0 device→host
byte 1   command   see §3
byte 2   type      register / parameter id, see §4
byte 3   len       number of data bytes (0..255)
byte 4.. data      len bytes
last     checksum  (type + len + sum(data)) mod 256
```

The checksum covers `type` through the last data byte (excludes start and command). It was verified on every received frame (hundreds, 0 mismatches).

Host GET (`len` = 1, data = `00`): `F1 A1 <type> 01 00 <(type+1) mod 256>`.
Host SET (float): `F1 B1 <type> 04 <f32 LE> <cksum>`.

Frames can arrive back-to-back and interleaved with the periodic push stream (§5), so the reader must resync on `0xF0`, take `len`, and match replies by `type`, not by arrival order. {#PRT-02}

## 3. Commands

| Cmd | Dir | Meaning | Status |
|---|---|---|---|
| `0xA1` | both | GET request (host) / data frame (device). Device replies to every GET with `F0 A1 <type> ...` | [V] |
| `0xB1` | host→dev | SET. **The device sends no acknowledgement.** Confirm by GET of the same type. | [V] |
| `0xC1` | host→dev | Connect/session open (`F1 C1 00 01 01 cksum`). **Required on 2026-10-29**: without it the device stayed silent (no stream, no GET reply, with or without DTR/RTS); right after it, the stream started. Earlier "not required" observation (2026-10-28) did not reproduce; likely the unit was already in a session. Always send it on open. | [V 2026-10-29] |
| `0xB0`, `0xC0` | — | Unknown; not exercised. Possibly disconnect / baud-rate commands per community sources. | [U] |

## 4. Parameter (type) table

R = readable by GET, W = writable by SET [V unless marked]. "float" = 4 bytes LE.

| Type | Name | Format / unit | R/W | Observed | Notes |
|---|---|---|---|---|---|
| `C0` | Input voltage | float, V | R | 20.25 | Pushed (§5) |
| `C1` | Set voltage | float, V | RW | 3.3 | |
| `C2` | Set current | float, A | RW | 2.0 | |
| `C3` | Output measurement | 3×float: V, I, P | R | 0,0,0 (output off) | Pushed. Only the all-zero case was seen; V/I/P order from community sources [U] |
| `C4` | Temperature | float, °C | R | 24.3 | Pushed |
| `C5`..`D0` | Presets M1..M6 | pairs of float: (V, I). `C5`=M1 V, `C6`=M1 I, `C7`=M2 V, … `D0`=M6 I | RW (`C5`,`C6` tested) | all 5.0 V / 1.0 A | Device-side memory slots, writable |
| `D1` | OVP threshold | float, V | RW | 30.0 | |
| `D2` | OCP threshold | float, A | RW | 5.1 | |
| `D3` | OPP threshold | float, W | RW | 150.0 | |
| `D4` | OTP threshold | float, °C | RW | 80.0 | |
| `D5` | LVP threshold (input under-voltage) | float, V | RW | 5.0 | Meaning inferred from name in community sources [U] |
| `D6` | Brightness | u8 | RW | 3 | Range not probed (value 2 accepted) |
| `D7` | Volume | u8 | RW | 1 | Range not probed (value 2 accepted) |
| `D8` | Unknown flag | u8 | R | 1 | Possibly "meter open/closed" [U] |
| `D9` | Capacity | float, Ah | R | 0.0 | Accumulated while output is on [U] |
| `DA` | Energy | float, Wh | R | 0.0 | Accumulated while output is on [U] |
| `DB` | Output state | u8: 0 = off, 1 = on | R (W [U]) | 0 | **Write not tested** — see §8 |
| `DC` | Protection state | u8, 0 = none | R | 0 | Value→OVP/OCP/OPP/OTP/LVP mapping not tested [U] |
| `DD` | Regulation mode | u8: 1 = CV, 0 = CC | R | 1 (CV) | CC value from community sources [U] |
| `DE` | Model name | ASCII, no terminator | R | `DPS-150` | |
| `DF` | Hardware version | ASCII | R | `V1.0` | |
| `E0` | Firmware version | ASCII | R | `V1.2` | |
| `E1` | Unknown flag | u8 | R | 1 | |
| `E2` | Max settable voltage (live) | float, V | R | ≈20.05 | Tracks input voltage (20.25 V in → 20.05 V). Pushed. Inferred [U] |
| `E3` | Max settable current | float, A | R | 5.1 | Pushed. Inferred [U] |
| `E4` | Range limit for OVP | float, V | R | 30.0 | Inferred: `E4..E8` pair with `D1..D5` |
| `E5` | Range limit for OCP | float, A | R | 5.1 | |
| `E6` | Range limit for OPP | float, W | R | 150.0 | |
| `E7` | Range limit for OTP | float, °C | R | 99.0 | |
| `E8` | Range limit for LVP | float, V | R | 30.0 | |
| `FF` | Full state dump | 139 bytes, see §6 | R | | One GET returns everything |

All types `00`–`FF` not listed above produced no reply to GET.

## 5. Update model

- The device **pushes unsolicited** `A1` frames about once per second, cycling these types: `C0`, `C3`, `C4`, `E2`, `E3` (about 1.6 Hz each, interleaved, about 8 frames/s in total) [V 2026-11-01]. In `tests/fixtures/capture-dev-to-host.bin` (345 frames, all checksums valid) the order within a cycle is `C3`, `C0`, `E2`, `E3`, `C4` [V]; the first frames carry input 20.234 V, temperature 23.608 °C, live maximum 20.034 V and 5.1 A. On 2026-10-29 this started only after the `C1` connect frame (see §3); on 2026-10-28 it had appeared on plain open, so treat connect as required. H1 Part A (2026-11-01, output off, 10 s without polling, then 60 s of polling) counted 7.7 and 8.5 frames/s without polling on the bridge and directly, 1.5 to 1.7 per second for each of the five types, and 7.8 to 8.0 frames/s while polling `FF` at 10 Hz. The 5 frames/s implied above and the 21 frames/s of the 2026-10-29 bridge session were both wrong.
- Everything else is poll-only (GET) or returned in the `FF` dump.
- Polling `FF` is the simplest way to get a coherent snapshot [V]. H1 Part A, `GET FF` at 10 Hz for 60 s, directly on the Mac (the bridge adds about 1 ms and shows the same pattern): median reply 0.4 ms, but about 4 % of replies take 90 to 125 ms (p95 94 ms, max 125 ms). A slow reply arrives after the next tick, so that tick is skipped (24 of 600); the worst gap between replies was 225 ms and the longest run of missed ticks was 1, so `LNK-01` (3 missed ticks, 300 ms) did not fire. The margin is about 75 ms. [V 2026-11-01]
- SET frames get no reply. The setpoint shows up in the next GET/`FF` [V].

## 6. Full dump (`GET FF`, 139-byte payload) [V]

Offsets within the payload; all floats little-endian.

| Offset | Size | Field (type) |
|---|---|---|
| 0 | 4 | input voltage (`C0`) |
| 4 | 4 | set voltage (`C1`) |
| 8 | 4 | set current (`C2`) |
| 12 | 4 | output V (`C3`[0]) |
| 16 | 4 | output I (`C3`[1]) |
| 20 | 4 | output P (`C3`[2]) |
| 24 | 4 | temperature (`C4`) |
| 28 | 48 | presets M1..M6 as (V, I) pairs (`C5`..`D0`) |
| 76 | 4 | OVP (`D1`) |
| 80 | 4 | OCP (`D2`) |
| 84 | 4 | OPP (`D3`) |
| 88 | 4 | OTP (`D4`) |
| 92 | 4 | LVP (`D5`) |
| 96 | 1 | brightness (`D6`) |
| 97 | 1 | volume (`D7`) |
| 98 | 1 | `D8` flag |
| 99 | 4 | capacity Ah (`D9`) |
| 103 | 4 | energy Wh (`DA`) |
| 107 | 1 | output on (`DB`) |
| 108 | 1 | protection state (`DC`) |
| 109 | 1 | mode (`DD`) |
| 110 | 1 | `E1` flag |
| 111 | 4 | max V live (`E2`) |
| 115 | 4 | max I (`E3`) |
| 119 | 4 | OVP range (`E4`) |
| 123 | 4 | OCP range (`E5`) |
| 127 | 4 | OPP range (`E6`) |
| 131 | 4 | OTP range (`E7`) |
| 135 | 4 | LVP range (`E8`) |

Model/hardware/firmware strings (`DE`..`E0`) are not in the dump; GET them individually.

The offsets were checked against the unit on 2026-11-01 with `tests/fixtures/ff-dump.bin`, a real reply: set voltage 3.3 V and set current 2.0 A match the front panel, and every other field decodes to a plausible value.

## 7. Key findings for implementers

1. **The device does not validate SET values.** Writing 31.0 V to `C1` (above the 30 V limit and above the live max in `E2`), −1.0 V, and 6.0 A to `C2` (above 5.1 A) all read back verbatim. Whether the output stage clamps, and what the front panel shows, was not observed. **The host must check against `E2`/`E3`/`E4..E8` before sending** (SAFETY.md §1), and never send negative or non-finite values.
2. Protection thresholds are independent of setpoints; the device does not reject OVP < set V etc. (tested OVP 25 V with set V 12.3 V, which is fine; inverse case not tested). Host should enforce sane ordering.
3. Float precision: values round-trip as binary32 (3.3 → 3.299999952). Round for display to 10 mV / 1 mA.
4. Frame parsing must tolerate the push stream interleaved with replies, and partial frames at port open.
5. Writes persist in device settings (presets, thresholds, brightness, volume were changed and then restored to original values by this investigation). Whether setpoints persist across power cycle was not tested.
6. A SET to a read-only type (e.g. `C0`, `C4`, `DE`) was not tested.

## 8. Not verified / next steps

- **Output enable**: `SET DB = 01/00` (`F1 B1 DB 01 01 DC`). Not tested because the output load was unknown; the unit was left with output off (`DB = 0`). Needs a safe resistive load. Also confirm: `C3` real V/I/P values, `DD` CC=0 vs CV=1, how `D9`/`DA` accumulate.
- Protection trip: value mapping of `DC` and whether output turns off / how to clear it.
- Ranges for brightness (`D6`) and volume (`D7`).
- Behaviour when setting values out of range on the display itself (clamp vs accept).
- Meaning of `0xB0`/`0xC0`; the unknown flags `D8`, `E1`.
- Whether state set via SET survives power cycle; whether the push stream stops without a connect after idle.
- Behaviour on unplug/replug; whether the device is silent until connect only after a power cycle (suspected: a session from the previous run persists).
- ~~Timing under load, read-only~~ Done in H1 Part A (2026-11-01), see §5: `LNK-01` holds with a margin of about 75 ms over 60 s; the 10-minute run of the stage 1 gate (PLAN.md §1) is the longer test.
- Preset recall (selecting M1..M6 into setpoints) — no command identified; the host can emulate by writing `C1`/`C2` from the preset values.

## 9. Reference probe sequence

```
open port 115200
send  F1 C1 00 01 01 02       -> connect/session open (§3); the device stays silent without it
read frames (device now pushes C0,C3,C4,E2,E3)
send  F1 A1 FF 01 00 00       -> F0 A1 FF 8B <139 bytes> <cksum>
send  F1 A1 DE 01 00 DF       -> "DPS-150"
```

Example live frames: `F0 A1 C0 04 B5 F8 A1 41 <ck>` = input 20.246 V; `F0 A1 C3 0C <12 zero bytes> CF` = output 0 V / 0 A / 0 W.
