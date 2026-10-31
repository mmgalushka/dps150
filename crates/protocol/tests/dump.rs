//! The `FF` dump against the offset table of PROTOCOL.md §6. There is no
//! recorded dump yet, so the payload is built here from that table, not from
//! the decoder's own code.

use protocol::{
    DUMP_LEN, Dir, Dump, Frame, MilliCelsius, Milliamps, Millivolts, Milliwatts, Mode, Reader,
    Reply, command,
};

/// `(offset, value)` for every float in PROTOCOL.md §6.
fn floats() -> Vec<(usize, f32)> {
    let mut floats = vec![
        (0, 20.25),   // input voltage
        (4, 3.3),     // set voltage
        (8, 2.0),     // set current
        (12, 3.29),   // output V
        (16, 1.5),    // output I
        (20, 4.935),  // output P
        (24, 24.3),   // temperature
        (76, 30.0),   // OVP
        (80, 5.1),    // OCP
        (84, 150.0),  // OPP
        (88, 80.0),   // OTP
        (92, 5.0),    // LVP
        (99, 0.125),  // capacity
        (103, 1.5),   // energy
        (111, 20.05), // live max V
        (115, 5.1),   // max I
        (119, 30.0),  // OVP range
        (123, 5.1),   // OCP range
        (127, 150.0), // OPP range
        (131, 99.0),  // OTP range
        (135, 31.0),  // LVP range
    ];
    for slot in 0..6 {
        floats.push((28 + 8 * slot, 1.0 + slot as f32)); // preset V
        floats.push((32 + 8 * slot, 0.5 + slot as f32)); // preset I
    }
    floats
}

