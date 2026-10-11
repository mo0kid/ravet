//! MAIN-thread editors for Graph-bank children (decision 0276).
//!
//! [`Vst3GraphBankControl::open_editor`] creates the controller's `IPlugView`
//! for one child of a built bank while that bank's AUDIO leases stay live on
//! their workers. It uses the native editor companion, whose entries hold the
//! module's busy flag but not the live-core CONTROL admission, so a sibling or
//! the same child keeps processing. The returned [`Vst3GraphEditor`] holds no
//! borrow of the control or of an `Instance`: ordering is enforced by state
//! rather than by the borrow checker.
//!
//! - Each child has at most one editor ([`BankError::EditorOpen`]).
//! - While an editor is open, reclamation of its bank is deferred and counted
//!   in [`ReclaimReport::editors`](crate::vst3::instances::hosted_audio::graph::ReclaimReport::editors). No lease is
//!   ended and no child is stopped or retired. Independently, native child
//!   destruction returns `LIFECYCLE` while the view lives. The view is
//!   therefore always released before the child's controller, component and
//!   module retire. Reclamation never closes a caller's editor itself: close or
//!   drop the guard on MAIN, then reclaim again.
//! - [`Vst3GraphEditor::close`] and drop run the ordered native teardown
//!   (`removed` if attached, `setFrame(nullptr)`, release). A drop-time destroy
//!   refused before teardown starts keeps the native editor and the bank
//!   retained and increments [`refused_drops`](crate::vst3::instances::editor::refused_drops). `mem::forget` on the
//!   guard keeps the bank retained for the process lifetime, a memory-safe
//!   leak of bounded registry capacity.
//! - [`Vst3GraphEditor::attach`] is `unsafe`: the caller's `NSView` must stay
//!   alive until a successful detach or until the guard is closed or dropped.
//! - No callback enters Rust, and no view, frame or controller pointer escapes.
//!   The guard is neither `Send` nor `Sync`.
//! - Thread safety between the plug-in's MAIN controller/view calls and its
//!   AUDIO `process()` is the plug-in's responsibility, as VST3 permits; RAVET
//!   guarantees lifecycle ordering only. Only RAVET's split-component Gain has
//!   been exercised.
//!
//! ```compile_fail
//! fn require_send<T: Send>() {}
//! require_send::<ravet_host::vst3::instances::hosted_audio::graph::Vst3GraphEditor>();
//! ```
//! ```
//! fn require_send<T: Send>() {}
//! require_send::<ravet_host::vst3::instances::hosted_audio::graph::BankId>();
//! ```
use super::{BankError, BankId, Vst3GraphBankControl};
use crate::vst3::instances::editor::{Closed, Error, Platform, Provider, Size, State, View};
use std::{cell::Cell, ffi::c_void, ptr::NonNull, rc::Rc};

