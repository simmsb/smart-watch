#![feature(const_fn_floating_point_arithmetic)]
#![feature(const_float_bits_conv)]
#![feature(adt_const_params)]

use std::time::Duration;

use esp_idf_hal::gpio::{Gpio27, Output};
use esp_idf_hal::prelude::*;
use esp_idf_hal::rmt::config::TransmitConfig;
use esp_idf_hal::rmt::{FixedLengthSignal, HwChannel, PinState, Pulse, Transmit};
use esp_idf_sys as _;
use eyre::{eyre, Result};
use smart_leds::SmartLedsWrite;

use crate::dither::GammaDither;

mod dither;
mod leds;

fn leds(pin: Gpio27<Output>, rmt: impl HwChannel) -> Result<()> {
    fn conv_colour(c: cichlid::ColorRGB) -> smart_leds::RGB8 {
        smart_leds::RGB8::new(c.r, c.g, c.b)
    }

    let mut leds = leds::Esp32Neopixel::new(pin, rmt)?;

    const STEPS: usize = 8;
    const LEDS: u8 = 25;

    let mut i = 0u16;

    loop {
        // for step in 0..STEPS {
            let it = (0..LEDS).map(|x| {
                let v = cichlid::HSV {
                    h: ((i / 4) as u8).wrapping_add(x * 10),
                    s: 10,
                    v: 10,
                };
                conv_colour(v.to_rgb_rainbow())
            });

            // let _ = leds.write(GammaDither::<STEPS, 28>::dither(step, it));
            let _ = leds.write(it);

            // std::thread::sleep(Duration::from_micros(100));
            std::thread::sleep(Duration::from_millis(10));
        // }

        i = i.wrapping_add(1);
    }
}

fn main() -> Result<()> {
    // Temporary. Will disappear once ESP-IDF 4.4 is released, but for now it is necessary to call this function once,
    // or else some patches to the runtime implemented by esp-idf-sys might not link properly.
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    println!("Hello, world!");

    let peripherals = Peripherals::take().ok_or_else(|| eyre!("Peripherals were already taken"))?;

    let pins = peripherals.pins;

    let led_thread = {
        let pin = pins.gpio27.into_output()?;
        std::thread::spawn(move || leds(pin, peripherals.rmt.channel0).unwrap())
    };

    led_thread
        .join()
        .map_err(|e| eyre!("led thread fucked up {:?}", e))?;

    Ok(())
}
