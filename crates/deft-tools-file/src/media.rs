//! What a video or audio file holds, from ffprobe's JSON report, and the
//! shape of a JSON document.

use serde_json::{Map, Value, json};

/// Facts for `deft info` from `ffprobe -show_format -show_streams -of json`:
/// duration, the picture size as players show it (rotation and a container
/// crop applied), codecs and sound.
pub fn facts(probe: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    let num = |v: &Value| -> Option<f64> {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    };
    if let Some(d) = num(&probe["format"]["duration"]) {
        out.insert("duration".into(), json!((d * 1000.0).round() / 1000.0));
    }
    let empty = vec![];
    let streams = probe["streams"].as_array().unwrap_or(&empty);
    let videos: Vec<&Value> = streams
        .iter()
        .filter(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .collect();
    // The main picture: the default track, else the first. A motion photo
    // also holds a track of a few stills, which is not the clip.
    let video = videos
        .iter()
        .find(|s| s["disposition"]["default"] == 1)
        .or(videos.first());
    if let Some(v) = video {
        let (mut w, mut h) = (
            v["width"].as_i64().unwrap_or(0),
            v["height"].as_i64().unwrap_or(0),
        );
        let sides = v["side_data_list"].as_array().unwrap_or(&empty);
        let mut rotation = 0i64;
        for side in sides {
            if let Some(r) = num(&side["rotation"]) {
                rotation = r.round() as i64;
            }
            // iPhone Live Photos keep a larger picture and a crop in the container.
            if side["side_data_type"] == "Frame Cropping" {
                let crop = |k: &str| side[k].as_i64().unwrap_or(0);
                w -= crop("crop_left") + crop("crop_right");
                h -= crop("crop_top") + crop("crop_bottom");
            }
        }
        if rotation == 0
            && let Some(r) = num(&v["tags"]["rotate"])
        {
            rotation = r.round() as i64;
        }
        let sideways = rotation.rem_euclid(180) == 90;
        let (shown_w, shown_h) = if sideways { (h, w) } else { (w, h) };
        out.insert("width".into(), json!(shown_w));
        out.insert("height".into(), json!(shown_h));
        if rotation.rem_euclid(360) != 0 {
            out.insert("rotation".into(), json!(rotation.rem_euclid(360)));
        }
        // Pixels that are not square: players stretch the picture.
        let sar = v["sample_aspect_ratio"].as_str().and_then(|s| {
            let (a, b) = s.split_once(':')?;
            let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
            (a > 0.0 && b > 0.0 && a != b).then_some(a / b)
        });
        if let Some(sar) = sar {
            let wide = (w as f64 * sar).round() as i64;
            let (dw, dh) = if sideways { (h, wide) } else { (wide, h) };
            out.insert("display_width".into(), json!(dw));
            out.insert("display_height".into(), json!(dh));
        }
        if let Some(c) = v["codec_name"].as_str() {
            out.insert("video_codec".into(), json!(c));
        }
        let rate = v["avg_frame_rate"].as_str().and_then(|r| {
            let (a, b) = r.split_once('/')?;
            let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
            (a > 0.0 && b > 0.0).then_some(a / b)
        });
        if let Some(fps) = rate {
            out.insert("fps".into(), json!((fps * 100.0).round() / 100.0));
        }
    }
    let audio = streams.iter().find(|s| s["codec_type"] == "audio");
    if let Some(a) = audio {
        if let Some(c) = a["codec_name"].as_str() {
            out.insert("audio_codec".into(), json!(c));
        }
        if let Some(n) = a["channels"].as_i64() {
            out.insert("channels".into(), json!(n));
        }
        if let Some(r) = num(&a["sample_rate"]) {
            out.insert("sample_rate".into(), json!(r as i64));
        }
    } else if video.is_some() {
        out.insert("audio".into(), json!(false));
    }
    out
}

/// A one-line summary of media facts, such as "1920x1080, 12.4 s, h264".
pub fn summary(facts: &Map<String, Value>) -> String {
    let mut parts = vec![];
    if let (Some(w), Some(h)) = (facts.get("width"), facts.get("height")) {
        parts.push(format!("{w}x{h}"));
    }
    if let Some(d) = facts.get("duration").and_then(Value::as_f64) {
        parts.push(format!("{d:.1} s"));
    }
    for key in ["video_codec", "audio_codec"] {
        if let Some(c) = facts.get(key).and_then(Value::as_str) {
            parts.push(c.to_string());
        }
    }
    parts.join(", ")
}

/// The shape of a JSON document: whether it is an object or an array, and how
/// many keys or items it has at the top. `None` when it is not JSON.
pub fn json_shape(bytes: &[u8]) -> Option<Map<String, Value>> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    let mut out = Map::new();
    match &v {
        Value::Object(o) => {
            out.insert("top".into(), json!("object"));
            out.insert("keys".into(), json!(o.len()));
            let names: Vec<&String> = o.keys().take(10).collect();
            out.insert("first_keys".into(), json!(names));
        }
        Value::Array(a) => {
            out.insert("top".into(), json!("array"));
            out.insert("items".into(), json!(a.len()));
        }
        _ => {
            out.insert("top".into(), json!("value"));
        }
    }
    Some(out)
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
