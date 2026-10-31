//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::loader::{get_app_data, get_num_app};
use crate::mm::{MapPermission, PTEFlags, PhysAddr, VirtAddr};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus, TaskInfo};

pub use context::TaskContext;


const MAX_USER_VA: usize = 1usize << 38;
/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// tasks_info
    tasks_info: Vec<TaskInfo>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` global instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        let mut tasks_info: Vec<TaskInfo> = Vec::new(); //[TaskInfo { syscall_times: [0usize; 5], syscall_id: [0usize; 5] }; MAX_APP_NUM];
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
            
            for _i in 0..num_app {
                tasks_info.push(TaskInfo { syscall_times: [0usize; 8], syscall_id: [0usize; 8]});
            }
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    tasks_info,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Change the current 'Running' task's program break
    pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
    }

    /// translate_va_to_pa
    pub fn translate_va_to_pa(&self, va: usize, need_write: bool) -> Option<usize> {
        if MAX_USER_VA < va {
            return None;
        }
        let inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let task_control_block = & inner.tasks[cur];
        let pt = task_control_block.memory_set.translate(VirtAddr::from(va).floor());
        match pt {
            Some(ptable) => {
                let flag = ptable.flags();
                if !ptable.is_valid() || (need_write && !ptable.writable()) || !flag.contains(PTEFlags::U) {
                    None
                } else {
                    Some(PhysAddr::from(ptable.ppn()).0 + VirtAddr::from(va).page_offset())
                }
            },
            None => None,
        }
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

    /// Record the syscall id and the number of times the current task has been called.
    pub fn record_syscall(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let task = &mut inner.tasks_info[cur];
        let Some(index) = task.syscall_id.iter().position(|&v| v == syscall_id ||v == 0) else {
            panic!("No available slot for syscall_id in record_syscall!");
        };
        match task.syscall_id[index] {
            0 => {
                task.syscall_id[index] = syscall_id;
                task.syscall_times[index] = 1;
            }
            _ => {
                task.syscall_times[index] += 1;
            }
        }
        // println!("Task {} called syscall {} for the {} time(s)", inner.current_task, syscall_id, syscall_times[index]);
    }

    /// Get the number of times the current task has been called.
    fn get_sys_call_times(&self, id: usize) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks_info[current].syscall_id.iter().position(|&a| a == id).map(|a| {
            inner.tasks_info[current].syscall_times[a]
        }).unwrap_or(0)
    }

    /// insert_framed_area
    pub fn mmap(&self, va_start: VirtAddr, va_end: VirtAddr, perm: MapPermission) -> bool {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let tcb = &mut inner.tasks[cur];
        tcb.memory_set.mmap(va_start, va_end, perm)

    }

    /// insert_framed_area
    pub fn munmap(&self, va_start: VirtAddr, va_end: VirtAddr) -> bool {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        let tcb = &mut inner.tasks[cur];
        tcb.memory_set.munmap(va_start, va_end)
    }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Get the number of times the current task has been called.
pub fn get_sys_call_times(id: usize) -> usize {
    TASK_MANAGER.get_sys_call_times(id)
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change the current 'Running' task's program break
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

/// translate_va_to_pa
pub fn translate_va_to_pa(va: usize) -> Option<usize> {
    TASK_MANAGER.translate_va_to_pa(va, true)
}

/// translate_va_to_pa
pub fn translate_va_to_pa_tr(va: usize) -> Option<usize> {
    TASK_MANAGER.translate_va_to_pa(va, false)
}

/// mmap
pub fn mmap(va_start: VirtAddr, va_end: VirtAddr, perm: MapPermission) -> isize {
    match TASK_MANAGER.mmap(va_start, va_end, perm) {
        true => 0,
        false => -1
    }
}

/// mmap
pub fn munmap(va_start: VirtAddr, va_end: VirtAddr) -> isize {
    match TASK_MANAGER.munmap(va_start, va_end) {
        true => 0,
        false => -1
    }
}