fn payload() -> Vec<u8> {
    let mut data = vec![0; DUMP_LEN];
    for (offset, value) in floats() {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    data[96] = 3; // brightness
    data[97] = 2; // volume
    data[98] = 1; // D8
    data[107] = 1; // output on
    data[108] = 0; // protection
    data[109] = 1; // CV
    data[110] = 1; // E1
    data
}

fn dump_of(data: Vec<u8>) -> Result<Dump, protocol::DecodeError> {
    let frame = Frame::new(Dir::DeviceToHost, command::DATA, 0xFF, data).unwrap();
    match Reply::from_frame(&frame)? {
        Reply::Dump(dump) => Ok(*dump),
        other => panic!("{other:?}"),
    }
}

#[test]
fn prt_02_dump_fields_are_read_from_their_offsets() {
    let dump = dump_of(payload()).unwrap();
    assert_eq!(dump.input_voltage, 20.25);
    assert_eq!(dump.set_voltage, Millivolts::new(3300));
    assert_eq!(dump.set_current, Milliamps::new(2000));
    assert_eq!(
        (dump.output.voltage, dump.output.current, dump.output.power),
        (3.29, 1.5, 4.935)
    );
    assert_eq!(dump.temperature, 24.3);
    for (index, preset) in dump.presets.iter().enumerate() {
        assert_eq!(preset.voltage, Millivolts::new(1000 + 1000 * index as u32));
        assert_eq!(preset.current, Milliamps::new(500 + 1000 * index as u32));
    }
    assert_eq!(dump.ovp, Millivolts::new(30_000));
    assert_eq!(dump.ocp, Milliamps::new(5_100));
    assert_eq!(dump.opp, Milliwatts::new(150_000));
    assert_eq!(dump.otp, MilliCelsius::new(80_000));
    assert_eq!(dump.lvp, Millivolts::new(5_000));
    assert_eq!(
        (dump.brightness, dump.volume, dump.flag_d8, dump.flag_e1),
        (3, 2, 1, 1)
    );
    assert_eq!((dump.capacity_ah, dump.energy_wh), (0.125, 1.5));
    assert!(dump.output_on);
    assert_eq!(dump.protection, 0);
    assert_eq!(dump.mode, Mode::ConstantVoltage);
    assert_eq!(dump.max_voltage, Millivolts::new(20_050));
    assert_eq!(dump.max_current, Milliamps::new(5_100));
    assert_eq!(dump.ovp_range, Millivolts::new(30_000));
    assert_eq!(dump.ocp_range, Milliamps::new(5_100));
    assert_eq!(dump.opp_range, Milliwatts::new(150_000));
    assert_eq!(dump.otp_range, MilliCelsius::new(99_000));
    assert_eq!(dump.lvp_range, Millivolts::new(31_000));
}

#[test]
fn prt_02_dump_in_a_stream_has_the_length_of_the_spec() {
    let wire = Frame::new(Dir::DeviceToHost, command::DATA, 0xFF, payload())
        .unwrap()
        .encode();
    assert_eq!(wire[..4], [0xF0, 0xA1, 0xFF, 0x8B]);
    let frames = Reader::new(Dir::DeviceToHost).feed(std::time::Duration::ZERO, &wire);
    assert_eq!(frames.len(), 1);
}

#[test]
fn prt_02_dump_of_the_wrong_length_is_an_error() {
    let err = dump_of(vec![0; DUMP_LEN - 1]).unwrap_err();
    assert!(
        matches!(
            err,
            protocol::DecodeError::WrongLength {
                expected: 139,
                got: 138,
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn lim_01_dump_with_a_negative_or_nan_setpoint_is_refused() {
    for bad in [-1.0_f32, f32::NAN, f32::INFINITY] {
        let mut data = payload();
        data[4..8].copy_from_slice(&bad.to_le_bytes());
        assert!(matches!(
            dump_of(data),
            Err(protocol::DecodeError::BadValue {
                field: "set voltage",
                ..
            })
        ));
    }
}

#[test]
fn prt_02_dump_flag_bytes_outside_their_values_are_refused() {
    let mut data = payload();
    data[107] = 2;
    assert!(matches!(
        dump_of(data),
        Err(protocol::DecodeError::BadFlag {
            field: "output state",
            value: 2
        })
    ));
    let mut data = payload();
    data[109] = 7;
    assert!(matches!(
        dump_of(data),
        Err(protocol::DecodeError::BadFlag {
            field: "mode",
            value: 7
        })
    ));
}

#[test]
fn prt_02_dump_in_constant_current_mode() {
    let mut data = payload();
    data[109] = 0;
    assert_eq!(dump_of(data).unwrap().mode, Mode::ConstantCurrent);
}

/// A real reply recorded from the unit on 2026-11-01 (H1 Part A): output off,
/// 3.3 V and 2.0 A set, which is what the front panel showed.
#[test]
fn prt_02_real_dump_from_the_unit_decodes() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/ff-dump.bin"
    ))
    .unwrap();
    let frames = Reader::new(Dir::DeviceToHost).feed(std::time::Duration::ZERO, &bytes);
    assert_eq!(frames.len(), 1);
    let Reply::Dump(dump) = Reply::from_frame(&frames[0]).unwrap() else {
        panic!()
    };
    assert_eq!(dump.set_voltage, Millivolts::new(3300));
    assert_eq!(dump.set_current, Milliamps::new(2000));
    assert!(!dump.output_on);
    assert_eq!(dump.mode, Mode::ConstantVoltage);
    assert_eq!(dump.protection, 0);
    assert!(
        (20.0..21.0).contains(&dump.input_voltage),
        "{}",
        dump.input_voltage
    );
    assert!(
        (15.0..40.0).contains(&dump.temperature),
        "{}",
        dump.temperature
    );
    assert_eq!(dump.output.power, 0.0);
    assert!(
        dump.presets
            .iter()
            .all(|p| p.voltage == Millivolts::new(5000) && p.current == Milliamps::new(1000))
    );
    assert_eq!(dump.ovp, Millivolts::new(30_000));
    assert_eq!(dump.ocp, Milliamps::new(5_100));
    assert_eq!(dump.opp, Milliwatts::new(150_000));
    assert_eq!(dump.otp, MilliCelsius::new(80_000));
    assert_eq!(dump.lvp, Millivolts::new(5_000));
    assert_eq!(
        (dump.brightness, dump.volume, dump.flag_d8, dump.flag_e1),
        (3, 1, 1, 1)
    );
    assert_eq!(dump.max_current, Milliamps::new(5_100));
    assert!(
        (20_000..20_100).contains(&dump.max_voltage.get()),
        "{:?}",
        dump.max_voltage
    );
    assert_eq!(dump.ovp_range, Millivolts::new(30_000));
    assert_eq!(dump.ocp_range, Milliamps::new(5_100));
    assert_eq!(dump.opp_range, Milliwatts::new(150_000));
    assert_eq!(dump.otp_range, MilliCelsius::new(99_000));
    assert_eq!(dump.lvp_range, Millivolts::new(30_000));
}
