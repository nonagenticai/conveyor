//! A stored-only, deterministic ZIP32 writer.
//!
//! ADR 0007 §7 puts OOXML writing in this crate, and an OOXML package is a ZIP. The archive
//! here is written by hand rather than through a compression library for one reason: the
//! determinism gate. Every field has exactly one legal value for a given entry — stored method,
//! no flags, no extra field, no comment, and a fixed 1980-01-01 DOS timestamp — so the same
//! entries can only produce the same bytes. A general-purpose zip crate carrying deflate
//! settings, platform attributes or a clock would make that a property to remember instead of a
//! property of the format this module emits.
//!
//! ZIP32's 4 GiB limits are a refusal rather than a panic: [`archive`] returns the name of the
//! part that did not fit and the caller turns it into a `DeckError`.

/// Local file header signature, `PK\x03\x04`.
const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
/// Central directory header signature, `PK\x01\x02`.
const CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0201_4b50;
/// End of central directory signature, `PK\x05\x06`.
const END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;
/// ZIP 2.0: the first version that covers directories and the format read here.
const VERSION: u16 = 20;
/// Stored, not deflated: one legal method, so no compressor may change a byte.
const METHOD_STORED: u16 = 0;
/// 00:00:00.
const DOS_TIME: u16 = 0;
/// 1980-01-01, the earliest DOS date: `(year - 1980) << 9 | month << 5 | day`.
const DOS_DATE: u16 = 0x0021;

/// CRC-32 (IEEE 802.3) lookup table, the polynomial every ZIP reader uses.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                0xedb8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

/// The CRC-32 of `bytes`, as ZIP headers carry it.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        let slot = ((crc ^ u32::from(byte)) & 0xff) as usize;
        crc = CRC_TABLE[slot] ^ (crc >> 8);
    }
    !crc
}

