// PNG decompression implementation.
// * https://www.w3.org/TR/png-3/
use crate::inflate;
use std::mem;

// ----------------------------------------------------------------------------
#[derive(Debug, PartialEq)]
pub enum Error {
    InvalidPng,
    InvalidSignature,
    InvalidFormat,
    InvalidColorFormat,
    InvalidPalette,
    InvalidFilterType,
    UnsupportedFormat,
    CompressionError,
    BufferError,
    BufferUnderrun,
    InvalidIDAT,
    MissingIHDR,
    MissingIEND,
}

// ----------------------------------------------------------------------------
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let err = format!("{:?}", self);
        f.write_str(&err)
    }
}

// ----------------------------------------------------------------------------
impl std::error::Error for Error {}

// ----------------------------------------------------------------------------
impl From<inflate::Error> for Error {
    fn from(_: inflate::Error) -> Self {
        Error::CompressionError
    }
}

// ----------------------------------------------------------------------------
impl From<std::array::TryFromSliceError> for Error {
    fn from(_: std::array::TryFromSliceError) -> Self {
        Error::BufferError
    }
}

// ----------------------------------------------------------------------------
pub type Result<T> = std::result::Result<T, Error>;

// ----------------------------------------------------------------------------
macro_rules! fourcc {
    ($a:expr, $b:expr, $c:expr, $d:expr) => {
        (($a as u32) << 24) | (($b as u32) << 16) | (($c as u32) << 8) | (($d as u32) << 0)
    };
}

// ----------------------------------------------------------------------------
const IHDR: u32 = fourcc!('I', 'H', 'D', 'R');
const IDAT: u32 = fourcc!('I', 'D', 'A', 'T');
const IEND: u32 = fourcc!('I', 'E', 'N', 'D');
const PLTE: u32 = fourcc!('P', 'L', 'T', 'E');

// ----------------------------------------------------------------------------
#[derive(Debug)]
struct PNGChunkHead {
    length: u32,
    r#type: u32,
}

// ----------------------------------------------------------------------------
#[derive(Debug, PartialEq)]
pub enum PNGColorType {
    Greyscale = 0,
    TrueColor = 2,
    IndexedColor = 3,
    GreyscaleAplha = 4,
    TrueColorAlpha = 6,
}

// ----------------------------------------------------------------------------
impl TryFrom<u8> for PNGColorType {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        use PNGColorType::*;
        match value {
            0 => Ok(Greyscale),
            2 => Ok(TrueColor),
            3 => Ok(IndexedColor),
            4 => Ok(GreyscaleAplha),
            6 => Ok(TrueColorAlpha),
            _ => Err(Error::InvalidColorFormat),
        }
    }
}

// ----------------------------------------------------------------------------
impl PNGColorType {
    fn channels(&self) -> usize {
        use PNGColorType::*;
        match self {
            Greyscale | IndexedColor => 1,
            TrueColor => 3,
            GreyscaleAplha => 2,
            TrueColorAlpha => 4,
        }
    }
}

// ----------------------------------------------------------------------------
#[derive(Debug)]
pub struct PNGChunkIHDR {
    pub width: usize,
    pub height: usize,
    pub bit_depth: usize,
    pub color_type: PNGColorType,
    pub compression: u8,
    pub filter: u8,
    pub interlace: u8,
}

// ----------------------------------------------------------------------------
#[derive(Debug, Copy, Clone, PartialEq)]
enum PNGFilterType {
    None = 0,
    Sub = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
}

// ----------------------------------------------------------------------------
impl TryFrom<u8> for PNGFilterType {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(PNGFilterType::None),
            1 => Ok(PNGFilterType::Sub),
            2 => Ok(PNGFilterType::Up),
            3 => Ok(PNGFilterType::Average),
            4 => Ok(PNGFilterType::Paeth),
            _ => Err(Error::InvalidFilterType),
        }
    }
}

