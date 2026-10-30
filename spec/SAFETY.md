# dps150 — Safety and Behaviour Rules

Status: v0.1, 2026-10-30. Split from DESIGN.md v0.2. Authority for the host-side rules that keep the user and the load safe: ranges and limits, confirmations, link loss and reconnect, and the summary of safety rules. How these look on screen (layout, copy, styles) is in [DESIGN.md](DESIGN.md); polling, data binding and files are in [APP.md](APP.md); wire-protocol facts in [PROTOCOL.md](PROTOCOL.md); scope and the decision log in [MAIN.md](MAIN.md). Status tags **[D]**, **[P]**, **[U]** are as defined in DESIGN.md §0. Requirement IDs are listed in [REQUIREMENTS.md](REQUIREMENTS.md).

## 1. Ranges and limits [D]

Every value is checked by the host before sending (the device does not validate; PROTOCOL.md §7): {#LIM-01} {#LIM-02} {#LIM-03}

| Value | Allowed range |
|---|---|
| Set voltage | 0 ≤ V ≤ min(live max V (`E2`), 30.00), and V below OVP (ordering rule) |
| Set current | 0 ≤ I ≤ live max I (`E3`), and I below OCP (ordering rule) |
| OVP / OCP / OPP / OTP / LVP | 0 ≤ x ≤ the device range for that limit (`E4`..`E8`), and the ordering rules |
| Any value | Never negative, NaN or infinite; never sent if parsing failed |

**Ordering rules (setpoints vs protection limits)** hold on **every** screen, in both directions: {#LIM-05}

- set voltage < OVP; set current < OCP; set voltage × set current < OPP; LVP < live input voltage. {#LIM-04}
- "Below" means at least one resolution step (10 mV, 1 mA, 0.1 W), except when the limit sits at its device maximum (OVP 30.00 V, OCP 5.10 A, OPP 150.0 W): a limit at its maximum is "no protection" and equality is allowed. Whether the device trips at equality is unverified [U]; keep the margin in one named constant.
- On the main panel and Presets, a typed value or recall that would break the rule is **rejected** with the reason (`… not below OVP 13.00 V`); a cursor step stops just below the limit with the notice `Limited by OVP 13.00 V`.
- On the Protection screen, a candidate that would break the rule shows `✗` and disables `⏎` (DESIGN.md §8.3).
- If the device already violates a rule when the app starts or reconnects (for example the limit was changed on the front panel), the app changes nothing. It shows the violation on the Protection screen (DESIGN.md §8.3) and **refuses to turn the output ON** with the notice `Cannot turn ON: set voltage is not below OVP (o)`. {#LIM-06}

The live maximum voltage follows the input voltage. When it drops below the current voltage setpoint, the app **does not change the setpoint** (§4.10). It shows the persistent warning `Set 20.00 V is above the live maximum 18.40 V. Lower it to turn output ON` (`wn`) in the message line whenever no other message is active, and refuses to turn the output ON with `Cannot turn ON: set voltage is above the live maximum`. Turning the output OFF and editing the setpoint stay possible. The same rule applies to the current setpoint and `E3`. [D] {#LIM-07}

## 2. Confirmations [D]

Controlled by Settings → Safety (DESIGN.md §8.4). Jump and recall prompts apply only while the output is ON. Every prompt is answered with `y` / `n` / `Esc` (DESIGN.md §5.12) and shows the voltage **and** current it will apply. {#SAF-03}

| Trigger | Prompt text (style `wn`, then key caps ` y  yes   Esc  no`) | Reference |
|---|---|---|
| Turning output ON, "Output on = ask first" (default) | `Turn output ON at 5.00 V / 1.500 A?` | `main-standard-confirm-on`, `main-compact-confirm-on` |
| The voltage setpoint would move more than the "V jump over" threshold (default 5.0 V) away from its **reference** (below), in either direction, while output is ON | `Set 15.00 V / 1.500 A (+10.00 V), output ON?` | `main-standard-confirm-jump`, `main-compact-confirm-jump` |
| The current setpoint would move more than "I jump over" (default 2.0 A) away from its reference | `Set 5.00 V / 3.500 A (+2.000 A), output ON?` | |
| Preset recall (`m` + digit, or `⏎` on the Presets screen) while output is ON, **whatever the size of the change** | `Recall M4 12.00 V / 2.000 A, output ON?` | |
| Overwriting or clearing a preset slot | `Overwrite M4?  12.00 V/2.000 A → 12.00 V/1.000 A` (DESIGN.md §8.1) | |
| Quitting while that would leave the output ON | `Quit with output ON (12.00 V)?` (§4.9) | |

**Reference point and cumulative window.** Each quantity (voltage, current) has a reference value: the setpoint at the moment the output was last turned ON, or at the last jump / recall prompt answered `y`, whichever is later. While the output is OFF the reference follows the setpoint. A change that would put the setpoint more than the threshold away from its reference is held (not sent) and the prompt is shown; a sequence of small steps therefore triggers the prompt when their *sum* crosses the threshold. After `y` the reference becomes the new setpoint; after `n`/`Esc` the setpoint stays where it was. Deltas are signed: `(-7.00 V)` for a drop. Only the components that crossed a threshold appear in the parentheses, joined with a comma. {#SAF-06}

Turning output **off** never asks. With "Output on = immediately" the ON prompt is skipped (the ordering rules of §1 still apply). Setting a threshold to `off` disables that jump prompt only. {#SAF-04}

## 3. Link loss and reconnect [D]

What the UI shows while the link is down is in DESIGN.md §6.7; this section owns the behaviour.

### 3.1 When the link is lost

**Link lost.** The link is considered lost after 3 consecutive poll ticks without a complete reply (APP.md §2). The device is powered from its own input, so a pulled data cable most likely leaves the output ON (unverified, PROTOCOL.md §8): the UI therefore never claims it is off. {#LNK-01}

- **Nothing that writes to the device is sent.** Edits, recalls, protection and settings writes are refused with the notice `Link down. Changes are not sent.` {#LNK-03}
- `Space` and `Ctrl-X` **queue** an output-OFF, sent as the first frame when the port answers again, with the notice `Off queued · use the front-panel button`. The pending queue is cleared if the user presses `Space` again (toggle back). {#LNK-04}
- Typed entry and prompts are cancelled when the link is lost. {#LNK-05}
- The same unknown-state rules apply to the live strip (DESIGN.md §5.10) and the too-small screen (DESIGN.md §2.4).

### 3.2 Reconnect

- With Settings → Reconnect = on, the app tries to reopen the port every 1 s while the link is down; with Reconnect = off it tries once per `r` press (DESIGN.md §9.2). {#LNK-06}
- On success: send the connect frame (APP.md §2), read the full state, then send the queued output-OFF (if the user pressed `Space` or `Ctrl-X` while the link was down) as the **first** frame, discarding nothing the device did not change, then show a 3-second notice `Link restored · output ON at 12.00 V / 1.000 A`, or `Link restored · queued OFF sent, output OFF` when the queue was used (copy: DESIGN.md §10). {#LNK-07}
- Until the first full dump after reconnecting has arrived, all writes stay blocked and the SET values on screen are the device's, not the ones from before the loss.

## 4. Safety rules (summary) [D unless noted]

1. Every setpoint and threshold is checked on the host before sending (§1). Typed values out of range are rejected, never silently changed; cursor steps stop at the limit.
2. Output ON asks first by default; output OFF never asks (§2). `Space` and `Ctrl-X` can only turn the output OFF wherever they are not the plain toggle (DESIGN.md §1.5).
3. A large voltage or current jump with output ON asks first, measured cumulatively from the last confirmed value (§2).
4. Preset recall with output ON always asks; a preset above the live maximum or breaking an ordering rule is blocked, not clamped (DESIGN.md §8.1).
5. Setpoints and protection limits keep their ordering in **both** directions, on every screen (§1, DESIGN.md §8.3). The output is refused ON while the ordering is violated or a setpoint is above the live maximum.
6. *(Not used; numbering kept.)*
7. Link lost: no stale numbers, and the output state is shown as unknown, never OFF; nothing is written; `Space` queues an OFF (DESIGN.md §6.7).
8. `Space` toggles output from everywhere; in typed entry and prompts it only cancels and turns OFF (DESIGN.md §1.5, §5.12).
9. **Quit** [D]. Behaviour depends on Settings → On exit (default *leave output*): {#SAF-11}
   - `q` on the main panel with the output OFF quits at once. On other screens `q` acts like `Esc`.
   - With the output ON and **On exit = leave output** (default): `q` asks `Quit with output ON (12.00 V)?  y quit  o off+quit  Esc stay`. `y` quits leaving the output ON, `o` sends output-off first, `Esc` stays. After leaving the alternate screen, print one line to stdout: `dps150: output left ON (12.00 V / 1.000 A)`.
   - With **On exit = turn off**: `q` sends output-off first, waits up to 500 ms for the `DB` read-back, then prints `dps150: output turned OFF`, or `dps150: could not confirm output OFF, check the unit` with exit status 1 when it does not read back as OFF.
   - Link lost: `q` asks `Link down: cannot turn output off. Quit anyway?  y quit  Esc stay`, then prints `dps150: link lost, output state unknown`.
   - `Ctrl-C`, `SIGTERM` and `SIGHUP` quit immediately **without a prompt** and follow the On exit setting; the printed line is always the last thing written.
   - On the too-small screen only `Ctrl-C` quits (DESIGN.md §2.4).
10. On start, the app changes no setting on the device. It sends the connect frame, which only opens a session (PROTOCOL.md §3), then reads full state and shows it. Setpoint and protection writes persist on the device (spec/MAIN.md §3), so the app writes only on explicit user action. [D] {#PRT-05}
