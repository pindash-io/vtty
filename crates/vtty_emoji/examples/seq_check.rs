fn main() {
    // UTS #51 §2.7.1: a VS15 (text selector) inside a ZWJ family BREAKS it.
    // 👩 ZWJ 👨 ZWJ 👧 VS15  →  the VS15 on 👧 breaks the family.
    for s in [
        "\u{1F469}\u{200D}\u{1F468}\u{200D}\u{1F467}\u{FE0E}", // 👩👨👧︎ (VS15 on 👧)
        "\u{1F469}\u{200D}\u{1F468}\u{200D}\u{1F467}",         // 👩👨👧 (no VS)
        "\u{270C}\u{FE0F}\u{1F3FB}",
    ] {
        // ✌️🏻 (defective VS16-modifier)
        let w = vtty_emoji::cell_widths(s);
        println!("{s:?} -> {w:?}");
    }
}
