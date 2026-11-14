//! GPS tracks and places: read and write GPX and KML, and export to CSV.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
    pub elevation: Option<f64>,
    pub time: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub name: String,
    pub points: Vec<Point>,
    /// Where a new segment starts in `points`. The recording stopped there,
    /// so the gap is not part of the route.
    pub breaks: Vec<usize>,
}

impl Track {
    pub fn segments(&self) -> Vec<&[Point]> {
        let mut out = Vec::with_capacity(self.breaks.len() + 1);
        let mut start = 0;
        for &at in &self.breaks {
            out.push(&self.points[start..at]);
            start = at;
        }
        out.push(&self.points[start..]);
        out
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub name: String,
    /// What the file says about the place: an address, a full name, notes.
    pub description: String,
    pub point: Point,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geo {
    pub tracks: Vec<Track>,
    pub places: Vec<Place>,
    /// Outlines of areas (KML polygons): kept in conversions, never counted
    /// as distance travelled.
    pub areas: Vec<Track>,
}

fn child_text(node: roxmltree::Node, name: &str) -> Option<String> {
    node.children()
        .find(|c| c.tag_name().name() == name)
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
}

fn gpx_point(node: roxmltree::Node) -> Option<Point> {
    Some(Point {
        lat: node.attribute("lat")?.parse().ok()?,
        lon: node.attribute("lon")?.parse().ok()?,
        elevation: child_text(node, "ele").and_then(|e| e.parse().ok()),
        time: child_text(node, "time"),
    })
}

pub fn parse_gpx(text: &str) -> Result<Geo, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let mut geo = Geo::default();
    for node in doc.root_element().children().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "wpt" => {
                if let Some(point) = gpx_point(node) {
                    let name = child_text(node, "name").unwrap_or_default();
                    let description = child_text(node, "desc")
                        .filter(|d| !d.is_empty())
                        .or_else(|| child_text(node, "cmt"))
                        .unwrap_or_default();
                    geo.places.push(Place {
                        name,
                        description,
                        point,
                    });
                }
            }
            "trk" | "rte" => {
                let mut track = Track {
                    name: child_text(node, "name").unwrap_or_default(),
                    ..Default::default()
                };
                for segment in node.children().filter(|n| n.tag_name().name() == "trkseg") {
                    if !track.points.is_empty() {
                        track.breaks.push(track.points.len());
                    }
                    let points = segment
                        .children()
                        .filter(|n| n.tag_name().name() == "trkpt");
                    track.points.extend(points.filter_map(gpx_point));
                }
                let route = node.children().filter(|n| n.tag_name().name() == "rtept");
                track.points.extend(route.filter_map(gpx_point));
                geo.tracks.push(track);
            }
            _ => {}
        }
    }
    Ok(geo)
}

/// `lon,lat,alt` tuples, the order KML uses.
fn coordinates(text: &str) -> Vec<Point> {
    text.split_whitespace()
        .filter_map(|tuple| {
            let mut parts = tuple.split(',');
            let lon = parts.next()?.parse().ok()?;
            let lat = parts.next()?.parse().ok()?;
            let elevation = parts.next().and_then(|e| e.parse().ok());
            Some(Point {
                lat,
                lon,
                elevation,
                time: None,
            })
        })
        .collect()
}

/// A `gx:Track`: `when` elements paired in order with `lon lat alt` coords.
fn gx_track(shape: roxmltree::Node) -> Vec<Point> {
    let texts = |name: &str| -> Vec<String> {
        shape
            .children()
            .filter(|c| c.tag_name().name() == name)
            .map(|c| c.text().unwrap_or("").trim().to_string())
            .collect()
    };
    let times = texts("when");
    texts("coord")
        .iter()
        .enumerate()
        .filter_map(|(i, coord)| {
            let mut parts = coord.split_whitespace();
            let lon = parts.next()?.parse().ok()?;
            let lat = parts.next()?.parse().ok()?;
            Some(Point {
                lat,
                lon,
                elevation: parts.next().and_then(|e| e.parse().ok()),
                time: times.get(i).filter(|t| !t.is_empty()).cloned(),
            })
        })
        .collect()
}

