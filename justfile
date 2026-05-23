# ManaOS - OS for Developers

# Detect OS
os := os_family()
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Default: Build and run
default: run

# Build the kernel
build:
    @echo "[build] Compiling ManaOS kernel..."
    cargo build

# Setup ESP and Run QEMU
run: build
    @echo "[run] Setting up ESP and starting QEMU..."
    {{ if os == "windows" { "powershell -ExecutionPolicy Bypass -File run.bat" } else { "./run.sh" } }}

# Format code (checks in CI)
fmt:
    @echo "[fmt] Formatting code..."
    cargo fmt --all {{ if os == "linux" { "--check" } else { "" } }}

# Run static analysis (clippy)
lint:
    @echo "[lint] Running clippy..."
    cargo clippy --target x86_64-unknown-uefi -- -D warnings

# Regenerate bundled third-party license metadata
licenses:
    @echo "[licenses] Regenerating third-party license inventory..."
    {{ if os == "windows" { "powershell -ExecutionPolicy Bypass -File scripts/generate_third_party_licenses.ps1" } else { "pwsh -File scripts/generate_third_party_licenses.ps1" } }}

# Clean build artifacts
clean:
    cargo clean
    {{ if os == "windows" { "if (Test-Path esp/EFI) { Remove-Item -Recurse -Force esp/EFI }; if (Test-Path esp/bootx64.efi) { Remove-Item -Force esp/bootx64.efi }" } else { "rm -rf esp/EFI esp/bootx64.efi" } }}
