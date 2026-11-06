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

#[test]
fn seconds_match_a_known_time() {
    assert_eq!(seconds("1970-01-02T00:00:01Z"), Some(86_401));
    assert_eq!(seconds("2000-03-01T00:00:00Z"), Some(951_868_800));
}
