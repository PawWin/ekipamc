//! An 8x8 pixel face derived from the nick, standing in for a skin head.
//! The same nick always gets the same face.

use md5::{Digest, Md5};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const SKIN: [u32; 6] = [0xf1c27d, 0xe0ac69, 0xc68642, 0x8d5524, 0xffdbac, 0xd9a066];
const HAIR: [u32; 7] = [
    0x3b2412, 0x6b4423, 0xa0522d, 0x1c1c1c, 0xd4a017, 0xb33c1c, 0xe8e0c8,
];
const EYES: [u32; 4] = [0x3b5ba5, 0x2f7d32, 0x5a3a1e, 0x6a4fb0];

/// Face for `nick`, or a grey placeholder when there is no nick yet.
pub fn face(nick: &str) -> Image {
    let hash = Md5::digest(nick.as_bytes());
    let pick = |table: &[u32], byte: usize| table[hash[byte] as usize % table.len()];
    let (skin, hair, eyes) = if nick.is_empty() {
        (0x5a5a5a, 0x444444, 0x2e2e2e)
    } else {
        (pick(&SKIN, 0), pick(&HAIR, 1), pick(&EYES, 2))
    };
    let fringe = hash[3] % 3;
    let beard = !nick.is_empty() && hash[4] % 4 == 0;

    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(8, 8);
    for (i, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (i % 8, i / 8);
        let color = match (x, y) {
            (_, 0 | 1) => hair,
            (0 | 7, 2 | 3) => hair,
            // Fringe falling to one side, the other or not at all.
            (1..=3, 2) if fringe == 0 => hair,
            (4..=6, 2) if fringe == 1 => hair,
            (1 | 6, 4) => 0xffffff,
            (2 | 5, 4) => eyes,
            (3 | 4, 5) => shade(skin, 0.85),
            (2..=5, 6) if beard => hair,
            (3 | 4, 6) => shade(skin, 0.6),
            (_, 6 | 7) if beard => hair,
            _ => skin,
        };
        *pixel = Rgba8Pixel::new((color >> 16) as u8, (color >> 8) as u8, color as u8, 255);
    }
    Image::from_rgba8(buffer)
}

fn shade(color: u32, factor: f32) -> u32 {
    let channel = |shift: u32| ((((color >> shift) & 0xff) as f32 * factor) as u32) << shift;
    channel(16) | channel(8) | channel(0)
}
