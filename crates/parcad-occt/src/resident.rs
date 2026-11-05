//! What this process still holds once a request is answered.
//!
//! Measured, not estimated: OpenCASCADE allocates through the C++ runtime, so
//! nothing on the Rust side can add up what a cached shape costs. A build
//! leaves most of its peak behind as freed pages the allocator has not yet
//! handed back — measured after `examples/pleated-shade.js`, 500 MB of a 1.4 GB
//! peak with the cache itself under 10 MB — so when there is enough of it the
//! allocator is asked to return it first, and the number reported is what the
//! system cannot take back.

/// How many kilobytes this process holds, once the allocator has given its
/// freed pages back if there were enough to be worth it; `None` where the
/// platform does not say.
pub fn held_kb() -> Option<u64> {
    held_kb_trimming_over(TRIM_OVER_KB)
}

/// Below this the trim is not worth what the next build pays to fault the
/// pages back in: 8 ms on an 82 ms bracket rebuild, measured.
const TRIM_OVER_KB: u64 = 128 * 1024;

fn held_kb_trimming_over(threshold_kb: u64) -> Option<u64> {
    let held = held_kb_now()?;
    if held <= threshold_kb {
        return Some(held);
    }
    return_freed_pages();
    held_kb_now()
}

#[cfg(target_os = "macos")]
fn return_freed_pages() {
    extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: libc::size_t) -> libc::size_t;
    }
    unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn return_freed_pages() {
    unsafe { libc::malloc_trim(0) };
}

#[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))))]
fn return_freed_pages() {}

/// The physical footprint: resident and compressed pages, without the freed
/// pages the kernel may reclaim at will, which `ps` counts in RSS.
#[cfg(target_os = "macos")]
fn held_kb_now() -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    let pid = std::process::id() as libc::c_int;
    let rc = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V2, info.as_mut_ptr().cast()) };
    if rc != 0 {
        return None;
    }
    Some(unsafe { info.assume_init() }.ri_phys_footprint / 1024)
}

#[cfg(target_os = "linux")]
fn held_kb_now() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page_bytes = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    u64::try_from(page_bytes).ok().map(|page| resident_pages * page / 1024)
}

#[cfg(windows)]
fn held_kb_now() -> Option<u64> {
    #[repr(C)]
    struct ProcessMemoryCounters {
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
    extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }
    let mut counters = std::mem::MaybeUninit::<ProcessMemoryCounters>::zeroed();
    let cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), counters.as_mut_ptr(), cb) };
    if ok == 0 {
        return None;
    }
    Some(unsafe { counters.assume_init() }.working_set_size as u64 / 1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn held_kb_now() -> Option<u64> {
    None
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux", windows)))]
mod tests {
    use super::*;

    /// The number is a process's memory, not a page count or a zero from a
    /// failed call: a test binary holds more than a megabyte and less than
    /// the machine.
    #[test]
    fn reports_this_process_in_kilobytes() {
        let held = held_kb().expect("this platform reports its footprint");
        assert!(held > 1024, "{held} kB is too little for a running test binary");
        assert!(held < 64 * 1024 * 1024, "{held} kB is more than 64 GB");
    }

    /// Memory given back is memory the number no longer counts. Small blocks,
    /// because a large one is unmapped the moment it is freed and would pass
    /// without the allocator being asked for anything; measured in a process
    /// of its own, because the footprint is the whole process's and the other
    /// tests in this binary build geometry on their threads meanwhile.
    #[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
    #[test]
    fn freed_memory_is_returned_before_it_is_counted() {
        const ALONE: &str = "PARCAD_RESIDENT_TEST_ALONE";
        if std::env::var_os(ALONE).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "resident::tests::freed_memory_is_returned_before_it_is_counted"])
                .env(ALONE, "1")
                .status()
                .unwrap();
            assert!(status.success(), "measured alone, the footprint did not return");
            return;
        }
        let before = held_kb_now().expect("this platform reports its footprint");
        let blocks: Vec<Vec<u8>> = (0..256 * 1024).map(|_| vec![1u8; 1024]).collect();
        let during = held_kb_now().expect("this platform reports its footprint");
        drop(blocks);
        let after = held_kb_trimming_over(0).expect("this platform reports its footprint");
        assert!(during >= before + 200 * 1024, "256 MB of small blocks raised the footprint by {} kB", during - before);
        // Without the trim, macOS keeps 10 to 26 MB of these pages dirty.
        assert!(after < before + 4 * 1024, "{} kB stayed held after they were freed", after.saturating_sub(before));
    }
}
