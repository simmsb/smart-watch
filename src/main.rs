#![feature(const_fn_floating_point_arithmetic)]
#![feature(const_float_bits_conv)]
#![feature(adt_const_params)]
#![feature(generic_const_exprs)]
#![feature(mixed_integer_ops)]
#![feature(core_ffi_c)]
#![feature(const_mut_refs)]
#![feature(const_unsafecell_get_mut)]

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use color_eyre::{eyre::eyre, Result};
use embedded_hal::digital::blocking::InputPin;
use esp_idf_hal::gpio::{Gpio39, SubscribedInput};
use esp_idf_hal::{i2c, prelude::*};
use esp_idf_sys as _;
use tracing::info;

use crate::rtc::EspRtc;
use crate::utils::I2c0;

pub mod bluetooth;
pub mod message;
pub mod rtc;
pub mod utils;

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
                ::esp_idf_hal::gpio::InterruptType::NegEdge,
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

    let i2c0 = I2c0::new(i2c::Master::new(
        peripherals.i2c0,
        i2c::MasterPins {
            sda: pins.gpio21.into_input_output()?,
            scl: pins.gpio22.into_input_output()?,
        },
        i2c::config::MasterConfig::default().baudrate(400.kHz().into()),
    )?);

    let mut rtc = EspRtc::new(i2c0)?;

    loop {
        info!("The time is: {}", rtc.read()?);

        std::thread::sleep(Duration::from_secs(1));
    }

    // bluetooth::init_ble(peripherals.uart0)?;

    Ok(())
}
