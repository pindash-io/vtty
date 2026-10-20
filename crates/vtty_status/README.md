# vtty_status

OSC 7501 **Program Status Protocol** for [`vtty`](https://crates.io/crates/vtty):
the [`ProgramStatus`] codec plus the [`PerformProgramStatus`] hook, which a
[`vtty_core::Perform`] sink implements to opt in.

```text
ESC ] 7501 ; state=working:app=brew:msg=<base64> ESC \
```

Reference: <https://www.superlogical.com/rex/docs/build/program-status>.

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
