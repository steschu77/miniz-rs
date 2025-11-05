// ZIP parsing implementation.
// * https://pkwaredownloads.blob.core.windows.net/pkware-general/Documentation/APPNOTE-6.3.9.TXT
use crate::inflate;

// ----------------------------------------------------------------------------
#[derive(Debug, PartialEq)]
pub enum Error {
    InvalidZip,
    NoCentralDirectory,
    InvalidSignature,
    InvalidCompressionMethod,
    FileNotFound,
    CompressionError,
    BufferError,
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
#[derive(Debug)]
pub struct File {
    pub name: String,
    pub offset: usize,
}

// ----------------------------------------------------------------------------
fn read_eocd(data: &[u8]) -> Result<(usize, usize, u16)> {
    const MAX_COMMENT_LEN: usize = 0x10000;
    const EOCD_SIZE: usize = 22;
    let start = data.len().saturating_sub(EOCD_SIZE + MAX_COMMENT_LEN);
    let end = data.len().saturating_sub(EOCD_SIZE - 4);
    for i in (start..end).rev() {
        if data[i..i + 4] == [0x50, 0x4b, 0x05, 0x06] {
            let data = &data[i..i + 20];
            let cd_size = u32::from_le_bytes(data[12..16].try_into()?) as usize;
            let cd_offset = u32::from_le_bytes(data[16..20].try_into()?) as usize;
            let total_entries = u16::from_le_bytes(data[10..12].try_into()?);
            return Ok((cd_size, cd_offset, total_entries));
        }
    }
    Err(Error::NoCentralDirectory)
}

// ----------------------------------------------------------------------------
fn read_cd(data: &[u8], total_entries: u16) -> Result<Vec<File>> {
    let mut data = data;
    let mut entries = Vec::new();

    for _ in 0..total_entries {
        if !data.starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err(Error::InvalidSignature);
        }

        let name_len = u16::from_le_bytes(data[28..30].try_into()?) as usize;
        let extra_len = u16::from_le_bytes(data[30..32].try_into()?) as usize;
        let comment_len = u16::from_le_bytes(data[32..34].try_into()?) as usize;
        let offset = u32::from_le_bytes(data[42..46].try_into()?) as usize;
        let name = String::from_utf8_lossy(&data[46..46 + name_len]).into_owned();

        entries.push(File { name, offset });

        data = &data[46 + name_len + extra_len + comment_len..];
    }

    Ok(entries)
}

// ----------------------------------------------------------------------------
fn extract_file(data: &[u8], file: &File) -> Result<Vec<u8>> {
    println!("{file:?}",);
    let ofs = file.offset;
    let hdr = &data[ofs..ofs + 30];

    if !data.starts_with(&[0x50, 0x4b, 0x03, 0x04]) {
        return Err(Error::InvalidSignature);
    }

    let compression_method = u16::from_le_bytes(hdr[8..10].try_into()?);
    let compressed_size = u32::from_le_bytes(hdr[18..22].try_into()?) as usize;
    let uncompressed_size = u32::from_le_bytes(hdr[22..26].try_into()?) as usize;
    let name_len = u16::from_le_bytes(hdr[26..28].try_into()?) as usize;
    let extra_len = u16::from_le_bytes(hdr[28..30].try_into()?) as usize;

    let ofs = ofs + 30 + name_len + extra_len;
    let compressed = &data[ofs..ofs + compressed_size];

    match compression_method {
        0 => Ok(compressed.into()),
        8 => {
            let mut uncompressed = vec![0u8; uncompressed_size];
            if inflate::inflate(&mut uncompressed, compressed)? != uncompressed_size {
                return Err(Error::InvalidZip);
            }
            Ok(uncompressed)
        }
        _ => Err(Error::InvalidCompressionMethod),
    }
}

// ----------------------------------------------------------------------------
pub fn zip_read(data: &[u8], files: &[File], name: &str) -> Result<Vec<u8>> {
    for file in files {
        if file.name == name {
            return extract_file(data, file);
        }
    }
    Err(Error::FileNotFound)
}

// ----------------------------------------------------------------------------
pub fn zip_open(data: &[u8]) -> Result<Vec<File>> {
    let (cd_size, cd_offset, total_entries) = read_eocd(data)?;
    read_cd(&data[cd_offset..cd_offset + cd_size], total_entries)
}
