//! Peak memory, for the merge's Verbose log (Static, 2026-11-17). Two figures:
//! - the process's: the most it held at once since it started, the app's own memory too. Linux
//!   reads VmHWM; Windows asks kernel32 for the peak working set; elsewhere there is none.
//! - the merge's own: the most the Rust heap grew above what it held when the merge started,
//!   counted by peakmem-alloc's allocator (Static's pick). A program has it only where it
//!   installs [`COUNTING`] as its global allocator; elsewhere there is no such figure. Other
//!   threads' allocations in the window count too (the viewer's, run from the app); memory the
//!   webview or GTK hold does not, nor the allocator's own overhead, so it reads below the
//!   process's figure.

use peakmem_alloc::PeakMemAllocTrait as _;

/// The counting allocator: the system's, its bytes counted. A program installs it with
/// `#[global_allocator] static GLOBAL: &landcore::memory::Counting = &landcore::memory::COUNTING;`
pub type Counting = peakmem_alloc::PeakMemAlloc<std::alloc::System>;
pub use peakmem_alloc::INSTRUMENTED_SYSTEM as COUNTING;

/// Starts the merge's window: the count and its peak back to 0. Two merges at once would share it.
pub fn start_merge_window() {
    COUNTING.reset_peak_memory();
}

/// The most the heap grew since [`start_merge_window`], in bytes; `None` where [`COUNTING`] is
/// not the program's allocator (it never moves, and a merge always allocates).
pub fn merge_peak_bytes() -> Option<u64> {
    let peak = COUNTING.get_peak_memory();
    (peak > 0).then_some(peak as u64)
}

/// The peak resident memory in bytes, when the system says.
pub fn peak_bytes() -> Option<u64> {
    imp::peak_bytes()
}

/// Bytes as the log shows them: "1.31 GB", "153 MB".
pub fn human(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    if mb >= 1000.0 { format!("{:.2} GB", mb / 1000.0) } else { format!("{mb:.0} MB") }
}

#[cfg(target_os = "linux")]
mod imp {
    pub fn peak_bytes() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
        let kb: u64 = line.trim_start_matches("VmHWM:").trim().trim_end_matches("kB").trim().parse().ok()?;
        Some(kb * 1024)
    }
}

#[cfg(windows)]
mod imp {
    // PROCESS_MEMORY_COUNTERS (psapi.h); K32GetProcessMemoryInfo is kernel32's since Windows 7,
    // so no psapi.lib and no windows-sys features for one call.
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn K32GetProcessMemoryInfo(process: *mut core::ffi::c_void, counters: *mut Counters, cb: u32) -> i32;
    }
    pub fn peak_bytes() -> Option<u64> {
        let size = std::mem::size_of::<Counters>() as u32;
        let mut c = Counters {
            cb: size,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: the pseudo-handle needs no closing, and `c` is the size `cb` says.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, size) };
        (ok != 0).then_some(c.peak_working_set_size as u64)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    pub fn peak_bytes() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn peak_is_read_where_the_system_says_and_shown_short() {
        #[cfg(any(target_os = "linux", windows))]
        assert!(super::peak_bytes().is_some_and(|b| b > 0));
        assert_eq!(super::human(1_310_000_000), "1.31 GB");
        assert_eq!(super::human(153_400_000), "153 MB");
        // This test binary does not install the counting allocator, so there is no such figure.
        super::start_merge_window();
        let _held: Vec<u8> = Vec::with_capacity(1 << 20);
        assert_eq!(super::merge_peak_bytes(), None);
    }
}
