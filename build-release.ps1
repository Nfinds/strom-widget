# Builds the release exe with no local paths (user name, folder) inside the binary
# and puts Strom-Widget.exe next to this script.
$here = (Resolve-Path $PSScriptRoot).Path
$env:RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~ --remap-path-prefix=$here=src"
cargo build --release
if ($LASTEXITCODE -eq 0) { Copy-Item target\release\strom-widget.exe .\Strom-Widget.exe -Force }
