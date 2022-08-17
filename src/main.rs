#![feature(const_fn_floating_point_arithmetic)]
#![feature(const_float_bits_conv)]
#![feature(adt_const_params)]
#![feature(generic_const_exprs)]
#![feature(mixed_integer_ops)]
#![feature(core_ffi_c)]

use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};

use color_eyre::{eyre::eyre, Result};
use embedded_hal::digital::blocking::InputPin;
use esp_idf_hal::gpio::{Gpio39, SubscribedInput};
use esp_idf_hal::prelude::*;
use esp_idf_sys as _;

pub mod display;
pub mod dither;
pub mod espnow;
pub mod font;
pub mod leds;
pub mod message;
pub mod bluetooth;

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

    // let bus = Arc::new(Mutex::new(bus::Bus::<message::Message>::new(4)));
    // let _espnow_data = espnow::espnow_setup(Arc::clone(&bus))?;

    bluetooth::init_ble(peripherals.uart0)?;

    let led_counter = Arc::new(AtomicU8::new(0));
    let led_thread = {
        let pin = pins.gpio27.into_output()?;
        let led_counter = Arc::clone(&led_counter);
        std::thread::Builder::new()
            .stack_size(8192)
            .spawn(move || display::led_task(led_counter, pin, peripherals.rmt.channel0).unwrap())?
    };

    let btn_callback = move |p: &Gpio39<SubscribedInput>| {
        if p.is_high().unwrap() {
            led_counter.store(0, std::sync::atomic::Ordering::Relaxed);
        }
    };

    let _button = pin_handler!(pins.gpio39, btn_callback);

    led_thread
        .join()
        .map_err(|e| eyre!("led thread fucked up {:?}", e))?;

    Ok(())
}
