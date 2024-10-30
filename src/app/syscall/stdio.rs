use crate::app::types::{Env, Error};
use crate::macros::syscall;
use esp_println::print;
use log::Level as LogLevel;
use wasmi::Caller;

const LOG_LEVEL_ERROR: u32 = 1;
const LOG_LEVEL_WARN: u32 = 2;
const LOG_LEVEL_INFO: u32 = 3;
const LOG_LEVEL_DEBUG: u32 = 4;
const LOG_LEVEL_TRACE: u32 = 5;

#[syscall]
pub extern "wasm" fn print(
    caller: Caller<'_, Env>,
    ptr: usize,
    len: usize,
    newline: bool,
) -> Result<(), wasmi::Error> {
    let end = ptr + len;
    let new_line = if newline { "\n" } else { "" };

    let memory = caller.data().lock_data_blocking().memory();
    let range = memory
        .data(&caller)
        .get(ptr..end)
        .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

    let string = core::str::from_utf8(range).map_err(|e| Error::InvalidUtf8 {
        start: ptr,
        len,
        valid_up_to: e.valid_up_to(),
    })?;

    print!("{string}{new_line}");

    Ok(())
}

#[syscall]
pub extern "wasm" fn log(
    caller: Caller<'_, Env>,
    level: u32,
    ptr: usize,
    len: usize,
) -> Result<(), wasmi::Error> {
    let end = ptr + len;

    let level = match level {
        LOG_LEVEL_ERROR => LogLevel::Error,
        LOG_LEVEL_WARN => LogLevel::Warn,
        LOG_LEVEL_INFO => LogLevel::Info,
        LOG_LEVEL_DEBUG => LogLevel::Debug,
        LOG_LEVEL_TRACE => LogLevel::Trace,
        unknown => return Err(Error::InvalidLogLevel(unknown).into()),
    };

    let memory = caller.data().lock_data_blocking().memory();
    let range = memory
        .data(&caller)
        .get(ptr..end)
        .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

    let string = core::str::from_utf8(range).map_err(|e| Error::InvalidUtf8 {
        start: ptr,
        len,
        valid_up_to: e.valid_up_to(),
    })?;

    log::log!(level, "{string}");

    Ok(())
}
