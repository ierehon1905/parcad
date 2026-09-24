//! `tEXt` entries in a PNG: how a thumbnail records the script it was drawn from.
//!
//! Inside the image rather than beside it, so the picture and its provenance
//! cannot be copied, deleted or restored apart.

use std::io::{BufRead, Seek};

/// `png` re-encoded with `entries` as `tEXt` chunks. Decoding it first is
/// also what refuses an upload that is not a PNG at all.
pub fn with_text(png: &[u8], entries: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let bad = |e: &dyn std::fmt::Display| format!("the preview is not a readable PNG: {e}");
    let mut reader = png::Decoder::new(std::io::Cursor::new(png)).read_info().map_err(|e| bad(&e))?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or_else(|| bad(&"too large"))?];
    let frame = reader.next_frame(&mut pixels).map_err(|e| bad(&e))?;

    let mut out = Vec::with_capacity(png.len() + 128);
    let mut encoder = png::Encoder::new(&mut out, frame.width, frame.height);
    encoder.set_color(frame.color_type);
    encoder.set_depth(frame.bit_depth);
    for (keyword, text) in entries {
        encoder.add_text_chunk(keyword.to_string(), text.to_string()).map_err(|e| e.to_string())?;
    }
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&pixels[..frame.buffer_size()]).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Every `tEXt` entry before the image data; the pixels are never read.
pub fn text(file: impl BufRead + Seek) -> Vec<(String, String)> {
    let Ok(reader) = png::Decoder::new(file).read_info() else {
        return Vec::new();
    };
    reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .map(|chunk| (chunk.keyword.clone(), chunk.text.clone()))
        .collect()
}

/// A 1 × 1 PNG as a browser canvas encodes one.
#[cfg(test)]
pub(crate) fn pixel() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_written_are_the_entries_read() {
        let png = with_text(&pixel(), &[("parcad:source", "abc123"), ("parcad:look", "1")]).unwrap();
        assert_eq!(
            text(std::io::Cursor::new(&png)),
            vec![("parcad:source".into(), "abc123".into()), ("parcad:look".into(), "1".into())]
        );
    }

    #[test]
    fn a_png_without_entries_reads_as_none_and_a_non_png_is_refused() {
        assert!(text(std::io::Cursor::new(pixel())).is_empty());
        assert!(text(std::io::Cursor::new(b"not a png".to_vec())).is_empty());
        assert!(with_text(b"not a png", &[]).is_err());
    }
}
