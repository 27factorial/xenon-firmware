use crate::app::types::{Env, Error};
use crate::macros::cvt;
use log::Level as LogLevel;
use wasmi::Caller;

const LOG_LEVEL_ERROR: u32 = 1;
const LOG_LEVEL_WARN: u32 = 2;
const LOG_LEVEL_INFO: u32 = 3;
const LOG_LEVEL_DEBUG: u32 = 4;
const LOG_LEVEL_TRACE: u32 = 5;

pub fn print(
    caller: Caller<'_, Env>,
    ptr: u32,
    len: u32,
    newline: u32,
) -> Result<(), wasmi::Error> {
    let (ptr, len, newline) = cvt!(ptr as usize, len as usize, newline as bool);
    let end = ptr + len;
    let new_line = if newline { "\n" } else { "" };

    let memory = caller.data().lock_sync().memory();
    let range = memory
        .data(&caller)
        .get(ptr..end)
        .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

    let string = core::str::from_utf8(range).map_err(|e| Error::InvalidUtf8 {
        start: ptr,
        len,
        valid_up_to: e.valid_up_to(),
    })?;

    esp_println::print!("{string}{new_line}");

    Ok(())
}

pub fn log(caller: Caller<'_, Env>, level: u32, ptr: u32, len: u32) -> Result<(), wasmi::Error> {
    let (ptr, len) = cvt!(ptr as usize, len as usize);
    let end = ptr + len;

    let level = match level {
        LOG_LEVEL_ERROR => LogLevel::Error,
        LOG_LEVEL_WARN => LogLevel::Warn,
        LOG_LEVEL_INFO => LogLevel::Info,
        LOG_LEVEL_DEBUG => LogLevel::Debug,
        LOG_LEVEL_TRACE => LogLevel::Trace,
        unknown => return Err(Error::InvalidLogLevel(unknown).into()),
    };

    let memory = caller.data().lock_sync().memory();
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
