use core::any::type_name;
use thiserror::Error;
use wasmi::WasmTy;

pub trait TryFromWasm: Sized {
    type WasmTy: WasmTy;

    fn try_from_wasm(value: Self::WasmTy) -> Result<Self, InvalidValueError>;
}

macro_rules! int_try_from_wasm {
    ($($wasm_ty:ty as $into_ty:ty),* $(,)?) => {
        $(
            impl TryFromWasm for $into_ty {
                type WasmTy = $wasm_ty;

                fn try_from_wasm(value: Self::WasmTy) -> Result<Self, InvalidValueError> {
                    <Self as TryFrom<_>>::try_from(value)
                        .map_err(|_| InvalidValueError(core::any::type_name::<Self>()))
                }
            }
        )*

    };
}

int_try_from_wasm! {
    u32 as u8,
    u32 as u16,
    u32 as u32,
    u32 as usize,
    u64 as u64,
    i32 as i8,
    i32 as i16,
    i32 as i32,
    i32 as isize,
    i64 as i64,
    f32 as f32,
    f64 as f64,
}

impl TryFromWasm for bool {
    type WasmTy = u32;

    fn try_from_wasm(value: Self::WasmTy) -> Result<Self, InvalidValueError> {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(InvalidValueError(type_name::<bool>())),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Error)]
#[error("invalid value for type {0}")]
pub struct InvalidValueError(pub &'static str);
