use std::ptr::NonNull;

use libc::{MAP_ANON, MAP_PRIVATE, PROT_READ, PROT_WRITE};

pub const PAGE_SIZE: usize = 4096;

pub unsafe fn request_memory(length: usize) -> NonNull<u8> {
    let protections = PROT_READ | PROT_WRITE;
    let flags = MAP_ANON | MAP_PRIVATE;

    match libc::mmap(core::ptr::null_mut(), length, protections, flags, -1, 0) {
        // todo: I should probably use AllocationError on nightly here
        libc::MAP_FAILED => panic!("Failed to request memory!"),
        address => NonNull::new_unchecked(address).cast()
    }

}
