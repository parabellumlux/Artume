use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Quick classification based on magic bytes of the file.
pub fn classify_magic(path: &Path) -> std::io::Result<&'static str> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 16];
    let bytes_read = file.read(&mut header)?;

    if bytes_read < 4 {
        return Ok("unknown/empty");
    }

    // Check magic signatures
    match &header[0..4] {
        [0x89, 0x50, 0x4E, 0x47] => Ok("image/png"),
        [0xFF, 0xD8, 0xFF, _] => Ok("image/jpeg"),
        [0x47, 0x49, 0x46, 0x38] => Ok("image/gif"),
        [0x25, 0x50, 0x44, 0x46] => Ok("application/pdf"),
        [0x50, 0x4B, 0x03, 0x04] => Ok("application/zip"),
        [0x7F, 0x45, 0x4C, 0x46] => Ok("application/x-elf"),
        [0x49, 0x44, 0x33, _] => Ok("audio/mpeg"), // MP3 with ID3 tag
        _ => {
            if bytes_read >= 12 && &header[8..12] == b"WAVE" && &header[0..4] == b"RIFF" {
                return Ok("audio/wav");
            }
            if bytes_read >= 8 && &header[4..8] == b"ftyp" {
                return Ok("video/mp4");
            }
            // Check if it's text (simple utf-8 check on first few bytes)
            if is_utf8(&header[..bytes_read]) {
                // If it starts with JSON structure
                let trimmed = header[..bytes_read].iter().find(|&&b| !b.is_ascii_whitespace());
                if let Some(&b'{') | Some(&b'[') = trimmed {
                    return Ok("text/json");
                }
                return Ok("text/plain");
            }
            Ok("application/octet-stream")
        }
    }
}

fn is_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}
