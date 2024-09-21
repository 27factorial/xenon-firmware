use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use esp_hal::cpu_control::{AppCoreGuard, CpuControl, Stack};
use esp_hal::interrupt::Priority;
use esp_hal::peripherals::CPU_CTRL;
use esp_hal::rng::Trng;
use esp_hal::system::SoftwareInterrupt;
use esp_hal::Cpu;
use esp_hal_embassy::InterruptExecutor;
use static_cell::StaticCell;

use crate::app::wasm;

const STACK_SIZE: usize = 32 * 1024;
const APP_SWI: u8 = 0;

static RUNNING: AtomicBool = AtomicBool::new(false);
static STACK: StaticCell<Stack<STACK_SIZE>> = StaticCell::new();

#[clippy::has_significant_drop]
pub struct AppCpu<'a> {
    control: CpuControl<'static>,
    guard: Option<AppCoreGuard<'a>>,
    _not_send_sync: PhantomData<*mut ()>,
}

impl<'a> AppCpu<'a> {
    pub fn new(ctrl: CPU_CTRL) -> Self {
        Self {
            control: CpuControl::new(ctrl),
            guard: None,
            _not_send_sync: PhantomData,
        }
    }

    pub fn start(&mut self, rng: Trng<'static>, interrupt: SoftwareInterrupt<APP_SWI>) {
        if RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let stack = STACK.init(Stack::new());
            let guard = self
                .control
                .start_app_core(stack, move || Self::cpu_main(rng, interrupt))
                .unwrap();

            self.guard = Some(guard);
        } else {
            panic!("attempted to start application core more than once")
        }
    }

    pub fn park(&mut self) {
        assert_ne!(
            esp_hal::get_core(),
            Cpu::AppCpu,
            "the application core can only be parked from the main core"
        );

        unsafe {
            self.control.park_core(Cpu::AppCpu);
        }
    }

    pub fn unpark(&mut self) {
        self.control.unpark_core(Cpu::AppCpu)
    }

    fn cpu_main(rng: Trng<'static>, interrupt: SoftwareInterrupt<APP_SWI>) -> ! {
        static APP_EXECUTOR: StaticCell<InterruptExecutor<APP_SWI>> = StaticCell::new();
        let async_executor = APP_EXECUTOR.init(InterruptExecutor::new(interrupt));
        let spawner = async_executor.start(Priority::max());

        if let Err(e) = wasm::start(rng, spawner) {
            log::error!("app error occurred: {e}")
        }

        #[allow(clippy::empty_loop)]
        loop {}
    }
}