/// A placemark's ExtendedData as "name: value" lines, for a place with no
/// description: Google Earth keeps a geocache's details there.
fn extended_data(mark: roxmltree::Node) -> String {
    let Some(data) = mark
        .children()
        .find(|c| c.tag_name().name() == "ExtendedData")
    else {
        return String::new();
    };
    let mut lines = vec![];
    for field in data.descendants() {
        let value = match field.tag_name().name() {
            "Data" => child_text(field, "value"),
            "SimpleData" => field.text().map(|t| t.trim().to_string()),
            _ => continue,
        };
        if let (Some(name), Some(value)) = (field.attribute("name"), value)
            && !value.is_empty()
        {
            lines.push(format!("{name}: {value}"));
        }
    }
    lines.join("\n")
}

pub fn parse_kml(text: &str) -> Result<Geo, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let mut geo = Geo::default();
    for mark in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Placemark")
    {
        let name = child_text(mark, "name").unwrap_or_default();
        let description = child_text(mark, "description")
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| extended_data(mark));
        for shape in mark.descendants().filter(|n| n.is_element()) {
            let points = || {
                child_text(shape, "coordinates")
                    .map(|c| coordinates(&c))
                    .unwrap_or_default()
            };
            match shape.tag_name().name() {
                "Point" => {
                    if let Some(point) = points().into_iter().next() {
                        geo.places.push(Place {
                            name: name.clone(),
                            description: description.clone(),
                            point,
                        });
                    }
                }
                "LineString" => {
                    geo.tracks.push(Track {
                        name: name.clone(),
                        points: points(),
                        ..Default::default()
                    });
                }
                // A polygon's outline: an area, not a way travelled.
                "LinearRing" => {
                    geo.areas.push(Track {
                        name: name.clone(),
                        points: points(),
                        ..Default::default()
                    });
                }
                "Track" => {
                    geo.tracks.push(Track {
                        name: name.clone(),
                        points: gx_track(shape),
                        ..Default::default()
                    });
                }
                _ => {}
            }
        }
    }
    Ok(geo)
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn gpx_fields(p: &Point) -> String {
    let mut s = String::new();
    if let Some(e) = p.elevation {
        s.push_str(&format!("<ele>{e}</ele>"));
    }
    if let Some(t) = &p.time {
        s.push_str(&format!("<time>{}</time>", xml(t)));
    }
    s
}

pub fn write_gpx(geo: &Geo) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"deft\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n",
    );
    for place in &geo.places {
        let p = &place.point;
        let desc = if place.description.is_empty() {
            String::new()
        } else {
            format!("<desc>{}</desc>", xml(&place.description))
        };
        s.push_str(&format!(
            "  <wpt lat=\"{}\" lon=\"{}\">{}<name>{}</name>{desc}</wpt>\n",
            p.lat,
            p.lon,
            gpx_fields(p),
            xml(&place.name)
        ));
    }
    // GPX has no areas: an outline is written as a track of its own.
    for track in geo.tracks.iter().chain(&geo.areas) {
        s.push_str(&format!("  <trk><name>{}</name>\n", xml(&track.name)));
        for segment in track.segments() {
            s.push_str("  <trkseg>\n");
            for p in segment {
                s.push_str(&format!(
                    "    <trkpt lat=\"{}\" lon=\"{}\">{}</trkpt>\n",
                    p.lat,
                    p.lon,
                    gpx_fields(p)
                ));
            }
            s.push_str("  </trkseg>\n");
        }
        s.push_str("  </trk>\n");
    }
    s.push_str("</gpx>\n");
    s
}

fn kml_tuple(p: &Point) -> String {
    format!("{},{},{}", p.lon, p.lat, p.elevation.unwrap_or(0.0))
}

