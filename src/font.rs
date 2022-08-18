use smart_leds::RGB8;

use crate::leds;

#[derive(Clone, Copy)]
pub struct Glyph([u8; 5]);

impl Glyph {
    const fn new(a: u8, b: u8, c: u8, d: u8, e: u8) -> Self {
        Self([a, b, c, d, e])
    }

    pub fn bit_set_at(self, x: u8, y: u8) -> bool {
        self.0[y as usize] & 1 << x != 0
    }

    pub fn mask_inv<I>(
        self,
        it: impl Iterator<Item = ((u8, u8), I)>,
    ) -> impl Iterator<Item = ((u8, u8), Option<I>)> {
        it.map(move |((x, y), v)| {
            let r = if self.bit_set_at(x, y) { None } else { Some(v) };

            ((x, y), r)
        })
    }

    pub fn mask_with_x_offset<I>(
        self,
        offset: i8,
        it: impl Iterator<Item = ((u8, u8), I)>,
    ) -> impl Iterator<Item = ((u8, u8), Option<I>)> {
        it.map(move |((x, y), v)| {
            let offset_x = match x.checked_add_signed(offset) {
                Some(x) if x < 5 => x,
                _ => return ((x, y), Some(v)),
            };

            let r = if self.bit_set_at(offset_x, y) {
                Some(v)
            } else {
                None
            };

            ((x, y), r)
        })
    }
}

