$ErrorActionPreference = 'Stop'

$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargoCommand) {
    throw 'Rust/Cargo is missing. See login-prototype/README.md for build prerequisites.'
}

Push-Location $PSScriptRoot
try {
    & $cargoCommand.Source run
    if ($LASTEXITCODE -ne 0) {
        throw "Prototype exited with code $LASTEXITCODE. See the Cargo output above."
    }
}
finally {
    Pop-Location
}
