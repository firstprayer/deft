use super::_common::{PASSWORD, open, page_list, plural, text_image, write};
use deft_core::prelude::*;
use deft_tools_pdf::{Mark, Stamper};

pub static OP: Op = Op {
    id: "pdf.number",
    summary: "Add page numbers: --format",
    input: Input::each(&[Format::Pdf], "pdf"),
    details: &[
        "Without --position the number goes bottom centre; where that already has text, \
         it goes to the first free place of bottom-right, bottom-left, top-right, top, top-left, \
         then the same places at half the margin unless --margin is given.",
    ],
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("format", Kind::Text, "Template with {n} and {total}").default("{n}"),
        param(
            "position",
            Kind::Position,
            "Where on the page (default: bottom, or a free corner)",
        ),
        param(
            "margin",
            Kind::Length,
            "Distance from the page edge (default 10mm)",
        ),
        param(
            "font-size",
            Kind::Float {
                min: 1.0,
                max: 1000.0,
            },
            "Text size in points",
        )
        .default("10"),
        param("color", Kind::Color, "Text color").default("black"),
        param(
            "first-page",
            Kind::Int {
                min: 1,
                max: 1_000_000,
            },
            "First page that gets a number",
        )
        .default("1"),
        param(
            "start",
            Kind::Int {
                min: 0,
                max: 1_000_000,
            },
            "Number shown on that page",
        )
        .default("1"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf number three-pages.pdf -o numbered.pdf",
        "deft pdf number report.pdf --format \"Page {n} of {total}\" --first-page 2",
    ],
    tags: &["page numbers", "paginate", "footer"],
    result: &["numbered"],
    suffix: Some("numbered"),
    run,
    ..Op::DEFAULT
};

/// Where the number may go when --position is not given, best first.
const PLACES: [(f64, f64); 6] = [
    (0.5, 1.0),
    (1.0, 1.0),
    (0.0, 1.0),
    (1.0, 0.0),
    (0.5, 0.0),
    (0.0, 0.0),
];

/// 10 mm in points.
const DEFAULT_MARGIN: f64 = 10.0 / 25.4 * 72.0;

/// More than this share of a number's box already inked means the number
/// would land on text or a picture.
const BUSY: f64 = 0.01;

/// What is stamped on one page, before it is placed.
struct Label {
    page: usize,
    mark: Mark,
    w: f64,
    h: f64,
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let ids = deft_tools_pdf::page_ids(&doc);
    let first = p.int("first-page") as usize - 1;
    let start = p.int("start");
    let total = ids.len().saturating_sub(first) as i64 + start - 1;
    let size = p.float("font-size");
    let given_margin = p.opt_points("margin");
    let margin = given_margin.unwrap_or(DEFAULT_MARGIN);
    let chosen = p.opt_position("position");
    let template = p.text("format");
    let mut stamper = Stamper::new();
    let mut labels = Vec::new();
    for (i, _) in ids.iter().enumerate().skip(first) {
        let n = start + (i - first) as i64;
        let label = template
            .replace("{n}", &n.to_string())
            .replace("{total}", &total.to_string());
        labels.push(match deft_tools_pdf::win_ansi(&label) {
            Ok(bytes) => Label {
                page: i,
                w: deft_tools_pdf::text_width(&bytes, size),
                h: size,
                mark: Mark::Text {
                    bytes,
                    size,
                    color: p.color("color"),
                    x: 0.0,
                    y: 0.0,
                    angle: 0.0,
                },
            },
            Err(_) => {
                let (img, uw, uh) = text_image(&label, p.color("color"), None)?;
                let name = stamper.add_image(&mut doc, &img);
                let (w, h) = (uw * size, uh * size);
                Label {
                    page: i,
                    w,
                    h,
                    mark: Mark::Image {
                        name,
                        x: 0.0,
                        y: 0.0,
                        w,
                        h,
                        angle: 0.0,
                    },
                }
            }
        });
    }
    // Where each label would sit at each place: (x, y) of its lower left.
    // Without --position and --margin, half the margin is tried last.
    let places: Vec<(f64, f64, f64)> = match chosen {
        Some(pos) => vec![(pos.x, pos.y, margin)],
        None if given_margin.is_some() => PLACES.iter().map(|&(x, y)| (x, y, margin)).collect(),
        None => PLACES
            .iter()
            .map(|&(x, y)| (x, y, margin))
            .chain(PLACES.iter().map(|&(x, y)| (x, y, margin / 2.0)))
            .collect(),
    };
    let spots: Vec<Vec<(f64, f64)>> = labels
        .iter()
        .map(|l| {
            let (pw, ph) = deft_tools_pdf::display_size(&doc, ids[l.page]);
            places
                .iter()
                .map(|&(px, py, m)| {
                    (
                        m + (pw - 2.0 * m - l.w) * px,
                        m + (ph - 2.0 * m - l.h) * (1.0 - py),
                    )
                })
                .collect()
        })
        .collect();
    // Which places already have ink on which pages.
    let busy = busy_places(&doc, &labels, &spots, size)?;
    let (pick, moved) = choose(&busy);
    let mut covered = Vec::new();
    for ((label, spot), (&at, busy)) in labels.iter_mut().zip(&spots).zip(pick.iter().zip(&busy)) {
        if busy[at] {
            covered.push(label.page + 1);
        }
        let (x, y) = spot[at];
        match &mut label.mark {
            Mark::Text { x: mx, y: my, .. } => (*mx, *my) = (x, y),
            Mark::Image { x: mx, y: my, .. } => (*mx, *my) = (x, y),
        }
    }
    let numbered = labels.len();
    let labels_pages: Vec<usize> = labels.iter().map(|l| l.page).collect();
    for label in labels {
        stamper.stamp(&mut doc, ids[label.page], &[label.mark], 255)?;
    }
    let mut res = OpResult::one(write(&mut doc)?)
        .fact("numbered", numbered)
        .note(format!("{} numbered", plural(numbered, "page")));
    if chosen.is_none() && moved > 0 {
        let name = |i: usize| {
            let (x, y) = PLACES[i % PLACES.len()];
            let at = deft_core::value::Position { x, y }.name();
            if i < PLACES.len() {
                at.to_string()
            } else {
                format!("{at} at half the margin")
            }
        };
        let taken: Vec<usize> = busy
            .iter()
            .zip(&labels_pages)
            .filter(|(b, _)| b[0])
            .map(|(_, &p)| p + 1)
            .collect();
        let mut used: Vec<usize> = pick.iter().copied().filter(|&at| at != 0).collect();
        used.sort_unstable();
        used.dedup();
        let names: Vec<String> = used.iter().map(|&i| name(i)).collect();
        let what = if moved == numbered {
            format!("the numbers went to the {}", names.join(" or "))
        } else {
            format!(
                "the number went to the {} on those pages",
                names.join(" or ")
            )
        };
        res = res.warn(format!(
            "the bottom centre already has text or pictures on {} (page {}), so {what}; \
             give --position to choose",
            plural(taken.len(), "page"),
            page_list(&taken)
        ));
    }
    if !covered.is_empty() {
        res = res.warn(format!(
            "the number overlaps existing text or pictures on {} (page {}); \
             try another --position or --margin",
            plural(covered.len(), "page"),
            page_list(&covered)
        ));
    }
    Ok(res)
}

