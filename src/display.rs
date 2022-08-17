use std::sync::atomic::AtomicU8;
use std::sync::Arc;
use std::time::Duration;

use embassy_util::Forever;
use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::rmt::HwChannel;
use smart_leds::{SmartLedsWrite, RGB8};

use crate::dither::GammaDither;
use crate::font::ScrollingRender;
use crate::leds;

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

pub fn led_task(
    counter: Arc<AtomicU8>,
    pin: impl OutputPin,
    rmt: impl HwChannel,
) -> color_eyre::Result<()> {
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
