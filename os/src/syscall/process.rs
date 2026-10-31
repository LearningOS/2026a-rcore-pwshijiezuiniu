//! Process management syscalls


use crate::{
    mm::{MapPermission, VirtAddr, copy_from_user, copy_to_user}, task::{change_program_brk, exit_current_and_run_next, get_sys_call_times, mmap, munmap, suspend_current_and_run_next}, timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let content = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    copy_to_user(ts as usize, &content)
}


// TODO: implement the syscall
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => {
            let mut byte = 0u8;
            if copy_from_user(id, &mut byte) == 0 { byte as isize } else { -1 }
        },
        1 => {
            copy_to_user(id as *mut u8 as usize, &(data as u8))
        },
        2 => {
            get_sys_call_times(id) as isize
        },
        _ => {
            -1
        }
    }
    
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    let va = VirtAddr::from(start);
    if va.page_offset() != 0 {
        return -1;
    }
    if len == 0 {
        return -1;
    }
    let Some(end) = start.checked_add(len) else { return -1; };
    if end > (1usize << 38) {
        return -1;   // SV39 用户空间上界,防止 VirtAddr::from 截断别名
    }
    
    if prot & !0x7 != 0 || prot & 0x7 == 0 {
        return -1
    }
    let mut  u = MapPermission::U;
    if prot & 1 == 1 {
        u = u | MapPermission::R;
    }
    if prot & 2 == 2 {
        u = u | MapPermission::W;
    }
    if prot & 4 == 4 {
        u = u | MapPermission::X;
    }

    // map user memory_set
    mmap(va, VirtAddr::from(end).ceil().into(), u)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    let va = VirtAddr::from(start);
    if va.page_offset() != 0 {
        return -1;
    }
    if len == 0 {
        return -1;
    }

    let Some(end) = start.checked_add(len) else { return -1; };
    if end > (1usize << 38) {
        return -1;   // SV39 用户空间上界,防止 VirtAddr::from 截断别名
    }
    // map user memory_set
    munmap(va, VirtAddr::from(end).ceil().into())
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
