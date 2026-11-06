use super::*;

const AUTO: &str = "WEBVTT\nKind: captions\n\n00:00:00.000 --> 00:00:02.000\nhello<00:00:00.500><c> there</c>\n\n00:00:02.000 --> 00:00:02.010\nhello there\n\n00:00:02.010 --> 00:00:04.000\nhello there\ngeneral<00:00:03.000><c> kenobi</c>\n\n00:01:05.000 --> 00:01:06.000\nnext minute &amp; more\n";

#[test]
fn rolling_captions_are_deduplicated() {
    let cues = parse_subtitles(AUTO);
    assert_eq!(
        cues.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
        ["hello there", "general kenobi", "next minute & more"]
    );
    assert_eq!(
        cues_to_text(&cues),
        "hello there general kenobi\n\nnext minute & more\n"
    );
    let md = cues_to_markdown(&cues, Some("T"));
    assert!(md.starts_with("# T\n\n**[0:00]** hello there general kenobi\n\n**[1:05]** next"));
    assert!(cues_to_srt(&cues).contains("00:01:05,000 --> 00:01:06,000"));
    assert!(
        cues_to_vtt(&cues).starts_with("WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nhello there\n")
    );
    let moved = shift_cues(&cues, -1.0);
    assert_eq!((moved[0].start, moved[0].end), (0.0, 1.0));
    assert_eq!(
        shift_cues(&cues, -3.0).len(),
        2,
        "the first cue fell off the start"
    );
}
