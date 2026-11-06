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
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub name: String,
    pub point: Point,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geo {
    pub tracks: Vec<Track>,
    pub places: Vec<Place>,
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
                    geo.places.push(Place { name, point });
                }
            }
            "trk" | "rte" => {
                let points: Vec<Point> = node
                    .descendants()
                    .filter(|n| matches!(n.tag_name().name(), "trkpt" | "rtept"))
                    .filter_map(gpx_point)
                    .collect();
                let name = child_text(node, "name").unwrap_or_default();
                geo.tracks.push(Track { name, points });
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

pub fn parse_kml(text: &str) -> Result<Geo, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let mut geo = Geo::default();
    for mark in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "Placemark")
    {
        let name = child_text(mark, "name").unwrap_or_default();
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
                            point,
                        });
                    }
                }
                "LineString" | "LinearRing" => {
                    geo.tracks.push(Track {
                        name: name.clone(),
                        points: points(),
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
        s.push_str(&format!(
            "  <wpt lat=\"{}\" lon=\"{}\">{}<name>{}</name></wpt>\n",
            p.lat,
            p.lon,
            gpx_fields(p),
            xml(&place.name)
        ));
    }
    for track in &geo.tracks {
        s.push_str(&format!(
            "  <trk><name>{}</name><trkseg>\n",
            xml(&track.name)
        ));
        for p in &track.points {
            s.push_str(&format!(
                "    <trkpt lat=\"{}\" lon=\"{}\">{}</trkpt>\n",
                p.lat,
                p.lon,
                gpx_fields(p)
            ));
        }
        s.push_str("  </trkseg></trk>\n");
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
        s.push_str(&format!(
            "  <Placemark><name>{}</name><Point><coordinates>{}</coordinates></Point></Placemark>\n",
            xml(&place.name),
            kml_tuple(&place.point)
        ));
    }
    for track in &geo.tracks {
        let line: Vec<String> = track.points.iter().map(kml_tuple).collect();
        s.push_str(&format!(
            "  <Placemark><name>{}</name><LineString><coordinates>{}</coordinates></LineString></Placemark>\n",
            xml(&track.name),
            line.join(" ")
        ));
    }
    s.push_str("</Document></kml>\n");
    s
}

pub fn to_csv(geo: &Geo) -> Result<Vec<u8>, String> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(["kind", "name", "lat", "lon", "elevation", "time"])
        .map_err(|e| e.to_string())?;
    let mut row = |kind: &str, name: &str, p: &Point| {
        let elevation = p.elevation.map(|e| e.to_string()).unwrap_or_default();
        w.write_record([
            kind,
            name,
            &p.lat.to_string(),
            &p.lon.to_string(),
            &elevation,
            p.time.as_deref().unwrap_or(""),
        ])
        .map_err(|e| e.to_string())
    };
    for place in &geo.places {
        row("place", &place.name, &place.point)?;
    }
    for track in &geo.tracks {
        for p in &track.points {
            row("track", &track.name, p)?;
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

pub fn stats(geo: &Geo) -> Stats {
    let mut out = Stats::default();
    for track in &geo.tracks {
        out.points += track.points.len();
        for pair in track.points.windows(2) {
            out.distance_km += distance_km(&pair[0], &pair[1]);
            if let (Some(a), Some(b)) = (pair[0].elevation, pair[1].elevation) {
                out.climb_m += (b - a).max(0.0);
            }
        }
        let time = |p: Option<&Point>| p.and_then(|p| p.time.as_deref()).and_then(seconds);
        if let (Some(a), Some(b)) = (time(track.points.first()), time(track.points.last())) {
            out.seconds = Some(out.seconds.unwrap_or(0) + b - a);
        }
    }
    out
}

#[cfg(test)]
#[path = "geo_tests.rs"]
mod tests;
