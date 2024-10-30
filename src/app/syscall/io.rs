use crate::app::types::{Env, Error, Registration, WakerFunc};
use crate::macros::task;
use embassy_time::{Duration, Instant, Timer};
use esp_println::dbg;
use wasmi::Caller;
use xenon_proc_macros::syscall;

#[syscall]
pub extern "wasm" fn schedule_timer(
    caller: Caller<'_, Env>,
    func_index: u32,
    data: u32,
    micros: u64,
) -> Result<(), wasmi::Error> {
    let env = caller.data();
    let env_data = env.lock_data_blocking();

    let wake_func = env_data
        .get_func(&caller, func_index)
        .func()
        .ok_or(Error::NullFunction)?
        .typed::<u32, ()>(&caller)?;

    let deadline = Instant::from_micros(micros);

    env.spawn(task! {
        (
            env: Env = env.clone(), 
            wake_func: WakerFunc,
            data: u32,
            deadline: Instant
        ) {
            Timer::after_secs(1).await;
            env.push_registration(Registration::new_timer(deadline, data, wake_func)).await;
        }
    })
}

#[syscall]
pub extern "wasm" fn schedule_io(
    caller: Caller<'_, Env>,
    wake_index: u32,
    id: i32,
    readable: bool,
    writable: bool,
) -> Result<(), wasmi::Error> {
    todo!()
}
