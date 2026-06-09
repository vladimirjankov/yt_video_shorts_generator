use serde::{Deserialize, Serialize};

/// Caption style requested from the API. When omitted, no subtitles are burned.
/// Each variant maps to a bundled font (see `../fonts`) plus an accent colour and
/// pacing tuned to that creator's look.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptionStyle {
    /// Montserrat ExtraBold — clean, modern, authoritative.
    Hormozi,
    /// Anton — high-energy condensed (Burbank look-alike).
    MrBeast,
    /// Bangers — comic, humorous (Komika look-alike).
    Comic,
    /// Archivo Black — industrial, punchy (The Bold Font look-alike).
    Bold,
    /// Inter — tech-focused, sleek, neutral.
    Tech,
}

impl CaptionStyle {
    /// Family name as embedded in the bundled font files in `../fonts`.
    pub fn font_family(self) -> &'static str {
        match self {
            CaptionStyle::Hormozi => "Montserrat ExtraBold",
            CaptionStyle::MrBeast => "Anton",
            CaptionStyle::Comic => "Bangers",
            CaptionStyle::Bold => "Archivo Black",
            CaptionStyle::Tech => "Inter",
        }
    }

    /// Highlight colour as an ASS `&Hbbggrr&` override literal.
    pub fn accent(self) -> &'static str {
        match self {
            // yellow
            CaptionStyle::Hormozi | CaptionStyle::MrBeast => "&H00FFFF&",
            // cyan
            CaptionStyle::Comic | CaptionStyle::Tech => "&HFFFF00&",
            // green
            CaptionStyle::Bold => "&H00FF00&",
        }
    }

    /// Max words shown on screen at once (Hormozi "1-3 words" pacing).
    pub fn words_per_cue(self) -> usize {
        match self {
            CaptionStyle::MrBeast => 1,
            CaptionStyle::Hormozi | CaptionStyle::Comic | CaptionStyle::Bold => 2,
            CaptionStyle::Tech => 3,
        }
    }
}

/// A single spoken word with absolute timing (milliseconds from video start).
#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Gemini's emphasis/visual-anchor decision for a segment's words.
/// Indices refer to positions in the word slice handed to Gemini.
#[derive(Debug, Default, Deserialize)]
pub struct CaptionAnnotation {
    #[serde(default)]
    pub highlights: Vec<usize>,
    #[serde(default)]
    pub emojis: Vec<EmojiAnchor>,
}

#[derive(Debug, Deserialize)]
pub struct EmojiAnchor {
    pub index: usize,
    pub emoji: String,
}

/// A color emoji to composite over the clip while a caption cue is on screen.
/// libass cannot render color emoji, so these are overlaid as Twemoji PNGs by
/// ffmpeg instead of being baked into the ASS.
#[derive(Debug, Clone)]
pub struct EmojiEvent {
    /// Twemoji asset basename, e.g. "1f4c8" (no extension).
    pub codepoints: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Map an emoji grapheme to its Twemoji filename codepoints (lowercase hex
/// joined by '-'), dropping the U+FE0F variation selector as Twemoji does.
/// Returns None if the string has no scalar values.
pub fn emoji_codepoints(emoji: &str) -> Option<String> {
    let parts: Vec<String> = emoji
        .chars()
        .filter(|c| *c != '\u{FE0F}')
        .map(|c| format!("{:x}", c as u32))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("-"))
    }
}

