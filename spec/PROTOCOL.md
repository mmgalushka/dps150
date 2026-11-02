# FNIRSI DPS-150 — Serial Protocol

Status: baseline, 2026-10-30; H1 Part B results added 2026-11-02. Derived by interrogating a real unit on 2026-10-28 (firmware `V1.2`, hardware `V1.0`, model string `DPS-150`). This is the reference for the `protocol` and `device` crates (see spec/MAIN.md §3, §5).

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
| `0xB1` | host→dev | SET. **The device sends no acknowledgement.** Confirm by GET of the same type. **Send nothing else for at least 50 ms after a SET**: a frame that follows within a few ms makes the device drop the SET (see §7.7). | [V 2026-11-02] |
| `0xC1` | host→dev | Connect/session open and close. `F1 C1 00 01 01 02` opens a session and **locks the front panel** (lock icon, output button dead); `F1 C1 00 01 00 01` closes it, releases the lock, and the unit goes silent again (§7.8). Details of the open frame: (`F1 C1 00 01 01 cksum`). **Required on 2026-10-29**: without it the device stayed silent (no stream, no GET reply, with or without DTR/RTS); right after it, the stream started. Earlier "not required" observation (2026-10-28) did not reproduce; likely the unit was already in a session. Always send it on open. | [V 2026-10-29] |
| `0xB0` | host→dev | Baud-rate selection per community sources (index 5 = 115200). Not exercised. | [U] |
| `0xC0` | host→dev | **Firmware upgrade command per community sources (the USB port vanishes until the cable is unplugged). Never send it.** Not exercised. | [U] |

## 4. Parameter (type) table

R = readable by GET, W = writable by SET [V unless marked]. "float" = 4 bytes LE.

| Type | Name | Format / unit | R/W | Observed | Notes |
|---|---|---|---|---|---|
| `C0` | Input voltage | float, V | R | 20.25 | Pushed (§5) |
| `C1` | Set voltage | float, V | RW | 3.3 | |
| `C2` | Set current | float, A | RW | 2.0 | |
| `C3` | Output measurement | 3×float: V, I, P | R | 0,0,0 (output off); 5.000 V / 0.480 A / 2.40 W on a 10 Ω load | Pushed. Order V, I, P [V 2026-11-02]. The voltage read about 20 mV above the multimeter and the display (5.000 V against 4.98 V) |
| `C4` | Temperature | float, °C | R | 24.3 | Pushed |
| `C5`..`D0` | Presets M1..M6 | pairs of float: (V, I). `C5`=M1 V, `C6`=M1 I, `C7`=M2 V, … `D0`=M6 I | RW (`C5`,`C6` tested) | all 5.0 V / 1.0 A | Device-side memory slots, writable |
| `D1` | OVP threshold | float, V | RW | 30.0 | |
| `D2` | OCP threshold | float, A | RW | 5.1 | |
| `D3` | OPP threshold | float, W | RW | 150.0 | |
| `D4` | OTP threshold | float, °C | RW | 80.0 | |
| `D5` | LVP threshold (input under-voltage) | float, V | RW | 5.0 | Trips when the input voltage is below it [V 2026-11-02] |
| `D6` | Brightness | u8 | RW | 3 | 0..10 read back as written; only the low 4 bits are kept (16 → 0, 17 → 1, 255 → 15). The display visibly dims and brightens, so the value takes effect; the panel's own range was not observed [V 2026-11-02] |
| `D7` | Volume | u8 | RW | 1 | Same behaviour as `D6`; the sound turns off and on audibly [V 2026-11-02] |
| `D8` | Unknown flag | u8 | R | 1 | Read 1 in every state tried: idle, output ON in CV and CC, OVP trip, LVP trip [V 2026-11-02]. Meaning unknown |
| `D9` | Capacity | float, Ah | R | 0.0 | Accumulates while the output is on, stays after OFF, resets on power cycle [V 2026-11-02] |
| `DA` | Energy | float, Wh | R | 0.0 | Same as `D9` [V 2026-11-02] |
| `DB` | Output state | u8: 0 = off, 1 = on | RW | 0 | `F1 B1 DB 01 01 DD` = ON, `F1 B1 DB 01 00 DC` = OFF [V 2026-11-02]. Needs the quiet gap after the SET (§7.7). On: V reaches the setpoint in about 1 s. Off: decays in about 0.5 s. `DB=01` while a protection is latched clears it **and** turns the output on (§7.9) |
| `DC` | Protection state | u8: 0 none, 1 OVP, 2 OCP, 3 OPP, 4 OTP, 5 LVP | R | 0 | Mapping [V 2026-11-02]; behaviour in §7.9 |
| `DD` | Regulation mode | u8: 1 = CV, 0 = CC | R | 1 (CV) | CC = 0 seen: Iset 0.2 A into 10 Ω gave 2.07 V / 0.200 A [V 2026-11-02] |
| `DE` | Model name | ASCII, no terminator | R | `DPS-150` | |
| `DF` | Hardware version | ASCII | R | `V1.0` | |
| `E0` | Firmware version | ASCII | R | `V1.2` | |
| `E1` | Unknown flag | u8 | R | 1 | Same as `D8`: always 1 [V 2026-11-02] |
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

