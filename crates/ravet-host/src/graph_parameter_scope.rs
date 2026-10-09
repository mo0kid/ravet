//! Explicit scoped parameter granularity policy, independent of global flags.
use clack_extensions::params::ParamInfoFlags;
use ravet_foundation::NoteTarget;
pub(crate) fn supports(flags: ParamInfoFlags, target: NoteTarget, modulation: bool) -> bool {
    let flag = match (
        target.id.is_some(),
        target.key.is_some(),
        target.channel.is_some(),
        modulation,
    ) {
        (true, _, _, false) => ParamInfoFlags::IS_AUTOMATABLE_PER_NOTE_ID,
        (true, _, _, true) => ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID,
        (_, true, _, false) => ParamInfoFlags::IS_AUTOMATABLE_PER_KEY,
        (_, true, _, true) => ParamInfoFlags::IS_MODULATABLE_PER_KEY,
        (_, _, true, false) => ParamInfoFlags::IS_AUTOMATABLE_PER_CHANNEL,
        (_, _, true, true) => ParamInfoFlags::IS_MODULATABLE_PER_CHANNEL,
        (_, _, _, false) => ParamInfoFlags::IS_AUTOMATABLE_PER_PORT,
        (_, _, _, true) => ParamInfoFlags::IS_MODULATABLE_PER_PORT,
    };
    target.is_valid() && flags.contains(flag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_foundation::NoteId;
    #[test]
    fn specific_granularity_flags_are_independent_of_global_and_less_specific_support() {
        let targets = [
            NoteTarget {
                id: Some(NoteId(123)),
                key: Some(60),
                channel: Some(2),
            },
            NoteTarget {
                key: Some(60),
                channel: Some(2),
                id: None,
            },
            NoteTarget {
                channel: Some(2),
                key: None,
                id: None,
            },
            NoteTarget::default(),
        ];
        let flags = [
            ParamInfoFlags::IS_AUTOMATABLE_PER_NOTE_ID,
            ParamInfoFlags::IS_AUTOMATABLE_PER_KEY,
            ParamInfoFlags::IS_AUTOMATABLE_PER_CHANNEL,
            ParamInfoFlags::IS_AUTOMATABLE_PER_PORT,
            ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID,
            ParamInfoFlags::IS_MODULATABLE_PER_KEY,
            ParamInfoFlags::IS_MODULATABLE_PER_CHANNEL,
            ParamInfoFlags::IS_MODULATABLE_PER_PORT,
        ];
        let counts = allocation_counter::measure(|| {
            for mask in 0u16..256 {
                let mut metadata = ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE;
                for (index, flag) in flags.into_iter().enumerate() {
                    metadata.set(flag, mask & (1 << index) != 0);
                }
                for (index, target) in targets.into_iter().enumerate() {
                    assert_eq!(supports(metadata, target, false), mask & (1 << index) != 0);
                    assert_eq!(
                        supports(metadata, target, true),
                        mask & (1 << (index + 4)) != 0
                    );
                }
            }
        });
        assert_eq!(counts.count_total, 0);
    }
}
