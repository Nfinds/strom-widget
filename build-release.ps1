# Baut die Release-exe ohne lokale Pfade (Benutzername, Ordner) im Binary.
$here = (Resolve-Path $PSScriptRoot).Path
$env:RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~ --remap-path-prefix=$here=src"
cargo build --release
if ($LASTEXITCODE -eq 0) { Copy-Item target\release\strom-widget.exe .\Strom-Widget.exe -Force }
