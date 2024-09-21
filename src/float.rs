use libm::Libm;

/// A trait implementing most of the floating point operations defined in `std` which are missing in
/// `core` by using `libm`.
///
/// # Notes
/// 1. `powi` is not defined because `libm` does not supply an equivalent function, and it's defined
///    with an intrinsic in `std`.
/// 2. The result(s) of a given method may not exactly match the result(s) of the equivalent method
///    in `std` due to possible precision differences between `std` and `libm`.
pub trait FloatExt: Copy + Sized {
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn round(self) -> Self;
    fn round_ties_even(self) -> Self;
    fn trunc(self) -> Self;
    fn fract(self) -> Self;
    fn abs(self) -> Self;
    fn signum(self) -> Self;
    fn copysign(self, sign: Self) -> Self;
    fn mul_add(self, a: Self, b: Self) -> Self;
    fn div_euclid(self, rhs: Self) -> Self;
    fn rem_euclid(self, rhs: Self) -> Self;
    fn pow(self, n: Self) -> Self;
    fn sqrt(self) -> Self;
    fn cbrt(self) -> Self;
    fn exp(self) -> Self;
    fn exp2(self) -> Self;
    fn exp10(self) -> Self;
    fn ln(self) -> Self;
    fn log(self, base: Self) -> Self;
    fn log2(self) -> Self;
    fn log10(self) -> Self;
    fn hypot(self, other: Self) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn tan(self) -> Self;
    fn asin(self) -> Self;
    fn acos(self) -> Self;
    fn atan(self) -> Self;
    fn atan2(self, other: Self) -> Self;
    fn exp_m1(self) -> Self;
    fn ln_1p(self) -> Self;
    fn sinh(self) -> Self;
    fn cosh(self) -> Self;
    fn tanh(self) -> Self;
    fn asinh(self) -> Self;
    fn acosh(self) -> Self;
    fn atanh(self) -> Self;
    fn gamma(self) -> Self;
    fn ln_gamma(self) -> (Self, i32);

    fn sin_cos(self) -> (Self, Self) {
        (self.sin(), self.cos())
    }
}

impl FloatExt for f32 {
    fn floor(self) -> f32 {
        Libm::<f32>::floor(self)
    }

    fn ceil(self) -> f32 {
        Libm::<f32>::ceil(self)
    }

    fn round(self) -> f32 {
        Libm::<f32>::round(self)
    }

    fn round_ties_even(self) -> f32 {
        Libm::<f32>::rint(self)
    }

    fn trunc(self) -> f32 {
        Libm::<f32>::modf(self).1
    }

    fn fract(self) -> f32 {
        Libm::<f32>::modf(self).0
    }

    fn abs(self) -> f32 {
        Libm::<f32>::fabs(self)
    }

    fn signum(self) -> f32 {
        // Taken from std::f32::signum
        if self.is_nan() {
            f32::NAN
        } else {
            1.0f32.copysign(self)
        }
    }

    fn copysign(self, sign: f32) -> f32 {
        Libm::<f32>::copysign(self, sign)
    }

    fn mul_add(self, a: f32, b: f32) -> f32 {
        Libm::<f32>::fma(self, a, b)
    }

    fn div_euclid(self, rhs: f32) -> f32 {
        // Taken from std::f32::div_euclid
        let q = (self / rhs).trunc();
        if self % rhs < 0.0 {
            return if rhs > 0.0 { q - 1.0 } else { q + 1.0 };
        }
        q
    }

    fn rem_euclid(self, rhs: f32) -> f32 {
        // Taken from std::f32::rem_euclid
        let r = self % rhs;
        if r < 0.0 {
            r + rhs.abs()
        } else {
            r
        }
    }

    fn pow(self, n: f32) -> f32 {
        Libm::<f32>::pow(self, n)
    }

    fn sqrt(self) -> f32 {
        Libm::<f32>::sqrt(self)
    }

    fn cbrt(self) -> f32 {
        Libm::<f32>::cbrt(self)
    }

    fn exp(self) -> f32 {
        Libm::<f32>::exp(self)
    }

    fn exp2(self) -> f32 {
        Libm::<f32>::exp2(self)
    }

    fn exp10(self) -> f32 {
        Libm::<f32>::exp10(self)
    }

    fn ln(self) -> f32 {
        Libm::<f32>::log(self)
    }

    fn log(self, base: f32) -> f32 {
        Libm::<f32>::log(self) / Libm::<f32>::log(base)
    }

    fn log2(self) -> f32 {
        Libm::<f32>::log2(self)
    }

    fn log10(self) -> f32 {
        Libm::<f32>::log10(self)
    }

    fn hypot(self, other: f32) -> f32 {
        Libm::<f32>::hypot(self, other)
    }

    fn sin(self) -> f32 {
        Libm::<f32>::sin(self)
    }

    fn cos(self) -> f32 {
        Libm::<f32>::cos(self)
    }

    fn tan(self) -> f32 {
        Libm::<f32>::tan(self)
    }

    fn asin(self) -> f32 {
        Libm::<f32>::asin(self)
    }

    fn acos(self) -> f32 {
        Libm::<f32>::acos(self)
    }

    fn atan(self) -> f32 {
        Libm::<f32>::atan(self)
    }

