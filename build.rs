use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

const USERLAND_TARGET: &str = "x86_64-unknown-none";

fn main() {
    println!("cargo:rerun-if-changed=userland/Cargo.toml");
    println!("cargo:rerun-if-changed=userland/linker.ld");
    println!("cargo:rerun-if-changed=userland/src/lib.rs");
    println!("cargo:rerun-if-changed=userland/src/syscall.rs");
    println!("cargo:rerun-if-changed=userland/src/bin/file_demo.rs");
    println!("cargo:rerun-if-changed=userland/src/bin/bad_pointer_demo.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let output_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let profile = env::var("PROFILE").expect("PROFILE");
    let target_dir = manifest_dir.join("target").join("userland");

    build_userland(&manifest_dir, &target_dir, &profile);

    let file_demo = extract_binary(
        &manifest_dir,
        &target_dir,
        &output_dir,
        &profile,
        "file_demo",
    );
    let bad_pointer_demo = extract_binary(
        &manifest_dir,
        &target_dir,
        &output_dir,
        &profile,
        "bad_pointer_demo",
    );

    println!(
        "cargo:rustc-env=MANAOS_USER_FILE_DEMO_BIN={}",
        file_demo.display()
    );
    println!(
        "cargo:rustc-env=MANAOS_USER_BAD_POINTER_DEMO_BIN={}",
        bad_pointer_demo.display()
    );
}

fn build_userland(manifest_dir: &Path, target_dir: &Path, profile: &str) {
    let mut command = Command::new(env::var("CARGO").unwrap_or_else(|_| String::from("cargo")));
    command
        .current_dir(manifest_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .args([
            "build",
            "--manifest-path",
            "userland/Cargo.toml",
            "--target",
            USERLAND_TARGET,
            "--target-dir",
        ])
        .arg(target_dir)
        .args(["--bin", "file_demo", "--bin", "bad_pointer_demo"]);

    if profile == "release" {
        command.arg("--release");
    }

    let status = command.status().expect("failed to spawn userland cargo");
    assert!(status.success(), "failed to build ManaOS userland demos");
}

fn extract_binary(
    manifest_dir: &Path,
    target_dir: &Path,
    output_dir: &Path,
    profile: &str,
    binary_name: &str,
) -> PathBuf {
    let profile_dir = if profile == "release" {
        "release"
    } else {
        "debug"
    };
    let elf_path = target_dir
        .join(USERLAND_TARGET)
        .join(profile_dir)
        .join(binary_name);
    let binary_dir = output_dir.join("userland-bin");
    std::fs::create_dir_all(&binary_dir).expect("failed to create userland binary directory");
    let binary_path = binary_dir.join(format!("{binary_name}.bin"));

    let status = Command::new("llvm-objcopy")
        .current_dir(manifest_dir)
        .args(["-O", "binary"])
        .arg(&elf_path)
        .arg(&binary_path)
        .status()
        .expect("failed to spawn llvm-objcopy");
    assert!(status.success(), "failed to extract flat userland binary");

    binary_path
}
