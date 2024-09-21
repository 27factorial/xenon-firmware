macro_rules! make_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

macro_rules! singleton {
    ($flag:expr, $msg:literal, $f:expr) => {
        if $flag
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            $f()
        } else {
            panic!($msg)
        }
    };
}

macro_rules! cvt {
    ( $($v:tt as $t:tt),* $(,)? ) => {{
        let x = ($(
            <$t as $crate::app::wasm::convert::TryFromWasm>::try_from_wasm($v)
                .map_err(|e| $crate::app::wasm::types::Error::InvalidValue(e.0))?
        ),*);

        x
    }};
}

macro_rules! syscalls {
    (
        $(
            $(#[$meta:meta])*
            $vis:vis extern "wasm" fn $name:ident (
                $( $param:ident $( : $cvt_ty:tt )? ),* $(,)?
            ) $(-> $ret:ty )? $code:block
        )*
    ) => {
        $(
            syscalls! {
                $(#[$meta])*
                $vis fn $name (
                    $( $param $( : $cvt_ty )?),*
                ) $(-> $ret )? $code
            }
        )*
    };
    (
        $(#[$meta:meta])*
        $vis:vis fn $name:ident (
            $caller:ident
        ) $(-> $ret:ty )? $code:block
    ) => {
        syscalls! {
            $(#[$meta])*
            $vis fn $name (
                $caller,
            ) $(-> $ret )? $code
        }
    };
    (
        $(#[$meta:meta])*
        $vis:vis fn $name:ident (
            $caller:ident,
            $($param:ident : $cvt_ty:tt),*
        ) $(-> $ret:ty )? $code:block
    ) => {
        $(#[$meta])*
        #[allow(unused_parens, clippy::too_many_arguments)]
        $vis fn $name (
            #[allow(unused_variables, unused_mut)]
            mut $caller: wasmi::Caller<'_, $crate::app::wasm::types::Env>,
            $(
                $param : <$cvt_ty as $crate::app::wasm::convert::TryFromWasm>::WasmTy
            ),*
         ) $( -> $ret)? {
            let (
                $(
                    $param
                ),*
            ) = $crate::macros::cvt!(
                $(
                    $param as $cvt_ty
                ),*
            );

            $code
         }
    };
}

pub(crate) use {cvt, make_static, singleton, syscalls};
