use std::time::Duration;

use bitvec::order::{Msb0, Lsb0};
use itertools::Itertools;

use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::rmt::config::TransmitConfig;
use esp_idf_hal::rmt::{HwChannel, PinState, Pulse, Transmit, VariableLengthSignal};
use esp_idf_sys::EspError;
use smart_leds::RGB8;

pub struct Esp32Neopixel<P: OutputPin, R: HwChannel> {
    tx: Transmit<P, R>,
}

impl<P: OutputPin, R: HwChannel> Esp32Neopixel<P, R> {
    pub fn new(pin: P, channel: R) -> eyre::Result<Self> {
        let config = TransmitConfig::new().clock_divider(1);
        let tx = Transmit::new(pin, channel, &config)?;

        Ok(Self { tx })
    }
}

fn ns(x: u64) -> Duration {
    Duration::from_nanos(x)
}

fn into_bits(colour: RGB8) -> impl Iterator<Item = bool> {
    bitvec::array::BitArray::<[u8; 3], Msb0>::new([colour.r, colour.g, colour.b]).into_iter()
}

impl<P: OutputPin, R: HwChannel> smart_leds_trait::SmartLedsWrite for Esp32Neopixel<P, R> {
    type Error = EspError;
    type Color = RGB8;

    fn write<T, I>(&mut self, iterator: T) -> Result<(), Self::Error>
    where
        T: Iterator<Item = I>,
        I: Into<Self::Color>,
    {
        let ticks_hz = self.tx.counter_clock()?;
        let t0h = Pulse::new_with_duration(ticks_hz, PinState::High, &ns(350))?;
        let t0l = Pulse::new_with_duration(ticks_hz, PinState::Low, &ns(800))?;
        let t1h = Pulse::new_with_duration(ticks_hz, PinState::High, &ns(700))?;
        let t1l = Pulse::new_with_duration(ticks_hz, PinState::Low, &ns(600))?;

        const CHUNK_SIZE: usize = 25;

        for chunk in &iterator.chunks(CHUNK_SIZE) {
            let mut signal = VariableLengthSignal::new();

            signal.push(chunk.flat_map(|led| {
                into_bits(led.into()).flat_map(|bit| {
                    let (high_pulse, low_pulse) = if bit { (&t1h, &t1l) } else { (&t0h, &t0l) };
                    [high_pulse, low_pulse]
                })
            }))?;

            self.tx.start_blocking(&signal)?;
        }

        Ok(())
    }
}
