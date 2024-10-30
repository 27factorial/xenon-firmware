use crate::app::types::Executor as WasmExecutor;
use crate::macros::make_static;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::{task, SendSpawner};
use embassy_time::{Instant, Timer};
use esp_hal::cpu_control::{AppCoreGuard, CpuControl, Stack};
use esp_hal::interrupt::Priority;
use esp_hal::peripherals::CPU_CTRL;
use esp_hal::rng::Trng;
use esp_hal::Cpu;
use esp_hal_embassy::{Executor, InterruptExecutor};
use static_cell::StaticCell;

const STACK_SIZE: usize = 32 * 1024;
const APP_SWI: u8 = 0;
const REACTOR_SWI: u8 = 1;

static RUNNING: AtomicBool = AtomicBool::new(false);
static STACK: StaticCell<Stack<STACK_SIZE>> = StaticCell::new();
static WASM_MODULE: &[u8] = include_bytes!("../../assets/xenon-test-app.wasm");

#[task]
async fn start(rng: Trng<'static>, reactor_spawner: SendSpawner) {
    let start = Instant::now();
    // TODO: Load wasm module from "filesystem" and handle errors more gracefully.
    let mut wasm_executor = match WasmExecutor::new(rng, reactor_spawner, WASM_MODULE) {
        Ok(ex) => ex,
        Err(e) => {
            log::error!("failed to create wasm executor: {e}");
            return;
        }
    };

    // Timer::after_secs(1).await;

    log::debug!(
        "wasm engine startup time was {}ms",
        start.elapsed().as_millis()
    );

    if let Err(e) = wasm_executor.run().await {
        log::error!("wasm error occurred: {e}");
    }
}

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

    pub fn start(
        &mut self,
        rng: Trng<'static>,
        reactor_spawner: SendSpawner,
    ) {
        if RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let stack = STACK.init(Stack::new());
            let guard = self
                .control
                .start_app_core(stack, move || {
                    Self::cpu_main(rng, reactor_spawner)
                })
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

    fn cpu_main(
        rng: Trng<'static>,
        spawner: SendSpawner,
    ) {
        let app_executor = make_static!(
            Executor,
            Executor::new()
        );

        app_executor.run(move |app_spawner| app_spawner.must_spawn(start(rng, spawner)))
    }
}
