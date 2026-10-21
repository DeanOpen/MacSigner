use std::io::{self, Read, Seek, SeekFrom};

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid Mach-O module header")
}

fn number(bytes: &[u8], little_endian: bool) -> io::Result<u32> {
    let bytes = bytes.try_into().map_err(|_| invalid())?;
    Ok(if little_endian {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

pub(super) fn architectures(reader: &mut (impl Read + Seek)) -> io::Result<Vec<u32>> {
    let mut header = [0_u8; 8];
    reader.seek(SeekFrom::Start(0))?;
    reader.read_exact(&mut header)?;
    let (little_endian, universal, entry_size) = match &header[..4] {
        [0xce, 0xfa, 0xed, 0xfe] | [0xcf, 0xfa, 0xed, 0xfe] => (true, false, 0),
        [0xfe, 0xed, 0xfa, 0xce] | [0xfe, 0xed, 0xfa, 0xcf] => (false, false, 0),
        [0xca, 0xfe, 0xba, 0xbe] => (false, true, 20),
        [0xbe, 0xba, 0xfe, 0xca] => (true, true, 20),
        [0xca, 0xfe, 0xba, 0xbf] => (false, true, 32),
        [0xbf, 0xba, 0xfe, 0xca] => (true, true, 32),
        _ => return Err(invalid()),
    };
    let value = number(&header[4..], little_endian)?;
    if !universal {
        return Ok(vec![value]);
    }
    if value == 0 || value > 64 {
        return Err(invalid());
    }
    let mut architectures = Vec::new();
    let mut entry = vec![0_u8; entry_size];
    for _ in 0..value {
        reader.read_exact(&mut entry)?;
        architectures.push(number(&entry[..4], little_endian)?);
    }
    Ok(architectures)
}

pub(super) fn matches(cpu: u32, architecture: &str) -> bool {
    matches!(
        (cpu, architecture),
        (0x0100_000c, "aarch64") | (0x0100_0007, "x86_64") | (7, "x86")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn thin_modules_require_the_process_architecture() {
        for (magic, cpu) in [
            ([0xcf, 0xfa, 0xed, 0xfe], 0x0100_000c_u32),
            ([0xce, 0xfa, 0xed, 0xfe], 7),
        ] {
            let bytes = [magic.as_slice(), &cpu.to_le_bytes()].concat();
            assert_eq!(architectures(&mut Cursor::new(bytes)).unwrap(), vec![cpu]);
        }
        assert!(matches(0x0100_000c, "aarch64"));
        assert!(!matches(0x0100_0007, "aarch64"));
        assert!(!matches(0x0100_000c, "x86_64"));
    }

    #[test]
    fn universal_headers_support_both_widths_and_byte_orders() {
        for (magic, little_endian, width) in [
            ([0xca, 0xfe, 0xba, 0xbe], false, 20),
            ([0xbe, 0xba, 0xfe, 0xca], true, 20),
            ([0xca, 0xfe, 0xba, 0xbf], false, 32),
            ([0xbf, 0xba, 0xfe, 0xca], true, 32),
        ] {
            let encode = |value: u32| {
                if little_endian {
                    value.to_le_bytes()
                } else {
                    value.to_be_bytes()
                }
            };
            let mut bytes = [magic.as_slice(), &encode(2)].concat();
            for cpu in [0x0100_0007, 0x0100_000c] {
                bytes.extend(encode(cpu));
                bytes.extend(vec![0; width - 4]);
            }
            assert_eq!(
                architectures(&mut Cursor::new(bytes)).unwrap(),
                vec![0x0100_0007, 0x0100_000c]
            );
        }
    }

    #[test]
    fn malformed_and_unbounded_headers_are_rejected() {
        for bytes in [
            vec![],
            b"MZnotmac".to_vec(),
            [0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 0].to_vec(),
            [0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65].to_vec(),
            [0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 1].to_vec(),
        ] {
            assert!(architectures(&mut Cursor::new(bytes)).is_err());
        }
    }
}
