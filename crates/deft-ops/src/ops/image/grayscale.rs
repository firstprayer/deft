use super::_common::{IN, INOUT, QUALITY, decode_srgb, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.grayscale",
    summary: "Remove colour",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[QUALITY],
    examples: &["deft image grayscale photo-1.jpg -o mono.jpg"],
    tags: &["black and white", "greyscale", "monochrome", "desaturate"],
    result: &["width", "height"],
    suffix: Some("gray"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    // Brightness is judged from the colours seen, so the profile comes first.
    let img = decode_srgb(job, job.input())?;
    save(job, &img.grayscale(), job.input().require_format()?)
}

#[cfg(test)]
mod tests {
    use super::super::_common::testing::{picture, png, run};

    #[test]
    fn brightness_comes_from_the_colours_the_profile_means() {
        let adobe = moxcms::ColorProfile::new_adobe_rgb().encode().unwrap();
        let colour = [40, 160, 60];
        let plain = picture(&run(&super::OP, vec![png(4, 4, colour, None)], &[]));
        let managed = picture(&run(&super::OP, vec![png(4, 4, colour, Some(&adobe))], &[]));
        let (a, b) = (
            plain.to_luma8().get_pixel(0, 0)[0],
            managed.to_luma8().get_pixel(0, 0)[0],
        );
        // In Adobe RGB these numbers are a deeper green, so a darker gray.
        assert!(b + 5 < a, "{a} {b}");
    }
}