pub static FONT: &[Glyph] = &[
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(10, 0, 4, 17, 14),
    Glyph::new(10, 0, 0, 14, 17),
    Glyph::new(27, 31, 31, 14, 4),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 4, 10, 4, 14),
    Glyph::new(4, 14, 14, 4, 14),
    Glyph::new(0, 14, 14, 14, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 4, 10, 4, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(30, 28, 31, 21, 7),
    Glyph::new(5, 13, 31, 12, 4),
    Glyph::new(20, 22, 31, 6, 4),
    Glyph::new(15, 10, 10, 10, 5),
    Glyph::new(21, 14, 27, 14, 21),
    Glyph::new(4, 12, 28, 12, 4),
    Glyph::new(4, 6, 7, 6, 4),
    Glyph::new(4, 14, 4, 14, 4),
    Glyph::new(10, 10, 10, 0, 10),
    Glyph::new(12, 11, 10, 10, 10),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 31, 31),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(4, 14, 21, 4, 4),
    Glyph::new(4, 4, 21, 14, 4),
    Glyph::new(4, 8, 31, 8, 4),
    Glyph::new(4, 2, 31, 2, 4),
    Glyph::new(0, 2, 2, 30, 0),
    Glyph::new(0, 14, 14, 14, 0),
    Glyph::new(4, 14, 31, 0, 0),
    Glyph::new(0, 0, 31, 14, 4),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(4, 4, 4, 0, 4),
    Glyph::new(10, 10, 0, 0, 0),
    Glyph::new(10, 31, 10, 31, 10),
    Glyph::new(31, 5, 31, 20, 31),
    Glyph::new(17, 8, 4, 2, 17),
    Glyph::new(6, 9, 22, 9, 22),
    Glyph::new(8, 4, 0, 0, 0),
    Glyph::new(8, 4, 4, 4, 8),
    Glyph::new(2, 4, 4, 4, 2),
    Glyph::new(21, 14, 31, 14, 21),
    Glyph::new(0, 4, 14, 4, 0),
    Glyph::new(0, 0, 0, 4, 2),
    Glyph::new(0, 0, 14, 0, 0),
    Glyph::new(0, 0, 0, 0, 2),
    Glyph::new(8, 4, 4, 4, 2),
    Glyph::new(14, 25, 21, 19, 14),
    Glyph::new(4, 6, 4, 4, 14),
    Glyph::new(14, 8, 14, 2, 14),
    Glyph::new(14, 8, 12, 8, 14),
    Glyph::new(2, 2, 10, 14, 8),
    Glyph::new(14, 2, 14, 8, 14),
    Glyph::new(6, 2, 14, 10, 14),
    Glyph::new(14, 8, 12, 8, 8),
    Glyph::new(14, 10, 14, 10, 14),
    Glyph::new(14, 10, 14, 8, 14),
    Glyph::new(0, 4, 0, 4, 0),
    Glyph::new(0, 4, 0, 4, 2),
    Glyph::new(8, 4, 2, 4, 8),
    Glyph::new(0, 14, 0, 14, 0),
    Glyph::new(2, 4, 8, 4, 2),
    Glyph::new(14, 17, 12, 0, 4),
    Glyph::new(14, 9, 5, 1, 14),
    Glyph::new(6, 9, 17, 31, 17),
    Glyph::new(7, 9, 15, 17, 15),
    Glyph::new(14, 17, 1, 17, 14),
    Glyph::new(15, 25, 17, 17, 15),
    Glyph::new(31, 1, 15, 1, 31),
    Glyph::new(31, 1, 15, 1, 1),
    Glyph::new(14, 1, 25, 17, 14),
    Glyph::new(9, 17, 31, 17, 17),
    Glyph::new(14, 4, 4, 4, 14),
    Glyph::new(12, 8, 8, 10, 14),
    Glyph::new(9, 5, 3, 5, 9),
    Glyph::new(1, 1, 1, 1, 15),
    Glyph::new(17, 27, 21, 17, 17),
    Glyph::new(17, 19, 21, 25, 17),
    Glyph::new(14, 25, 17, 17, 14),
    Glyph::new(7, 9, 7, 1, 1),
    Glyph::new(14, 17, 17, 25, 30),
    Glyph::new(7, 9, 7, 5, 9),
    Glyph::new(30, 1, 14, 16, 15),
    Glyph::new(31, 4, 4, 4, 4),
    Glyph::new(9, 17, 17, 17, 14),
    Glyph::new(10, 10, 10, 10, 4),
    Glyph::new(9, 17, 21, 21, 10),
    Glyph::new(17, 10, 4, 10, 17),
    Glyph::new(17, 10, 4, 4, 4),
    Glyph::new(31, 8, 4, 2, 31),
    Glyph::new(12, 4, 4, 4, 12),
    Glyph::new(2, 4, 4, 4, 8),
    Glyph::new(6, 4, 4, 4, 6),
    Glyph::new(4, 10, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 14),
    Glyph::new(4, 8, 0, 0, 0),
    Glyph::new(6, 9, 17, 31, 17),
    Glyph::new(7, 9, 15, 17, 15),
    Glyph::new(14, 17, 1, 17, 14),
    Glyph::new(15, 25, 17, 17, 15),
    Glyph::new(31, 1, 15, 1, 31),
    Glyph::new(31, 1, 15, 1, 1),
    Glyph::new(14, 1, 25, 17, 14),
    Glyph::new(9, 17, 31, 17, 17),
    Glyph::new(14, 4, 4, 4, 14),
    Glyph::new(12, 8, 8, 10, 14),
    Glyph::new(18, 10, 6, 10, 18),
    Glyph::new(1, 1, 1, 1, 15),
    Glyph::new(17, 27, 21, 17, 17),
    Glyph::new(17, 19, 21, 25, 17),
    Glyph::new(14, 25, 17, 17, 14),
    Glyph::new(7, 9, 7, 1, 1),
    Glyph::new(14, 17, 17, 25, 30),
    Glyph::new(7, 9, 7, 5, 9),
    Glyph::new(30, 1, 14, 16, 15),
    Glyph::new(31, 4, 4, 4, 4),
    Glyph::new(9, 17, 17, 17, 14),
    Glyph::new(10, 10, 10, 10, 4),
    Glyph::new(9, 17, 21, 21, 10),
    Glyph::new(17, 10, 4, 10, 17),
    Glyph::new(17, 10, 4, 4, 4),
    Glyph::new(31, 8, 4, 2, 31),
    Glyph::new(12, 4, 2, 4, 12),
    Glyph::new(4, 4, 4, 4, 4),
    Glyph::new(6, 4, 8, 4, 6),
    Glyph::new(10, 5, 0, 0, 0),
    Glyph::new(0, 4, 10, 10, 14),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(10, 0, 10, 10, 14),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(10, 0, 14, 10, 30),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(31, 17, 17, 17, 31),
    Glyph::new(0, 14, 10, 14, 0),
    Glyph::new(0, 0, 4, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 4, 0, 0),
    Glyph::new(0, 14, 10, 14, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(10, 0, 14, 10, 30),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(10, 0, 14, 10, 14),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(3, 25, 11, 9, 11),
    Glyph::new(28, 23, 21, 21, 29),
    Glyph::new(0, 3, 1, 1, 1),
    Glyph::new(10, 0, 14, 10, 14),
    Glyph::new(10, 0, 10, 10, 14),
    Glyph::new(0, 0, 0, 0, 31),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 31),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 31),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(4, 0, 6, 17, 14),
    Glyph::new(0, 0, 28, 4, 4),
    Glyph::new(0, 0, 7, 4, 4),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(4, 0, 4, 4, 4),
    Glyph::new(4, 18, 9, 18, 4),
    Glyph::new(4, 9, 18, 9, 4),
    Glyph::new(0, 10, 0, 10, 0),
    Glyph::new(10, 21, 10, 21, 10),
    Glyph::new(21, 10, 21, 10, 21),
    Glyph::new(4, 4, 4, 4, 4),
    Glyph::new(4, 4, 7, 4, 4),
    Glyph::new(4, 7, 4, 7, 4),
    Glyph::new(10, 10, 11, 10, 10),
    Glyph::new(0, 0, 15, 10, 10),
    Glyph::new(0, 7, 4, 7, 4),
    Glyph::new(10, 11, 8, 11, 10),
    Glyph::new(10, 10, 10, 10, 10),
    Glyph::new(0, 15, 8, 11, 10),
    Glyph::new(10, 11, 8, 15, 0),
    Glyph::new(10, 10, 15, 0, 0),
    Glyph::new(4, 7, 4, 7, 0),
    Glyph::new(0, 0, 7, 4, 4),
    Glyph::new(4, 4, 28, 0, 0),
    Glyph::new(4, 4, 31, 0, 0),
    Glyph::new(0, 0, 31, 4, 4),
    Glyph::new(4, 4, 28, 4, 4),
    Glyph::new(0, 0, 31, 0, 0),
    Glyph::new(4, 4, 31, 4, 4),
    Glyph::new(4, 28, 4, 28, 4),
    Glyph::new(10, 10, 26, 10, 10),
    Glyph::new(10, 26, 2, 30, 0),
    Glyph::new(0, 30, 2, 26, 10),
    Glyph::new(10, 27, 0, 31, 0),
    Glyph::new(0, 31, 0, 27, 10),
    Glyph::new(10, 26, 2, 26, 10),
    Glyph::new(0, 31, 0, 31, 0),
    Glyph::new(10, 27, 0, 27, 10),
    Glyph::new(4, 31, 0, 31, 0),
    Glyph::new(10, 10, 31, 0, 0),
    Glyph::new(0, 31, 0, 31, 4),
    Glyph::new(0, 0, 31, 10, 10),
    Glyph::new(10, 10, 30, 0, 0),
    Glyph::new(4, 28, 4, 28, 0),
    Glyph::new(0, 28, 4, 28, 4),
    Glyph::new(0, 0, 30, 10, 10),
    Glyph::new(10, 10, 31, 10, 10),
    Glyph::new(4, 31, 4, 31, 4),
    Glyph::new(4, 4, 7, 0, 0),
    Glyph::new(0, 0, 28, 4, 4),
    Glyph::new(31, 31, 31, 31, 31),
    Glyph::new(0, 0, 31, 31, 31),
    Glyph::new(3, 3, 3, 3, 3),
    Glyph::new(24, 24, 24, 24, 24),
    Glyph::new(31, 31, 31, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(6, 9, 13, 17, 13),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(14, 17, 17, 17, 14),
    Glyph::new(0, 4, 10, 4, 0),
    Glyph::new(0, 0, 4, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 4, 0, 0),
    Glyph::new(0, 4, 10, 4, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 14, 31, 14, 0),
    Glyph::new(16, 14, 10, 14, 1),
    Glyph::new(12, 2, 14, 2, 12),
    Glyph::new(6, 9, 9, 9, 9),
    Glyph::new(14, 0, 14, 0, 14),
    Glyph::new(4, 14, 4, 0, 14),
    Glyph::new(2, 4, 8, 4, 14),
    Glyph::new(8, 4, 2, 4, 14),
    Glyph::new(8, 20, 4, 4, 4),
    Glyph::new(4, 4, 4, 5, 2),
    Glyph::new(4, 0, 14, 0, 4),
    Glyph::new(10, 5, 0, 10, 5),
    Glyph::new(4, 14, 4, 0, 0),
    Glyph::new(0, 14, 14, 14, 0),
    Glyph::new(0, 0, 4, 0, 0),
    Glyph::new(24, 8, 11, 10, 4),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
    Glyph::new(0, 0, 0, 0, 0),
];

