//! Allocation counts for memory measurements. With the `alloc-stats`
//! feature, `Counting` is the global allocator of the build. It counts the
//! live bytes and their high-water mark, and it gives each allocation of a
//! threshold size or more to a hook. Release builds do not enable the
//! feature.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// The system allocator with counts.
pub struct Counting;

#[global_allocator]
static GLOBAL: Counting = Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
/// High-water mark of `LIVE` since the last `take_peak`.
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// High-water mark of `LIVE` since the start.
static TOP: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);
static HOOK: AtomicUsize = AtomicUsize::new(0);
static THRESHOLD: AtomicUsize = AtomicUsize::new(usize::MAX);

/// A hook receives `(freed, address, bytes, live)` for each allocation of
/// at least the threshold: `freed` is false for an allocation and true for
/// its release, and `live` is the count of live bytes after the event.
pub type Hook = fn(bool, usize, usize, usize);

/// Give each allocation of `threshold` bytes or more to `hook`. `None`
/// stops the hook.
pub fn set_hook(threshold: usize, hook: Option<Hook>) {
    THRESHOLD.store(usize::MAX, Relaxed);
    HOOK.store(hook.map_or(0, |h| h as usize), Relaxed);
    if hook.is_some() {
        THRESHOLD.store(threshold, Relaxed);
    }
}

/// Live bytes.
pub fn live() -> usize {
    LIVE.load(Relaxed)
}

/// High-water mark of the live bytes since the start.
pub fn top() -> usize {
    TOP.load(Relaxed)
}

/// Allocations since the start.
pub fn count() -> usize {
    COUNT.load(Relaxed)
}

/// High-water mark of the live bytes since the last call. The next mark
/// starts at the live bytes now.
pub fn take_peak() -> usize {
    PEAK.swap(LIVE.load(Relaxed), Relaxed)
}

fn mark(live: usize) {
    PEAK.fetch_max(live, Relaxed);
    TOP.fetch_max(live, Relaxed);
}

fn report(freed: bool, address: *mut u8, bytes: usize) {
    if bytes >= THRESHOLD.load(Relaxed) {
        let hook = HOOK.load(Relaxed);
        if hook != 0 {
            // SAFETY: `set_hook` stores only values of type `Hook`.
            let hook: Hook = unsafe { std::mem::transmute::<usize, Hook>(hook) };
            hook(freed, address as usize, bytes, LIVE.load(Relaxed));
        }
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            COUNT.fetch_add(1, Relaxed);
            mark(LIVE.fetch_add(layout.size(), Relaxed) + layout.size());
            report(false, p, layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            COUNT.fetch_add(1, Relaxed);
            mark(LIVE.fetch_add(layout.size(), Relaxed) + layout.size());
            report(false, p, layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        LIVE.fetch_sub(layout.size(), Relaxed);
        report(true, p, layout.size());
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = System.realloc(p, layout, size);
        if !q.is_null() {
            COUNT.fetch_add(1, Relaxed);
            let before = LIVE.load(Relaxed);
            if q != p {
                // The old and the new block were both live during the copy.
                mark(before + size);
            }
            if size >= layout.size() {
                mark(LIVE.fetch_add(size - layout.size(), Relaxed) + size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Relaxed);
            }
            report(true, p, layout.size());
            report(false, q, size);
        }
        q
    }
}
