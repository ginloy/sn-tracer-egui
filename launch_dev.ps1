# Change directory to the directory of the script
Set-Location $PSScriptRoot

powershell -Command {
    # Set Environment Variables
    $env:RUST_LOG = "debug"
    $env:SCANNER_PATH = "$(Get-Location)\target\release\scanner"

    # Run
    cargo build --release --bin scanner
    cargo run $args --bin sn-tracer-egui
}