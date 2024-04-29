use std::alloc::{GlobalAlloc, Layout};
use linked_list::LinkedList;

use crate::large_allocator::LargeAllocator;

mod avl_tree;
mod linked_list;
mod large_allocator;
mod rb_tree;
mod common;

struct Allocator<T: LargeAllocator> {
    segregated_list: [LinkedList; 7],
    mmapped_values: T
}

impl<T: LargeAllocator> Allocator<T> {

}

unsafe impl<T: LargeAllocator> GlobalAlloc for Allocator<T> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        todo!()
    }
}
