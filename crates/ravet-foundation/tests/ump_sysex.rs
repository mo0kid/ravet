use ravet_foundation::{
    SysExAssembler, SysExAssembly as A, SysExForm as F, UmpPacket, UmpSysExChunk as C,
    UmpSysExError as E,
};
#[test]
fn known_words_and_all_forms_counts_groups_streams() {
    let chunk = C::new(2, None, F::Complete, &[0x7d, 1, 2, 3, 4, 5]).unwrap();
    assert_eq!(chunk.packet().as_words(), [0x32067d01, 0x02030405]);
    let chunk8 = C::new(2, Some(0xab), F::Start, &[0x80, 0xff, 0x12]).unwrap();
    assert_eq!(chunk8.packet().as_words(), [0x5214ab80, 0xff120000, 0, 0]);
    for group in 0..16 {
        for stream in [None, Some(0), Some(255)] {
            let max = if stream.is_some() { 13 } else { 6 };
            let payload = [0x7f; 13];
            for form in [F::Complete, F::Start, F::Continue, F::End] {
                for count in 0..=max {
                    let chunk = C::new(group, stream, form, &payload[..count]).unwrap();
                    assert_eq!(C::from_packet(chunk.packet()).unwrap(), chunk);
                    assert_eq!(chunk.payload(), &payload[..count]);
                    assert_eq!(chunk.form(), form);
                    assert_eq!(chunk.stream(), stream);
                    assert_eq!(chunk.group(), group);
                }
            }
        }
    }
    let abort = C::abort(2, 255).unwrap();
    assert!(abort.is_abort());
    assert_eq!(abort.packet().as_words(), [0x523fff00, 0, 0, 0]);
    assert_eq!(C::from_packet(abort.packet()).unwrap(), abort);
    assert!(C::new(16, None, F::Complete, &[]).is_err());
    assert!(C::new(0, None, F::Complete, &[128]).is_err());
    assert!(C::new(0, None, F::Complete, &[0; 7]).is_err());
    assert!(C::new(0, Some(0), F::Complete, &[0; 14]).is_err());
    for words in [[0x30070000, 0], [0x30400000, 0], [0x30018000, 0]] {
        assert!(C::from_packet(UmpPacket::from_words(&words).unwrap()).is_err());
    }
    for first in [0x50000000, 0x500f0000, 0x50410000] {
        assert!(C::from_packet(UmpPacket::from_words(&[first, 0, 0, 0]).unwrap()).is_err());
    }
    let padded = UmpPacket::from_words(&[0x30017dff, u32::MAX]).unwrap();
    assert_eq!(
        C::from_packet(padded).unwrap().packet().as_words(),
        [0x30017d00, 0]
    );
}
#[test]
fn assembly_sequence_capacity_abort_and_retry_preserve_accepted_state() {
    let mut storage = [0xee; 8];
    let mut assembler = SysExAssembler::new(&mut storage);
    let chunk = |form, payload: &[u8]| C::new(1, Some(9), form, payload).unwrap();
    assert_eq!(assembler.push(chunk(F::End, &[])), Err(E::Sequence));
    assert_eq!(assembler.push(chunk(F::Start, &[1, 2, 3])), Ok(A::Pending));
    assert!(assembler.is_pending());
    assert_eq!(assembler.push(chunk(F::Start, &[99])), Err(E::Sequence));
    assert_eq!(
        assembler.push(C::new(1, Some(8), F::End, &[99]).unwrap()),
        Err(E::Sequence)
    );
    assert_eq!(
        assembler.push(chunk(F::Continue, &[9; 6])),
        Err(E::Capacity)
    );
    assert_eq!(assembler.push(chunk(F::Continue, &[])), Ok(A::Pending));
    assert_eq!(
        assembler.push(chunk(F::End, &[4, 5])),
        Ok(A::Complete(&[1, 2, 3, 4, 5]))
    );
    assert!(!assembler.is_pending());
    assert_eq!(
        assembler.push(chunk(F::Complete, &[7; 9])),
        Err(E::Capacity)
    );
    assert_eq!(assembler.push(chunk(F::Start, &[1])), Ok(A::Pending));
    assert_eq!(assembler.push(C::abort(1, 8).unwrap()), Err(E::Sequence));
    assert_eq!(assembler.push(C::abort(1, 9).unwrap()), Ok(A::Aborted));
    assert!(!assembler.is_pending());
    for _ in 0..10_000 {
        assert_eq!(assembler.push(chunk(F::Start, &[1, 2])), Ok(A::Pending));
        assert_eq!(
            assembler.push(chunk(F::End, &[3, 4])),
            Ok(A::Complete(&[1, 2, 3, 4]))
        );
    }
    assert_eq!(assembler.push(chunk(F::Start, &[1])), Ok(A::Pending));
    assembler.reset();
    assert_eq!(
        assembler.push(chunk(F::Complete, &[])),
        Ok(A::Complete(&[]))
    );
    assert_eq!(&storage[5..], &[0xee; 3]);
    let mut empty = [];
    let mut assembler = SysExAssembler::new(&mut empty);
    assert_eq!(
        assembler.push(chunk(F::Complete, &[])),
        Ok(A::Complete(&[]))
    );
    assert_eq!(assembler.push(chunk(F::Complete, &[1])), Err(E::Capacity));
}