/// Write `entries` — names and stored bytes, in the given order — as one ZIP archive.
///
/// The result is a pure function of the entries: there is no clock, no platform attribute and
/// no map iteration in the path. On a part whose size or offset does not fit ZIP32, the name of
/// that part is returned so the caller can refuse it by name.
pub(crate) fn archive(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut directory = Vec::new();

    for (name, data) in entries {
        let name_len = u16::try_from(name.len()).map_err(|_| (*name).to_string())?;
        let size = u32::try_from(data.len()).map_err(|_| (*name).to_string())?;
        let offset = u32::try_from(out.len()).map_err(|_| (*name).to_string())?;
        let crc = crc32(data);

        out.extend_from_slice(&LOCAL_FILE_HEADER_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // general-purpose flags
        out.extend_from_slice(&METHOD_STORED.to_le_bytes());
        out.extend_from_slice(&DOS_TIME.to_le_bytes());
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes()); // compressed size
        out.extend_from_slice(&size.to_le_bytes()); // uncompressed size
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        directory.extend_from_slice(&CENTRAL_DIRECTORY_SIGNATURE.to_le_bytes());
        directory.extend_from_slice(&VERSION.to_le_bytes()); // version made by
        directory.extend_from_slice(&VERSION.to_le_bytes()); // version needed
        directory.extend_from_slice(&0u16.to_le_bytes()); // general-purpose flags
        directory.extend_from_slice(&METHOD_STORED.to_le_bytes());
        directory.extend_from_slice(&DOS_TIME.to_le_bytes());
        directory.extend_from_slice(&DOS_DATE.to_le_bytes());
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&name_len.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        directory.extend_from_slice(&0u16.to_le_bytes()); // comment length
        directory.extend_from_slice(&0u16.to_le_bytes()); // disk number
        directory.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }

    let count = u16::try_from(entries.len()).map_err(|_| "archive index".to_string())?;
    let directory_offset = u32::try_from(out.len()).map_err(|_| "central directory".to_string())?;
    let directory_size =
        u32::try_from(directory.len()).map_err(|_| "central directory".to_string())?;
    out.extend_from_slice(&directory);

    out.extend_from_slice(&END_OF_CENTRAL_DIRECTORY_SIGNATURE.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // this disk
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with the central directory
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An entry as the test reader sees it: the header fields this writer promises, plus bytes.
    struct ReadEntry {
        name: String,
        data: Vec<u8>,
        date: u16,
        time: u16,
        method: u16,
        extra: u16,
        offset: usize,
    }

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// CRC-32 the bit-at-a-time way, independent of [`CRC_TABLE`], so the reader is not trusting
    /// the writer's table to verify the writer's table.
    fn bitwise_crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    0xedb8_8320 ^ (crc >> 1)
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// Read an archive back by walking its local headers, then its central directory and EOCD.
    fn read_archive(bytes: &[u8]) -> Vec<ReadEntry> {
        let mut entries: Vec<ReadEntry> = Vec::new();
        let mut at = 0;
        while at + 30 <= bytes.len() && u32_at(bytes, at) == LOCAL_FILE_HEADER_SIGNATURE {
            let local_offset = at;
            assert_eq!(u16_at(bytes, at + 4), VERSION);
            let flags = u16_at(bytes, at + 6);
            let method = u16_at(bytes, at + 8);
            let time = u16_at(bytes, at + 10);
            let date = u16_at(bytes, at + 12);
            let crc = u32_at(bytes, at + 14);
            let compressed = u32_at(bytes, at + 18) as usize;
            let uncompressed = u32_at(bytes, at + 22) as usize;
            let name_len = u16_at(bytes, at + 26) as usize;
            let extra = u16_at(bytes, at + 28);
            let name_start = at + 30;
            let data_start = name_start + name_len + usize::from(extra);
            let data_end = data_start + compressed;
            assert!(data_end <= bytes.len(), "truncated entry data");
            assert_eq!(flags, 0, "no flags are ever set");
            assert_eq!(compressed, uncompressed, "stored entries are not shrunk");
            let name = std::str::from_utf8(&bytes[name_start..name_start + name_len])
                .expect("entry names are UTF-8")
                .to_string();
            let data = bytes[data_start..data_end].to_vec();
            assert_eq!(crc, bitwise_crc32(&data), "CRC mismatch for {name}");
            entries.push(ReadEntry {
                name,
                data,
                date,
                time,
                method,
                extra,
                offset: local_offset,
            });
            at = data_end;
        }

        let directory_offset = at;
        let mut central = 0usize;
        while at + 46 <= bytes.len() && u32_at(bytes, at) == CENTRAL_DIRECTORY_SIGNATURE {
            assert_eq!(u16_at(bytes, at + 4), VERSION);
            assert_eq!(u16_at(bytes, at + 6), VERSION);
            assert_eq!(u16_at(bytes, at + 8), 0);
            assert_eq!(u16_at(bytes, at + 10), METHOD_STORED);
            let crc = u32_at(bytes, at + 16);
            let compressed = u32_at(bytes, at + 20) as usize;
            let uncompressed = u32_at(bytes, at + 24) as usize;
            let name_len = u16_at(bytes, at + 28) as usize;
            let extra = u16_at(bytes, at + 30) as usize;
            let comment = u16_at(bytes, at + 32) as usize;
            let local_offset = u32_at(bytes, at + 42) as usize;
            let name = std::str::from_utf8(&bytes[at + 46..at + 46 + name_len])
                .expect("entry names are UTF-8")
                .to_string();
            let entry = &entries[central];
            assert_eq!(entry.name, name);
            assert_eq!(entry.data.len(), compressed);
            assert_eq!(entry.data.len(), uncompressed);
            assert_eq!(bitwise_crc32(&entry.data), crc);
            assert_eq!(entry.offset, local_offset);
            central += 1;
            at += 46 + name_len + extra + comment;
        }
        assert_eq!(central, entries.len(), "central directory is incomplete");

        assert_eq!(u32_at(bytes, at), END_OF_CENTRAL_DIRECTORY_SIGNATURE);
        assert_eq!(at + 22, bytes.len(), "no archive comment is written");
        assert_eq!(u16_at(bytes, at + 4), 0);
        assert_eq!(u16_at(bytes, at + 6), 0);
        assert_eq!(u16_at(bytes, at + 8) as usize, entries.len());
        assert_eq!(u16_at(bytes, at + 10) as usize, entries.len());
        assert_eq!(u32_at(bytes, at + 12) as usize, at - directory_offset);
        assert_eq!(u32_at(bytes, at + 16) as usize, directory_offset);
        assert_eq!(u16_at(bytes, at + 20), 0);
        entries
    }

    #[test]
    fn an_archive_reads_back_with_its_entries() {
        let entries = [
            ("[Content_Types].xml", b"<Types/>".as_slice()),
            (
                "ppt/media/image1.png",
                &[0x89, b'P', b'N', b'G', 0, 1, 2, 255][..],
            ),
            ("empty.bin", b"".as_slice()),
        ];
        let bytes = archive(&entries).expect("the archive fits");
        let read = read_archive(&bytes);
        assert_eq!(read.len(), entries.len());
        for (entry, (name, data)) in read.iter().zip(entries.iter()) {
            assert_eq!(entry.name, *name);
            assert_eq!(entry.data, *data);
            assert_eq!(entry.method, METHOD_STORED);
            assert_eq!(entry.time, DOS_TIME);
            assert_eq!(entry.date, DOS_DATE);
            assert_eq!(entry.extra, 0);
        }
    }

    #[test]
    fn the_same_entries_produce_the_same_bytes() {
        let entries = [("a.txt", b"one".as_slice()), ("b/c.txt", b"two".as_slice())];
        let first = archive(&entries).expect("first write");
        let second = archive(&entries).expect("second write");
        assert_eq!(first, second);
    }

    #[test]
    fn the_crc_matches_the_known_vector() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        for sample in [
            b"a".as_slice(),
            b"conveyor".as_slice(),
            &[0u8, 255, 128][..],
        ] {
            assert_eq!(crc32(sample), bitwise_crc32(sample));
        }
    }
}
