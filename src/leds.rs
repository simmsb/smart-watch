use std::time::Duration;

use bitvec::order::Msb0;
use color_eyre::{eyre::eyre, Section};

use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::rmt::config::TransmitConfig;
use esp_idf_hal::rmt::{FixedLengthSignal, HwChannel, PinState, Pulse, Transmit};
use smart_leds::RGB8;

pub struct Esp32NeopixelMem<const LEN: usize>
where
    FixedLengthSignal<{ LEN * 24 }>: Sized,
{
    inner: FixedLengthSignal<{ LEN * 24 }>,
}

impl<const LEN: usize> Esp32NeopixelMem<LEN>
where
    FixedLengthSignal<{ LEN * 24 }>: Sized,
{
    pub fn new() -> Self {
        Self {
            inner: FixedLengthSignal::<{ LEN * 24 }>::new(),
        }
    }
}

pub struct Esp32Neopixel<'a, P: OutputPin, R: HwChannel, const LEN: usize>
where
    FixedLengthSignal<{ LEN * 24 }>: Sized,
{
    tx: Transmit<P, R>,
    working_mem: &'a mut Esp32NeopixelMem<LEN>,
}

impl<'a, P: OutputPin, R: HwChannel, const LEN: usize> Esp32Neopixel<'a, P, R, LEN>
where
    FixedLengthSignal<{ LEN * 24 }>: Sized,
{
    pub fn new(pin: P, channel: R, mem: &'a mut Esp32NeopixelMem<LEN>) -> color_eyre::Result<Self> {
        let config = TransmitConfig::new().clock_divider(1);
        let tx = Transmit::new(pin, channel, &config)?;

        Ok(Self {
            tx,
            working_mem: mem,
        })
    }
}

fn ns(x: u64) -> Duration {
    Duration::from_nanos(x)
}

fn into_bits(colour: RGB8) -> impl Iterator<Item = bool> {
    bitvec::array::BitArray::<[u8; 3], Msb0>::new([colour.g, colour.r, colour.b]).into_iter()
}

pub static PATTERN: &[(u8, u8)] = &[
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (0, 1),
    (1, 1),
    (2, 1),
    (3, 1),
    (4, 1),
    (0, 2),
    (1, 2),
    (2, 2),
    (3, 2),
    (4, 2),
    (0, 3),
    (1, 3),
    (2, 3),
    (3, 3),
    (4, 3),
    (0, 4),
    (1, 4),
    (2, 4),
    (3, 4),
    (4, 4),
];

pub fn with_positions<I>(f: impl Fn(u8, u8) -> I) -> impl Iterator<Item = ((u8, u8), I)> {
    PATTERN.iter().cloned().map(move |(x, y)| ((x, y), f(x, y)))
}

impl<'a, P: OutputPin, R: HwChannel, const LEN: usize> smart_leds_trait::SmartLedsWrite
    for Esp32Neopixel<'a, P, R, LEN>
where
    FixedLengthSignal<{ LEN * 24 }>: Sized,
{
    type Error = color_eyre::Report;
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

        let mut n = 0;

        for (i, pulse) in iterator
            .flat_map(|l| into_bits(l.into()).map(|bit| if bit { (t1h, t1l) } else { (t0h, t0l) }))
            .enumerate()
        {
            self.working_mem.inner.set(i, &pulse)?;
            n += 1;
        }

        if n != LEN * 24 {
            return Err(eyre!(
                "Sent incorrect amount of LEDS. Expected {} got {}",
                LEN,
                n / 24
            )
            .with_section(|| format!("Send exactly {} leds", LEN)));
        }

        self.tx.start_blocking(&self.working_mem.inner)?;

        Ok(())
    }
}
