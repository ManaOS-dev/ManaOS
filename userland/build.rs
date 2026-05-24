use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let linker_script = manifest_dir.join("linker.ld");

    println!("cargo:rerun-if-changed=linker.ld");
    println!(
        "cargo:rustc-link-arg-bin=file_demo=-T{}",
        linker_script.display()
    );
    println!(
        "cargo:rustc-link-arg-bin=bad_pointer_demo=-T{}",
        linker_script.display()
    );
    println!("cargo:rustc-link-arg-bin=file_demo=-z");
    println!("cargo:rustc-link-arg-bin=file_demo=max-page-size=0x1000");
    println!("cargo:rustc-link-arg-bin=bad_pointer_demo=-z");
    println!("cargo:rustc-link-arg-bin=bad_pointer_demo=max-page-size=0x1000");
}
