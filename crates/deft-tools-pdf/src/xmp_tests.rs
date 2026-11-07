use super::*;

const PACKET: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"
  xmlns:pdf="http://ns.adobe.com/pdf/1.3/" pdf:Keywords="old, words">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Old title</rdf:li></rdf:Alt></dc:title>
<dc:creator><rdf:Seq><rdf:li>John Doe</rdf:li><rdf:li>Jane Roe</rdf:li></rdf:Seq></dc:creator>
</rdf:Description>
<rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" pdfaid:part="1"/>
</rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

#[test]
fn fields_are_read_from_elements_and_attributes() {
    assert_eq!(read(PACKET, b"Title").as_deref(), Some("Old title"));
    assert_eq!(
        read(PACKET, b"Author").as_deref(),
        Some("John Doe; Jane Roe")
    );
    assert_eq!(read(PACKET, b"Keywords").as_deref(), Some("old, words"));
    assert_eq!(read(PACKET, b"Subject"), None);
}

#[test]
fn setting_fields_keeps_the_rest_of_the_packet() {
    let fields: Vec<(&[u8], String)> = vec![
        (b"Title", "New & better".to_string()),
        (b"Keywords", "a, b".to_string()),
    ];
    let out = update(PACKET, &fields).unwrap();
    assert_eq!(read(&out, b"Title").as_deref(), Some("New & better"));
    assert_eq!(read(&out, b"Keywords").as_deref(), Some("a, b"));
    assert_eq!(read(&out, b"Author").as_deref(), Some("John Doe; Jane Roe"));
    assert!(out.contains("pdfaid:part=\"1\""));
    assert!(!out.contains("Old title") && !out.contains("old, words"));
    assert!(out.starts_with("<?xpacket begin"));
}

#[test]
fn a_packet_that_is_not_xml_is_not_changed() {
    assert!(update("<x:xmpmeta><broken", &[(b"Title", "t".to_string())]).is_none());
}
