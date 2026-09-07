//! Decode complete SSE lines from bytes, preserving split UTF-8 characters.

#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 2_000_000 {
            return Err("SSE frame is too large".into());
        }
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line = std::str::from_utf8(&self.pending[..end])
                .map_err(|e| format!("Invalid UTF-8 in SSE: {e}"))?
                .trim_end_matches('\r')
                .to_string();
            self.pending.drain(..=end);
            lines.push(line);
        }
        Ok(lines)
    }

    pub fn finish(&self) -> Result<(), String> {
        if self.pending.iter().all(u8::is_ascii_whitespace) {
            Ok(())
        } else {
            Err("SSE ended in a partial frame".into())
        }
    }
}

/// Missing finish reasons are accepted for compatible servers only when a
/// complete response/[DONE] was received. Explicit truncation always fails.
pub fn check_finish(reason: Option<&str>) -> Result<(), String> {
    match reason {
        None | Some("stop") | Some("tool_calls") => Ok(()),
        Some(other) => Err(format!("Model output is incomplete ({other}); no changes were applied. Increase the output limit or shorten the input.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_boundary_preserves_korean_and_emoji() {
        let input = "data: {\"text\":\"한글 🚀\"}\r\n\n";
        for split in 0..=input.len() {
            let mut decoder = Decoder::default();
            let mut lines = decoder.push(&input.as_bytes()[..split]).unwrap();
            lines.extend(decoder.push(&input.as_bytes()[split..]).unwrap());
            assert_eq!(lines, vec!["data: {\"text\":\"한글 🚀\"}", ""]);
            decoder.finish().unwrap();
        }
    }

    #[test]
    fn rejects_truncated_frames_and_completions() {
        let mut decoder = Decoder::default();
        decoder.push(b"data: {\"unfinished\":").unwrap();
        assert!(decoder.finish().is_err());
        for reason in ["length", "content_filter", "error"] {
            assert!(check_finish(Some(reason)).is_err());
        }
        assert!(check_finish(Some("stop")).is_ok());
        assert!(check_finish(Some("tool_calls")).is_ok());
    }
}
