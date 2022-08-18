use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use embassy_util::Forever;
use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::rmt::HwChannel;
use smart_leds::{SmartLedsWrite, RGB8};
use tracing::{error, info};

use crate::bluetooth::CURRENT_MESSAGE;
use crate::dither::GammaDither;
use crate::font::{self, ScrollingRender};
use crate::leds;

fn rgb(x: u8, y: u8, offs: u8) -> RGB8 {
    fn conv_colour(c: cichlid::ColorRGB) -> smart_leds::RGB8 {
        smart_leds::RGB8::new(c.r, c.g, c.b)
    }

    let v = cichlid::HSV {
        h: ((y / 4) as u8).wrapping_add(x * 10).wrapping_add(offs),
        s: 200,
        v: 60,
    };

    conv_colour(v.to_rgb_rainbow())
}

pub fn led_task(
    heart: Arc<AtomicBool>,
    pin: impl OutputPin,
    rmt: impl HwChannel,
) -> color_eyre::Result<()> {
    static MEM: Forever<leds::Esp32NeopixelMem<25>> = Forever::new();
    let mem = MEM.put_with(leds::Esp32NeopixelMem::<25>::new);
    let mut leds = leds::Esp32Neopixel::<_, _, 25>::new(pin, rmt, mem)?;

    const STEPS: usize = 4;
    let mut i = 0u8;

    let mut message = ScrollingRender::from_str("hello world")?;

    loop {
        for _ in 0..12 {
            for step in 0..STEPS {
                if heart.load(std::sync::atomic::Ordering::Relaxed) {
                    let it = font::FONT[0x3]
                        .mask_with_x_offset(
                            0,
                            leds::with_positions(|_x, _y| RGB8::new(0xFD, 0x3F, 0x92)),
                        )
                        .map(|(_, v)| v.unwrap_or(RGB8::new(0, 0, 0)));

                    let _ = leds.write(GammaDither::<STEPS, 15>::dither(step, it));
                } else {
                    let it = message.render(|x, y| rgb(x, y, i as u8));

                    let _ = leds.write(GammaDither::<STEPS, 15>::dither(step, it));
                }

                std::thread::sleep(Duration::from_micros(100));
            }
            i += 1;
        }
        if message.step() {
            info!("message done! seeing if there's a new one");
            match ScrollingRender::from_str(CURRENT_MESSAGE.lock().unwrap().as_str()) {
                Ok(m) => {
                    message = m;
                }
                Err(err) => {
                    error!(?err, "Failed to update message");
                }
            }
        }
    }
}
