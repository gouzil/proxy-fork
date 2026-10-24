#[test]
fn bundled_font_has_regular_static_outlines_for_the_stb_renderer() {
    let font = include_bytes!("../assets/fonts/NotoSansSC.ttf");
    let count = u16::from_be_bytes(font[4..6].try_into().unwrap()) as usize;
    let tables = font[12..].as_chunks::<16>().0.iter().take(count);
    let os2 = tables.clone().find(|table| &table[..4] == b"OS/2").unwrap();
    let offset = u32::from_be_bytes(os2[8..12].try_into().unwrap()) as usize;
    assert_eq!(
        u16::from_be_bytes(font[offset + 4..offset + 6].try_into().unwrap()),
        400,
        "body text needs Regular outlines, not the variable font's Thin default"
    );
    assert!(
        !tables.clone().any(|table| &table[..4] == b"fvar"),
        "stb_truetype does not select variable-font weight instances"
    );
}
