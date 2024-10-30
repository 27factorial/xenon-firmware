use crate::app::types::{Env, Error, PollRequest, WakerFunc};
use crate::macros::syscall;
use crate::macros::task;
use embassy_time::{Duration, Instant, Timer};
use wasmi::{Caller, Func};

#[syscall]
pub extern "wasm" fn wait(caller: Caller<'_, Env>) -> Result<(), wasmi::Error> {
    let mut env_data = caller.data().lock_data_blocking();

    // After calling `resume` on the resumable Wasm function, wasmi will resume here. The
    // `notified` flag is used to ensure we don't get into an infinite loop of returning
    // `PollRequest::Wait`.
    if env_data.notified() {
        env_data.set_notified(false);
        Ok(())
    } else {
        Err(PollRequest::Wait.into())
    }
}

#[syscall]
pub extern "wasm" fn poll(caller: Caller<'_, Env>) -> Result<(), wasmi::Error> {
    let mut env_data = caller.data().lock_data_blocking();

    if env_data.notified() {
        env_data.set_notified(false);
        Ok(())
    } else {
        Err(PollRequest::Poll.into())
    }
}
