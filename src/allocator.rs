use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{self, NonNull};
use critical_section as cs;
use linked_list_allocator::Heap;
use spin::mutex::TicketMutex;

#[global_allocator]
pub static ALLOCATOR: Allocator = Allocator::new();

pub struct Allocator(TicketMutex<Heap>);

impl Allocator {
    const fn new() -> Self {
        Self(TicketMutex::new(Heap::empty()))
    }

    /// # Safety
    /// 1. This method must be called exactly once.
    /// 2. Pointers in the range `heap_bottom..heap_bottom.byte_add(size)` must be valid for reads and writes,
    ///    assuming proper alignment for the type being read/written.
    /// 2. The provided pointer and memory range must also be valid for the `'static`
    ///    lifetime and not used anywhere else.
    pub unsafe fn init(&self, heap_bottom: *mut u8, size: usize) {
        cs::with(|_| unsafe { self.0.lock().init(heap_bottom, size) })
    }

    pub fn used(&self) -> usize {
        cs::with(|_| self.0.lock().used())
    }

    pub fn free(&self) -> usize {
        cs::with(|_| self.0.lock().free())
    }
}

impl Default for Allocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        cs::with(|_| {
            self.0
                .lock()
                .allocate_first_fit(layout)
                .map(|nonnull| nonnull.as_ptr())
                .unwrap_or(ptr::null_mut())
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        cs::with(|_| unsafe {
            self.0
                .lock()
                .deallocate(NonNull::new_unchecked(ptr), layout)
        })
    }
}
