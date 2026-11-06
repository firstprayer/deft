use super::*;

#[test]
fn tail_keeps_last_lines() {
    assert_eq!(tail("a\n\nb\nc\n", 2), "b | c");
}

#[test]
fn checksum_hex() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn missing_engine_has_a_hint() {
    let ctx = Ctx::new(std::env::temp_dir());
    // SAFETY: tests in this crate do not read this variable concurrently.
    unsafe { std::env::set_var(env_key("yt-dlp"), "") };
    let e = require(&ctx, "yt-dlp", "youtube.download").unwrap_err();
    assert_eq!(e.code, ErrorCode::EngineMissing);
    assert!(e.hint.unwrap().contains("deft engine install yt-dlp"));
}

#[test]
fn ogg_uses_libvorbis_when_present_and_the_built_in_encoder_otherwise() {
    assert_eq!(
        vorbis_args(" A....D libvorbis  libvorbis (codec vorbis)\n"),
        ["-c:a", "libvorbis"]
    );
    assert_eq!(
        vorbis_args(" A..X.D vorbis  Vorbis\n"),
        ["-c:a", "vorbis", "-strict", "experimental", "-ac", "2"]
    );
}
