use crate::registry::registry;

fn first(query: &str) -> String {
    registry()
        .find(query)
        .first()
        .map(|h| h.0.clone())
        .unwrap_or_default()
}

#[test]
fn a_rare_word_outranks_a_common_one() {
    assert_eq!(first("compress pdf"), "pdf compress");
    assert_eq!(first("rotate video"), "video rotate");
}

#[test]
fn inflected_words_match() {
    assert_eq!(first("compressing pdfs"), "pdf compress");
    assert_eq!(first("resizing image"), "image resize");
    assert_eq!(first("merged pdf"), "pdf merge");
}

#[test]
fn misspelled_words_are_corrected() {
    for (query, expected) in [
        ("compres pdf", "pdf compress"),
        ("rotat video", "video rotate"),
        ("watermrk pdf", "pdf watermark"),
        ("shrnk pdf", "pdf compress"),
    ] {
        assert_eq!(first(query), expected, "{query}");
    }
}

#[test]
fn deleting_a_letter_from_any_command_still_finds_it() {
    for (path, op) in registry().all_paths() {
        if op.path() != path {
            continue;
        }
        let (group, command) = path.split_once(' ').unwrap();
        let Some(word) = command.split('-').find(|w| w.len() >= 5) else {
            continue;
        };
        let mut typo = word.to_string();
        typo.remove(word.len() / 2);
        let query = format!("{group} {}", command.replace(word, &typo));
        let hits = registry().find(&query);
        assert!(
            hits.iter().any(|h| h.1.id == op.id),
            "{query:?} did not find {path}"
        );
    }
}

#[test]
fn format_pairs_name_the_converter() {
    assert_eq!(first("png to jpg"), "image convert");
    assert_eq!(first("mp4 to gif"), "video to-gif");
    assert_eq!(first("md to pdf"), "md to-pdf");
}

#[test]
fn a_named_file_picks_commands_that_accept_it() {
    assert_eq!(first("rotate scan.png by 90 degrees"), "image rotate");
    assert_eq!(first("rotate report.pdf"), "pdf rotate");
    assert_eq!(first("compress talk.mp3"), "audio compress");
}

#[test]
fn a_name_after_to_is_the_output_not_the_input() {
    let inputs = |q: &str| super::inputs(q);
    use deft_core::format::Format;
    assert_eq!(inputs("turn notes.md into a PDF"), [Format::Md]);
    assert_eq!(inputs("save clip.mp4 as out.gif"), [Format::Mp4]);
    assert_eq!(inputs("zip my files"), []);
}

#[test]
fn words_of_no_use_find_nothing() {
    assert!(registry().find("the of to").is_empty());
    assert!(registry().find("zzzzqqqq").is_empty());
}

#[test]
fn an_operation_is_listed_once() {
    let hits = registry().find("extract audio from video");
    let mut ids: Vec<_> = hits.iter().map(|h| h.1.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), hits.len());
}

#[test]
fn a_job_no_command_knows_is_reported() {
    let unknown = |q: &str| registry().unknown_job(q);
    assert_eq!(unknown("translate pdf to french"), ["translate", "french"]);
    assert_eq!(unknown("transcribe audio"), ["transcribe"]);
}

#[test]
fn a_known_job_is_not_reported() {
    for query in [
        "shrink pdf",
        "pdf",
        "compress my holiday pdf",
        "resize image",
        "compres pdf",
        "rezise image",
        "把 销售.csv 转成 Excel",
        "merge pdfs",
        "png to jpg",
    ] {
        assert!(registry().unknown_job(query).is_empty(), "{query}");
    }
}

#[test]
fn a_doubled_consonant_is_undone() {
    let hits = registry().find("padding");
    assert!(
        hits.iter().all(|h| h.0.ends_with(" pad")),
        "{:?}",
        hits.iter().map(|h| &h.0).collect::<Vec<_>>()
    );
    assert!(registry().unknown_job("padding").is_empty());
    assert_eq!(first("cropping video"), "video crop");
    assert_eq!(first("trimming audio"), "audio split");
}
