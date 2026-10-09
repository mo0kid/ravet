use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags as C,
    ParamRescanFlags as R,
};
use clack_host::{host::SharedHandler, prelude::*};
use clack_plugin::{entry::SinglePluginEntry, prelude::*};
use ravet_host::{ParameterHost, ParameterMainRequests, ParameterSharedRequests};
struct Fixture;
struct Main<'a>(HostMainThreadHandle<'a>);
impl Plugin for Fixture {
    type Shared<'a> = ();
    type MainThread<'a> = Main<'a>;
    type AudioProcessor<'a> = ();
}
impl DefaultPluginFactory for Fixture {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(
            "org.ravet.parameter-request-fixture",
            "Synthetic parameter requests",
        )
    }
    fn new_shared(host: HostSharedHandle<'_>) -> Result<(), PluginError> {
        let extension = host
            .get_extension::<HostParams>()
            .expect("missing advertised host params");
        extension.request_flush(&host);
        host.request_callback();
        Ok(())
    }
    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        _: &'a (),
    ) -> Result<Main<'a>, PluginError> {
        Ok(Main(host))
    }
}
impl<'a> PluginMainThread<'a, ()> for Main<'a> {
    fn on_main_thread(&self) {
        let extension = self.0.shared().get_extension::<HostParams>().unwrap();
        extension.rescan(&self.0, R::VALUES | R::TEXT);
        extension.clear(&self.0, ClapId::new(19), C::AUTOMATIONS);
        extension.clear(&self.0, ClapId::new(7), C::ALL);
        extension.clear(&self.0, ClapId::new(19), C::MODULATIONS);
        extension.request_flush(&self.0.shared());
    }
}
#[test]
fn native_extension_callbacks_defer_exact_changes_and_flush_without_plugin_reentry() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<Fixture>>(c"").unwrap();
    let info = HostInfo::new("Synthetic parameter host", "RAVET", "", "0.1").unwrap();
    let shared = ParameterSharedRequests::new();
    let main = ParameterMainRequests::new();
    let mut instance = PluginInstance::<ParameterHost>::new(
        |_| shared.clone(),
        |_| main.clone(),
        &entry,
        c"org.ravet.parameter-request-fixture",
        &info,
    )
    .unwrap();
    assert!(shared.take_flush());
    assert!(!shared.take_flush());
    assert_eq!(main.take().clears().len(), 0);
    assert!(shared.core().service_callback(&mut instance));
    assert!(!shared.core().service_callback(&mut instance));
    assert!(shared.take_flush());
    let pending = main.take();
    assert_eq!(pending.rescan, R::VALUES | R::TEXT);
    assert!(!pending.overflow);
    let clears: Vec<_> = pending.clears().collect();
    assert_eq!(clears[0].id, ClapId::new(19));
    assert_eq!(clears[0].flags, C::AUTOMATIONS | C::MODULATIONS);
    assert_eq!(clears[1].id, ClapId::new(7));
    assert_eq!(clears[1].flags, C::ALL);
    assert_eq!(main.take().clears().len(), 0);
}
#[test]
fn bounded_clear_overflow_preserves_ids_and_does_not_expand_to_unrelated_references() {
    let main = ParameterMainRequests::new();
    for id in 0..64 {
        main.clear(ClapId::new(id), C::AUTOMATIONS);
    }
    main.clear(ClapId::new(0), C::MODULATIONS);
    main.clear(ClapId::new(64), C::ALL);
    main.clear(ClapId::new(65), C::empty());
    main.rescan(R::INFO);
    main.rescan(R::ALL);
    let pending = main.take();
    assert!(pending.overflow);
    assert_eq!(pending.rescan, R::INFO | R::ALL);
    assert_eq!(pending.clears().len(), 64);
    for (id, clear) in pending.clears().enumerate() {
        assert_eq!(clear.id, ClapId::new(id as u32));
        assert_eq!(
            clear.flags,
            if id == 0 {
                C::AUTOMATIONS | C::MODULATIONS
            } else {
                C::AUTOMATIONS
            }
        );
    }
    assert!(!main.take().overflow);
    main.clear(ClapId::new(64), C::ALL);
    assert_eq!(main.take().clears().next().unwrap().id, ClapId::new(64));
}
#[test]
fn concurrent_flush_is_coalesced_and_core_requests_are_preserved() {
    let shared = ParameterSharedRequests::new();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let shared = shared.clone();
            scope.spawn(move || {
                for _ in 0..10_000 {
                    shared.request_flush();
                    shared.request_restart();
                }
            });
        }
    });
    assert!(shared.take_flush());
    assert!(!shared.take_flush());
    assert!(shared.core().take().restart);
    shared.defer_flush();
    assert!(shared.take_flush());
}

#[test]
fn prepared_callbacks_and_drains_do_not_allocate_or_deallocate() {
    let main = ParameterMainRequests::new();
    let shared = ParameterSharedRequests::new();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            for id in 0..65 {
                main.clear(ClapId::new(id), C::ALL);
            }
            main.rescan(R::VALUES | R::INFO);
            shared.request_flush();
            shared.request_process();
            assert!(main.take().overflow);
            assert!(shared.take_flush());
            assert!(shared.core().take().process);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
