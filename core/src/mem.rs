//! Honest memory figure for the status bar: this process plus every
//! descendant, because the WebView runs in child processes that a plain RSS
//! reading of the main process would hide.

#[derive(Clone, Copy)]
pub struct MemInfo {
    /// Windows: private working set (the Task Manager "Memory" column).
    /// Linux: PSS, which splits shared pages fairly between processes.
    pub bytes: u64,
    pub processes: u32,
}

/// `None` on platforms where the process tree cannot be measured.
pub fn usage() -> Option<MemInfo> {
    imp::usage(std::process::id())
}

/// The same figure for another process tree, for side-by-side measurements.
pub fn usage_of(root: u32) -> Option<MemInfo> {
    imp::usage(root)
}

/// Collects `root` and all of its descendants from `(pid, parent_pid)` pairs.
#[cfg(any(windows, target_os = "linux", test))]
fn descendants(root: u32, processes: &[(u32, u32)]) -> Vec<u32> {
    let mut tree = vec![root];
    let mut next = 0;
    while next < tree.len() {
        let parent = tree[next];
        next += 1;
        for &(pid, parent_pid) in processes {
            if parent_pid == parent && pid != parent && !tree.contains(&pid) {
                tree.push(pid);
            }
        }
    }
    tree
}

#[cfg(windows)]
mod imp {
    use std::mem::{size_of, zeroed};

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    use super::{MemInfo, descendants};

    pub fn usage(root: u32) -> Option<MemInfo> {
        let tree = descendants(root, &snapshot()?);
        let mut info = MemInfo {
            bytes: 0,
            processes: 0,
        };
        for pid in tree {
            if let Some(bytes) = private_working_set(pid) {
                info.bytes += bytes;
                info.processes += 1;
            }
        }
        (info.processes > 0).then_some(info)
    }

    fn snapshot() -> Option<Vec<(u32, u32)>> {
        // SAFETY: plain Win32 calls; `entry` is a properly sized, zeroed
        // PROCESSENTRY32W and the snapshot handle is closed before returning.
        unsafe {
            let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if handle == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

            let mut processes = Vec::with_capacity(256);
            let mut more = Process32FirstW(handle, &mut entry);
            while more != 0 {
                processes.push((entry.th32ProcessID, entry.th32ParentProcessID));
                more = Process32NextW(handle, &mut entry);
            }
            CloseHandle(handle);
            Some(processes)
        }
    }

    fn private_working_set(pid: u32) -> Option<u64> {
        // SAFETY: the counters structs are zeroed, sized via `cb`, and outlive
        // the calls; the process handle is closed before returning.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }

            let mut ex2: PROCESS_MEMORY_COUNTERS_EX2 = zeroed();
            ex2.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
            let bytes = if K32GetProcessMemoryInfo(handle, (&raw mut ex2).cast(), ex2.cb) != 0 {
                Some(ex2.PrivateWorkingSetSize as u64)
            } else {
                // Windows builds older than 10 22H2 only know the short struct.
                let mut basic: PROCESS_MEMORY_COUNTERS = zeroed();
                basic.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                (K32GetProcessMemoryInfo(handle, &mut basic, basic.cb) != 0)
                    .then_some(basic.WorkingSetSize as u64)
            };
            CloseHandle(handle);
            bytes
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::fs;

    use super::{MemInfo, descendants};

    pub fn usage(root: u32) -> Option<MemInfo> {
        let tree = descendants(root, &snapshot()?);
        let mut info = MemInfo {
            bytes: 0,
            processes: 0,
        };
        for pid in tree {
            if let Some(bytes) = proportional_set_size(pid) {
                info.bytes += bytes;
                info.processes += 1;
            }
        }
        (info.processes > 0).then_some(info)
    }

    fn snapshot() -> Option<Vec<(u32, u32)>> {
        let mut processes = Vec::with_capacity(256);
        for entry in fs::read_dir("/proc").ok()?.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            // The command name may contain spaces and parentheses, so parse
            // from the last ')': " <state> <ppid> ...".
            let parent = stat
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().nth(1))
                .and_then(|s| s.parse::<u32>().ok());
            if let Some(parent) = parent {
                processes.push((pid, parent));
            }
        }
        Some(processes)
    }

    fn proportional_set_size(pid: u32) -> Option<u64> {
        let kib = |path: String, key: &str| {
            fs::read_to_string(path).ok().and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix(key))
                    .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            })
        };
        kib(format!("/proc/{pid}/smaps_rollup"), "Pss:")
            .or_else(|| kib(format!("/proc/{pid}/status"), "VmRSS:"))
            .map(|kib| kib * 1024)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn usage(_root: u32) -> Option<super::MemInfo> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendants_follow_the_whole_subtree() {
        // 1 -> 2 -> 4, 1 -> 3, unrelated 9 -> 10, and a self-parented 0.
        let processes = [(0, 0), (1, 0), (2, 1), (3, 1), (4, 2), (9, 0), (10, 9)];
        let mut tree = descendants(1, &processes);
        tree.sort_unstable();
        assert_eq!(tree, [1, 2, 3, 4]);
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn measures_at_least_this_process() {
        let info = usage().expect("memory usage is available on this platform");
        assert!(info.processes >= 1);
        assert!(info.bytes > 0);
    }
}
