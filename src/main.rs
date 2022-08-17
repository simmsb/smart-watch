#![feature(const_fn_floating_point_arithmetic)]
#![feature(const_float_bits_conv)]
#![feature(adt_const_params)]
#![feature(generic_const_exprs)]
#![feature(mixed_integer_ops)]

use std::sync::atomic::AtomicU8;
use std::sync::Arc;
use std::time::Duration;

use color_eyre::{eyre::eyre, Result};
use embassy_util::Forever;
use embedded_hal::digital::blocking::InputPin;
use esp_idf_hal::gpio::{Gpio27, Gpio39, Output, SubscribedInput};
use esp_idf_hal::prelude::*;
use esp_idf_hal::rmt::HwChannel;
use esp_idf_sys as _;
use smart_leds::{SmartLedsWrite, RGB8};

use crate::dither::GammaDither;
use crate::font::ScrollingRender;

mod dither;
mod font;
mod leds;

fn rgb(x: u8, y: u8, offs: u8) -> RGB8 {
    fn conv_colour(c: cichlid::ColorRGB) -> smart_leds::RGB8 {
        smart_leds::RGB8::new(c.r, c.g, c.b)
    }

    let v = cichlid::HSV {
        h: ((y / 4) as u8).wrapping_add(x * 10).wrapping_add(offs),
        s: 200,
        v: 70,
    };

    conv_colour(v.to_rgb_rainbow())
}

fn leds(counter: Arc<AtomicU8>, pin: Gpio27<Output>, rmt: impl HwChannel) -> Result<()> {
    static MEM: Forever<leds::Esp32NeopixelMem<25>> = Forever::new();
    let mem = MEM.put_with(|| leds::Esp32NeopixelMem::<25>::new());
    let mut leds = leds::Esp32Neopixel::<_, _, 25>::new(pin, rmt, mem)?;

    const STEPS: usize = 4;

    let mut message = ScrollingRender::from_str("hello world")?;

    loop {
        for _ in 0..8 {
            let i = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            for step in 0..STEPS {
                let it = message.render(|x, y| rgb(x, y, i as u8));

                let _ = leds.write(GammaDither::<STEPS, 15>::dither(step, it));

                std::thread::sleep(Duration::from_micros(100));
            }
        }
        message.step();
    }
}

macro_rules! pin_handler {
    ($pin:expr, $cb:expr) => {{
        use ::embedded_svc::event_bus::{EventBus, Postbox};
        let mut notif = ::esp_idf_svc::notify::EspNotify::new(
            &::esp_idf_svc::notify::Configuration::default(),
        )?;
        let mut rx = notif.clone();
        let cb = $cb;
        let p = unsafe {
            $pin.into_subscribed(
                move || {
                    let _ = notif.post(&0, None);
                },
                ::esp_idf_hal::gpio::InterruptType::AnyEdge,
            )?
        };
        rx.subscribe(move |_| {
            (cb)(&p);
        })?
    }};
}

fn main() -> Result<()> {
    // Temporary. Will disappear once ESP-IDF 4.4 is released, but for now it is necessary to call this function once,
    // or else some patches to the runtime implemented by esp-idf-sys might not link properly.
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    // color_eyre::install()?;

    println!("Hello, world!");

    let peripherals = Peripherals::take().ok_or_else(|| eyre!("Peripherals were already taken"))?;

    let pins = peripherals.pins;

    let led_counter = Arc::new(AtomicU8::new(0));

    let led_thread = {
        let pin = pins.gpio27.into_output()?;
        let led_counter = Arc::clone(&led_counter);
        std::thread::Builder::new()
            .stack_size(8192)
            .spawn(move || leds(led_counter, pin, peripherals.rmt.channel0).unwrap())?
    };

    // let btn_callback = move |p: &Gpio39<SubscribedInput>| {
    //     if p.is_high().unwrap() {
    //         led_counter.store(0, std::sync::atomic::Ordering::Relaxed);
    //     }
    // };

    // let _button = pin_handler!(pins.gpio39, btn_callback);

    led_thread
        .join()
        .map_err(|e| eyre!("led thread fucked up {:?}", e))?;

    Ok(())
}
