// RFC 1951 decompression implementation.
// * https://datatracker.ietf.org/doc/html/rfc1951
// * based on the excellent LodePNG implementation by Lode Vandevenne:
//   https://lodev.org/lodepng/

use super::Error;

// ----------------------------------------------------------------------------
fn show_bits(bp: &usize, src: &[u8], count: usize) -> std::result::Result<u16, Error> {
    let bytepos = *bp >> 3;
    let shift = *bp & 7;
    let mask = (1 << count) - 1;

    if bytepos + 2 < src.len() {
        let bits32 = ((src[bytepos + 2] as u32) << 16)
            | ((src[bytepos + 1] as u32) << 8)
            | (src[bytepos] as u32);
        Ok((bits32 >> shift) as u16 & mask)
    } else if bytepos + 1 < src.len() {
        let bits16 = ((src[bytepos + 1] as u16) << 8) | (src[bytepos] as u16);
        Ok((bits16 >> shift) & mask)
    } else if bytepos + 1 == src.len() {
        let bits8 = src[bytepos] as u16;
        Ok((bits8 >> shift) & mask)
    } else {
        Err(Error::Underflow)
    }
}

// ----------------------------------------------------------------------------
fn read_bits(src: &[u8], sptr: &mut usize, count: usize) -> std::result::Result<u16, Error> {
    let res = show_bits(sptr, src, count)?;
    *sptr += count;
    Ok(res)
}

// ----------------------------------------------------------------------------
fn reverse_bits(x: u16, count: usize) -> u16 {
    let x = ((x & 0x5555) << 1) | ((x >> 1) & 0x5555);
    let x = ((x & 0x3333) << 2) | ((x >> 2) & 0x3333);
    let x = ((x & 0x0f0f) << 4) | ((x >> 4) & 0x0f0f);
    let x = ((x & 0x00ff) << 8) | ((x >> 8) & 0x00ff);
    x >> (16 - count)
}

// ----------------------------------------------------------------------------
const FIRSTBITS: u8 = 9;
const INVALIDSYMBOL: u16 = 32767;

// ----------------------------------------------------------------------------
#[derive(Copy, Clone)]
struct VarLenCode {
    code: u16,
    len: u8,
}

type LookupTable = [VarLenCode; 512 + 512];

// ------------------------------------------------------------------------
fn generate_codes(table: &mut [VarLenCode]) {
    const MAX_CODE_LENGTH: usize = 16;

    // count number of instances of each code length
    let mut code_len_count = [0; MAX_CODE_LENGTH];
    for code in table.iter() {
        code_len_count[code.len as usize] += 1;
    }

    // calculate next code for each code length
    let mut next_code = [0; MAX_CODE_LENGTH];
    for i in 1..MAX_CODE_LENGTH {
        next_code[i] = (next_code[i - 1] + code_len_count[i - 1]) << 1;
    }

    for code in table.iter_mut() {
        if code.len != 0 {
            let len = code.len as usize;
            // Huffman bits are given in MSB first order but the bit reader reads LSB first
            code.code = reverse_bits(next_code[len], len);
            next_code[len] += 1;
        }
    }
}

