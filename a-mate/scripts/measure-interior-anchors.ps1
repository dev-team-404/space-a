param(
    [Parameter(Mandatory = $true)] [string] $AssetRoot,
    [Parameter(Mandatory = $true)] [string] $Output
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$rootPath = (Resolve-Path -LiteralPath $AssetRoot).Path
$metrics = [ordered]@{}

Get-ChildItem -LiteralPath $rootPath -Recurse -Filter "*.png" | Sort-Object FullName | ForEach-Object {
    $bitmap = [System.Drawing.Bitmap]::FromFile($_.FullName)
    try {
        $maxY = -1
        for ($y = 0; $y -lt $bitmap.Height; $y++) {
            for ($x = 0; $x -lt $bitmap.Width; $x++) {
                if ($bitmap.GetPixel($x, $y).A -gt 16) {
                    $maxY = [Math]::Max($maxY, $y)
                }
            }
        }
        if ($maxY -lt 0) { throw "Empty sprite: $($_.FullName)" }

        $contactXs = New-Object System.Collections.Generic.List[int]
        $contactTop = [Math]::Max(0, $maxY - 3)
        for ($y = $contactTop; $y -le $maxY; $y++) {
            for ($x = 0; $x -lt $bitmap.Width; $x++) {
                if ($bitmap.GetPixel($x, $y).A -gt 128) { $contactXs.Add($x) }
            }
        }
        $sorted = $contactXs.ToArray() | Sort-Object
        $medianX = if ($sorted.Count -eq 0) { ($bitmap.Width - 1) / 2 } else { $sorted[[Math]::Floor($sorted.Count / 2)] }
        $relative = $_.FullName.Substring($rootPath.Length + 1).Replace("\", "/").Replace(".png", "")
        $metrics[$relative] = [ordered]@{
            width = $bitmap.Width
            height = $bitmap.Height
            ground = @(
                [Math]::Round(($medianX + 0.5) / $bitmap.Width, 6),
                [Math]::Round(($maxY + 1.0) / $bitmap.Height, 6)
            )
        }
    } finally {
        $bitmap.Dispose()
    }
}

$json = $metrics | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText([System.IO.Path]::GetFullPath($Output), $json + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
Write-Output "Wrote $($metrics.Count) metrics to $Output"
