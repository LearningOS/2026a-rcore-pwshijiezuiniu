use core::{cmp::min, mem::size_of, ptr::copy_nonoverlapping};
use crate::{config::PAGE_SIZE, task::translate_va_to_pa};

/// copy_to_user
pub fn copy_to_user<T>(dst_va: usize, src: &T) -> isize {
    
    let mut start = dst_va as *const T as usize;
    let Some(end) = size_of::<T>().checked_add(start) else {
        return -1;
    };
    let mut origin_start = src as *const T as usize;
    while start < end {
        let Some(cur_pa) = translate_va_to_pa(start) else {
            return -1;
        };
        let left = min(PAGE_SIZE - (cur_pa % PAGE_SIZE), end - start);
        unsafe {copy_nonoverlapping(origin_start as *const u8, cur_pa as *mut u8, left);}
        start += left;
        origin_start +=left;
    }
    0
}