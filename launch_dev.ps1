# Change directory to the directory of the script
Set-Location $PSScriptRoot

# Set environment variables
$env:RUST_LOG = "debug"
$env:SCANNER_PATH = "$(Get-Location)\target\release\scanner"

# Build and run the Rust code
cargo build --release --bin scanner
cargo run $args --bin sn-tracer-egui
