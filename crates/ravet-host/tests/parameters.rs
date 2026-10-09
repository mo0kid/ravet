use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_host::{HostRequests, HostedParameterError as E, HostedParameters, RequestHost};
#[test]
fn gain_plain_edits_validate_before_dispatch_and_active_edits_reject() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_gain_clap::GainPlugin>>(c"")
        .unwrap();
    let mailbox = HostRequests::new();
    let info = HostInfo::new("Synthetic parameter host", "RAVET", "", "0.1").unwrap();
    let mut instance = PluginInstance::<RequestHost>::new(
        |_| mailbox.clone(),
        |_| (),
        &entry,
        c"org.ravet.gain",
        &info,
    )
    .unwrap();
    {
        assert!(matches!(
            HostedParameters::new(&mut instance, 1),
            Err(E::Capacity)
        ));
        assert!(matches!(
            HostedParameters::new(&mut instance, 0),
            Err(E::Capacity)
        ));
        let mut parameters = HostedParameters::new(&mut instance, 2).unwrap();
        assert_eq!(parameters.parameters().len(), 2);
        assert_eq!(parameters.parameters()[0].name, b"Gain");
        assert_eq!(parameters.parameters()[0].minimum, 0.);
        assert_eq!(parameters.parameters()[0].maximum, 4.);
        let gain = ClapId::new(1);
        let bypass = ClapId::new(2);
        assert_eq!(parameters.value(gain).unwrap(), 1.);
        for value in [f64::NAN, f64::INFINITY, -1., 5.] {
            assert_eq!(
                parameters.set_inactive(gain, value, &mut OutputEvents::void()),
                Err(E::Value)
            );
            assert_eq!(parameters.value(gain).unwrap(), 1.);
        }
        assert_eq!(
            parameters.set_inactive(bypass, 0.5, &mut OutputEvents::void()),
            Err(E::Value)
        );
        assert_eq!(
            parameters.set_inactive(ClapId::new(77), 1., &mut OutputEvents::void()),
            Err(E::Unknown)
        );
        parameters
            .set_inactive(gain, 2., &mut OutputEvents::void())
            .unwrap();
        parameters
            .set_inactive(bypass, 1., &mut OutputEvents::void())
            .unwrap();
        assert_eq!(parameters.value(gain).unwrap(), 2.);
        assert_eq!(parameters.value(bypass).unwrap(), 1.);
        parameters.refresh().unwrap();
        assert_eq!(parameters.value(gain).unwrap(), 2.);
    }
    let stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap();
    {
        let mut parameters = HostedParameters::new(&mut instance, 2).unwrap();
        assert_eq!(
            parameters.set_inactive(ClapId::new(1), 3., &mut OutputEvents::void()),
            Err(E::Active)
        );
        assert_eq!(parameters.value(ClapId::new(1)).unwrap(), 2.);
    }
    instance.deactivate(stopped);
}

