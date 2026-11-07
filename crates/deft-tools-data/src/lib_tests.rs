use super::*;

#[test]
fn yaml_merge_keys_and_numeric_keys_survive() {
    let v = parse(
        b"base: &b {a: 1}\nuse:\n  <<: *b\n  c: 2\n3: x\n",
        Format::Yaml,
    )
    .unwrap();
    assert_eq!(v["use"]["a"], json!(1));
    assert_eq!(v["3"], json!("x"));
}

#[test]
fn several_yaml_documents_become_a_list() {
    let v = parse(b"a: 1\n---\na: 2\n", Format::Yaml).unwrap();
    assert_eq!(v, json!([{"a": 1}, {"a": 2}]));
}

#[test]
fn toml_keeps_dates_as_text() {
    let v = parse(b"when = 2026-01-02\n[t]\nx = 1\n", Format::Toml).unwrap();
    assert_eq!(v["when"], json!("2026-01-02"));
    assert_eq!(v["t"]["x"], json!(1));
}

#[test]
fn toml_output_drops_nulls_with_a_warning() {
    let (bytes, warnings) =
        render(&json!({"a": 1, "b": null}), Format::Toml, Style::default()).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap().trim(), "a = 1");
    assert_eq!(warnings.len(), 1);
}

#[test]
fn xml_round_trip_keeps_attributes_and_lists() {
    let src = br#"<shop id="7"><item>a</item><item>b &amp; c</item></shop>"#;
    let v = parse(src, Format::Xml).unwrap();
    assert_eq!(v, json!({"shop": {"@id": "7", "item": ["a", "b & c"]}}));
    let (out, _) = render(&v, Format::Xml, Style::default()).unwrap();
    assert_eq!(parse(&out, Format::Xml).unwrap(), v);
}

#[test]
fn query_uses_jq_syntax() {
    let v = json!({"people": [{"name": "Ada", "age": 36}, {"name": "Lin", "age": 29}]});
    let out = query(&v, ".people[] | select(.age > 30) | .name").unwrap();
    assert_eq!(out, vec![json!("Ada")]);
    assert_eq!(query(&v, ".people | length").unwrap(), vec![json!(2)]);
    assert!(query(&v, ".people[").is_err());
}

#[test]
fn merge_is_deep_and_arrays_replace_by_default() {
    let mut a = json!({"x": {"p": 1, "q": 2}, "l": [1]});
    merge(&mut a, json!({"x": {"q": 3}, "l": [2]}), false);
    assert_eq!(a, json!({"x": {"p": 1, "q": 3}, "l": [2]}));
    merge(&mut a, json!({"l": [3]}), true);
    assert_eq!(a["l"], json!([2, 3]));
}

#[test]
fn diff_reports_paths() {
    let changes = diff(
        &json!({"a": 1, "b": [1, 2], "c": 3}),
        &json!({"a": 2, "b": [1], "d": 4}),
    );
    let got: Vec<(&str, &str)> = changes.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(
        got,
        vec![
            (".a", "changed"),
            (".b[1]", "removed"),
            (".c", "removed"),
            (".d", "added")
        ]
    );
}

#[test]
fn json_schema() {
    let schema =
        json!({"type": "object", "required": ["a"], "properties": {"a": {"type": "integer"}}});
    assert!(
        validate_schema(&json!({"a": 1}), &schema)
            .unwrap()
            .is_empty()
    );
    assert!(
        !validate_schema(&json!({"a": "x"}), &schema)
            .unwrap()
            .is_empty()
    );
    assert!(validate_schema(&json!({}), &json!({"type": "nope"})).is_err());
}

#[test]
fn yaml_quotes_what_older_parsers_would_misread() {
    let v = serde_json::json!({"on": true, "yes": "no", "date": "2026-11-06", "at": "1:30", "name": "deft"});
    let (bytes, _) = render(&v, Format::Yaml, Style::default()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("'on': true"), "{text}");
    assert!(text.contains("'yes': 'no'"), "{text}");
    assert!(text.contains("date: '2026-11-06'"), "{text}");
    assert!(text.contains("at: '1:30'"), "{text}");
    assert!(text.contains("name: deft"), "{text}");
    assert_eq!(parse(text.as_bytes(), Format::Yaml).unwrap(), v);
}

#[test]
fn vcards_read_folded_escaped_and_old_style_lines() {
    let text = "BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=E5=BC=A0;=E4=B8=89;;;\r\nTEL;CELL:+1 555\r\nitem1.EMAIL:a@b.c\r\nNOTE:one\\, two\r\n  more\r\nEND:VCARD\r\n";
    let c = &vcard::parse(text)[0];
    assert_eq!(c.name, "三 张");
    assert_eq!(c.phones, ["+1 555"]);
    assert_eq!(c.emails, ["a@b.c"]);
    assert_eq!(c.note, "one, two more");
}

