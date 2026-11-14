use super::*;

const GPX: &str = r#"<gpx xmlns="http://www.topografix.com/GPX/1/1"><wpt lat="59.91" lon="10.75"><name>Oslo</name></wpt>
<trk><name>Ride</name><trkseg>
<trkpt lat="0" lon="0"><ele>10</ele><time>2026-11-06T09:00:00Z</time></trkpt>
<trkpt lat="0" lon="1"><ele>25</ele><time>2026-11-06T10:30:00Z</time></trkpt>
</trkseg></trk></gpx>"#;

#[test]
fn stats_come_from_the_track() {
    let s = stats(&parse_gpx(GPX).unwrap());
    assert_eq!(s.points, 2);
    assert!((s.distance_km - 111.2).abs() < 0.1, "{}", s.distance_km);
    assert_eq!(s.seconds, Some(5400));
    assert_eq!(s.climb_m, 15.0);
}

#[test]
fn gpx_and_kml_round_trip() {
    let geo = parse_gpx(GPX).unwrap();
    let back = parse_gpx(&write_gpx(&geo)).unwrap();
    assert_eq!(back, geo);
    let kml = parse_kml(&write_kml(&geo)).unwrap();
    assert_eq!(kml.tracks[0].points[1].lon, 1.0);
    assert_eq!(kml.places[0].name, "Oslo");
}

/// Two rides 111 km apart, each 1 km long and 10 minutes.
const TWO_SEGMENTS: &str = r#"<gpx xmlns="http://www.topografix.com/GPX/1/1"><trk><name>Ride</name>
<trkseg><trkpt lat="0" lon="0"><time>2026-11-06T09:00:00Z</time></trkpt><trkpt lat="0" lon="0.009"><time>2026-11-06T09:10:00Z</time></trkpt></trkseg>
<trkseg><trkpt lat="1" lon="0"><time>2026-11-06T12:00:00Z</time></trkpt><trkpt lat="1" lon="0.009"><time>2026-11-06T12:10:00Z</time></trkpt></trkseg>
</trk></gpx>"#;

#[test]
fn the_gap_between_segments_is_not_part_of_the_ride() {
    let geo = parse_gpx(TWO_SEGMENTS).unwrap();
    assert_eq!(geo.tracks.len(), 1);
    let s = stats(&geo);
    assert_eq!(s.points, 4);
    assert!((s.distance_km - 2.0).abs() < 0.01, "{}", s.distance_km);
    assert_eq!(s.seconds, Some(1200));
}

#[test]
fn segments_survive_conversion() {
    let geo = parse_gpx(TWO_SEGMENTS).unwrap();
    let gpx = write_gpx(&geo);
    assert_eq!(gpx.matches("<trk>").count(), 1, "{gpx}");
    assert_eq!(gpx.matches("<trkseg>").count(), 2, "{gpx}");
    assert_eq!(parse_gpx(&gpx).unwrap(), geo);
    let kml = write_kml(&geo);
    assert_eq!(kml.matches("<LineString>").count(), 2, "{kml}");
    assert!((stats(&parse_kml(&kml).unwrap()).distance_km - 2.0).abs() < 0.01);
    let csv = String::from_utf8(to_csv(&geo).unwrap()).unwrap();
    assert!(csv.lines().nth(4).unwrap().ends_with(",2,"), "{csv}");
}

#[test]
fn kml_gx_tracks_keep_points_and_times() {
    let kml = r#"<kml xmlns="http://www.opengis.net/kml/2.2" xmlns:gx="http://www.google.com/kml/ext/2.2"><Placemark><name>Walk</name><gx:Track><when>2026-10-03T08:00:00Z</when><when>2026-10-03T08:01:00Z</when><gx:coord>-0.1 51.5 10</gx:coord><gx:coord>-0.11 51.51 12</gx:coord></gx:Track></Placemark></kml>"#;
    let geo = parse_kml(kml).unwrap();
    let points = &geo.tracks[0].points;
    assert_eq!(geo.tracks[0].name, "Walk");
    assert_eq!(points.len(), 2);
    assert_eq!((points[1].lon, points[1].lat), (-0.11, 51.51));
    assert_eq!(points[1].elevation, Some(12.0));
    assert_eq!(points[1].time.as_deref(), Some("2026-10-03T08:01:00Z"));
}