/// MAIN-thread guard owning the native editor view of one Graph-bank child.
pub struct Vst3GraphEditor {
    view: View<'static>,
    marker: Rc<Cell<bool>>,
    bank: BankId,
    child: usize,
}
/// A refused close keeps the editor for explicit retry on the main thread.
pub struct Vst3GraphEditorCloseFailure {
    pub owner: Vst3GraphEditor,
    pub error: Error,
}
impl Vst3GraphBankControl {
    /// Create the editor of `child` on MAIN while the bank keeps running.
    ///
    /// Refuses a foreign table ([`BankError::Editor`] with [`Error::Abi`]), a
    /// poisoned child, a second editor for the child
    /// ([`BankError::EditorOpen`]), a missing editor or unsupported platform
    /// (native report), and a malformed initial size. The child is marked
    /// before the native call so reclamation cannot begin meanwhile; any
    /// failure clears the mark.
    pub fn open_editor(
        &self,
        child: usize,
        provider: Provider<'static>,
        platform: Platform,
    ) -> Result<Vst3GraphEditor, BankError> {
        let (handle, marker) = self.with_slot(|s| {
            let slot = s.children.get(child).ok_or(BankError::Shape)?;
            let instance = slot.instance.as_ref().ok_or(BankError::Stale)?;
            if !std::ptr::eq(provider.raw.instance_origin, instance.library.provider.raw) {
                return Err(BankError::Editor {
                    child,
                    error: Error::Abi,
                });
            }
            if instance.poisoned || instance.state_fault {
                return Err(BankError::Editor {
                    child,
                    error: Error::Poisoned,
                });
            }
            if slot.editor.get() {
                return Err(BankError::EditorOpen { child });
            }
            let handle = instance.handle.ok_or(BankError::Stale)?;
            slot.editor.set(true);
            Ok((handle, Rc::clone(&slot.editor)))
        })?;
        match View::create(provider.raw, handle.as_ptr(), platform) {
            Ok(view) => Ok(Vst3GraphEditor {
                view,
                marker,
                bank: self.id,
                child,
            }),
            Err(error) => {
                marker.set(false);
                Err(BankError::Editor { child, error })
            }
        }
    }
}
impl Vst3GraphEditor {
    /// The bank this editor belongs to.
    pub fn bank(&self) -> BankId {
        self.bank
    }
    /// The child index within the bank.
    pub fn child(&self) -> usize {
        self.child
    }
    /// The native owner's attached state as of the last owner reply.
    pub fn attached(&self) -> bool {
        self.view.attached()
    }
    /// Last size admitted by this guard.
    pub fn last_size(&self) -> Size {
        self.view.last_size()
    }
    /// Attach the view to a caller-owned `NSView` and return the size afterwards.
    ///
    /// # Safety
    /// `parent` must be a live `NSView` owned by the caller on the main thread.
    /// It must remain alive until a successful [`detach`](Self::detach) or until
    /// this guard is closed or dropped. The plug-in may add subviews to it.
    pub unsafe fn attach(&mut self, parent: NonNull<c_void>) -> Result<Size, Error> {
        // SAFETY: the caller's parent obligation is forwarded unchanged.
        unsafe { self.view.attach(parent) }
    }
    /// Call `removed`. A returned refusal keeps the view attached and retryable.
    pub fn detach(&mut self) -> Result<(), Error> {
        self.view.detach()
    }
    /// Query `getSize`.
    pub fn size(&mut self) -> Result<Size, Error> {
        self.view.size()
    }
    /// Host-initiated resize, as [`Editor::resize`](crate::vst3::instances::editor::Editor::resize).
    pub fn resize(&mut self, proposed: Size) -> Result<Size, Error> {
        self.view.resize(proposed)
    }
    /// Content scale in 0.25..=8.0, as [`Editor::set_content_scale`](crate::vst3::instances::editor::Editor::set_content_scale).
    pub fn set_content_scale(&mut self, factor: f32) -> Result<Size, Error> {
        self.view.set_content_scale(factor)
    }
    /// Consume the latest size applied for a plug-in `resizeView` request.
    pub fn take_resize_request(&mut self) -> Result<Option<Size>, Error> {
        self.view.take_resize_request()
    }
    /// Copied flags and frame counters; no foreign call.
    pub fn state(&self) -> Result<State, Error> {
        self.view.state()
    }
    /// Ordered consuming teardown. On success the bank may be reclaimed again.
    /// A thread or reentry refusal returns the guard for retry.
    pub fn close(mut self) -> Result<Closed, Box<Vst3GraphEditorCloseFailure>> {
        match self.view.close() {
            Ok(closed) => {
                self.marker.set(false);
                Ok(closed)
            }
            Err(error) => Err(Box::new(Vst3GraphEditorCloseFailure { owner: self, error })),
        }
    }
}
impl Drop for Vst3GraphEditor {
    fn drop(&mut self) {
        // A refused drop-time destroy keeps the bank retained (marker stays set).
        if self.view.drop_destroy() {
            self.marker.set(false);
        }
    }
}
