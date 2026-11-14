use super::*;

#[test]
fn a_phone_clip_is_shown_upright() {
    let probe = json!({
        "format": {"duration": "12.345678"},
        "streams": [
            {"codec_type": "video", "codec_name": "hevc", "width": 1920, "height": 1080,
             "avg_frame_rate": "30000/1001", "disposition": {"default": 1},
             "side_data_list": [{"side_data_type": "Display Matrix", "rotation": -90}]},
            {"codec_type": "audio", "codec_name": "aac", "channels": 2, "sample_rate": "44100"}
        ]
    });
    let f = facts(&probe);
    assert_eq!(f["duration"], json!(12.346));
    assert_eq!(
        (f["width"].clone(), f["height"].clone()),
        (json!(1080), json!(1920))
    );
    assert_eq!(f["rotation"], json!(270));
    assert_eq!(f["fps"], json!(29.97));
    assert_eq!(f["audio_codec"], json!("aac"));
    assert_eq!(f["sample_rate"], json!(44100));
}

#[test]
fn the_main_track_and_the_container_crop_count() {
    let probe = json!({
        "format": {"duration": "1.8"},
        "streams": [
            {"codec_type": "video", "width": 2048, "height": 1536, "disposition": {"default": 0}},
            {"codec_type": "video", "width": 1920, "height": 1440, "disposition": {"default": 1},
             "side_data_list": [{"side_data_type": "Frame Cropping",
               "crop_top": 66, "crop_bottom": 66, "crop_left": 88, "crop_right": 88}]},
            {"codec_type": "video", "width": 600, "height": 600, "disposition": {"attached_pic": 1}}
        ]
    });
    let f = facts(&probe);
    assert_eq!(
        (f["width"].clone(), f["height"].clone()),
        (json!(1744), json!(1308))
    );
    assert_eq!(f["audio"], json!(false));
}

#[test]
fn stretched_pixels_give_the_shown_size() {
    let probe = json!({"format": {}, "streams": [{"codec_type": "video", "width": 1440,
        "height": 1080, "sample_aspect_ratio": "4:3"}]});
    let f = facts(&probe);
    assert_eq!(f["width"], json!(1440));
    assert_eq!(f["display_width"], json!(1920));
}

#[test]
fn any_json_has_a_shape() {
    let package = json_shape(br#"{"name": "koa", "version": "2.0"}"#).unwrap();
    assert_eq!(package["top"], json!("object"));
    assert_eq!(package["keys"], json!(2));
    assert_eq!(json_shape(b"[1, 2, 3]").unwrap()["items"], json!(3));
    assert!(json_shape(b"not json").is_none());
}
