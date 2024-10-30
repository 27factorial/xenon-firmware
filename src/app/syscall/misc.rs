use crate::app::types::{Env, Error};
use crate::driver::lcd::LCD_BUFFER;
use crate::macros::{syscall, task};
use wasmi::Caller;

#[syscall]
pub extern "wasm" fn clear_buffer(caller: Caller<'_, Env>) -> Result<(), wasmi::Error> {
    caller.data().spawn(task! {
        () {
            LCD_BUFFER.lock().await.clear();
        }
    })
}

#[syscall]
pub extern "wasm" fn clone_binary_data(
    caller: Caller<'_, Env>,
    id: i32,
) -> Result<i32, wasmi::Error> {
    let mut env = caller.data().lock_data_blocking();
    let data = usize::try_from(id)
        .map_err(|_| Error::InvalidId(id))
        .and_then(|index| env.get_binary_data(index).ok_or(Error::InvalidId(id)))?
        .to_vec();

    let index = env.push_binary_data(data);

    Ok(index as i32)
}

#[syscall]
pub extern "wasm" fn drop_binary_data(
    caller: Caller<'_, Env>,
    id: i32,
) -> Result<(), wasmi::Error> {
    let mut env = caller.data().lock_data_blocking();
    let index = usize::try_from(id).map_err(|_| Error::InvalidId(id))?;

    env.remove_binary_data(index).ok_or(Error::InvalidId(id))?;

    Ok(())
}
