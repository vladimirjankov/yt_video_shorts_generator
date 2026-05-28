use tokio::sync::mpsc;
use serde::{Deserialize, Serialize};
use crate::workers::cutter::CutterMessage;


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ShortSegment {
    pub timestamp_start: String,
    pub timestamp_end: String,
    pub reason: String,
}

pub struct AnalysisMessage {
    pub srt_content: String,
    pub video_path: Option<String>,
    pub number_of_videos: i32,
    pub max_short_length: i32,
}

pub async fn spawn_worker_gemini_analysis(
    api_key: String,
    tx_cutter: mpsc::Sender<CutterMessage>,
) -> mpsc::Sender<AnalysisMessage> {
    let (tx, mut rx) = mpsc::channel::<AnalysisMessage>(100);

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("[debug] calling gemini (api_key_len={}, max_short_length={}, number_of_videos={}, srt_chars={})",
                     api_key.len(), msg.max_short_length, msg.number_of_videos, msg.srt_content.len());
            let segments = analyse_with_gemini(&msg.srt_content, msg.max_short_length, msg.number_of_videos, &api_key).await;
            println!("[debug] gemini returned {} segments", segments.len());
            for segment in &segments {
                println!("{} --> {} | {}", segment.timestamp_start, segment.timestamp_end, segment.reason);
            }

            if let Some(video_path) = msg.video_path {
                if !segments.is_empty() {
                    let _ = tx_cutter.send(CutterMessage {
                        video_path,
                        srt_content: msg.srt_content,
                        segments,
                    }).await;
                }
            }
        }
    });
    tx
}

pub async fn analyse_with_gemini(srt_content: &str, max_short_length: i32, number_of_videos: i32, api_key: &str) -> Vec<ShortSegment> {

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={api_key}"
    );

    let total_duration = extract_last_timestamp(srt_content).unwrap_or_else(|| "unknown".to_string());

    let body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": format!(
                    "You are selecting the strongest segments from a video transcript to use as standalone short clips. \
                     The transcript spans from 00:00:00,000 to approximately {total_duration}. \
                     Read the entire transcript carefully before deciding — the position of a moment in the video \
                     has no bearing on its value. A great hook at minute 15 is just as valid as one at minute 1. \
                     Select up to {number_of_videos} segments (minimum 1), each at most {max_short_length} seconds long. \
                     Judge each candidate on its own merit: humor, surprise, emotional payoff, a punchy claim, \
                     a satisfying reveal, a quotable line, a cliff-hanger that makes someone want more. \
                     Always extend the end of each segment by half a second so the clip doesn't cut mid-word. \
                     Return ONLY a JSON array.\n\n{srt_content}"
                )
            }]
        }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "maxOutputTokens": 2048,
            "temperature": 0.7,
            "thinkingConfig": {
                "thinkingBudget": 1024
            },
            "responseSchema": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "timestamp_start": {
                            "type": "STRING",
                            "description": "SRT timestamp in HH:MM:SS,mmm format, e.g. 00:00:21,940"
                        },
                        "timestamp_end": {
                            "type": "STRING",
                            "description": "SRT timestamp in HH:MM:SS,mmm format, e.g. 00:00:26,240"
                        },
                        "reason": {
                            "type": "STRING",
                            "description": "One short sentence explaining why this segment is interesting"
                        }
                    },
                    "required": ["timestamp_start", "timestamp_end", "reason"]
                }
            }
        }
    });

    println!("[debug] POST {}", url);
    let http_response = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .unwrap();

    let status = http_response.status();
    let raw_body = http_response.text().await.unwrap();
    println!("[debug] gemini status: {status}");
    println!("[debug] gemini raw body:\n{raw_body}");

    let response: serde_json::Value = match serde_json::from_str(&raw_body) {
        Ok(v) => v,
        Err(e) => {
            println!("[debug] failed to parse gemini body as JSON: {e}");
            return vec![];
        }
    };

    let text = response["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("[]");
    println!("[debug] gemini inner text payload:\n{text}");

    match serde_json::from_str::<Vec<ShortSegment>>(text) {
        Ok(v) => v,
        Err(e) => {
            println!("[debug] failed to parse inner text as Vec<ShortSegment>: {e}");
            vec![]
        }
    }
}

fn extract_last_timestamp(srt: &str) -> Option<String> {
    srt.lines()
        .rev()
        .find_map(|l| l.split(" --> ").nth(1).map(|s| s.trim().to_string()))
}
