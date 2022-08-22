#![feature(const_fn_floating_point_arithmetic)]
#![feature(const_float_bits_conv)]
#![feature(adt_const_params)]
#![feature(generic_const_exprs)]
#![feature(mixed_integer_ops)]
#![feature(core_ffi_c)]
#![feature(const_mut_refs)]
#![feature(const_unsafecell_get_mut)]

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use color_eyre::{eyre::eyre, Result};
use embedded_hal::digital::blocking::InputPin;
use eos::{DateTime, Timestamp, Utc};
use esp_idf_hal::gpio::{Gpio37, Gpio39, SubscribedInput};
use esp_idf_hal::{i2c, prelude::*};
use esp_idf_sys as _;
use once_cell::sync::Lazy;
use tracing::{error, info};

use crate::rtc::EspRtc;
use crate::utils::I2c0;

pub mod axp192;
pub mod bluetooth;
pub mod display;
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
                ::esp_idf_hal::gpio::InterruptType::AnyEdge,
            )?
        };
        rx.subscribe(move |_| {
            (cb)(&p);
        })?
    }};
}

static CURRENT_NOTIF: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

fn waker_thread(wake_tx: Sender<bool>) {
    let rx = message::get_receiver();

    for msg in rx {
        if let Some(message::message::Body::PushNotification(notif)) = msg.body {
            CURRENT_NOTIF.lock().unwrap().replace_range(.., &notif.body);
            let _ = wake_tx.send(true);
        }
    }
}

fn syncer_thread(rtc: Arc<Mutex<EspRtc>>) {
    let rx = message::get_receiver();

    for msg in rx {
        if let Some(message::message::Body::SyncClock(message::SyncClock {
            timestamp: Some(ts),
        })) = msg.body
        {
            let ts = Timestamp::new(ts.seconds, ts.nanos as u32);
            if let Err(err) = rtc.lock().unwrap().set(ts.to_utc()) {
                error!(?err, "Failed to set RTC");
            }
        }
    }
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

    let pwr = axp192::Axp192::new(i2c0.clone())?;
    let rtc = Arc::new(Mutex::new(EspRtc::new(i2c0)?));
    let mut display = display::Display::new(
        peripherals.spi2,
        pins.gpio13.into_output()?,
        pins.gpio15.into_output()?,
        pins.gpio5.into_output()?,
        pins.gpio23.into_output()?,
        pins.gpio18.into_output()?,
    )?;

    let button_state = Arc::new(AtomicBool::new(false));
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let btn_callback = {
        let button_state = Arc::clone(&button_state);
        let wake_tx = wake_tx.clone();
        move |p: &Gpio37<SubscribedInput>| {
            let current = p.is_low().unwrap();
            let prev = button_state.swap(current, std::sync::atomic::Ordering::Relaxed);

            info!(current, prev, "button");

            if prev != current {
                let _ = wake_tx.send(current);
            }
        }
    };

    let _waker_thread = std::thread::spawn({
        let wake_tx = wake_tx.clone();
        move || waker_thread(wake_tx)
    });

    let _syncer_thread = std::thread::Builder::new().stack_size(4096).spawn({
        let rtc = Arc::clone(&rtc);
        move || syncer_thread(rtc)
    });

    let _battery_thread = pwr.start_battery_thread();

    let _button = pin_handler!(pins.gpio37, btn_callback);

    bluetooth::init_ble()?;

    loop {
        let mut end_time = Instant::now() + Duration::from_secs(20);

        'inner: loop {
            let batt_pwr = pwr.get_batt_power()?;
            let batt_vol = pwr.get_batt_voltage()?;
            let vbus_cur = pwr.get_vbus_current()?;
            info!(
                "Battery pwr: {}, volt: {}. vbus cur: {}",
                batt_pwr, batt_vol, vbus_cur
            );

            let now = rtc.lock().unwrap().read()?;

            display.display_time(now, batt_vol)?;

            while let Ok(v) = wake_rx.try_recv() {
                info!("Button press: {v}");
                if v == true {
                    end_time = Instant::now() + Duration::from_secs(20);
                }
            }

            if Instant::now() > end_time {
                info!("Disabling backlight");
                pwr.set_backlight(false)?;
                break 'inner;
            }

            std::thread::sleep(Duration::from_secs(1));
        }

        while wake_rx.recv().unwrap() == false {}

        pwr.set_backlight(true)?;
    }
}
