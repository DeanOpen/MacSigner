use std::io::{self, Read, Write};

// Chromium caps native-host responses at 1 MiB. Use the same bound for input.
pub const MAX_MESSAGE: usize = 1024 * 1024;

pub fn read_frame(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0; 4];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    reader.read_exact(&mut header[1..])?;
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 || len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid native message length",
        ));
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    Ok(Some(bytes))
}

pub fn write_frame(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid native message length",
        ));
    }
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

pub fn serve(reader: &mut impl Read, writer: &mut impl Write) -> io::Result<()> {
    while let Some(frame) = read_frame(reader)? {
        let reply = crate::esign::protocol::handle(&frame);
        write_frame(
            writer,
            &serde_json::to_vec(&reply).map_err(io::Error::other)?,
        )?;
    }
    Ok(())
}
