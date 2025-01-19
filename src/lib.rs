pub mod inflate;

#[derive(Debug, PartialEq)]
pub enum Error {
    Underflow,
    Overflow,
    InvalidHeader,
    InvalidBitstream,
    InvalidBlockType,
    InvalidBlockLength,
    InvalidDistance,
    InvalidLength,
    InvalidSymbol,
    InvalidData,
    InvalidCodeLength,
    UnderSubscribedTree,
    OverSubscribedTree,
}

// ----------------------------------------------------------------------------
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let err = format!("{:?}", self);
        f.write_str(&err)
    }
}

impl std::error::Error for Error {}