pub fn write_kml(geo: &Geo) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document>\n",
    );
    for place in &geo.places {
        let desc = if place.description.is_empty() {
            String::new()
        } else {
            format!("<description>{}</description>", xml(&place.description))
        };
        s.push_str(&format!(
            "  <Placemark><name>{}</name>{desc}<Point><coordinates>{}</coordinates></Point></Placemark>\n",
            xml(&place.name),
            kml_tuple(&place.point)
        ));
    }
    for area in &geo.areas {
        let ring: Vec<String> = area.points.iter().map(kml_tuple).collect();
        s.push_str(&format!(
            "  <Placemark><name>{}</name><Polygon><outerBoundaryIs><LinearRing><coordinates>{}</coordinates></LinearRing></outerBoundaryIs></Polygon></Placemark>\n",
            xml(&area.name),
            ring.join(" ")
        ));
    }
    for track in &geo.tracks {
        let lines: Vec<String> = track
            .segments()
            .iter()
            .map(|segment| {
                let line: Vec<String> = segment.iter().map(kml_tuple).collect();
                format!(
                    "<LineString><coordinates>{}</coordinates></LineString>",
                    line.join(" ")
                )
            })
            .collect();
        let shape = if lines.len() > 1 {
            format!("<MultiGeometry>{}</MultiGeometry>", lines.concat())
        } else {
            lines.concat()
        };
        s.push_str(&format!(
            "  <Placemark><name>{}</name>{shape}</Placemark>\n",
            xml(&track.name)
        ));
    }
    s.push_str("</Document></kml>\n");
    s
}

pub fn to_csv(geo: &Geo) -> Result<Vec<u8>, String> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record([
        "kind",
        "name",
        "lat",
        "lon",
        "elevation",
        "time",
        "segment",
        "description",
    ])
    .map_err(|e| e.to_string())?;
    let mut row = |kind: &str, name: &str, p: &Point, segment: &str, desc: &str| {
        let elevation = p.elevation.map(|e| e.to_string()).unwrap_or_default();
        w.write_record([
            kind,
            name,
            &p.lat.to_string(),
            &p.lon.to_string(),
            &elevation,
            p.time.as_deref().unwrap_or(""),
            segment,
            desc,
        ])
        .map_err(|e| e.to_string())
    };
    for place in &geo.places {
        row("place", &place.name, &place.point, "", &place.description)?;
    }
    for track in &geo.tracks {
        for (n, segment) in track.segments().iter().enumerate() {
            for p in *segment {
                row("track", &track.name, p, &(n + 1).to_string(), "")?;
            }
        }
    }
    for area in &geo.areas {
        for p in &area.points {
            row("area", &area.name, p, "", "")?;
        }
    }
    w.into_inner().map_err(|e| e.to_string())
}

/// Great-circle distance in kilometres.
fn distance_km(a: &Point, b: &Point) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6371.0088 * h.sqrt().asin()
}

/// Seconds since 1970 for a time such as `2026-11-06T09:00:00Z`.
fn seconds(time: &str) -> Option<i64> {
    let number = |r: std::ops::Range<usize>| time.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (h, min, s) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let year = y - era * 400;
    let shifted = (m + 9) % 12;
    let day = (153 * shifted + 2) / 5 + d - 1;
    let days = era * 146_097 + year * 365 + year / 4 - year / 100 + day - 719_468;
    Some(days * 86_400 + h * 3600 + min * 60 + s)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub points: usize,
    pub distance_km: f64,
    pub seconds: Option<i64>,
    pub climb_m: f64,
}

/// Rises smaller than this are GPS noise, not climbing.
const CLIMB_STEP_M: f64 = 2.0;

/// Metres climbed. A rise counts once it passes the step, measured from the
/// lowest point since the last rise, so a slow steady climb still adds up.
fn climb(points: &[Point]) -> f64 {
    let mut total = 0.0;
    let mut base: Option<f64> = None;
    for e in points.iter().filter_map(|p| p.elevation) {
        match base {
            Some(b) if e - b >= CLIMB_STEP_M => {
                total += e - b;
                base = Some(e);
            }
            Some(b) if e >= b => {}
            _ => base = Some(e),
        }
    }
    total
}

pub fn stats(geo: &Geo) -> Stats {
    let mut out = Stats::default();
    for track in &geo.tracks {
        out.points += track.points.len();
        // Each segment alone: the gap between two is a pause, not a journey.
        for segment in track.segments() {
            for pair in segment.windows(2) {
                out.distance_km += distance_km(&pair[0], &pair[1]);
            }
            out.climb_m += climb(segment);
            let time = |p: Option<&Point>| p.and_then(|p| p.time.as_deref()).and_then(seconds);
            if let (Some(a), Some(b)) = (time(segment.first()), time(segment.last())) {
                out.seconds = Some(out.seconds.unwrap_or(0) + b - a);
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "geo_tests.rs"]
mod tests;
