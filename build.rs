// Build scripts are build-time tooling: a failure must abort the build loudly,
// and there is no error-reporting machinery to propagate through. Panics are
// the conventional, correct signal here.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Build script: package `well_known_licenses/*` into a flat `spdx-licenses.zip`
//! embedded into the binary via `include_bytes!` in `src/well_known.rs`.
//!
//! Entries are written with bare filenames (e.g. `MIT.txt`, `MIT.toml`) — no
//! directory prefix — so runtime `ZipArchive::by_name("MIT.txt")` works.
//!
//! The output `spdx-licenses.zip` lives at the crate root and is gitignored.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const OUT_ZIP: &str = "spdx-licenses.zip";

fn main() {
    // Re-run whenever the corpus changes.
    println!("cargo:rerun-if-changed=well_known_licenses");

    let src_dir = PathBuf::from("well_known_licenses");
    let out_path = PathBuf::from(OUT_ZIP);

    // Collect entries (both .txt and .toml), sorted for determinism.
    let mut entries: Vec<PathBuf> = fs::read_dir(&src_dir)
        .expect("well_known_licenses/ must exist")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext == "txt" || ext == "toml")
        })
        .collect();
    entries.sort();

    // Write the zip atomically-ish: build to a temp file then rename.
    let tmp_path = out_path.with_extension("zip.tmp");
    let file = File::create(&tmp_path).expect("create temp zip");
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for entry in &entries {
        let name = entry
            .file_name()
            .expect("entry has filename")
            .to_string_lossy()
            .to_string();
        zip.start_file(&name, opts)
            .unwrap_or_else(|e| panic!("zip start_file {name}: {e}"));
        let mut f = File::open(entry).expect("open source file");
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).expect("read source file");
        zip.write_all(&buf).expect("write zip entry");
    }

    zip.finish().expect("finalize zip");
    fs::rename(&tmp_path, &out_path).expect("rename temp zip into place");

    println!(
        "cargo:warning=spdx-licenses.zip: {} entries written",
        entries.len()
    );
}