// ------------------------------------------------------------------------
fn make_lookup_table(inp: &mut [VarLenCode]) -> std::result::Result<LookupTable, Error> {
    const HEADSIZE: usize = 1 << FIRSTBITS; // size of the first table
    const MASK: u16 = (1 << FIRSTBITS) - 1;

    generate_codes(inp);

    // compute maxlens: max total bit length of symbols sharing prefix in the first table
    let mut maxlens = [0; HEADSIZE];
    for code in inp.iter() {
        let l = code.len;
        if l <= FIRSTBITS {
            // symbols that fit in first table don't increase secondary table size
            continue;
        }

        // get the FIRSTBITS MSBs, the MSBs of the symbol are encoded first.
        let index = (code.code & MASK) as usize;
        maxlens[index] = maxlens[index].max(l);
    }

    // compute total table size: size of first table plus all secondary tables for symbols longer than FIRSTBITS
    let mut size = HEADSIZE;
    for l in maxlens.iter() {
        if *l > FIRSTBITS {
            size += 1 << (*l - FIRSTBITS);
        }
    }

    // initialize with an invalid length to indicate unused entries
    let mut table = [VarLenCode { code: 0, len: 16 }; 1024];

    // fill in the first table for long symbols: max prefix size and pointer to secondary tables
    let mut pointer = HEADSIZE;
    for i in 0..HEADSIZE {
        let l = maxlens[i];
        if l <= FIRSTBITS {
            continue;
        }
        table[i].len = l;
        table[i].code = pointer as u16;

        let scondary_table_size = 1 << (l - FIRSTBITS);
        pointer += scondary_table_size;
    }

    // fill in the first table for short symbols, or secondary table for long symbols
    let mut numpresent = 0;
    for (code, i) in inp.iter().zip(0..) {
        let l = code.len;
        if l == 0 {
            continue;
        }

        numpresent += 1;

        if l <= FIRSTBITS {
            // short symbol, fully in first table, replicated num times if l < FIRSTBITS
            let num = 1 << (FIRSTBITS - l);

            for j in 0..num {
                // bit reader will read the l bits of symbol first, the remaining FIRSTBITS - l bits go to the MSB's
                let index = (code.code | (j << l)) as usize;
                if table[index].len != 16 {
                    return Err(Error::OverSubscribedTree);
                }
                table[index].len = l;
                table[index].code = i as u16;
            }
        } else {
            // long symbol, shares prefix with other long symbols in first lookup table, needs second lookup
            // the FIRSTBITS MSBs of the symbol are the first table index
            let index = (code.code & MASK) as usize;
            let maxlen = table[index].len;
            // log2 of secondary table length, should be >= l - FIRSTBITS
            let tablelen = maxlen - FIRSTBITS;
            let start = table[index].code as usize;

            // amount of entries of this symbol in secondary table
            let num = 1 << (tablelen - (l - FIRSTBITS));

            if maxlen < l {
                // invalid tree: long symbol shares prefix with short symbol
                return Err(Error::OverSubscribedTree);
            }

            for j in 0..num {
                let reverse2 = code.code >> FIRSTBITS; // l - FIRSTBITS bits
                let index2 = start + (reverse2 | (j << (l - FIRSTBITS))) as usize;
                table[index2].len = l;
                table[index2].code = i as u16;
            }
        }
    }

    if numpresent < 2 {
        // In case of exactly 1 symbol, in theory the Huffman symbol needs 0 bits,
        // but deflate uses 1 bit instead. In case of 0 symbols, no symbols can
        // appear at all, but such Huffman tree could still exist (e.g. if distance
        // codes are never used). In both cases, not all symbols of the table will be
        // filled in. Fill them in with an invalid symbol value so returning them from
        // read_symbol will cause an error.
        for item in table.iter_mut().take(HEADSIZE) {
            if item.len == 16 {
                item.len = 1;
                item.code = INVALIDSYMBOL;
            }
        }
    } else {
        // A good Huffman tree has N * 2 - 1 nodes, of which N - 1 are internal nodes.
        // If that is not the case (due to too long length codes), the table will not
        // have been fully used, and this is an error (not all bit combinations can be
        // decoded): an undersubscribed Huffman tree
        for item in table.iter().take(size) {
            if item.len == 16 {
                return Err(Error::UnderSubscribedTree);
            }
        }
    }

    Ok(table)
}

// ----------------------------------------------------------------------------
fn read_symbol(
    src: &[u8],
    sptr: &mut usize,
    lookup_table: &LookupTable,
) -> std::result::Result<u16, Error> {
    let bits9 = show_bits(sptr, src, 9)?;
    let idx = bits9 as usize;
    let code0 = &lookup_table[idx];

    if code0.len <= FIRSTBITS {
        *sptr += code0.len as usize;
        return Ok(code0.code);
    }

    *sptr += FIRSTBITS as usize;
    let count = code0.len - FIRSTBITS;

    let bits = show_bits(sptr, src, count as usize)?;
    let idx = code0.code as usize + bits as usize;
    let code = &lookup_table[idx];

    *sptr += code.len as usize - FIRSTBITS as usize;
    Ok(code.code)
}

// ----------------------------------------------------------------------------
fn generate_fixed_luts() -> std::result::Result<(LookupTable, LookupTable), Error> {
    const NUM_DEFLATE_CODE_SYMBOLS: usize = 288;
    let mut table_ll = [VarLenCode { code: 0, len: 8 }; NUM_DEFLATE_CODE_SYMBOLS];
    table_ll[144..256].fill(VarLenCode { code: 0, len: 9 });
    table_ll[256..280].fill(VarLenCode { code: 0, len: 7 });
    let lut_ll = make_lookup_table(&mut table_ll)?;

    const NUM_DISTANCE_SYMBOLS: usize = 32;
    let mut table_d = [VarLenCode { code: 0, len: 5 }; NUM_DISTANCE_SYMBOLS];
    let lut_d = make_lookup_table(&mut table_d)?;

    Ok((lut_ll, lut_d))
}

