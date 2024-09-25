use crate::{app::types::Error, macros::syscalls};

syscalls! {
    pub extern "wasm" fn panic(
        caller,
        ptr: usize,
        len: usize,
    ) -> Result<(), wasmi::Error> {
        let memory = caller.data().lock_sync().memory();
        let end = ptr + len;

        let bytes = memory
            .data(&caller)
            .get(ptr..end)
            .ok_or(Error::InvalidMemoryRange { start: ptr, end})?;

        let message = core::str::from_utf8(bytes).map_err(|e| Error::InvalidUtf8 {
            start: ptr,
            len,
            valid_up_to: e.valid_up_to(),
        })?;

        log::error!(
            "Wasm app panicked! message: {message}\n"
        );

        Ok(())
    }
}
