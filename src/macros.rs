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

/// Creates an inline embassy spawn token.
macro_rules! task {
    (
        (
            $($arg:ident : $arg_ty:ty $(= $arg_expr:expr)? ),* $(,)?
        )  { $($tt:tt)* }
    ) => {{
        #[embassy_executor::task]
        #[inline(always)]
        async fn __xenon_anon_task(
            $($arg : $arg_ty),*
        ) { $($tt)* }

        $(
            $(let $arg = $arg_expr;)?
        )*

        __xenon_anon_task($($arg),*)
    }}
}


pub(crate) use xenon_proc_macros::syscall;
pub(crate) use {make_static, singleton, task};
