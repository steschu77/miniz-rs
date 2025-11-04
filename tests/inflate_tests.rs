// ----------------------------------------------------------------------------
mod test {

    use miniz::inflate::{inflate, Error};

    #[test]
    fn test_coverage() {
        // edge-case test vectors from
        // https://github.com/madler/zlib/blob/master/test/infcover.c
        let mut out = [0u8; 33025];

        let inp = [0x01, 0x01, 0x00, 0xfe, 0xff, 0x66];
        assert_eq!(inflate(&mut out, &inp), Ok(1), "stored mode");
        assert_eq!(out[0], 0x66);

        let inp = [0x03, 0x00];
        assert_eq!(inflate(&mut out, &inp), Ok(0), "fixed LUTs - no data");

        let inp = [0x2b, 0x1f, 0x05, 0x40, 0x0c, 0x00];
        assert_eq!(inflate(&mut out, &inp), Ok(262), "fixed LUTs - window wrap");
        assert_eq!(out[0], 0x77);

        // dynamic LUTs coverage
        // * all code-tree methods used (length, length repeat, 3 and 7 bit zeros range)
        // * 15 bit literal code
        // * 10 bit distance extra with MSB set spread over last 3 bytes
        let inp = [
            0xed, 0xf6, 0x49, 0x82, 0x24, 0x49, 0x12, 0x04, 0x49, 0xd2, 0xf3, 0xe7, 0xd9, 0xc8,
            0xa2, 0xe6, 0x91, 0x75, 0xec, 0x7d, 0x4e, 0x00, 0xaf, 0x80, 0xff, 0xdf, 0x00, 0x00,
            0xe0, 0x5c, 0x0c, 0x03,
        ];
        assert_eq!(inflate(&mut out, &inp), Ok(2588), "dynamic LUTs coverage");
        assert_eq!(out[0], 0x88);
        assert_eq!(out[1], 0x00);
        // check the 10 bit distance extra
        assert_eq!(out[2588 - 3], 0x88);
        assert_eq!(out[2588 - 2], 0x00);

        let inp = [
            0xed, 0xf6, 0x49, 0x82, 0x24, 0x49, 0x12, 0x04, 0x49, 0xd2, 0xf3, 0xe7, 0xd9, 0xc8,
            0xa2, 0xe6, 0x91, 0x75, 0xec, 0xbd, 0x4f, 0x00, 0xaf, 0x80, 0x00,
        ];
        assert_eq!(
            inflate(&mut out, &inp),
            Err(Error::OverSubscribedTree),
            "oversubscribed 2nd tree"
        );

        // length extra / 1 symbol in dist tree
        let inp = [
            0xed, 0xc0, 0x1, 0x1, 0x0, 0x0, 0x0, 0x40, 0xa0, 0xfb, 0x66, 0x1b, 0x42, 0x2c, 0x4f,
        ];
        assert_eq!(inflate(&mut out, &inp), Ok(516), "length extra");
        assert_eq!(out[0], 0x88);

        let inp = [
            0xed, 0xc0, 0x81, 0x0, 0x0, 0x0, 0x0, 0x80, 0xa0, 0xfd, 0xa9, 0x17, 0xa9, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x6,
        ];
        assert_eq!(inflate(&mut out, &inp), Ok(33025), "window end");

        // inflate_fast TYPE return
        let inp = [0x2, 0x8, 0x20, 0x80, 0x0, 0x3, 0x0];
        assert_eq!(inflate(&mut out, &inp), Ok(0), "inflate_fast TYPE return");

        // header - invalid block type
        assert_eq!(inflate(&mut out, &[0x06]), Err(Error::InvalidBlockType));

        // stored mode - invalid block length
        let inp = [0x01, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(inflate(&mut out, &inp), Err(Error::InvalidBlockLength));

        // too many length/distance codes in encoded LUTs
        let inp = [0xfc, 0x00, 0x00];
        assert_eq!(inflate(&mut out, &inp), Err(Error::InvalidCodeLength));

        // invalid code lengths set
        let inp = [0x04, 0x00, 0xfe, 0xff];
        assert_eq!(inflate(&mut out, &inp), Err(Error::UnderSubscribedTree));

        // invalid bit length repeat
        let inp = [0x04, 0x00, 0x24, 0x49, 0x00];
        let s7 = inflate(&mut out, &inp);
        assert_eq!(s7.err(), Some(Error::InvalidData));

        // invalid bit length repeat
        let inp = [0x04, 0x00, 0x24, 0xe9, 0xff, 0xff];
        let s8 = inflate(&mut out, &inp);
        assert_eq!(s8.err(), Some(Error::InvalidData));
    }

    #[test]
    fn test_functional() {
        let mut out = [0u8; 1024];

        let inp = [
            0xD3, 0xC5, 0x01, 0xB8, 0x80, 0x58, 0x21, 0xC4, 0xC3, 0x33, 0x58, 0x01, 0x88, 0xC0,
            0x74, 0x88, 0x6B, 0x70, 0x88, 0x02, 0x50, 0x02, 0xA7, 0x0E, 0x00,
        ];

        assert_eq!(inflate(&mut out, &inp), Ok(75));
        assert_eq!(
            &out[..75],
            b"------------------------\n--- THIS IS THIS TEST --\n------------------------\n"
        );
    }
}
