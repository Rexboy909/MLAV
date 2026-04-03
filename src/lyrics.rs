use std::fs;
use std::path::Path;
use id3::{Tag, TagLike};

pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
    /// Per-word timestamps from Enhanced LRC `<mm:ss.xx>word` markers.
    /// Empty when unavailable (standard LRC or unsynced).
    pub words: Vec<(u64, String)>,
}

pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
}

/// Load lyrics for the given audio file path.
/// Priority: .lrc sidecar → lrclib.net API → ID3 USLT tag.
/// Intended to be called from a background thread (lrclib fetch is blocking).
pub fn load_for_song(audio_path: &str) -> Option<Lyrics> {
    // 1. Try .lrc sidecar
    let lrc_path = Path::new(audio_path).with_extension("lrc");
    if lrc_path.exists() {
        if let Ok(content) = fs::read_to_string(&lrc_path) {
            if let Some(lyrics) = parse_lrc(&content) {
                return Some(lyrics);
            }
        }
    }

    // Read ID3 tags once; used for both lrclib and USLT fallback
    let tag = Tag::read_from_path(audio_path).ok();

    // 2. Try lrclib.net
    {
        // Prefer ID3 tags. When absent, try to parse the filename stem.
        // Handles common patterns:
        //   "Sleep Token - Gods"  → title="Gods",        artist="Sleep Token"
        //   "Gods Sleep Token"    → title="Gods Sleep Token", artist="" (freetext fallback)
        let (title, artist) = if let Some(ref t) = tag {
            let ti = t.title().filter(|s| !s.is_empty()).map(|s| s.to_string());
            let ar = t.artist().unwrap_or("").to_string();
            (ti, ar)
        } else {
            (None, String::new())
        };

        let (title, artist) = if title.is_some() {
            (title, artist)
        } else {
            // Parse filename stem
            let stem = Path::new(audio_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            // "Artist - Title" or "Title - Artist"
            if let Some(dash) = stem.find(" - ") {
                let left  = stem[..dash].trim().to_string();
                let right = stem[dash + 3..].trim().to_string();
                // Heuristic: lrclib will sort by relevance; try both orderings
                // by passing the shorter part as title (usually the song title)
                if left.len() <= right.len() {
                    (Some(left), right)
                } else {
                    (Some(right), left)
                }
            } else {
                (Some(stem), String::new())
            }
        };

        let dur = tag.as_ref().and_then(|t| t.duration());

        if let Some(title) = title {
            if let Some(lyrics) = fetch_from_lrclib(&title, &artist, dur) {
                return Some(lyrics);
            }
        }
    }

    // 3. Try ID3 USLT tag (unsynced lyrics)
    if let Some(t) = tag {
        let mut lyric_lines: Vec<LyricLine> = Vec::new();
        for lyr in t.lyrics() {
            for line_text in lyr.text.lines() {
                if !line_text.trim().is_empty() {
                    lyric_lines.push(LyricLine {
                        time_ms: 0,
                        text: line_text.to_string(),
                        words: Vec::new(),
                    });
                }
            }
            if !lyric_lines.is_empty() {
                return Some(Lyrics { lines: lyric_lines, synced: false });
            }
        }
    }

    None
}

/// Fetch synced (or plain) lyrics from lrclib.net.
/// Uses /api/search (accepts partial title/artist) and picks the best result.
pub fn fetch_from_lrclib(title: &str, artist: &str, duration_secs: Option<u32>) -> Option<Lyrics> {
    eprintln!("lrclib: fetching for title={:?} artist={:?}", title, artist);
    let mut req = ureq::get("https://lrclib.net/api/search")
        .set("User-Agent", "MLAV/0.1 (github.com/Rexboy909/MLAV)")
        .query("track_name", title);
    if !artist.is_empty() {
        req = req.query("artist_name", artist);
    }

    let resp = match req.call() {
        Ok(r) => r,
        Err(e) => { eprintln!("lrclib: request failed: {e}"); return None; }
    };
    let results: serde_json::Value = match resp.into_json() {
        Ok(j) => j,
        Err(e) => { eprintln!("lrclib: JSON parse failed: {e}"); return None; }
    };

    // If track_name search returned nothing, retry with free-text ?q= combining title + artist
    let results = if results.as_array().map(|a| a.is_empty()).unwrap_or(true) {
        let q = if artist.is_empty() { title.to_string() } else { format!("{} {}", title, artist) };
        eprintln!("lrclib: 0 results, retrying with q={:?}", q);
        let resp2 = match ureq::get("https://lrclib.net/api/search")
            .set("User-Agent", "MLAV/0.1 (github.com/Rexboy909/MLAV)")
            .query("q", &q)
            .call() {
            Ok(r) => r,
            Err(e) => { eprintln!("lrclib: retry failed: {e}"); return None; }
        };
        match resp2.into_json() {
            Ok(j) => j,
            Err(e) => { eprintln!("lrclib: retry JSON parse failed: {e}"); return None; }
        }
    } else {
        results
    };

    let arr = results.as_array()?;
    eprintln!("lrclib: {} results", arr.len());

    // Pick the best entry: prefer one whose duration is closest to ours,
    // then prefer synced over plain. Fall back to first entry if nothing matches.
    let scored = arr.iter().filter_map(|entry| {
        let has_synced = entry["syncedLyrics"].as_str().map(|s| !s.is_empty()).unwrap_or(false);
        let has_plain  = entry["plainLyrics"].as_str().map(|s| !s.is_empty()).unwrap_or(false);
        if !has_synced && !has_plain { return None; }
        let dur_delta = match (duration_secs, entry["duration"].as_f64()) {
            (Some(want), Some(got)) => ((want as f64) - got).abs() as u32,
            _ => u32::MAX / 2,
        };
        Some((dur_delta, !has_synced as u8, entry))
    });

    // sort: smallest dur_delta first, then synced (0) before plain (1)
    let best = scored.min_by_key(|(delta, synced_rank, _)| (*delta, *synced_rank))?;
    let entry = best.2;

    // Prefer synced lyrics (may be Enhanced LRC with per-word timestamps)
    if let Some(synced) = entry["syncedLyrics"].as_str() {
        if !synced.is_empty() {
            if let Some(lyrics) = parse_lrc(synced) {
                eprintln!("lrclib: using synced lyrics");
                return Some(lyrics);
            }
        }
    }

    // Fall back to plain lyrics
    if let Some(plain) = entry["plainLyrics"].as_str() {
        if !plain.is_empty() {
            let lines: Vec<LyricLine> = plain.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| LyricLine { time_ms: 0, text: l.to_string(), words: Vec::new() })
                .collect();
            if !lines.is_empty() {
                eprintln!("lrclib: using plain lyrics");
                return Some(Lyrics { lines, synced: false });
            }
        }
    }

    None
}

