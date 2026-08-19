$stable = "$env:APPDATA\Anthro Bridge\config.json"
$stamp  = Get-Date -Format "yyyyMMdd-HHmmss"

if (Test-Path $stable) {
    Copy-Item $stable "$env:APPDATA\Anthro Bridge\config.before-new-stable-$stamp.json"
    $hash = (Get-FileHash $stable -Algorithm SHA256).Hash
    Write-Host "Backup: config.before-new-stable-$stamp.json"
    Write-Host "Stable SHA256: $hash"
    $size = (Get-Item $stable).Length
    Write-Host "Size: $size bytes"
} else {
    Write-Host "Stable config not found"
}