// ----------------------------------------------------------------------------
fn generate_lut(bitlen: &[u8]) -> std::result::Result<LookupTable, Error> {
    let mut table = Vec::with_capacity(bitlen.len());
    for len in bitlen.iter() {
        table.push(VarLenCode { code: 0, len: *len });
    }
    make_lookup_table(table.as_mut_slice())
}

// ----------------------------------------------------------------------------
fn read_encoded_luts(
    src: &[u8],
    sptr: &mut usize,
) -> std::result::Result<(LookupTable, LookupTable), Error> {
    let ll_len = (read_bits(src, sptr, 5)? + 257) as usize;
    let dt_len = (read_bits(src, sptr, 5)? + 1) as usize;
    let cl_len = (read_bits(src, sptr, 4)? + 4) as usize;

    if ll_len > 286 || dt_len > 30 {
        return Err(Error::InvalidCodeLength);
    }

    const NUM_CODE_LENGTH_CODES: usize = 19;
    let mut table_cl = [VarLenCode { code: 0, len: 0 }; NUM_CODE_LENGTH_CODES];

    const CODE_LEN_PERM: [u8; NUM_CODE_LENGTH_CODES] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    for cl in &CODE_LEN_PERM[..cl_len] {
        table_cl[*cl as usize].len = read_bits(src, sptr, 3)? as u8;
    }

    let vlc_cl = make_lookup_table(&mut table_cl)?;

    let count = ll_len + dt_len;
    const NUM_DEFLATE_CODE_SYMBOLS: usize = 288;
    const NUM_DISTANCE_SYMBOLS: usize = 32;
    let mut bitlen = [0; NUM_DEFLATE_CODE_SYMBOLS + NUM_DISTANCE_SYMBOLS];

    let mut ptr = 0;
    while ptr < count {
        let code = read_symbol(src, sptr, &vlc_cl)?;
        match code {
            0..=15 => {
                bitlen[ptr] = code as u8;
                ptr += 1;
            }
            16 => {
                if ptr == 0 {
                    return Err(Error::InvalidData);
                }
                let len = 3 + read_bits(src, sptr, 2)? as usize;
                if ptr + len >= count {
                    return Err(Error::InvalidData);
                }
                let value = bitlen[ptr - 1];
                bitlen[ptr..ptr + len].fill(value);
                ptr += len;
            }
            17 | 18 => {
                let len = if code == 17 {
                    3 + read_bits(src, sptr, 3)?
                } else {
                    11 + read_bits(src, sptr, 7)?
                } as usize;
                if ptr + len >= count {
                    return Err(Error::InvalidData);
                }
                bitlen[ptr..ptr + len].fill(0);
                ptr += len;
            }
            _ => {
                return Err(Error::InvalidData);
            }
        }
    }

    if bitlen[256] == 0 {
        // end-marker is mandatory
        return Err(Error::InvalidData);
    }

    let lut_ll = generate_lut(&bitlen[0..ll_len])?;
    let lut_d = generate_lut(&bitlen[ll_len..ll_len + dt_len])?;

    Ok((lut_ll, lut_d))
}

// ----------------------------------------------------------------------------
#[rustfmt::skip]
const DIST_INFO: [(u8, u16); 30] = [
    ( 0,    1), ( 0,    2), ( 0,    3), ( 0,    4), ( 1,    5), ( 1,    7), ( 2,    9), ( 2,   13),
    ( 3,   17), ( 3,   25), ( 4,   33), ( 4,   49), ( 5,   65), ( 5,   97), ( 6,  129), ( 6,  193),
    ( 7,  257), ( 7,  385), ( 8,  513), ( 8,  769), ( 9, 1025), ( 9, 1537), (10, 2049), (10, 3073),
    (11, 4097), (11, 6145), (12, 8193), (12,12289), (13,16385), (13,24577),
];

// ----------------------------------------------------------------------------
#[rustfmt::skip]
const CODE_INFO: [(u8, u16); 29] = [
    ( 0,    3), ( 0,    4), ( 0,    5), ( 0,    6), ( 0,    7), ( 0,    8), ( 0,   9), ( 0,   10),
    ( 1,   11), ( 1,   13), ( 1,   15), ( 1,   17), ( 2,   19), ( 2,   23), ( 2,  27), ( 2,   31),
    ( 3,   35), ( 3,   43), ( 3,   51), ( 3,   59), ( 4,   67), ( 4,   83), ( 4,  99), ( 4,  115),
    ( 5,  131), ( 5,  163), ( 5,  195), ( 5,  227), ( 0,  258),
];