#[test]
fn vcards_survive_csv_and_back() {
    let csv = b"Name,Company,Phone,Email\nAda Lovelace,Analytical,+1 555; +1 666,ada@x.io\n";
    let contacts = vcard::from_csv(csv).unwrap();
    assert_eq!(contacts[0].phones.len(), 2);
    let back = vcard::parse(&vcard::write(&contacts));
    assert_eq!(back, contacts);
}

#[test]
fn events_keep_their_times_and_zones() {
    let text = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Oslo:20261106T090000\nDTEND;TZID=Europe/Oslo:20261106T103000\nSUMMARY:Review\\, Q4\nEND:VEVENT\nBEGIN:VEVENT\nDTSTART;VALUE=DATE:20261105\nSUMMARY:Holiday\nEND:VEVENT\n";
    let events = ical::parse(text);
    assert_eq!(events[0].summary, "Holiday");
    assert_eq!(events[0].start, "2026-11-05");
    assert_eq!(events[1].start, "2026-11-06 09:00");
    assert_eq!(events[1].timezone, "Europe/Oslo");
    assert_eq!(events[1].summary, "Review, Q4");
    let back = ical::parse(&ical::write(&events).unwrap());
    assert_eq!(back, events);
}

const CALENDAR: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261110T090000Z\r\nDURATION:PT1H30M\r\nSUMMARY:Standup\r\nDESCRIPTION:Real agenda\r\nSTATUS:CANCELLED\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:This is an event reminder\r\nTRIGGER:-P0DT0H10M0S\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261224\r\nDTEND;VALUE=DATE:20261226\r\nSUMMARY:Christmas\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261231\r\nDURATION:P2D\r\nSUMMARY:New year\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

#[test]
fn an_alarm_does_not_replace_the_event_text() {
    let events = ical::parse(CALENDAR);
    let e = events.iter().find(|e| e.summary == "Standup").unwrap();
    assert_eq!(e.description, "Real agenda");
    assert_eq!(e.status, "CANCELLED");
}

#[test]
fn an_end_comes_from_a_duration_and_means_the_last_day() {
    let events = ical::parse(CALENDAR);
    let end = |name: &str| {
        let e = events.iter().find(|e| e.summary == name).unwrap();
        e.end.as_str()
    };
    assert_eq!(end("Standup"), "2026-11-10 10:30");
    assert_eq!(end("Christmas"), "2026-12-25");
    assert_eq!(end("New year"), "2027-01-01");
}

#[test]
fn all_day_events_survive_a_round_trip() {
    let events = ical::parse(CALENDAR);
    let text = ical::write(&events).unwrap();
    assert!(text.contains("DTEND;VALUE=DATE:20261226"), "{text}");
    assert_eq!(ical::parse(&text), events);
}

#[test]
fn impossible_dates_are_refused() {
    for bad in [
        "11/06/2026 09:00",
        "11/07/2026",
        "2026-13-01",
        "2026-02-30",
        "2026-11-06 25:00",
    ] {
        let csv = format!("summary,start\nx,{bad}\n");
        let events = ical::from_csv(csv.as_bytes()).unwrap();
        let e = ical::write(&events).unwrap_err();
        assert!(e.contains("is not a date"), "{bad}: {e}");
    }
    let leap = ical::from_csv(b"summary,start\nx,2028-02-29\n").unwrap();
    assert!(ical::write(&leap).is_ok());
}

#[test]
fn separate_time_columns_are_joined() {
    let csv = b"Subject,Start Date,Start Time,End Date,End Time,All Day Event\nMeet,2026-11-06,09:00,2026-11-06,10:00,False\nLate,2026-11-06,9:30 PM,,10:15 PM,False\nNoon,2026-11-07,12:00 AM,,,False\nOff,2026-12-24,,2026-12-24,,True\n";
    let text = ical::write(&ical::from_csv(csv).unwrap()).unwrap();
    assert!(
        text.contains("DTSTART:20261106T090000\r\nDTEND:20261106T100000"),
        "{text}"
    );
    assert!(
        text.contains("DTSTART:20261106T213000\r\nDTEND:20261106T221500"),
        "{text}"
    );
    assert!(text.contains("DTSTART:20261107T000000"), "{text}");
    assert!(
        text.contains("DTSTART;VALUE=DATE:20261224\r\nDTEND;VALUE=DATE:20261225"),
        "{text}"
    );
}

