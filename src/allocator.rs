use core::alloc::{GlobalAlloc, Layout};
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull};
use critical_section as cs;
use esp_hal::macros::ram;
use linked_list_allocator::Heap;
use spin::mutex::TicketMutex;
use spin::Lazy;
use static_cell::ConstStaticCell;

const WIFI_HEAP_BYTES: usize = 1 << 17; // 128 KiB

#[global_allocator]
pub static ALLOCATOR: Allocator = Allocator::new();

#[used]
static WIFI_ALLOCATOR: Lazy<Allocator> = Lazy::new(|| {
    #[ram]
    #[used]
    static WIFI_HEAP: ConstStaticCell<WifiHeap> =
        ConstStaticCell::new(WifiHeap([MaybeUninit::uninit(); WIFI_HEAP_BYTES]));

    let allocator = Allocator::new();
    let wifi_heap = WIFI_HEAP.take();

    unsafe {
        allocator.init(wifi_heap.0.as_mut_ptr().cast(), wifi_heap.0.len());
    }

    allocator
});

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

#[repr(C, align(4))]
struct WifiHeap([MaybeUninit<u8>; WIFI_HEAP_BYTES]);

// esp-wifi required functions

#[no_mangle]
pub extern "C" fn esp_wifi_free_internal_heap() -> usize {
    WIFI_ALLOCATOR.free()
}

#[no_mangle]
pub extern "C" fn esp_wifi_allocate_from_internal_ram(size: usize) -> *mut u8 {
    unsafe { WIFI_ALLOCATOR.alloc(Layout::from_size_align(size, 4).expect("valid size")) }
}