// ----------------------------------------------------------------------------
const fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let pa = u8::abs_diff(b, c) as u32;
    let pb = u8::abs_diff(a, c) as u32;
    let pc = u32::abs_diff(a as u32 + b as u32, 2 * c as u32);

    if pc < pa && pc < pb {
        c
    } else if pb < pa {
        b
    } else {
        a
    }
}

// ----------------------------------------------------------------------------
fn unfilter_scanline_0<const N: usize>(recon: &mut [u8], filter_type: PNGFilterType, cx: usize) {
    match filter_type {
        PNGFilterType::None | PNGFilterType::Up => (),
        PNGFilterType::Sub | PNGFilterType::Paeth => {
            // paeth(recon[i-1], 0, 0) is always recon[i-1]
            for i in N..cx {
                recon[i] = recon[i].wrapping_add(recon[i - N]);
            }
        }
        PNGFilterType::Average => {
            for i in N..cx {
                recon[i] = recon[i - N] / 2;
            }
        }
    }
}

// ----------------------------------------------------------------------------
fn unfilter_scanline_n<const N: usize>(
    recon: &mut [u8],
    precon: &[u8],
    filter_type: PNGFilterType,
    cx: usize,
) {
    match filter_type {
        PNGFilterType::None => (),
        PNGFilterType::Sub => {
            for i in N..cx {
                recon[i] = recon[i].wrapping_add(recon[i - N]);
            }
        }
        PNGFilterType::Up => {
            for i in 0..cx {
                recon[i] = recon[i].wrapping_add(precon[i]);
            }
        }
        PNGFilterType::Average => {
            for i in 0..N {
                recon[i] = recon[i].wrapping_add(precon[i] / 2);
            }

            for i in N..cx {
                let pred = (((recon[i - N] as u16) + (precon[i] as u16)) / 2) as u8;
                recon[i] = recon[i].wrapping_add(pred);
            }
        }
        PNGFilterType::Paeth => {
            // paeth(0, precon[i], 0) is always precon[i]
            for i in 0..N {
                recon[i] = recon[i].wrapping_add(precon[i]);
            }

            for i in N..cx {
                recon[i] = recon[i].wrapping_add(paeth(recon[i - N], precon[i], precon[i - N]));
            }
        }
    }
}

// ----------------------------------------------------------------------------
fn unfilter<const N: usize>(data: &mut [u8], line_bytes: usize, cy: usize) -> Result<()> {
    let mut data = data;

    let filter_type = data[0].try_into()?;
    unfilter_scanline_0::<N>(&mut data[1..], filter_type, line_bytes - 1);

    for _ in 1..cy {
        let (prev, line) = data.split_at_mut(line_bytes);
        let filter_type = line[0].try_into()?;
        unfilter_scanline_n::<N>(&mut line[1..], &prev[1..], filter_type, line_bytes - 1);

        data = line;
    }

    Ok(())
}

// ----------------------------------------------------------------------------
fn decode_idat(
    idat: Vec<u8>,
    plte: Vec<u32>,
    ihdr: PNGChunkIHDR,
) -> Result<(PNGChunkIHDR, Vec<u32>, Vec<u8>)> {
    // Check if fcheck is set correctly, compression method is inflate, sliding window is less than 32k,
    // and no dictonary is used as per PNG spec
    let check = ((idat[0] as usize) * 256 + (idat[1] as usize)) % 31;
    let cm = idat[0] & 15;
    let cinfo = (idat[0] >> 4) & 15;
    let fdict = (idat[1] >> 5) & 1;

    if check != 0 || cm != 8 || cinfo > 7 || fdict != 0 {
        return Err(Error::InvalidIDAT);
    }

    let bpp = ihdr.color_type.channels() * ihdr.bit_depth;
    let bpl = ihdr.width.checked_mul(bpp).ok_or(Error::InvalidPng)?;
    let bpl = bpl.div_ceil(8) + 1;
    let size = ihdr.height.checked_mul(bpl).ok_or(Error::InvalidPng)?;

    let mut data = vec![0u8; size];

    if inflate::inflate(&mut data, &idat[2..])? != size {
        return Err(Error::InvalidPng);
    }

    match ihdr.color_type {
        PNGColorType::Greyscale | PNGColorType::IndexedColor => {
            unfilter::<1>(&mut data, bpl, ihdr.height)?;
        }
        PNGColorType::TrueColor => {
            unfilter::<3>(&mut data, bpl, ihdr.height)?;
        }
        PNGColorType::GreyscaleAplha => {
            unfilter::<2>(&mut data, bpl, ihdr.height)?;
        }
        PNGColorType::TrueColorAlpha => {
            unfilter::<4>(&mut data, bpl, ihdr.height)?;
        }
    }

    Ok((ihdr, plte, data))
}