pub fn from_str(s: &str) -> color_eyre::Result<Vec<Glyph>> {
    use codepage_437::ToCp437;

    let r = s.to_cp437(&codepage_437::CP437_CONTROL).map_err(|e| {
        color_eyre::eyre::eyre!("Couldn't convert string {:?} to cp437: {:?}", s, e)
    })?;
    Ok(r.iter().cloned().map(|idx| FONT[idx as usize]).collect())
}

pub struct ScrollingRender {
    message: Vec<Glyph>,
    char_offset: u8,
    scroll: u16,
}

impl ScrollingRender {
    pub fn from_str(s: &str) -> color_eyre::Result<Self> {
        let mut message = from_str(s)?;
        message.push(FONT[0x20]);
        message.push(FONT[0x20]);

        Ok(Self {
            message,
            char_offset: 0,
            scroll: 0,
        })
    }

    pub fn from_glyphs(mut message: Vec<Glyph>) -> Self {
        message.push(FONT[0x20]);
        message.push(FONT[0x20]);

        Self {
            message,
            char_offset: 0,
            scroll: 0,
        }
    }

    pub fn render(&self, colour: impl Fn(u8, u8) -> RGB8) -> impl Iterator<Item = RGB8> {
        let left = self.message[self.scroll as usize % self.message.len()];
        let right = self.message[(self.scroll as usize + 1) % self.message.len()];

        let it = leds::with_positions(colour).collect::<Vec<_>>();
        // let it = left
        //     .mask_with_x_offset(0, it.into_iter())
        //     .collect::<Vec<_>>();
        let it = left
            .mask_with_x_offset(self.char_offset as i8, it.into_iter())
            .collect::<Vec<_>>();
        let it = right
            .mask_with_x_offset((self.char_offset as i8) - 6, it.into_iter())
            .collect::<Vec<_>>();
        let it = it
            .into_iter()
            .map(|((x, y), v)| {
                let v = if x == (5 - self.char_offset) {
                    None
                } else {
                    Some(v)
                };
                ((x, y), v)
            })
            .collect::<Vec<_>>();
        it.into_iter()
            .map(|(_, v)| v.flatten().flatten().unwrap_or(RGB8::new(0, 0, 0)))
        // it.into_iter()
        //     .map(|(_, v)| v.unwrap_or(RGB8::new(0, 0, 0)))
    }

    pub fn step(&mut self) -> bool {
        self.char_offset += 1;
        if self.char_offset > 5 {
            self.char_offset = 0;
            self.scroll += 1;
        }

        if self.scroll as usize == self.message.len() {
            self.scroll = 0;
            return true;
        }

        return false;
    }
}
