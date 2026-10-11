//! Test-only counting global allocator (feature `allocation-probe`).
//!
//! Built into the reference staticlib, it lets the native module tests measure
//! Rust heap operations inside the loaded reference bundles: every `alloc`,
//! `alloc_zeroed`, `realloc` and `dealloc` made by Rust code linked into the
//! bundle increments one process-wide counter, read through
//! `ravet_vst3_reference_allocation_count`. Allocations are forwarded to the
//! system allocator unchanged. C++ allocations in the wrapper or SDK and direct
//! `malloc` calls are not counted. Compiled out of this crate's own unit tests,
//! which install `allocation-counter` instead.
#![allow(unsafe_code)]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};

static OPERATIONS: AtomicU64 = AtomicU64::new(0);

struct Counting;

// SAFETY: every method forwards to `System` with the caller's arguments; the
// counter update has no effect on the returned memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        OPERATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarded GlobalAlloc contract.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        OPERATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarded GlobalAlloc contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        OPERATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarded GlobalAlloc contract.
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        OPERATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarded GlobalAlloc contract.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Total Rust heap operations (alloc, alloc_zeroed, realloc, dealloc) made by
/// this image since load.
#[unsafe(no_mangle)]
pub extern "C" fn ravet_vst3_reference_allocation_count() -> u64 {
    OPERATIONS.load(Ordering::Relaxed)
}
