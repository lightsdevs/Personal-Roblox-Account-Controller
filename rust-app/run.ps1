$ErrorActionPreference = 'Stop'
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path -LiteralPath $cargoPath)) { throw 'Install Rust and the Visual Studio C++ build tools first. See rust-app/README.md.' }
Push-Location $PSScriptRoot
try {
    & $cargoPath run --locked --release
    if ($LASTEXITCODE -ne 0) { throw "Controller exited with code $LASTEXITCODE." }
} finally { Pop-Location }