/// For each label, whether each of its spots already has ink under it.
fn busy_places(
    doc: &deft_tools_pdf::lopdf::Document,
    labels: &[Label],
    spots: &[Vec<(f64, f64)>],
    size: f64,
) -> Result<Vec<Vec<bool>>> {
    let renderer = deft_tools_pdf::Renderer::of(doc)?;
    let pad = size * 0.3;
    let check = |n: usize| -> Result<Vec<bool>> {
        let l = &labels[n];
        let boxes: Vec<[f64; 4]> = spots[n]
            .iter()
            .map(|&(x, y)| [x - pad, y - pad, x + l.w + pad, y + l.h + pad])
            .collect();
        Ok(renderer
            .ink(l.page, &boxes)?
            .into_iter()
            .map(|share| share > BUSY)
            .collect())
    };
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(labels.len().max(1));
    let chunk = labels.len().div_ceil(workers).max(1);
    let parts: Vec<Result<Vec<Vec<bool>>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..labels.len())
            .step_by(chunk)
            .map(|from| {
                let check = &check;
                scope.spawn(move || {
                    (from..(from + chunk).min(labels.len()))
                        .map(check)
                        .collect::<Result<Vec<_>>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(err(ErrorCode::Internal, "render thread")))
            })
            .collect()
    });
    let mut out = Vec::with_capacity(labels.len());
    for part in parts {
        out.extend(part?);
    }
    Ok(out)
}

/// The place for each page: the first place free on every page; else the one
/// free on most pages, with each page where it is taken moved to its own first
/// free place. Also returns how many pages are not at the first place.
fn choose(busy: &[Vec<bool>]) -> (Vec<usize>, usize) {
    let places = busy.first().map_or(1, Vec::len);
    let taken = |at: usize| busy.iter().filter(|b| b[at]).count();
    let best = (0..places).min_by_key(|&at| (taken(at), at)).unwrap_or(0);
    let pick: Vec<usize> = busy
        .iter()
        .map(|b| {
            if b[best] {
                (0..places).find(|&at| !b[at]).unwrap_or(best)
            } else {
                best
            }
        })
        .collect();
    let moved = pick.iter().filter(|&&at| at != 0).count();
    (pick, moved)
}

#[cfg(test)]
mod tests {
    use super::choose;

    #[test]
    fn the_number_goes_where_the_page_is_free() {
        // Bottom centre free everywhere: it stays there.
        let (pick, moved) = choose(&[vec![false, false], vec![false, true]]);
        assert_eq!((pick, moved), (vec![0, 0], 0));
        // Taken on one page of three: the place free on all of them wins.
        let (pick, moved) = choose(&[
            vec![true, false, false],
            vec![false, false, true],
            vec![false, false, false],
        ]);
        assert_eq!((pick, moved), (vec![1, 1, 1], 3));
        // No place free everywhere: the best one, and a page where it is
        // taken goes to its own free place, or stays when there is none.
        let (pick, _) = choose(&[
            vec![false, true],
            vec![true, false],
            vec![false, true],
            vec![true, true],
        ]);
        assert_eq!(pick, vec![0, 1, 0, 0]);
    }
}
