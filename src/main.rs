#![feature(const_fn_floating_point_arithmetic)]
#![feature(const_float_bits_conv)]
#![feature(adt_const_params)]
#![feature(generic_const_exprs)]
#![feature(mixed_integer_ops)]
#![feature(core_ffi_c)]
#![feature(const_mut_refs)]
#![feature(const_unsafecell_get_mut)]
#![feature(const_option)]

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use color_eyre::{eyre::eyre, Result};
use eh_0_2::prelude::_embedded_hal_adc_OneShot;
use embedded_hal::digital::blocking::{InputPin, OutputPin};
use eos::Timestamp;
use esp_idf_hal::adc::{self, PoweredAdc, ADC2};
use esp_idf_hal::gpio::{Gpio0, Gpio25, Gpio26, Gpio37, Gpio39, Output, SubscribedInput, Unknown};
use esp_idf_hal::{i2c, prelude::*};
use esp_idf_sys::{self as _, esp};
use once_cell::sync::Lazy;
use tracing::{error, info};

use crate::rtc::EspRtc;
use crate::utils::I2c0;

pub mod axp192;
pub mod bluetooth;
pub mod display;
pub mod ingerland;
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

// me when HKTs

macro_rules! impl_pinstate {
    ($name:ty) => {
        impl $name {
            fn set_high(mut self) -> Self {
                self.p.set_high().unwrap();
                self
            }

            fn set_low(mut self) -> Self {
                self.p.set_low().unwrap();
                self
            }

            fn read(self, adc: &mut PoweredAdc<ADC2>) -> (Self, u16) {
                use esp_idf_hal::gpio::Pull;
                let mut p = self
                    .p
                    .into_analog_atten_11db()
                    .unwrap()
                    .into_floating()
                    .unwrap();
                let r = adc.read(&mut p).unwrap();
                let mut p = p.into_pull_up().unwrap().into_output().unwrap();
                p.set_low().unwrap();

                info!(r, pin = stringify!($name), "Analog reading");

                (Self { p }, r)
            }
        }
    };
}

struct PinState0 {
    p: Gpio0<Output>,
}

impl_pinstate!(PinState0);

struct PinState25 {
    p: Gpio25<Output>,
}

impl_pinstate!(PinState25);

struct PinState26 {
    p: Gpio26<Output>,
}

impl_pinstate!(PinState26);

struct PinStates {
    g0: PinState0,
    g25: PinState25,
    g26: PinState26,
}

macro_rules! do_pinop {
    ($set_pin:expr, $state:ident, $pin:ident, $tx:ident, $adc:ident) => {
        match $set_pin.op() {
            message::PinOperation::SetHigh => $state.$pin = $state.$pin.set_high(),
            message::PinOperation::SetLow => $state.$pin = $state.$pin.set_low(),
            message::PinOperation::AnalogueRead => {
                let val;
                ($state.$pin, val) = $state.$pin.read(&mut $adc);

                let pinread = message::PinRead {
                    pin: $set_pin.pin,
                    value: val as f32,
                };
                let msg = message::Notification {
                    body: Some(message::notification::Body::PinRead(pinread)),
                };
                let _ = $tx.send(msg);
            }
        }
    };
}

fn pin_thread(g26: Gpio26<Unknown>, g25: Gpio25<Unknown>, g0: Gpio0<Unknown>, adc: ADC2) {
    let mut g26 = g26.into_output().unwrap();
    g26.set_low().unwrap();

    let mut g25 = g25.into_output().unwrap();
    g25.set_low().unwrap();

    let mut g0 = g0.into_output().unwrap();
    g0.set_low().unwrap();

    let mut adc = PoweredAdc::new(
        adc,
        adc::config::Config {
            resolution: adc::config::Resolution::Resolution10Bit,
            calibration: true,
        },
    )
    .unwrap();

    let mut state = PinStates {
        g0: PinState0 { p: g0 },
        g25: PinState25 { p: g25 },
        g26: PinState26 { p: g26 },
    };

    let rx = message::get_receiver();

    let tx = bluetooth::QUEUE.0.clone();

    for msg in rx {
        if let Some(message::message::Body::SetPin(set_pin)) = msg.body {
            match set_pin.pin() {
                message::Pins::G26 => do_pinop!(set_pin, state, g26, tx, adc),
                message::Pins::G25 => do_pinop!(set_pin, state, g25, tx, adc),
                message::Pins::G0 => do_pinop!(set_pin, state, g0, tx, adc),
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

    let pm_config = esp_idf_sys::esp_pm_config_esp32_t {
        max_freq_mhz: 80,
        min_freq_mhz: 40,
        light_sleep_enable: true,
    };

    esp!(unsafe { esp_idf_sys::esp_pm_configure(&pm_config as *const _ as *const _) })?;
    unsafe { esp_idf_sys::adc_power_acquire() };

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

    let front_btn_callback = {
        let button_state = Arc::clone(&button_state);
        let wake_tx = wake_tx.clone();
        move |p: &Gpio37<SubscribedInput>| {
            let current = p.is_low().unwrap();
            let prev = button_state.swap(current, std::sync::atomic::Ordering::Relaxed);

            info!(current, prev, "front button");

            if prev != current {
                let _ = wake_tx.send(current);
            }
        }
    };

    let _front_button = pin_handler!(pins.gpio37, front_btn_callback);

    let side_btn_callback = {
        let button_state = Arc::new(AtomicBool::new(false));

        move |p: &Gpio39<SubscribedInput>| {
            let current = p.is_low().unwrap();
            let prev = button_state.swap(current, std::sync::atomic::Ordering::Relaxed);

            info!(current, prev, "side button");

            if prev != current && current {
                info!("Starting advertise");
                bluetooth::ble_spp_server_advertise();
            }
        }
    };

    let _side_button = pin_handler!(pins.gpio39, side_btn_callback);

    let _waker_thread = std::thread::spawn({
        let wake_tx = wake_tx.clone();
        move || waker_thread(wake_tx)
    });

    let _syncer_thread = std::thread::Builder::new().stack_size(4096).spawn({
        let rtc = Arc::clone(&rtc);
        move || syncer_thread(rtc)
    });

    let _pin_thread = std::thread::Builder::new().stack_size(4096).spawn({
        let g26 = pins.gpio26;
        let g25 = pins.gpio25;
        let g0 = pins.gpio0;
        let adc = peripherals.adc2;
        || pin_thread(g26, g25, g0, adc)
    });

    let _battery_thread = pwr.start_battery_thread();

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
            info!(%now, "Current utc time");

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