// ----------------------------------------------------------------------------
fn inflate_huffman_block(
    dst: &mut [u8],
    dptr: &mut usize,
    src: &[u8],
    sptr: &mut usize,
    trees: (LookupTable, LookupTable),
) -> std::result::Result<(), Error> {
    loop {
        let code_ll = read_symbol(src, sptr, &trees.0)?;
        match code_ll {
            0..=255 => {
                dst[*dptr] = code_ll as u8;
                *dptr += 1;
            }
            256 => {
                return Ok(());
            }
            257..=285 => {
                let idx = (code_ll - 257) as usize;
                let info_ll = CODE_INFO.get(idx).ok_or(Error::InvalidLength)?;

                let start = *dptr;
                let length = info_ll.1 as usize + read_bits(src, sptr, info_ll.0.into())? as usize;

                let code_d = read_symbol(src, sptr, &trees.1)?;
                if code_d == 0 {
                    // distance is 1
                    let value = *dst.get(start - 1).ok_or(Error::InvalidDistance)?;
                    dst.get_mut(start..start + length)
                        .ok_or(Error::InvalidLength)?
                        .fill(value);
                    *dptr += length;
                } else {
                    let idx = code_d as usize;
                    let info_d = DIST_INFO.get(idx).ok_or(Error::InvalidDistance)?;

                    let distance =
                        info_d.1 as usize + read_bits(src, sptr, info_d.0.into())? as usize;

                    if distance > start {
                        return Err(Error::InvalidDistance);
                    }

                    if length > dst.len() - start {
                        return Err(Error::InvalidLength);
                    }

                    let loops = length / distance;
                    let remain = length % distance;

                    for _ in 0..loops {
                        dst.copy_within(start - distance..start, *dptr);
                        *dptr += distance;
                    }

                    dst.copy_within(start - distance..start - distance + remain, *dptr);
                    *dptr += remain;
                }
            }
            _ => {
                return Err(Error::InvalidSymbol);
            }
        }
    }
}

// ----------------------------------------------------------------------------
fn inflate_no_compression(
    dst: &mut [u8],
    dptr: &mut usize,
    src: &[u8],
    sptr: &mut usize,
) -> std::result::Result<(), Error> {
    // align on byte boundary
    *sptr = (*sptr + 7) & (!7);

    let bytepos = *sptr >> 3;

    if bytepos + 4 > src.len() {
        return Err(Error::Underflow);
    }

    let len = src[bytepos] as usize + ((src[bytepos + 1] as usize) << 8);
    let nlen = src[bytepos + 2] as usize + ((src[bytepos + 3] as usize) << 8);

    if len + nlen != 65535 {
        // error: NLEN is not one's complement of LEN
        return Err(Error::InvalidBlockLength);
    }

    if bytepos + 4 + len > src.len() {
        // error, bit pointer will jump past memory
        return Err(Error::Underflow);
    }

    // read the literal data: len bytes are now stored in the out buffer
    dst[*dptr..*dptr + len].copy_from_slice(&src[bytepos + 4..bytepos + 4 + len]);
    *dptr += len;
    *sptr += (4 + len) * 8;

    Ok(())
}

// ----------------------------------------------------------------------------
pub fn inflate(dst: &mut [u8], src: &[u8]) -> std::result::Result<usize, Error> {
    let mut sptr = 0;
    let mut dptr = 0;
    loop {
        let b_final = read_bits(src, &mut sptr, 1)?;
        let b_type = read_bits(src, &mut sptr, 2)?;

        match b_type {
            0 => {
                inflate_no_compression(dst, &mut dptr, src, &mut sptr)?;
            }
            1 => {
                let trees = generate_fixed_luts()?;
                inflate_huffman_block(dst, &mut dptr, src, &mut sptr, trees)?;
            }
            2 => {
                let trees = read_encoded_luts(src, &mut sptr)?;
                inflate_huffman_block(dst, &mut dptr, src, &mut sptr, trees)?;
            }
            _ => {
                return Err(Error::InvalidBlockType);
            }
        }

        if b_final != 0 {
            break;
        }
    }
    Ok(dptr)
}
