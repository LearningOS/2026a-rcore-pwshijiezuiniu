//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    loader::get_app_data_by_name, mm::{translated_refmut, translated_str, MapPermission, VirtAddr}, task::{
        TaskControlBlock, add_task, current_task, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, BIG_STRIDE
    }, timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel:pid[{}] sys_get_time", current_task().unwrap().pid.0);
    let us = get_time_us();
    let token = current_user_token();
    // Write the two fields separately so that a TimeVal crossing a page
    // boundary is still handled correctly.
    let sec_ptr = ts as *mut usize;
    let usec_ptr = unsafe { sec_ptr.add(1) };
    *translated_refmut(token, sec_ptr) = us / 1_000_000;
    *translated_refmut(token, usec_ptr) = us % 1_000_000;
    0
}

/// map a user memory area
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel:pid[{}] sys_mmap", current_task().unwrap().pid.0);
    let va = VirtAddr::from(start);
    if va.page_offset() != 0 {
        return -1;
    }
    if len == 0 {
        return -1;
    }
    let Some(end) = start.checked_add(len) else {
        return -1;
    };
    if end > (1usize << 38) {
        return -1; // SV39 user-space upper bound, avoid VirtAddr truncation aliasing
    }
    if prot & !0x7 != 0 || prot & 0x7 == 0 {
        return -1;
    }
    let mut perm = MapPermission::U;
    if prot & 1 == 1 {
        perm = perm | MapPermission::R;
    }
    if prot & 2 == 2 {
        perm = perm | MapPermission::W;
    }
    if prot & 4 == 4 {
        perm = perm | MapPermission::X;
    }

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if inner.memory_set.mmap(va, VirtAddr::from(end), perm) {
        0
    } else {
        -1
    }
}

/// unmap a user memory area
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap",
        current_task().unwrap().pid.0
    );
    let va = VirtAddr::from(start);
    if va.page_offset() != 0 {
        return -1;
    }
    if len == 0 {
        return -1;
    }
    let Some(end) = start.checked_add(len) else {
        return -1;
    };
    if end > (1usize << 38) {
        return -1; // SV39 user-space upper bound, avoid VirtAddr truncation aliasing
    }

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if inner.memory_set.munmap(va, VirtAddr::from(end)) {
        0
    } else {
        -1
    }
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let parent = current_task().unwrap();
        let task = Arc::new(TaskControlBlock::new(data));
        // TaskControlBlock::new is designed for initproc, so set the parent link here
        task.inner_exclusive_access().parent = Some(Arc::downgrade(&parent));
        parent.inner_exclusive_access().children.push(task.clone());
        let new_pid = task.pid.0;
        add_task(task);
        new_pid as isize
    } else {
        -1
    }

}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if prio < 2 {
        return -1;
    }
    current_task().unwrap().inner_exclusive_access().pass = BIG_STRIDE / (prio as usize);
    prio
}
