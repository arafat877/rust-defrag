$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectName = Split-Path -Leaf $ProjectRoot
$OutZip = Join-Path $ProjectRoot "$ProjectName-source.zip"

$excludeDirs = @(
    ".git",
    "target"
)

$files = Get-ChildItem -Path $ProjectRoot -Recurse -File | Where-Object {
    $full = $_.FullName
    foreach ($dir in $excludeDirs) {
        $needle = [IO.Path]::DirectorySeparatorChar + $dir + [IO.Path]::DirectorySeparatorChar
        if ($full.Contains($needle)) { return $false }
    }
    return $true
}

if (Test-Path $OutZip) {
    Remove-Item $OutZip -Force
}

Compress-Archive -Path $files.FullName -DestinationPath $OutZip -CompressionLevel Optimal
Write-Host "Created: $OutZip"