fn heights(list: &[f64]) -> Vec<Point> {
    list.iter()
        .map(|e| Point {
            elevation: Some(*e),
            ..Default::default()
        })
        .collect()
}

#[test]
fn climb_ignores_noise_but_not_a_slow_climb() {
    assert_eq!(
        climb(&heights(&[100.0, 101.0, 100.0, 101.0, 100.0, 101.0])),
        0.0
    );
    assert_eq!(climb(&heights(&[100.0, 101.0, 102.0, 103.0, 104.0])), 4.0);
    assert_eq!(climb(&heights(&[100.0, 110.0, 90.0, 100.0])), 20.0);
}

#[test]
fn seconds_match_a_known_time() {
    assert_eq!(seconds("1970-01-02T00:00:01Z"), Some(86_401));
    assert_eq!(seconds("2000-03-01T00:00:00Z"), Some(951_868_800));
}

#[test]
fn place_descriptions_survive_every_format() {
    let gpx = r#"<gpx xmlns="http://www.topografix.com/GPX/1/1">
<wpt lat="42.2" lon="-71.1"><name>LOOKOUT</name><desc>Lookout Rock &amp; tower</desc></wpt>
<wpt lat="42.3" lon="-71.2"><name>GATE</name><cmt>North gate</cmt></wpt></gpx>"#;
    let geo = parse_gpx(gpx).unwrap();
    assert_eq!(geo.places[0].description, "Lookout Rock & tower");
    assert_eq!(geo.places[1].description, "North gate");
    assert_eq!(parse_gpx(&write_gpx(&geo)).unwrap(), geo);
    let kml = parse_kml(&write_kml(&geo)).unwrap();
    assert_eq!(kml.places[0].description, "Lookout Rock & tower");
    let csv = String::from_utf8(to_csv(&geo).unwrap()).unwrap();
    assert!(csv.lines().next().unwrap().ends_with(",description"));
    assert!(csv.contains("Lookout Rock & tower"));
}

#[test]
fn a_polygon_is_an_area_not_a_track() {
    let kml = r#"<kml xmlns="http://www.opengis.net/kml/2.2"><Document>
<Placemark><name>Path</name><LineString><coordinates>0,0,0 1,0,0</coordinates></LineString></Placemark>
<Placemark><name>Building</name><Polygon><outerBoundaryIs><LinearRing>
<coordinates>0,0,0 0,1,0 1,1,0 0,0,0</coordinates></LinearRing></outerBoundaryIs></Polygon></Placemark>
</Document></kml>"#;
    let geo = parse_kml(kml).unwrap();
    assert_eq!((geo.tracks.len(), geo.areas.len()), (1, 1));
    let s = stats(&geo);
    assert_eq!(s.points, 2);
    assert!((s.distance_km - 111.2).abs() < 0.1, "{}", s.distance_km);
    let back = parse_kml(&write_kml(&geo)).unwrap();
    assert_eq!(back.areas[0].points.len(), 4);
}

#[test]
fn extended_data_describes_a_place_without_a_description() {
    let kml = r#"<kml xmlns="http://www.opengis.net/kml/2.2"><Document><Placemark><name>Cache</name>
<ExtendedData><Data name="gc_num"><value>GC7FA4</value></Data>
<SchemaData><SimpleData name="placer">Sverdrup2</SimpleData></SchemaData></ExtendedData>
<Point><coordinates>-71.2,46.8,0</coordinates></Point></Placemark></Document></kml>"#;
    let geo = parse_kml(kml).unwrap();
    assert_eq!(
        geo.places[0].description,
        "gc_num: GC7FA4\nplacer: Sverdrup2"
    );
}
