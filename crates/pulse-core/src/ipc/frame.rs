//! Length-prefixed frames: `u32 LE length | UTF-8 JSON body`. Max 4 MiB.

use std::io::{Read, Write};

use crate::error::{PulseError, Result};

pub const MAX_FRAME_BYTES: u32 = 4 * 1024 * 1024;

pub fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> Result<()> {
    if body.len() > MAX_FRAME_BYTES as usize {
        return Err(PulseError::Ipc(format!(
            "frame too large: {} bytes (max {MAX_FRAME_BYTES})",
            body.len()
        )));
    }
    let len = body.len() as u32;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(body)?;
    w.flush()?;
    Ok(())
}

pub fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_FRAME_BYTES {
        return Err(PulseError::Ipc(format!(
            "frame length {len} exceeds max {MAX_FRAME_BYTES}"
        )));
    }
    let mut body = vec![0u8; len as usize];
    if len > 0 {
        r.read_exact(&mut body)?;
    }
    Ok(body)
}

pub fn write_json<W: Write, T: serde::Serialize>(w: &mut W, value: &T) -> Result<()> {
    let body = serde_json::to_vec(value)
        .map_err(|e| PulseError::Ipc(format!("json encode: {e}")))?;
    write_frame(w, &body)
}

pub fn read_json<R: Read, T: serde::de::DeserializeOwned>(r: &mut R) -> Result<T> {
    let body = read_frame(r)?;
    serde_json::from_slice(&body).map_err(|e| PulseError::Ipc(format!("json decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, br#"{"ok":true}"#).unwrap();
        let mut cur = Cursor::new(buf);
        let body = read_frame(&mut cur).unwrap();
        assert_eq!(body, br#"{"ok":true}"#);
    }

    #[test]
    fn rejects_oversized_len() {
        let mut bad = (MAX_FRAME_BYTES + 1).to_le_bytes().to_vec();
        bad.extend_from_slice(&[0u8; 8]);
        let err = read_frame(&mut Cursor::new(bad)).unwrap_err();
        assert!(err.to_string().contains("exceeds"));
    }
}
