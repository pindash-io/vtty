//! Emoji-presentation check scratchpad: prints `cell_width` vs the CJK and
//! emoji fast-width probes for a few representative chars.

fn main() {
    for c in ['🚀', '✨', '🎯', '日', 'テ', 'e', '\u{301}', ' '] {
        let w = vtty_width::cell_width(c);
        println!("U+{:04X} {:?} width={:?} cjk={:?} ep={:?}", c as u32, c, w, vtty_width::cjk_fast_width(c), vtty_width::emoji_fast_width(c));
    }
}
