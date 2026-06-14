# ManaOS - OS for Developers

# Detect OS
os := os_family()
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Default: Build and run
default: run

# Build userland demos and the kernel
build:
    @echo "[build] Compiling ManaOS userland demos and kernel..."
    cargo build

# Create the QEMU storage image
disk:
    @echo "[build] Ensuring disk.img exists..."
    {{ if os == "windows" { "if (-not (Test-Path disk.img)) { $bytes = New-Object byte[] 67108864; [System.IO.File]::WriteAllBytes('disk.img', $bytes) }" } else { "test -f disk.img || dd if=/dev/zero of=disk.img bs=1M count=64" } }}

# Create a 64MB GPT-formatted QEMU storage image
disk-gpt: build
    @echo "[build] Creating GPT disk.img..."
    {{ if os == "windows" { "powershell -ExecutionPolicy Bypass -File scripts/create_gpt_disk_image.ps1 -Path disk.img" } else { "pwsh -File scripts/create_gpt_disk_image.ps1 -Path disk.img" } }}

# Setup ESP and Run QEMU
run: build disk
    @echo "[run] Setting up ESP and starting QEMU..."
    {{ if os == "windows" { "powershell -ExecutionPolicy Bypass -File run.bat" } else { "./run.sh" } }}

# Boot QEMU headlessly and assert storage serial milestones
storage-smoke: disk-gpt
    @echo "[test] Running headless storage smoke test..."
    cargo build --target x86_64-unknown-uefi
    {{ if os == "windows" { "powershell -ExecutionPolicy Bypass -File scripts/run_storage_smoke.ps1" } else { "pwsh -File scripts/run_storage_smoke.ps1" } }}

# Format code (checks in CI)
fmt:
    @echo "[fmt] Formatting code..."
    cargo fmt --all {{ if os == "linux" { "--check" } else { "" } }}
    cargo fmt --manifest-path userland/Cargo.toml --all {{ if os == "linux" { "--check" } else { "" } }}

# Run static analysis (clippy)
lint:
    @echo "[lint] Running kernel clippy..."
    cargo clippy --target x86_64-unknown-uefi -- -D warnings
    @echo "[lint] Running userland clippy..."
    cargo clippy --manifest-path userland/Cargo.toml --target x86_64-unknown-none --target-dir target/userland --lib --bin file_demo --bin bad_pointer_demo --bin smoke_demo --bin user_shell -- -D warnings
    just architecture-boundaries

# Check architecture-to-kernel dependency boundaries
architecture-boundaries:
    @echo "[lint] Checking architecture boundaries..."
    {{ if os == "windows" { "powershell -ExecutionPolicy Bypass -File scripts/check_architecture_boundaries.ps1" } else { "pwsh -File scripts/check_architecture_boundaries.ps1" } }}

# Regenerate bundled third-party license metadata
licenses:
    @echo "[licenses] Regenerating third-party license inventory..."
    {{ if os == "windows" { "powershell -ExecutionPolicy Bypass -File scripts/generate_third_party_licenses.ps1" } else { "pwsh -File scripts/generate_third_party_licenses.ps1" } }}

# Clean build artifacts
clean:
    cargo clean
    {{ if os == "windows" { "if (Test-Path esp/EFI) { Remove-Item -Recurse -Force esp/EFI }; if (Test-Path esp/bootx64.efi) { Remove-Item -Force esp/bootx64.efi }; if (Test-Path disk.img) { Remove-Item -Force disk.img }" } else { "rm -rf esp/EFI esp/bootx64.efi disk.img" } }}