1. **The device does not validate SET values.** Writing 31.0 V to `C1` (above the 30 V limit and above the live max in `E2`), −1.0 V, and 6.0 A to `C2` (above 5.1 A) all read back verbatim. **The output stage clamps to the live maximum**: with `C1` = 25 V, `C2` = 0.2 A and no load, the output delivered 19.98 V (`E2` was 20.05 V, input 20.24 V) and 0 A, in CV mode, with no trip. The front panel itself will not step the voltage setpoint above `E2` (20.05 V) nor the current above `E3` (5.1 A). [V 2026-11-02] **The host must check against `E2`/`E3`/`E4..E8` before sending** (SAFETY.md §1), and never send negative or non-finite values.
2. Protection thresholds are independent of setpoints; the device does not reject OVP < set V etc. (tested OVP 25 V with set V 12.3 V, which is fine; inverse case not tested). Host should enforce sane ordering.
3. Float precision: values round-trip as binary32 (3.3 → 3.299999952). Round for display to 10 mV / 1 mA.
4. Frame parsing must tolerate the push stream interleaved with replies, and partial frames at port open.
5. Writes persist in device settings (presets, thresholds, brightness, volume were changed and then restored to original values by this investigation). Setpoints (5 V / 0.6 A at the time) survived a power cycle; Ah/Wh counters reset. After a power cycle the unit is silent until the connect frame. [V 2026-11-02]
6. A SET to a read-only type is **ignored**: `DE`, `E0` and `C3` were written and read back unchanged, with no other change. `C0` and `C4` were ignored as well (the live values stayed). [V 2026-11-02]
7. **A SET is lost if another frame follows too soon.** A GET sent immediately after `SET DB` made the SET be ignored 8 of 9 times. With a quiet gap of 5, 20, 50, 100 or 500 ms before the next frame, the SET worked every time (tested for ON; OFF with 500 ms). The first GET after a lost SET also got no reply. Rule: after every SET, send nothing for at least 50 ms (the reference constant is `SET_GAP`, 100 ms), and do not poll during the gap. `C1`/`C2` SETs followed by reads did work in earlier sessions, but the same rule applies to all SETs. **The connect and close frames follow the same rule**: with a GET right behind them, the connect was dropped 4 of 4 times (the unit stayed silent) and the close was dropped 3 of 3 (the unit kept streaming); with a gap of 0.5 s both worked every time. [V 2026-11-02]
8. **Session and front panel.** The connect frame locks the panel; the lock is not released by closing the port or by restarting the bridge, only by the close frame `F1 C1 00 01 00 01` or by unplugging the USB cable. While a session is open the panel display is not reliable: the big V/I/W readings show 0 and Vset/Iset show the stored values (5 V / 0.6 A) although the unit delivers the host's setpoint (3.3 V) as the multimeter confirmed. Host SETs of `C1`/`C2` do not appear on the panel. Use `C3` (and a meter), not the screen, as the truth. [V 2026-11-02]
9. **Protection trips.** To trip, a threshold is set past the operating point (the test used 4.5 V OVP at 5 V, 0.3 A OCP at 0.48 A, 1 W OPP at 2.4 W, 20 °C OTP at 21 °C, 25 V LVP at 20.2 V input). On a trip the output turns off by itself (`DB` → 0), `DC` takes the code (§4), the panel shows the trip icon, and a short beep sounds. OVP/OCP/OPP/OTP are **latched**: restoring the threshold does not clear them, and `DB=00` does not clear them. A host `DB=01` (or one press of the panel output button) clears `DC` **and turns the output on** in one step; there is no clear-without-ON. LVP is **not latched**: it cleared by itself when the threshold was restored. OTP and LVP trip even with the output off. The comparison is strict: OVP equal to the output voltage (5.0 V at 5.0 V) did not trip, and OCP equal to the current in CC mode (0.2 A at Iset 0.2 A) did not trip. OPP, OTP and LVP equality could not be hit exactly, because their measured values drift. [V 2026-11-02]
10. **Pulling the USB cable with the output ON leaves the output ON.** After replug the unit reported `DB=1` at the same setpoint, and Ah/Wh kept growing in between. The panel lock is released as soon as the cable is out and the panel buttons work. [V 2026-11-02]

## 8. Not verified / next steps

H1 Part B (2026-11-02) resolved the output enable, `C3` order, `DD`, `D9`/`DA`, the `DC` codes, trip latching and clearing, brightness and volume masking, read-only SET, power cycle and the cable pull (§4, §7). Still open:

- The meaning of the flags `D8` and `E1`: both read 1 with the output off and the session open; not yet read in other states.
- **Display freeze.** In two sessions on 2026-11-02 the panel showed 0 V / 0 A / 0 W, stale setpoints (5 V / 0.6 A) and a stopped clock during a host session, while the unit delivered the host's setpoint. Unplugging the USB cable (or a power cycle) brought it back. In two later sessions of the same kind, including a 3-minute output-ON hold and the brightness/volume probe, the display worked normally, and host setpoint changes showed on the panel. The cause is unknown; it did not reproduce with a long read pause (90 s) or with extreme brightness/volume values.
- Panel ranges seen at the front panel on 2026-11-02: brightness 13 steps, volume 15 steps (the register keeps 0..15; how the steps map to values was not recorded).
- The multimeter reading at the 19.98 V output (the unit reported `C3` = 19.98 V).
- A SET to `C0` or `C4`; whether any other register accepts writes.
- Whether the quiet gap after a SET applies to the connect and close frames as well (the close frame was always sent after a gap).
- The multimeter once read 2.8 V on the terminals while the unit reported 3.3 V and 0.317 A (a 3.28 V reading just before). A 3-minute hold at 3.3 V showed a steady `C3` (3.300 V, 0.317 A, temperature up 0.7 °C), so the unit did not sag; the cause is probably a contact or the meter.
- `0xB0` and `0xC0` were not exercised on purpose (§3).
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
