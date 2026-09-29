//! A minimal work-stealing parallel map, so the engine needs no rayon.
//!
//! Workers pull indices off a shared atomic counter, which keeps every thread
//! busy when items cost wildly different amounts — exactly the case here, where
//! a dense cell can take fifty times as long as an empty one.
//!
//! # One level of parallelism, not two (round 18bw)
//!
//! A `par_map` running inside another `par_map`'s worker runs **sequentially**. This is
//! not a tuning preference; it is the fix for a measured six-times slowdown, and the
//! numbers are worth keeping because the pathology is invisible from the shell.
//!
//! The scatter maps cells across the pool, and inside each cell the painted-statics pass
//! mapped the cell's *every* reference across the pool again. On Robin's 8-core/16-thread
//! machine that is 16 cell workers each spawning 16 more — up to 256 threads, created and
//! joined **once per cell, 4,852 times an export**, to do the handful of references that
//! actually carry a mesh setup.
//!
//! Run alone, that survives: the threads are short, nothing preempts them, every nested
//! join completes. Run in the app, it collapses. The UI thread, the viewport's frame loop,
//! the progress poll and the webview all take cores, a nested join waits for its slowest
//! thread, and one descheduled inner thread stalls its whole cell — which its colour round
//! then waits for. Measured, same plugin byte for byte in both front ends:
//!
//! | | shell | window |
//! |---|---|---|
//! | scatter | 58.0 s | 313 s |
//! | slowest single cell | — | **57 s** (the average cell is 115 ms) |
//! | thread-seconds spent | 928 | ~5,000 |
//! | effective cores of 16 | 8.0 | **1.7** |
//!
//! So the rule: the outer level owns the machine, and an inner `par_map` is a loop. It
//! costs nothing where the nesting was doing good work — the inner list is short by the
//! time it runs, and the outer level is already saturating the cores — and it removes the
//! oversubscription entirely rather than trying to balance it.
//!
//! **It cannot change what comes out.** `par_map` writes results into slots by index and
//! returns them in input order, so running the items in one thread in that same order is
//! the same output by construction. `tests/round18bw.rs` pins both halves of that: the
//! order, and that a nested map really does stay on one thread.
//!
//! The depth is per thread and belongs to **spawned workers**, not to calls: a worker
//! starts at depth 1, and a map that ran its items on the calling thread without spawning
//! anything leaves the depth alone. The distinction is not pedantry — it is what keeps a
//! one-cell export fast. Such a run has a single cell in its round, so the outer map runs
//! it here and claims nothing, and the painted rocks in that cell should still have the
//! machine. A `GF_THREADS=1` pin is honoured anyway, because an inner map then reaches the
//! sequential branch by its own arithmetic rather than by the depth.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! {
    /// How many `par_map`/`par_fold` calls this thread is inside. 0 on the thread that
    /// starts a map; 1 in the workers it spawns, so a map they call sees that it is nested.
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn depth() -> usize {
    DEPTH.with(|d| d.get())
}

/// Runs `f` with this thread's depth set to `d`, restoring it afterwards.
fn with_depth<R>(d: usize, f: impl FnOnce() -> R) -> R {
    struct Restore(usize);
    impl Drop for Restore {
        fn drop(&mut self) {
            DEPTH.with(|d| d.set(self.0));
        }
    }
    let was = depth();
    let _restore = Restore(was);
    DEPTH.with(|c| c.set(d));
    f()
}

/// Whether this thread is a worker spawned by a `par_map` or `par_fold` — and so whether
/// a map starting here should run on this thread alone.
pub fn nested() -> bool {
    depth() > 0
}

pub fn threads() -> usize {
    // GF_THREADS pins the worker count, for benchmarking and for leaving a core
    // free on a machine that is doing something else.
    if let Ok(v) = std::env::var("GF_THREADS") {
        if let Ok(n) = v.parse::<usize>() {
            if n >= 1 {
                return n;
            }
        }
    }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// Maps `f` over `items` in parallel, preserving input order in the output.
pub fn par_map<T, R, F>(items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(usize, &T) -> R + Sync,
{
    let n = items.len();
    let mut out: Vec<Option<R>> = Vec::with_capacity(n);
    out.resize_with(n, || None);
    if n == 0 {
        return Vec::new();
    }
    // One level only — see the module note. A nested map is a loop, in the same order the
    // slots would have been filled in, which is why this is not a behaviour change.
    let nthreads = if nested() { 1 } else { threads().min(n) };
    if nthreads <= 1 {
        /* Deliberately *not* inside the depth: this branch spawned nothing, so it is
           holding none of the machine and has no reason to hold a nested map back. It is
           the "export just this cell" case — one cell in the round, running here on the
           calling thread, whose painted rocks should still spread across the cores. With
           `GF_THREADS=1` the inner map reaches the same branch by its own arithmetic, so
           the pin is still honoured. */
        for (i, it) in items.iter().enumerate() {
            out[i] = Some(f(i, it));
        }
        return out.into_iter().map(|o| o.unwrap()).collect();
    }

    let next = AtomicUsize::new(0);
    // Hand each worker a disjoint set of slots; ownership is proven by the
    // atomic counter, which never yields the same index twice.
    let slots: Vec<*mut Option<R>> = out.iter_mut().map(|s| s as *mut _).collect();
    struct Slots<R>(Vec<*mut Option<R>>);
    unsafe impl<R: Send> Send for Slots<R> {}
    unsafe impl<R: Send> Sync for Slots<R> {}
    let slots = Slots(slots);

    std::thread::scope(|s| {
        for _ in 0..nthreads {
            let next = &next;
            let slots = &slots;
            let f = &f;
            let d = depth() + 1;
            s.spawn(move || {
                with_depth(d, || loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let r = f(i, &items[i]);
                    let slot: *mut Option<R> = slots.0[i];
                    unsafe {
                        *slot = Some(r);
                    }
                })
            });
        }
    });

    out.into_iter().map(|o| o.unwrap()).collect()
}

