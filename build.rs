// Creates a placeholder model so that `kswime-build-model` (which lives in
// this crate and produces the real model) can be compiled first.
use std::path::Path;
fn main() {
    let p = Path::new("data/model.bin");
    if !p.exists() {
        std::fs::create_dir_all("data").unwrap();
        std::fs::write(p, b"KSWM\0\0\0\0").unwrap();
        println!("cargo:warning=data/model.bin missing; wrote placeholder. Run: cargo run --release --bin kswime-build-model -- data/raw data/model.bin");
    }
    println!("cargo:rerun-if-changed=data/model.bin");
}
