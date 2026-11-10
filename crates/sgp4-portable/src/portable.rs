//! Float math through the `libm` crate, whatever features `num-traits` has.

pub(crate) trait Float: Sized {
    fn p_sin(self) -> Self;
    fn p_cos(self) -> Self;
    fn p_atan2(self, other: Self) -> Self;
    fn p_powf(self, n: Self) -> Self;
    fn p_powi(self, n: i32) -> Self;
}

impl Float for f64 {
    fn p_sin(self) -> Self {
        libm::sin(self)
    }
    fn p_cos(self) -> Self {
        libm::cos(self)
    }
    fn p_atan2(self, other: Self) -> Self {
        libm::atan2(self, other)
    }
    fn p_powf(self, n: Self) -> Self {
        libm::pow(self, n)
    }
    /// compiler-rt's `__powidf2`: square and multiply.
    fn p_powi(self, n: i32) -> Self {
        let recip = n < 0;
        let mut b = n;
        let mut a = self;
        let mut r = 1.0;
        loop {
            if b & 1 != 0 {
                r *= a;
            }
            b /= 2;
            if b == 0 {
                break;
            }
            a *= a;
        }
        if recip {
            1.0 / r
        } else {
            r
        }
    }
}

pub(crate) trait Euclid: Sized {
    fn rem_euclid(&self, v: &Self) -> Self;
}

impl Euclid for f64 {
    fn rem_euclid(&self, v: &Self) -> Self {
        let r = libm::fmod(*self, *v);
        if r < 0.0 {
            r + libm::fabs(*v)
        } else {
            r
        }
    }
}
