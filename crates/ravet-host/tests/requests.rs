use clack_host::{
    host::SharedHandler,
    prelude::{HostInfo, PluginEntry, PluginInstance},
};
use clack_plugin::{entry::SinglePluginEntry, prelude::*};
use ravet_host::{HostRequests, PendingHostRequests, RequestHost};
use std::sync::atomic::{AtomicUsize, Ordering};
static CALLBACKS: AtomicUsize = AtomicUsize::new(0);
struct RequestsPlugin;
struct Main<'a>(HostMainThreadHandle<'a>);
impl Plugin for RequestsPlugin {
    type Shared<'a> = ();
    type MainThread<'a> = Main<'a>;
    type AudioProcessor<'a> = ();
}
impl DefaultPluginFactory for RequestsPlugin {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new("org.ravet.requests-fixture", "Synthetic requests fixture")
    }
    fn new_shared(host: HostSharedHandle<'_>) -> Result<(), PluginError> {
        host.request_restart();
        host.request_process();
        host.request_callback();
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
        if CALLBACKS.fetch_add(1, Ordering::Relaxed) == 0 {
            self.0.shared().request_callback();
        }
    }
}
#[test]
fn actual_plugin_init_requests_and_reentrant_callback_are_deferred_and_bounded() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<RequestsPlugin>>(c"").unwrap();
    let info = HostInfo::new("Synthetic request host", "RAVET", "", "0.1").unwrap();
    let mailbox = HostRequests::new();
    let mut instance = PluginInstance::<RequestHost>::new(
        |_| mailbox.clone(),
        |_| (),
        &entry,
        c"org.ravet.requests-fixture",
        &info,
    )
    .unwrap();
    assert_eq!(CALLBACKS.load(Ordering::Relaxed), 0);
    assert!(mailbox.service_callback(&mut instance));
    assert_eq!(CALLBACKS.load(Ordering::Relaxed), 1);
    assert!(mailbox.service_callback(&mut instance));
    assert_eq!(CALLBACKS.load(Ordering::Relaxed), 2);
    assert!(!mailbox.service_callback(&mut instance));
    let actions = mailbox.take();
    assert_eq!(
        actions,
        PendingHostRequests {
            restart: true,
            process: true,
            callback: false
        }
    );
    mailbox.defer(actions);
    assert_eq!(mailbox.take(), actions);
    assert_eq!(mailbox.take(), PendingHostRequests::default());
}
#[test]
fn concurrent_callback_publication_coalesces_and_preserves_all_request_kinds() {
    let mailbox = HostRequests::new();
    std::thread::scope(|scope| {
        for kind in 0..3 {
            let mailbox = mailbox.clone();
            scope.spawn(move || {
                for _ in 0..10000 {
                    match kind {
                        0 => mailbox.request_restart(),
                        1 => mailbox.request_process(),
                        _ => mailbox.request_callback(),
                    }
                }
            });
        }
    });
    assert_eq!(
        mailbox.take(),
        PendingHostRequests {
            restart: true,
            process: true,
            callback: true
        }
    );
    assert_eq!(mailbox.take(), PendingHostRequests::default());
}
