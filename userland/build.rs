use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let linker_script = manifest_dir.join("linker.ld");
    let binaries = ["file_demo", "bad_pointer_demo", "smoke_demo"];

    println!("cargo:rerun-if-changed=linker.ld");
    for binary in binaries {
        println!(
            "cargo:rustc-link-arg-bin={binary}=-T{}",
            linker_script.display()
        );
        println!("cargo:rustc-link-arg-bin={binary}=-static");
        println!("cargo:rustc-link-arg-bin={binary}=-no-pie");
        println!("cargo:rustc-link-arg-bin={binary}=--no-dynamic-linker");
        println!("cargo:rustc-link-arg-bin={binary}=-z");
        println!("cargo:rustc-link-arg-bin={binary}=max-page-size=0x1000");
    }
}
