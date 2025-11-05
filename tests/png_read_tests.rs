// ----------------------------------------------------------------------------
mod test {
    use miniz::png_read::png_read;

    #[allow(dead_code)]
    fn write_result(data: Vec<u8>) {
        use std::io::Write;
        let mut f = std::fs::File::create("basn2c08.rs").unwrap();
        let _ = writeln!(f, "pub const BASN2C08_IMG: [u8; 160] = [");
        for (i, byte) in data.iter().enumerate() {
            let _ = write!(f, "0x{byte:02x}, ");
            if i % (3 * 32 + 1) == (3 * 32) || i == data.len() - 1 {
                let _ = writeln!(f);
            }
        }
    }

    include!("../assets/png/basn0g01.rs");
    include!("../assets/png/basn2c08.rs");
    include!("../assets/png/f99n0g04.rs");

    #[test]
    fn test_basn0g01() {
        let (png, plte, data) = png_read(BASN0G01_PNG).unwrap();
        assert_eq!(png.width, 32, "width");
        assert_eq!(plte.len(), 0, "palette");
        assert_eq!(data, BASN0G01_IMG, "data")
    }

    #[test]
    fn test_basn2c08() {
        let (png, plte, data) = png_read(BASN2C08_PNG).unwrap();
        assert_eq!(png.width, 32, "width");
        assert_eq!(plte.len(), 0, "palette");
        assert_eq!(data, BASN2C08_IMG, "data");
    }

    #[test]
    fn test_f99n0g04() {
        let (png, plte, data) = png_read(F99N0G04_PNG).unwrap();
        assert_eq!(png.width, 32, "width");
        assert_eq!(plte.len(), 0, "palette");
        assert_eq!(data, F99N0G04_IMG, "data");
    }
}
