use crate::app::types::{Env, Error};
use crate::macros::syscall;
use wasmi::Caller;

#[syscall]
pub extern "wasm" fn panic(
    caller: Caller<'_, Env>,
    ptr: usize,
    len: usize,
) -> Result<(), wasmi::Error> {
    let memory = caller.data().lock_data_blocking().memory();
    let end = ptr + len;

    let bytes = memory
        .data(&caller)
        .get(ptr..end)
        .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

    let message = core::str::from_utf8(bytes).map_err(|e| Error::InvalidUtf8 {
        start: ptr,
        len,
        valid_up_to: e.valid_up_to(),
    })?;

    log::error!(target: "Wasm executor", "app panicked! message: {message}\n");

    Ok(())
}
