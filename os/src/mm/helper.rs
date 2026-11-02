use core::{cmp::min, mem::size_of, ptr::copy_nonoverlapping};
use crate::{config::PAGE_SIZE, task::{translate_va_to_pa, translate_va_to_pa_tr}};

/// copy_to_user
pub fn copy_to_user<T>(dst_va: usize, src: &T) -> isize {
    let mut start = dst_va;
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

/// copy_from_user
pub fn copy_from_user<T>(src_va: usize, dst: &mut T) -> isize {
    let mut from = src_va;
    let mut to = dst as *mut T as *mut u8;
    let mut left = size_of::<T>();
    while left > 0 {
        println!("src_va {}", src_va);
        let Some(cur_pa) = translate_va_to_pa_tr(from) else {
            println!("return fail");
            return -1;
        };
        let size = min(PAGE_SIZE - (cur_pa % PAGE_SIZE), left);
        unsafe {
            copy_nonoverlapping(cur_pa as *const u8, to as *mut u8, size);
            to = to.add(size);
        }
        from += size;
        left -= size;
    }
    0
}