/// Parse an SRT string into words with interpolated per-word timing.
/// Used on the upload path, where only sentence-level cues exist: each cue's
/// time range is distributed evenly across its words.
pub fn srt_to_interpolated_words(srt: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut lines = srt.lines().peekable();

    while let Some(line) = lines.next() {
        let line = line.trim();
        if !line.contains("-->") {
            continue;
        }
        let mut parts = line.split("-->");
        let (Some(start), Some(end)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(start_ms) = parse_srt_timestamp(start.trim()) else {
            continue;
        };
        let Some(end_ms) = parse_srt_timestamp(end.trim()) else {
            continue;
        };

        let mut text = String::new();
        while let Some(peek) = lines.peek() {
            let peek = peek.trim();
            if peek.is_empty() {
                break;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(peek);
            lines.next();
        }

        let cue_words: Vec<&str> = text.split_whitespace().collect();
        if cue_words.is_empty() || end_ms <= start_ms {
            continue;
        }
        let step = (end_ms - start_ms) / cue_words.len() as i64;
        for (i, w) in cue_words.iter().enumerate() {
            let ws = start_ms + step * i as i64;
            let we = if i + 1 == cue_words.len() {
                end_ms
            } else {
                ws + step
            };
            words.push(Word {
                text: (*w).to_string(),
                start_ms: ws,
                end_ms: we,
            });
        }
    }
    words
}

/// Words overlapping `[start_ms, end_ms)`, re-based so the clip starts at 0.
pub fn words_in_range(words: &[Word], start_ms: i64, end_ms: i64) -> Vec<Word> {
    words
        .iter()
        .filter(|w| w.end_ms > start_ms && w.start_ms < end_ms)
        .map(|w| Word {
            text: w.text.clone(),
            start_ms: (w.start_ms - start_ms).max(0),
            end_ms: (w.end_ms - start_ms).min(end_ms - start_ms),
        })
        .collect()
}

/// Build an ASS subtitle document for one clip plus the color-emoji overlay
/// events. `words` are clip-relative (start at 0); `annotation` indices align
/// with `words`. Text (with colour highlight) goes into the returned ASS;
/// color emoji are returned separately to be overlaid by ffmpeg.
pub fn build_ass(
    words: &[Word],
    annotation: &CaptionAnnotation,
    style: CaptionStyle,
    width: u32,
    height: u32,
) -> (String, Vec<EmojiEvent>) {
    let font_size = (height as f32 * 0.085).round() as i32;
    let outline = ((height as f32 * 0.006).round() as i32).max(3);
    let shadow = 1;

    let mut highlight = vec![false; words.len()];
    for &idx in &annotation.highlights {
        if idx < highlight.len() {
            highlight[idx] = true;
        }
    }
    let mut emoji_of: Vec<Option<&str>> = vec![None; words.len()];
    for anchor in &annotation.emojis {
        if anchor.index < emoji_of.len() {
            emoji_of[anchor.index] = Some(anchor.emoji.trim());
        }
    }

    let accent = style.accent();
    let mut doc = String::new();
    doc.push_str("[Script Info]\n");
    doc.push_str("ScriptType: v4.00+\n");
    doc.push_str(&format!("PlayResX: {width}\nPlayResY: {height}\n"));
    doc.push_str("WrapStyle: 2\n");
    doc.push_str("ScaledBorderAndShadow: yes\n\n");

    doc.push_str("[V4+ Styles]\n");
    doc.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    doc.push_str(&format!(
        "Style: Default,{font},{size},&H00FFFFFF,&H000000FF,&H00000000,&H64000000,1,0,0,0,100,100,0,0,1,{outline},{shadow},5,40,40,0,1\n\n",
        font = style.font_family(),
        size = font_size,
    ));

    doc.push_str("[Events]\n");
    doc.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");

    let per_cue = style.words_per_cue().max(1);
    let mut events: Vec<EmojiEvent> = Vec::new();
    for chunk in chunk_indices(words.len(), per_cue) {
        let start = words[chunk.0].start_ms;
        let end = words[chunk.1 - 1].end_ms.max(start + 250);
        let mut text = String::from("{\\fad(60,40)}");
        for i in chunk.0..chunk.1 {
            if i > chunk.0 {
                text.push(' ');
            }
            let word = escape_ass(&words[i].text);
            if highlight[i] {
                text.push_str(&format!("{{\\c{accent}}}{word}{{\\c&HFFFFFF&}}"));
            } else {
                text.push_str(&word);
            }
        }
        doc.push_str(&format!(
            "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
            ms_to_ass_time(start),
            ms_to_ass_time(end),
            text,
        ));

        // At most one color-emoji anchor per cue, shown for the cue's duration.
        for i in chunk.0..chunk.1 {
            if let Some(emoji) = emoji_of[i] {
                if let Some(codepoints) = emoji_codepoints(emoji) {
                    events.push(EmojiEvent { codepoints, start_ms: start, end_ms: end });
                    break;
                }
            }
        }
    }
    (doc, events)
}

/// Yield `(start, end)` index ranges covering `len` items in groups of `size`.
fn chunk_indices(len: usize, size: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < len {
        let end = (i + size).min(len);
        out.push((i, end));
        i = end;
    }
    out
}

fn escape_ass(s: &str) -> String {
    s.replace('\\', "\u{2216}")
        .replace('{', "(")
        .replace('}', ")")
        .replace('\n', " ")
        .trim()
        .to_string()
}

fn ms_to_ass_time(ms: i64) -> String {
    let ms = ms.max(0);
    let cs = ms / 10 % 100;
    let s = ms / 1000 % 60;
    let m = ms / 60_000 % 60;
    let h = ms / 3_600_000;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

fn parse_srt_timestamp(ts: &str) -> Option<i64> {
    // HH:MM:SS,mmm  (also tolerates '.')
    let ts = ts.replace(',', ".");
    let (hms, millis) = ts.split_once('.')?;
    let mut it = hms.split(':');
    let h: i64 = it.next()?.trim().parse().ok()?;
    let m: i64 = it.next()?.trim().parse().ok()?;
    let s: i64 = it.next()?.trim().parse().ok()?;
    let ms: i64 = millis.trim().parse().ok()?;
    Some(((h * 60 + m) * 60 + s) * 1000 + ms)
}
