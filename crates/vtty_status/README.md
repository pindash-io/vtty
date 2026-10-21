# vtty_status

<!-- cargo-rdme start -->

OSC 7501 Program Status Protocol for `vtty`.

A program reports its state — idle, working, done, blocked, or error — to
the terminal, or clears a previously reported status. This crate provides
the [`ProgramStatus`](https://docs.rs/vtty_status/latest/vtty_status/status/struct.ProgramStatus.html) codec and the [`PerformProgramStatus`](https://docs.rs/vtty_status/latest/vtty_status/status/trait.PerformProgramStatus.html) hook, which a
[`vtty_core::Perform`](https://docs.rs/vtty_core/latest/vtty_core/perform/trait.Perform.html) sink implements to opt in.

```text
ESC ] 7501 ; state=working:app=brew:msg=<base64> ESC \
```

Reference: <https://www.superlogical.com/rex/docs/build/program-status>.

<!-- cargo-rdme end -->

## Usage

```rust,ignore
use vtty_core::Perform;
use vtty_status::{PerformProgramStatus, ProgramStatus};

struct Screen;

impl Perform for Screen {}

impl PerformProgramStatus for Screen {
    fn program_status(&mut self, status: &ProgramStatus) {
        // OSC 7501 state arrived.
    }
}
```

Licensed under either of [Apache-2.0](../../LICENSE-APACHE) or
[MIT](../../LICENSE-MIT) at your option.
