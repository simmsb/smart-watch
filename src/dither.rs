use smart_leds::RGB8;

const fn f32_abs(a: f32) -> f32 {
    if a < 0.0 {
        -a
    } else {
        a
    }
}

const SIGN_MASK: u32 = 0b1000_0000_0000_0000_0000_0000_0000_0000;
const EXPONENT_MASK: u32 = 0b0111_1111_1000_0000_0000_0000_0000_0000;
const MANTISSA_MASK: u32 = 0b0000_0000_0111_1111_1111_1111_1111_1111;

const fn f32_exponent_value(a: f32) -> i32 {
    let bits = (a.to_bits() & EXPONENT_MASK).overflowing_shr(23).0;

    (bits as i32) - 127
}

const fn f32_ln(a: f32) -> f32 {
    if f32_abs(a - 1.0) < f32::EPSILON {
        return 0.0;
    }

    let x_less_than_1 = a < 1.0;

    let x_working = if x_less_than_1 { 1.0 / a } else { a };

    let base2_exponent = f32_exponent_value(x_working) as u32;
    let divisor = f32::from_bits(x_working.to_bits() & EXPONENT_MASK);

    let x_working = x_working / divisor;

    let ln_1to2_polynomial = -1.741_793_9
        + (2.821_202_6
            + (-1.469_956_8 + (0.447_179_55 - 0.056_570_851 * x_working) * x_working) * x_working)
            * x_working;

    let result = (base2_exponent as f32) * core::f32::consts::LN_2 + ln_1to2_polynomial;

    if x_less_than_1 {
        -result
    } else {
        result
    }
}

const fn f32_copysign(a: f32, sign: f32) -> f32 {
    let source_bits = sign.to_bits();
    let source_sign = source_bits & SIGN_MASK;
    let signless_dest_bits = a.to_bits() & !SIGN_MASK;
    f32::from_bits(signless_dest_bits | source_sign)
}

const fn f32_fract(a: f32) -> f32 {
    let x_bits = a.to_bits();
    let exponent = f32_exponent_value(a);

    if exponent < 0 {
        return a;
    }

    let fractional_part = x_bits.overflowing_shl(exponent as u32).0 & MANTISSA_MASK;

    if fractional_part == 0 {
        return 0.0;
    }

    let exponent_shift = (fractional_part.leading_zeros() - (32 - 23)) + 1;

    let fractional_normalized = fractional_part.overflowing_shl(exponent_shift).0 & MANTISSA_MASK;

    let new_exponent_bits = (127 - exponent_shift).overflowing_shl(23).0;

    f32_copysign(f32::from_bits(fractional_normalized | new_exponent_bits), a)
}

const fn f32_trunc(a: f32) -> f32 {
    let x_bits = a.to_bits();
    let exponent = f32_exponent_value(a);

    if exponent < 0 {
        return 0.0;
    }

    let exponent_clamped = if exponent < 0 { 0 } else { exponent as u32 };

    let fractional_part = x_bits.overflowing_shl(exponent_clamped).0 & MANTISSA_MASK;

    if fractional_part == 0 {
        return a;
    }

    let fractional_mask = fractional_part.overflowing_shr(exponent_clamped).0;

    f32::from_bits(x_bits & !fractional_mask)
}

const fn f32_exp_smallx(a: f32) -> f32 {
    let total = 1.0;
    let total = 1.0 + (a / 4.0) * total;
    let total = 1.0 + (a / 3.0) * total;
    let total = 1.0 + (a / 2.0) * total;
    let total = 1.0 + (a / 1.0) * total;
    total
}

const fn f32_set_exponent(a: f32, exponent: i32) -> f32 {
    let without_exponent = a.to_bits() & !EXPONENT_MASK;
    let only_exponent = ((exponent + 127) as u32).overflowing_shl(23).0;

    f32::from_bits(without_exponent | only_exponent)
}

const fn f32_exp(a: f32) -> f32 {
    if a == 0.0 {
        return 1.0;
    }

    if f32_abs(a - 1.0) < f32::EPSILON {
        return core::f32::consts::E;
    }

    if f32_abs(a - -1.0) < f32::EPSILON {
        return 1.0 / core::f32::consts::E;
    }

    let x_ln2recip = a * core::f32::consts::LOG2_E;
    let x_fract = f32_fract(x_ln2recip);
    let x_trunc = f32_trunc(x_ln2recip);

    let x_fract = x_fract * core::f32::consts::LN_2;
    let fract_exp = f32_exp_smallx(x_fract);

    let fract_exponent = f32_exponent_value(fract_exp).saturating_add(x_trunc as i32);

    if fract_exponent < -127 {
        return 0.0;
    }

    if fract_exponent > 127 {
        return f32::INFINITY;
    }

    f32_set_exponent(fract_exp, fract_exponent)
}

const fn f32_powf(a: f32, n: f32) -> f32 {
    if a > 0.0 {
        f32_exp(n * f32_ln(a))
    } else if a == 0.0 {
        return 0.0;
    } else {
        panic!("no")
    }
}

const fn temporal_dither_for_level<const STEPS: usize>(level: f32, gamma: f32) -> [u8; STEPS] {
    let gamma_corrected = f32_powf(level / 255.0, gamma) * 255.0 + 0.3;

    let up_count = (f32_fract(gamma_corrected) * STEPS as f32) as usize;
    let down_count = STEPS - up_count;

    let floor = f32_trunc(gamma_corrected) as u8;

    let mut result = [floor; STEPS];

    let mut ups = 1;
    let mut downs = 1;

    let mut i = 0;
    while i < STEPS {
        if ups * down_count < downs * up_count {
            ups += 1;
            result[i] = result[i].saturating_add(1);
        } else {
            downs += 1;
        }
        i += 1;
    }

    result
}

const fn gen_gamma_dither<const STEPS: usize>(gamma: f32) -> [[u8; STEPS]; 256] {
    let mut result = [[0u8; STEPS]; 256];

    let mut i = 1;
    while i < 256 {
        result[i] = temporal_dither_for_level::<STEPS>(i as f32, gamma);
        i += 1;
    }

    result
}

pub struct GammaDither<const STEPS: usize, const GAMMA: u32>;

impl<const STEPS: usize, const GAMMA: u32> GammaDither<STEPS, GAMMA> {
    const MAP: [[u8; STEPS]; 256] = gen_gamma_dither::<STEPS>(GAMMA as f32 / 10.0);

    pub fn dither(step: usize, it: impl Iterator<Item = RGB8>) -> impl Iterator<Item = RGB8> {
        it.map(move |c| {
            RGB8::new(
                Self::MAP[c.r as usize][step],
                Self::MAP[c.g as usize][step],
                Self::MAP[c.b as usize][step],
            )
        })
    }
}