#[test]
fn an_end_matches_the_kind_of_its_start() {
    let write = |csv: &str| ical::write(&ical::from_csv(csv.as_bytes()).unwrap());
    let text = write("summary,start,end\nx,2026-11-06 09:00,2026-11-08\n").unwrap();
    assert!(text.contains("DTEND:20261108T090000"), "{text}");
    let text = write("summary,start,end\nx,2026-11-06,2026-11-08 09:00\n").unwrap();
    assert!(text.contains("DTEND;VALUE=DATE:20261109"), "{text}");
    let text = write("summary,start,end\nx,2026-11-06 09:00,2026-11-06\n").unwrap();
    assert!(!text.contains("DTEND"), "{text}");
    let e = write("summary,start,end\nx,2026-11-06 09:00,2026-11-06 08:00\n").unwrap_err();
    assert!(e.contains("ends before it starts"), "{e}");
}

#[test]
fn stamps_are_valid_and_uids_belong_to_the_event() {
    let uids = |csv: &str| -> Vec<String> {
        ical::write(&ical::from_csv(csv.as_bytes()).unwrap())
            .unwrap()
            .lines()
            .filter(|l| l.starts_with("UID:"))
            .map(String::from)
            .collect()
    };
    let one = uids("summary,start\nKickoff,2026-11-06 09:00\nKickoff,2026-11-06 09:00\n");
    let two = uids("summary,start\nOther,2026-12-01\n");
    assert_ne!(one[0], one[1]);
    assert!(!one.contains(&two[0]));
    assert_eq!(
        one,
        uids("summary,start\nKickoff,2026-11-06 09:00\nKickoff,2026-11-06 09:00\n")
    );
    let text = ical::write(&ical::from_csv(b"summary,start\nx,2026-12-24\n").unwrap()).unwrap();
    assert!(text.contains("DTSTAMP:20000101T000000Z\r\n"), "{text}");
}

#[test]
fn a_name_splits_into_family_and_given() {
    let card = |name: &str| {
        vcard::write(&[vcard::Contact {
            name: name.to_string(),
            ..Default::default()
        }])
    };
    assert!(card("Ada Lovelace").contains("N:Lovelace;Ada;;;\r\n"));
    assert!(card("Mary Ann Smith").contains("N:Smith;Mary Ann;;;\r\n"));
    assert!(card("Lovelace, Ada").contains("N:Lovelace;Ada;;;\r\n"));
    assert!(card("张三").contains("N:张三;;;;\r\n"));
}

#[test]
fn first_and_last_name_columns_make_a_name() {
    let csv = b"First Name,Last Name,Company,Mobile Phone,E-mail Address\nAda,Lovelace,Analytical,+44 1,ada@x\n";
    let c = &vcard::from_csv(csv).unwrap()[0];
    assert_eq!(c.name, "Ada Lovelace");
    assert_eq!(c.phones, ["+44 1"]);
    assert_eq!(c.emails, ["ada@x"]);
    assert!(vcard::from_csv(b"company\nAnalytical\n").is_err());
}

#[test]
fn cards_keep_every_address_and_a_usable_name() {
    let text = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada\r\nTEL;VALUE=uri:tel:+1-555\r\nADR:;;1 Main St;Boston;;;\r\nitem3.ADR:;;2 High St;Cambridge;;;\r\nEND:VCARD\r\nBEGIN:VCARD\r\nVERSION:3.0\r\nORG:No Name Inc\r\nEND:VCARD\r\n";
    let cards = vcard::parse(text);
    assert_eq!(cards[0].phones, ["+1-555"]);
    assert_eq!(cards[0].address, "1 Main St, Boston; 2 High St, Cambridge");
    assert_eq!(cards[1].name, "No Name Inc");
}

#[test]
fn skipped_dates_of_a_repeating_event_are_kept() {
    let text = "BEGIN:VEVENT\r\nDTSTART:20261110T090000Z\r\nRRULE:FREQ=WEEKLY\r\nEXDATE:20261117T090000Z,20261124T090000Z\r\nEXDATE:20261201T090000Z\r\nSUMMARY:Weekly\r\nEND:VEVENT\r\n";
    let events = ical::parse(text);
    assert_eq!(
        events[0].except,
        "2026-11-17 09:00; 2026-11-24 09:00; 2026-12-01 09:00"
    );
    let written = ical::write(&events).unwrap();
    assert!(written.contains("EXDATE:20261124T090000Z\r\n"), "{written}");
    assert_eq!(ical::parse(&written), events);
}