fn parse_lrc(content: &str) -> Option<Lyrics> {
    let mut lines: Vec<LyricLine> = Vec::new();

    for raw_line in content.lines() {
        let raw_line = raw_line.trim();
        if raw_line.is_empty() { continue; }

        // Collect all leading [mm:ss.xx] timestamp tags
        let mut pos = 0;
        let mut timestamps: Vec<u64> = Vec::new();

        while pos < raw_line.len() && raw_line.as_bytes()[pos] == b'[' {
            if let Some(close) = raw_line[pos..].find(']') {
                let tag_inner = &raw_line[pos + 1..pos + close];
                if let Some(ms) = parse_timestamp(tag_inner) {
                    timestamps.push(ms);
                }
                pos += close + 1;
            } else {
                break;
            }
        }

        if timestamps.is_empty() { continue; }

        let text_raw = raw_line[pos..].trim();

        // Enhanced LRC: text contains <mm:ss.xx> per-word markers
        let (text, words) = if text_raw.contains('<') {
            (strip_word_timestamps(text_raw), parse_word_timestamps(text_raw))
        } else {
            (text_raw.to_string(), Vec::new())
        };

        for ts in timestamps {
            lines.push(LyricLine { time_ms: ts, text: text.clone(), words: words.clone() });
        }
    }

    if lines.is_empty() { return None; }

    lines.sort_by_key(|l| l.time_ms);
    Some(Lyrics { lines, synced: true })
}

