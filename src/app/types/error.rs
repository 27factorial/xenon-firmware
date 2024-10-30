use alloc::string::String;
use core::result;
use thiserror::Error;
use wasmi::core::HostError;

pub type Result<T> = result::Result<T, wasmi::Error>;

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Error)]
pub enum Error {
    #[error("wasm module did not export linear memory with the name `memory`")]
    NoMemory,
    #[error("wasm module did not export a table with the name `__indirect_function_table`")]
    NoFunctionTable,
    #[error("function reference in function table was null")]
    NullFunction,
    #[error("invalid value for type {0}")]
    InvalidValue(&'static str),
    #[error(
        "memory range [{}, {}) is invalid UTF-8 (valid up to index {} in range)", 
        start, start + len, valid_up_to,
    )]
    InvalidUtf8 {
        start: usize,
        len: usize,
        valid_up_to: usize,
    },
    #[error("invalid memory range [{}, {})", start, start + end)]
    InvalidMemoryRange { start: usize, end: usize },
    #[error("invalid log level {0}")]
    InvalidLogLevel(u32),
    #[error("invalid data id {0}")]
    InvalidId(i32),
    #[error("attempted to spawn too many tasks")]
    TooManyTasks,
    #[error("undefined behavior: mismatched critical section release")]
    MismatchedCriticalSection,
    #[error("wasm module panicked")]
    Panicked,
}

impl From<Error> for wasmi::Error {
    fn from(value: Error) -> Self {
        wasmi::Error::host(value)
    }
}

impl HostError for Error {}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Error)]
pub enum PollRequest {
    #[error("`wait` syscall called")]
    Wait,
    #[error("`handle_io` syscall called")]
    Poll,
}

impl From<PollRequest> for wasmi::Error {
    fn from(value: PollRequest) -> Self {
        wasmi::Error::host(value)
    }
}

impl HostError for PollRequest {}
