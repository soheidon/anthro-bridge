$stable = "$env:APPDATA\Anthro Bridge\config.json"
$dev    = "$env:APPDATA\Anthro Bridge Dev\config.json"
$stamp  = Get-Date -Format "yyyyMMdd-HHmmss"

$stableBefore = $null
$devBefore = $null

if (Test-Path $stable) {
    Copy-Item $stable "$env:APPDATA\Anthro Bridge\config.before-dev-test-$stamp.json"
    $stableBefore = (Get-FileHash $stable -Algorithm SHA256).Hash
    Write-Host "Stable backup: config.before-dev-test-$stamp.json"
}

if (Test-Path $dev) {
    Copy-Item $dev "$env:APPDATA\Anthro Bridge Dev\config.before-install-$stamp.json"
    $devBefore = (Get-FileHash $dev -Algorithm SHA256).Hash
    Write-Host "Dev backup: config.before-install-$stamp.json"
}

Write-Host "StableHashBefore=$stableBefore"
Write-Host "DevHashBefore=$devBefore"