mod malformed {
    use clack_extensions::params::*;
    use clack_plugin::{prelude::*, utils::Cookie};
    use std::sync::atomic::{AtomicU8, Ordering};
    pub static MODE: AtomicU8 = AtomicU8::new(0);
    pub struct Fixture;
    pub struct Main;
    pub struct Audio;
    impl Plugin for Fixture {
        type Shared<'a> = ();
        type MainThread<'a> = Main;
        type AudioProcessor<'a> = Audio;
        fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&()>) {
            builder.register::<PluginParams>();
        }
    }
    impl DefaultPluginFactory for Fixture {
        fn get_descriptor() -> PluginDescriptor {
            PluginDescriptor::new("org.ravet.parameter-fixture", "Synthetic metadata fixture")
        }
        fn new_shared(_: HostSharedHandle<'_>) -> Result<(), PluginError> {
            Ok(())
        }
        fn new_main_thread(_: HostMainThreadHandle<'_>, _: &()) -> Result<Main, PluginError> {
            Ok(Main)
        }
    }
    impl<'a> PluginAudioProcessor<'a, (), Main> for Audio {
        fn activate(
            _: HostAudioProcessorHandle<'a>,
            _: &Main,
            _: &'a (),
            _: PluginAudioConfiguration,
        ) -> Result<Self, PluginError> {
            Err(PluginError::Message("metadata-only fixture"))
        }
        fn process(
            &mut self,
            _: Process,
            _: clack_plugin::process::Audio,
            _: clack_plugin::process::Events,
        ) -> Result<ProcessStatus, PluginError> {
            Ok(ProcessStatus::Continue)
        }
    }
    impl PluginAudioProcessorParams for Audio {
        fn flush(
            &mut self,
            _: &clack_plugin::events::io::InputEvents,
            _: &mut clack_plugin::events::io::OutputEvents,
        ) {
        }
    }
    impl PluginMainThread<'_, ()> for Main {}
    impl PluginMainThreadParams for Main {
        fn count(&self) -> u32 {
            if MODE.load(Ordering::Relaxed) == 9 {
                4097
            } else {
                2
            }
        }
        fn get_info(&self, index: u32, writer: &mut ParamInfoWriter) {
            let mode = MODE.load(Ordering::Relaxed);
            if mode == 8 {
                return;
            }
            writer.set(&ParamInfo {
                id: ClapId::new(if mode == 1 { 1 } else { index + 1 }),
                flags: if mode == 6 {
                    ParamInfoFlags::IS_READONLY
                } else if mode == 7 {
                    ParamInfoFlags::IS_STEPPED
                } else {
                    ParamInfoFlags::empty()
                },
                cookie: Cookie::empty(),
                name: b"Synthetic",
                module: b"Fixture",
                min_value: if mode == 2 { f64::NAN } else { 0. },
                max_value: if mode == 3 { -1. } else { 1. },
                default_value: if mode == 4 {
                    2.
                } else if mode == 5 {
                    f64::NAN
                } else if mode == 7 {
                    0.5
                } else {
                    0.
                },
            });
        }
        fn get_value(&self, _: ClapId) -> Option<f64> {
            Some(if MODE.load(Ordering::Relaxed) == 10 {
                f64::NAN
            } else {
                0.
            })
        }
        fn value_to_text(&self, _: ClapId, _: f64, _: &mut ParamDisplayWriter) -> std::fmt::Result {
            Ok(())
        }
        fn text_to_value(&self, _: ClapId, _: &std::ffi::CStr) -> Option<f64> {
            None
        }
        fn flush(
            &self,
            _: &clack_plugin::events::io::InputEvents,
            _: &mut clack_plugin::events::io::OutputEvents,
        ) {
        }
    }
}
#[test]
fn malformed_refresh_preserves_catalogue_and_readonly_or_invalid_values_reject() {
    use malformed::{Fixture, MODE};
    use std::sync::atomic::Ordering;
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<Fixture>>(c"").unwrap();
    let info = HostInfo::new("Synthetic metadata host", "RAVET", "", "0.1").unwrap();
    let mut instance = PluginInstance::<()>::new(
        |_| (),
        |_| (),
        &entry,
        c"org.ravet.parameter-fixture",
        &info,
    )
    .unwrap();
    let mut parameters = HostedParameters::new(&mut instance, 2).unwrap();
    let before = parameters.parameters().to_vec();
    for mode in [1, 2, 3, 4, 5, 7, 8, 9] {
        MODE.store(mode, Ordering::Relaxed);
        assert!(parameters.refresh().is_err());
        assert_eq!(parameters.parameters(), before);
    }
    MODE.store(6, Ordering::Relaxed);
    parameters.refresh().unwrap();
    assert_eq!(
        parameters.set_inactive(ClapId::new(1), 1., &mut OutputEvents::void()),
        Err(E::ReadOnly)
    );
    MODE.store(10, Ordering::Relaxed);
    assert_eq!(parameters.value(ClapId::new(1)), Err(E::Metadata));
    MODE.store(0, Ordering::Relaxed);
}
