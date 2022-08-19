use std::time::SystemTime;

use color_eyre::eyre::eyre;
use display_interface::WriteOnlyDataCommand;
use display_interface_spi::SPIInterfaceNoCS;
use embedded_graphics::mono_font::ascii::FONT_6X12;
use embedded_graphics::mono_font::iso_8859_14::FONT_10X20;
use embedded_graphics::mono_font::{MonoTextStyle, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Dimensions, DrawTarget, DrawTargetExt, Point, RgbColor, Size};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::Alignment;
use embedded_graphics::Drawable;
use embedded_text::alignment::HorizontalAlignment;
use embedded_text::style::TextBoxStyleBuilder;
use embedded_text::TextBox;
use eos::fmt::format_spec;
use esp_idf_hal::delay::Ets;
use esp_idf_hal::gpio::{Gpio13, Gpio15, Gpio18, Gpio23, Gpio5, GpioPin, InputOutput, Output};
use esp_idf_hal::spi::{Master, SPI2};
use esp_idf_hal::units::FromValueType;
use mipidsi::models::{Model, ST7789};
use mipidsi::Orientation;
use profont::PROFONT_24_POINT;

type DisplayType = mipidsi::Display<
    SPIInterfaceNoCS<
        Master<SPI2, Gpio13<Output>, Gpio15<Output>, GpioPin<InputOutput>, Gpio5<Output>>,
        Gpio23<Output>,
    >,
    Gpio18<Output>,
    FixedST7789,
>;

struct FixedST7789(ST7789);

impl Model for FixedST7789 {
    type ColorFormat = <ST7789 as Model>::ColorFormat;

    fn new() -> Self {
        Self(ST7789::new())
    }

    fn init<RST, DELAY, DI>(
        &mut self,
        di: &mut DI,
        rst: &mut Option<RST>,
        delay: &mut DELAY,
        options: mipidsi::DisplayOptions,
    ) -> Result<u8, mipidsi::Error<RST::Error>>
    where
        RST: eh_0_2::digital::v2::OutputPin,
        DELAY: eh_0_2::prelude::_embedded_hal_blocking_delay_DelayUs<u32>,
        DI: WriteOnlyDataCommand,
    {
        self.0.init(di, rst, delay, options)
    }

    fn write_pixels<DI, I>(
        &mut self,
        di: &mut DI,
        colors: I,
    ) -> Result<(), display_interface::DisplayError>
    where
        DI: WriteOnlyDataCommand,
        I: IntoIterator<Item = Self::ColorFormat>,
    {
        self.0.write_pixels(di, colors)
    }

    fn display_size(&self, orientation: Orientation) -> (u16, u16) {
        self.0.framebuffer_size(orientation)
    }
}

pub struct Display {
    display: DisplayType,
}

impl Display {
    pub fn new(
        spi: SPI2,
        sclk: Gpio13<Output>,
        sdo: Gpio15<Output>,
        cs: Gpio5<Output>,
        dc: Gpio23<Output>,
        rst: Gpio18<Output>,
    ) -> color_eyre::Result<Self> {
        let config = esp_idf_hal::spi::config::Config::default().baudrate(26u32.MHz().into());

        let spi = Master::<SPI2, _, _, _, _>::new(
            spi,
            esp_idf_hal::spi::Pins {
                sclk,
                sdo,
                sdi: None,
                cs: Some(cs),
            },
            config,
        )?;
        let di = SPIInterfaceNoCS::new(spi, dc);
        let mut display = mipidsi::Display::with_model(di, Some(rst), FixedST7789::new());
        display
            .init(
                &mut Ets,
                mipidsi::DisplayOptions {
                    orientation: Orientation::Landscape(true),
                    invert_vertical_refresh: false,
                    color_order: mipidsi::ColorOrder::Bgr,
                    invert_horizontal_refresh: false,
                },
            )
            .map_err(|e| eyre!("Failed to init display: {:?}", e))?;
        display
            .clear(Rgb565::BLACK)
            .map_err(|e| eyre!("Failed to use display: {:?}", e))?;

        Ok(Self { display })
    }

    fn cropped_display(
        &mut self,
    ) -> impl DrawTarget<Color = Rgb565, Error = <DisplayType as DrawTarget>::Error> + '_ {
        let area = Rectangle::new(Point::new(40, 53), Size::new(240, 135));
        self.display.cropped(&area)
    }

    pub fn display_time(&mut self, bat_volt: f32) -> color_eyre::Result<()> {
        let now = eos::DateTime::utc_now();

        let bat_charge = (bat_volt.clamp(3.0, 4.2) - 3.0) / (4.2 - 3.0);
        let bat_charge = (bat_charge * 100.0) as u8;

        let text = now.format(format_spec!("%H:%M:%S")).to_string();
        let character_style = MonoTextStyleBuilder::new()
            .font(&profont::PROFONT_24_POINT)
            .text_color(Rgb565::WHITE)
            .background_color(Rgb565::BLACK)
            .build();

        let textbox_style = TextBoxStyleBuilder::new()
            .height_mode(embedded_text::style::HeightMode::FitToText)
            .alignment(HorizontalAlignment::Center)
            .vertical_alignment(embedded_text::alignment::VerticalAlignment::Middle)
            .build();

        let mut canvas = self.cropped_display();
        TextBox::with_textbox_style(
            &text,
            Rectangle::new(
                Point::new(0, (135 - PROFONT_24_POINT.character_size.height as i32) / 2),
                Size::new(240, 135 / 2),
            ),
            character_style,
            textbox_style,
        )
        .draw(&mut canvas)
        .map_err(|e| eyre!("Failed to draw to display: {:?}", e))?;

        let text = format!("{bat_charge}%");

        let textbox_style = TextBoxStyleBuilder::new()
            .height_mode(embedded_text::style::HeightMode::FitToText)
            .alignment(HorizontalAlignment::Right)
            .vertical_alignment(embedded_text::alignment::VerticalAlignment::Top)
            .leading_spaces(true)
            .trailing_spaces(true)
            .build();

        let bounds = Rectangle::new(Point::new(120, 0), Size::new(240 - 120, 29));

        TextBox::with_textbox_style(&text, bounds, character_style, textbox_style)
            .draw(&mut canvas)
            .map_err(|e| eyre!("Failed to draw to display: {:?}", e))?;

        Ok(())
    }
}
