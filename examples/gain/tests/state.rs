use ravet_gain::state::{GainState, STATE_BYTES, StateError};
use std::io::{self, Read, Write};

// A permanent version-one compatibility fixture, independent of the encoder.
const HALF: [u8; 20] = [
    82, 65, 86, 69, 84, 71, 78, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 63,
];

#[test]
fn version_one_fixture_and_canonical_round_trip() {
    assert_eq!(GainState::decode(&HALF).unwrap().gain(), 0.5);
    assert_eq!(GainState::new(0.5).unwrap().encode(), HALF);
    for gain in [0.0, 0.25, 1.0, 4.0] {
        let state = GainState::new(gain).unwrap();
        assert_eq!(GainState::decode(&state.encode()).unwrap(), state);
    }
    assert_eq!(GainState::new(-0.0).unwrap().gain().to_bits(), 0);
}

#[test]
fn invalid_identity_version_length_and_values_are_rejected() {
    for length in 0..STATE_BYTES {
        assert_eq!(
            GainState::decode(&HALF[..length]),
            Err(StateError::InvalidLength)
        );
    }
    let mut bytes = HALF;
    bytes[0] = 0;
    assert_eq!(GainState::decode(&bytes), Err(StateError::WrongProduct));
    bytes = HALF;
    bytes[8..12].copy_from_slice(&2_u32.to_le_bytes());
    assert_eq!(
        GainState::decode(&bytes),
        Err(StateError::UnsupportedVersion(2))
    );
    bytes = HALF;
    bytes[12] = 99;
    assert_eq!(GainState::decode(&bytes), Err(StateError::WrongParameter));
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 4.001] {
        bytes = HALF;
        bytes[16..20].copy_from_slice(&value.to_le_bytes());
        assert_eq!(GainState::decode(&bytes), Err(StateError::InvalidGain));
    }
}

struct Chunked<T>(T);
impl<T: Read> Read for Chunked<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let limit = bytes.len().min(2);
        self.0.read(&mut bytes[..limit])
    }
}
impl<T: Write> Write for Chunked<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(&bytes[..bytes.len().min(3)])
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
#[test]
fn stream_handles_partial_io_and_bounds_reading() {
    let mut reader = Chunked(HALF.as_slice());
    let state = GainState::read(&mut reader).unwrap();
    let mut writer = Chunked(Vec::new());
    state.write(&mut writer).unwrap();
    assert_eq!(writer.0, HALF);
    let oversized = [0_u8; 1000];
    let mut reader = oversized.as_slice();
    assert!(GainState::read(&mut reader).is_err());
    assert_eq!(reader.len(), 979); // never consumes more than 21 bytes
    let mut reader = &HALF[..19];
    assert_eq!(
        GainState::read(&mut reader).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

struct Broken;
impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("read failed"))
    }
}
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("write failed"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn stream_errors_are_returned() {
    assert!(GainState::read(&mut Broken).is_err());
    assert!(GainState::new(0.5).unwrap().write(&mut Broken).is_err());
    assert_eq!(
        GainState::new(0.5)
            .unwrap()
            .write(&mut &mut [][..])
            .unwrap_err()
            .kind(),
        io::ErrorKind::WriteZero
    );
}

#[test]
fn complete_state_streams_support_partial_io_migration_and_bound_reads() {
    use ravet_gain::{
        parameter_schema,
        state::{encode_parameters, read_parameters, write_parameters},
    };
    let state = parameter_schema().state([0.5, 1.0]).unwrap();
    let expected = encode_parameters(&state).unwrap();
    let mut reader = Chunked(expected.as_slice());
    assert_eq!(read_parameters(&mut reader).unwrap().values(), &[0.5, 1.0]);
    let mut writer = Chunked(Vec::new());
    write_parameters(&state, &mut writer).unwrap();
    assert_eq!(writer.0, expected);
    assert_eq!(
        read_parameters(&mut Chunked(HALF.as_slice()))
            .unwrap()
            .values(),
        &[0.5, 0.0]
    );
    let oversized = [0_u8; 1000];
    let mut reader = oversized.as_slice();
    assert!(read_parameters(&mut reader).is_err());
    assert_eq!(reader.len(), 967);
    for length in [0, 19, 21, 31] {
        assert!(read_parameters(&mut &expected[..length]).is_err());
    }
    assert!(read_parameters(&mut Broken).is_err());
    assert!(write_parameters(&state, &mut Broken).is_err());
}
