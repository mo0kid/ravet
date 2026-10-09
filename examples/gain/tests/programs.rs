use ravet::foundation::{
    AudioBuffer, AudioLayout, ProcessBlock, ProcessConfig, ProcessStatus, Processor, ProgramId,
    Transport,
};
use ravet_gain::{GAIN_PROGRAMS, GainProcessor, GainProcessor64, factory_program_bank, state};
#[test]
fn complete_factory_states_restore_both_precisions_and_persist_without_program_identity() {
    let bank = factory_program_bank();
    assert_eq!(bank.programs().len(), 3);
    for (index, expected) in [1., 0., 1.].into_iter().enumerate() {
        assert_eq!(bank.index(ProgramId(index as u32 + 1)), Some(index));
        assert_eq!(bank.midi_index(0, index as u8), Some(index));
        let selected = bank.state(index).unwrap();
        assert_eq!(selected.values(), &GAIN_PROGRAMS[index].values);
        let bytes = state::encode_parameters(&selected).unwrap();
        assert_eq!(
            state::decode_parameters(&bytes).unwrap().values(),
            selected.values()
        );
        let mut f32_processor = GainProcessor::new(5.).unwrap();
        let mut f64_processor = GainProcessor64::new(5.).unwrap();
        f32_processor.restore_parameters(&selected).unwrap();
        f64_processor.restore_parameters(&selected).unwrap();
        let config = ProcessConfig::new(48000., 4, 4, AudioLayout::MONO).unwrap();
        f32_processor.prepare(config).unwrap();
        f64_processor.prepare(config).unwrap();
        let mut a = [1_f32; 4];
        let mut b = [1_f64; 4];
        let mut ca = [&mut a[..]];
        let mut cb = [&mut b[..]];
        let mut ba = ProcessBlock::new(
            AudioBuffer::in_place(&mut ca, 4).unwrap(),
            &[],
            Transport::default(),
        );
        let mut bb = ProcessBlock::new(
            AudioBuffer::in_place(&mut cb, 4).unwrap(),
            &[],
            Transport::default(),
        );
        assert_eq!(f32_processor.process(&mut ba), ProcessStatus::Continue);
        assert_eq!(f64_processor.process(&mut bb), ProcessStatus::Continue);
        assert_eq!(a, [expected; 4]);
        assert_eq!(b, [f64::from(expected); 4]);
    }
}