    fn atan2(self, other: f32) -> f32 {
        Libm::<f32>::atan2(self, other)
    }

    fn exp_m1(self) -> f32 {
        Libm::<f32>::expm1(self)
    }

    fn ln_1p(self) -> f32 {
        Libm::<f32>::log1p(self)
    }

    fn sinh(self) -> f32 {
        Libm::<f32>::sinh(self)
    }

    fn cosh(self) -> f32 {
        Libm::<f32>::cosh(self)
    }

    fn tanh(self) -> f32 {
        Libm::<f32>::tanh(self)
    }

    fn asinh(self) -> f32 {
        Libm::<f32>::asinh(self)
    }

    fn acosh(self) -> f32 {
        Libm::<f32>::acosh(self)
    }

    fn atanh(self) -> f32 {
        Libm::<f32>::atanh(self)
    }

    fn gamma(self) -> f32 {
        Libm::<f32>::tgamma(self)
    }

    fn ln_gamma(self) -> (f32, i32) {
        Libm::<f32>::lgamma_r(self)
    }
}

impl FloatExt for f64 {
    fn floor(self) -> f64 {
        Libm::<f64>::floor(self)
    }

    fn ceil(self) -> f64 {
        Libm::<f64>::ceil(self)
    }

    fn round(self) -> f64 {
        Libm::<f64>::round(self)
    }

    fn round_ties_even(self) -> f64 {
        Libm::<f64>::rint(self)
    }

    fn trunc(self) -> f64 {
        Libm::<f64>::modf(self).1
    }

    fn fract(self) -> f64 {
        Libm::<f64>::modf(self).0
    }

    fn abs(self) -> f64 {
        Libm::<f64>::fabs(self)
    }

    fn signum(self) -> f64 {
        // Taken from std::f64::signum
        if self.is_nan() {
            f64::NAN
        } else {
            1.0f64.copysign(self)
        }
    }

    fn copysign(self, sign: f64) -> f64 {
        Libm::<f64>::copysign(self, sign)
    }

    fn mul_add(self, a: f64, b: f64) -> f64 {
        Libm::<f64>::fma(self, a, b)
    }

    fn div_euclid(self, rhs: f64) -> f64 {
        // Taken from std::f64::div_euclid
        let q = (self / rhs).trunc();
        if self % rhs < 0.0 {
            return if rhs > 0.0 { q - 1.0 } else { q + 1.0 };
        }
        q
    }

    fn rem_euclid(self, rhs: Self) -> Self {
        // Taken from std::f64::rem_euclid
        let r = self % rhs;
        if r < 0.0 {
            r + rhs.abs()
        } else {
            r
        }
    }

    fn pow(self, n: f64) -> f64 {
        Libm::<f64>::pow(self, n)
    }

    fn sqrt(self) -> f64 {
        Libm::<f64>::sqrt(self)
    }

    fn cbrt(self) -> f64 {
        Libm::<f64>::cbrt(self)
    }

    fn exp(self) -> f64 {
        Libm::<f64>::exp(self)
    }

    fn exp2(self) -> f64 {
        Libm::<f64>::exp2(self)
    }

    fn exp10(self) -> f64 {
        Libm::<f64>::exp10(self)
    }

    fn ln(self) -> f64 {
        Libm::<f64>::log(self)
    }

    fn log(self, base: f64) -> f64 {
        Libm::<f64>::log(self) / Libm::<f64>::log(base)
    }

    fn log2(self) -> f64 {
        Libm::<f64>::log2(self)
    }

    fn log10(self) -> f64 {
        Libm::<f64>::log10(self)
    }

    fn hypot(self, other: f64) -> f64 {
        Libm::<f64>::hypot(self, other)
    }

    fn sin(self) -> f64 {
        Libm::<f64>::sin(self)
    }

    fn cos(self) -> f64 {
        Libm::<f64>::cos(self)
    }

    fn tan(self) -> f64 {
        Libm::<f64>::tan(self)
    }

    fn asin(self) -> f64 {
        Libm::<f64>::asin(self)
    }

    fn acos(self) -> f64 {
        Libm::<f64>::acos(self)
    }

    fn atan(self) -> f64 {
        Libm::<f64>::atan(self)
    }

    fn atan2(self, other: f64) -> f64 {
        Libm::<f64>::atan2(self, other)
    }

    fn exp_m1(self) -> f64 {
        Libm::<f64>::expm1(self)
    }

    fn ln_1p(self) -> f64 {
        Libm::<f64>::log1p(self)
    }

    fn sinh(self) -> f64 {
        Libm::<f64>::sinh(self)
    }

    fn cosh(self) -> f64 {
        Libm::<f64>::cosh(self)
    }

    fn tanh(self) -> f64 {
        Libm::<f64>::tanh(self)
    }

    fn asinh(self) -> f64 {
        Libm::<f64>::asinh(self)
    }

    fn acosh(self) -> f64 {
        Libm::<f64>::acosh(self)
    }

    fn atanh(self) -> f64 {
        Libm::<f64>::atanh(self)
    }

    fn gamma(self) -> f64 {
        Libm::<f64>::tgamma(self)
    }

    fn ln_gamma(self) -> (f64, i32) {
        Libm::<f64>::lgamma_r(self)
    }
}
