//! The process's peak memory, for the merge's Verbose log (Static, 2026-11-17): the most it held
//! at once since it started, so a merge run in the app counts the app's own memory too.
//! Linux reads VmHWM; Windows asks kernel32 for the peak working set; elsewhere there is none.

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
    }
}
