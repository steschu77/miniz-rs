// ZIP tests
// * https://github.com/nih-at/libzip/tree/main/regress

// ----------------------------------------------------------------------------
mod test {
    use miniz::zip_read::{zip_open, zip_read};

    #[allow(dead_code)]
    fn write_result(data: Vec<u8>) {
        use std::io::Write;
        let mut f = std::fs::File::create("file.rs").unwrap();
        let _ = writeln!(f, "pub const FILE1: [u8; {}] = [", data.len());
        for (i, byte) in data.iter().enumerate() {
            let _ = write!(f, "0x{byte:02x}, ");
            if i % (16) == (15) || i == data.len() - 1 {
                let _ = writeln!(f);
            }
        }
    }

    include!("../assets/zip/comments.rs");
    include!("../assets/zip/deflated.rs");
    include!("../assets/zip/folder.rs");
    include!("../assets/zip/utf8.rs");

    #[test]
    fn test_comments_zip() {
        let zip = zip_open(&COMMENTS_ZIP).unwrap();
        assert_eq!(zip.len(), 4);
        let file1 = zip_read(&COMMENTS_ZIP, &zip, "file1").unwrap();
        let file2 = zip_read(&COMMENTS_ZIP, &zip, "file2").unwrap();
        let file3 = zip_read(&COMMENTS_ZIP, &zip, "file3").unwrap();
        let file4 = zip_read(&COMMENTS_ZIP, &zip, "file4").unwrap();
        assert_eq!(&file1, &FILE1);
        assert_eq!(&file2, &FILE2);
        assert_eq!(&file3, &FILE3);
        assert_eq!(&file4, &FILE4);
    }

    #[test]
    fn test_deflated_zip() {
        let zip = zip_open(&DEFLATED_ZIP).unwrap();
        assert_eq!(zip.len(), 2);
        let first = zip_read(&DEFLATED_ZIP, &zip, "first").unwrap();
        let second = zip_read(&DEFLATED_ZIP, &zip, "second").unwrap();
        assert_eq!(&first, &FIRST);
        assert_eq!(&second, &SECOND);
    }

    #[test]
    fn test_folder_zip() {
        let zip = zip_open(&FOLDER_ZIP).unwrap();
        assert_eq!(zip.len(), 3);
        let test = zip_read(&FOLDER_ZIP, &zip, "test").unwrap();
        let test2 = zip_read(&FOLDER_ZIP, &zip, "testdir/test2").unwrap();
        assert_eq!(&test, &TEST);
        assert_eq!(&test2, &TEST);
    }

    #[test]
    fn test_utf8_zip() {
        let zip = zip_open(&UTF8_ZIP).unwrap();
        assert_eq!(zip.len(), 1);
        assert_eq!(zip[0].name, UTF8_NAME);
    }
}
