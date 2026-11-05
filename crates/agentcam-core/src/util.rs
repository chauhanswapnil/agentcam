//! Small helpers shared by the parsers.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

/// Read a JSONL file record by record. Calls `f` with each record and the
/// byte offset of its line. Returns how many lines weren't valid JSON.
pub fn for_each_record(path: &Path, mut f: impl FnMut(Value, u64)) -> io::Result<u32> {
    let mut reader = BufReader::with_capacity(256 * 1024, File::open(path)?);
    let mut buf = Vec::new();
    let mut offset = 0u64;
    let mut bad = 0;
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        let line = buf.trim_ascii();
        if !line.is_empty() {
            match serde_json::from_slice::<Value>(line) {
                Ok(v) => f(v, offset),
                // A half-written last line is normal while an agent is running.
                Err(_) => bad += 1,
            }
        }
        offset += n as u64;
    }
    Ok(bad)
}

/// `v[key]` as a non-empty `&str`.
pub fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Join the text of a content value: a plain string, or a list of blocks
/// with `text` fields.
pub fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let parts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Milliseconds since the Unix epoch for an RFC 3339 UTC timestamp like
/// `2026-11-05T18:03:44.695Z`. Returns `None` for anything else.
pub fn parse_ts_ms(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut ms = 0;
    if b.get(19) == Some(&b'.') {
        let frac: String = ts[20..].chars().take_while(char::is_ascii_digit).collect();
        let padded = format!("{frac:0<3}");
        ms = padded[..3].parse().ok()?;
    }
    // Days from civil, Howard Hinnant's algorithm.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + mi) * 60_000 + s * 1000 + ms)
}

/// Milliseconds between two timestamps, if both parse and end >= start.
pub fn duration_ms(start: Option<&str>, end: Option<&str>) -> Option<u64> {
    let (a, b) = (parse_ts_ms(start?)?, parse_ts_ms(end?)?);
    u64::try_from(b - a).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timestamps() {
        assert_eq!(parse_ts_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts_ms("1970-01-01T00:00:01.5Z"), Some(1500));
        assert_eq!(
            parse_ts_ms("2026-11-05T00:00:00.000Z"),
            Some(1_793_836_800_000)
        );
        assert_eq!(parse_ts_ms("nope"), None);
    }

    #[test]
    fn durations() {
        assert_eq!(
            duration_ms(
                Some("2026-01-01T00:00:00Z"),
                Some("2026-01-01T00:00:02.250Z")
            ),
            Some(2250)
        );
        assert_eq!(
            duration_ms(Some("2026-01-01T00:00:02Z"), Some("2026-01-01T00:00:00Z")),
            None
        );
    }
}