/// Remove all `<...>` markers from an Enhanced LRC text fragment, yielding plain text.
fn strip_word_timestamps(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    while let Some(open) = remaining.find('<') {
        result.push_str(&remaining[..open]);
        if let Some(close) = remaining[open..].find('>') {
            remaining = &remaining[open + close + 1..];
        } else {
            result.push_str(&remaining[open..]);
            return result.trim().to_string();
        }
    }
    result.push_str(remaining);
    result.trim().to_string()
}

/// Extract `(time_ms, word)` pairs from an Enhanced LRC text fragment.
/// E.g. `<01:23.45>Hello <01:23.89>world` → `[(83345, "Hello"), (83369, "world")]`
fn parse_word_timestamps(text: &str) -> Vec<(u64, String)> {
    let mut words: Vec<(u64, String)> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut current_ts: Option<u64> = None;
    let mut word_buf = String::new();

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(rel) = text[i..].find('>') {
                let tag = &text[i + 1..i + rel];
                i += rel + 1;
                if let Some(ts) = parse_timestamp(tag) {
                    // Commit whatever accumulated for the previous timestamp
                    if let Some(prev_ts) = current_ts {
                        let w = word_buf.trim().to_string();
                        if !w.is_empty() { words.push((prev_ts, w)); }
                        word_buf.clear();
                    }
                    current_ts = Some(ts);
                }
                // Malformed or end-of-line marker with no text: keep going
            } else {
                if current_ts.is_some() { word_buf.push_str(&text[i..]); }
                break;
            }
        } else {
            let next_open = text[i..].find('<').map(|p| i + p).unwrap_or(bytes.len());
            if current_ts.is_some() { word_buf.push_str(&text[i..next_open]); }
            i = next_open;
        }
    }

    // Flush last word
    if let Some(prev_ts) = current_ts {
        let w = word_buf.trim().to_string();
        if !w.is_empty() { words.push((prev_ts, w)); }
    }

    words
}

/// Parse a timestamp string (without brackets) such as `mm:ss.xx` into milliseconds.
/// Returns `None` for metadata tags like `ti:Title` (contain letters before the first colon).
fn parse_timestamp(s: &str) -> Option<u64> {
    let colon = s.find(':')?;
    let mins_str = &s[..colon];
    if mins_str.chars().any(|c| c.is_alphabetic()) {
        return None; // metadata tag, not a timestamp
    }
    let mins: u64 = mins_str.trim().parse().ok()?;
    let rest = &s[colon + 1..];

    // Seconds + optional fractional part separated by '.' or ':'
    let (secs_str, frac_str) = if let Some(dot) = rest.find('.') {
        (&rest[..dot], &rest[dot + 1..])
    } else if let Some(col) = rest.find(':') {
        (&rest[..col], &rest[col + 1..])
    } else {
        (rest, "0")
    };

    let secs: u64 = secs_str.trim().parse().ok()?;
    let frac = frac_str.trim();
    let frac_ms: u64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        3 => frac.parse::<u64>().ok()?,
        _ => frac[..3].parse::<u64>().ok()?,
    };

    Some(mins * 60_000 + secs * 1_000 + frac_ms)
}

/// Return the index of the currently active lyric line.
/// Returns the last line whose `time_ms` is <= `pos_ms`.
pub fn get_current_index(lines: &[LyricLine], pos_ms: u64) -> usize {
    if lines.is_empty() { return 0; }
    // partition_point gives the first index where time_ms > pos_ms
    let idx = lines.partition_point(|l| l.time_ms <= pos_ms);
    if idx == 0 { 0 } else { idx - 1 }
}