// ----------------------------------------------------------------------------
pub fn png_read(png: &[u8]) -> Result<(PNGChunkIHDR, Vec<u32>, Vec<u8>)> {
    const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if png.len() < 8 || !png.starts_with(&SIGNATURE) {
        return Err(Error::InvalidSignature);
    }

    let mut png = &png[8..png.len()];

    if png.len() < mem::size_of::<PNGChunkHead>() {
        return Err(Error::BufferUnderrun);
    }

    let head = PNGChunkHead {
        length: u32::from_be_bytes(png[0..4].try_into()?),
        r#type: u32::from_be_bytes(png[4..8].try_into()?),
    };

    png = &png[8..png.len()];

    if head.r#type != IHDR {
        return Err(Error::MissingIHDR);
    }

    const IHDR_LEN: usize = 13;
    if head.length as usize != IHDR_LEN || png.len() < IHDR_LEN {
        return Err(Error::BufferUnderrun);
    }

    let ihdr = PNGChunkIHDR {
        width: u32::from_be_bytes(png[0..4].try_into()?) as usize,
        height: u32::from_be_bytes(png[4..8].try_into()?) as usize,
        bit_depth: png[8] as usize,
        color_type: png[9].try_into()?,
        compression: png[10],
        filter: png[11],
        interlace: png[12],
    };

    png = &png[IHDR_LEN + 4..png.len()];

    if ihdr.width == 0
        || ihdr.height == 0
        || ihdr.bit_depth == 0
        || ihdr.compression != 0
        || ihdr.filter != 0
        || ihdr.interlace > 1
    {
        return Err(Error::InvalidFormat);
    }

    if ihdr.interlace != 0 || ihdr.bit_depth > 8 {
        // Adam7 interlace is not supported
        return Err(Error::UnsupportedFormat);
    }

    let mut idat = Vec::with_capacity(png.len());
    let mut plte = Vec::new();

    while !png.is_empty() {
        let head = PNGChunkHead {
            length: u32::from_be_bytes(png[0..4].try_into()?),
            r#type: u32::from_be_bytes(png[4..8].try_into()?),
        };

        png = &png[8..png.len()];

        match head.r#type {
            IDAT => {
                idat.extend_from_slice(&png[0..head.length as usize]);
            }
            IEND => {
                return decode_idat(idat, plte, ihdr);
            }
            PLTE => {
                if !head.length.is_multiple_of(3) || head.length > 256 * 3 {
                    return Err(Error::InvalidPalette);
                }
                for i in (0..head.length as usize).step_by(3) {
                    let r = png[i + 2] as u32;
                    let g = png[i + 1] as u32;
                    let b = png[i] as u32;
                    plte.push((r << 16) | (g << 8) | b);
                }
            }
            _ => {
                // Skip other chunks
            }
        }

        png = &png[head.length as usize + 4..png.len()];
    }

    Err(Error::MissingIEND)
}

// ----------------------------------------------------------------------------
#[test]
fn test_paeth() {
    assert_eq!(paeth(10, 20, 30), 10);
    assert_eq!(paeth(20, 10, 30), 10);
    assert_eq!(paeth(30, 10, 20), 20);
    assert_eq!(paeth(30, 20, 10), 30);
    assert_eq!(paeth(10, 20, 50), 10);
    assert_eq!(paeth(210, 220, 250), 210);
    assert_eq!(paeth(210, 220, 0), 220);
}
