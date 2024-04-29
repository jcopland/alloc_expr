use std::alloc::Layout;

pub unsafe trait LargeAllocator {
    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8;
    unsafe fn dealloc(&mut self, ptr: *mut u8);
    unsafe fn realloc(&mut self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8;
}