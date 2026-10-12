// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Emit a `VERSION` const packed from the package version, so the FFI ABI
//! version is a compile-time constant synced with `Cargo.toml`.

use std::env;
use std::fs;
use std::path::Path;

fn part(name: &str) -> u32 {
    env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn main() {
    let major = part("CARGO_PKG_VERSION_MAJOR");
    let minor = part("CARGO_PKG_VERSION_MINOR");
    let patch = part("CARGO_PKG_VERSION_PATCH");
    let version = (major << 16) | (minor << 8) | patch;

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    let path = Path::new(&out_dir).join("version.rs");
    fs::write(
        &path,
        format!(
            "/// The ABI version, packed as `(major << 16) | (minor << 8) | patch`.\n\
             pub const VERSION: u32 = 0x{version:06x}; // {major}.{minor}.{patch}\n"
        ),
    )
    .expect("failed to write version.rs");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION_MAJOR");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION_MINOR");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION_PATCH");
}
