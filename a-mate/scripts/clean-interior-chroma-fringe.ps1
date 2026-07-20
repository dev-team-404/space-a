param(
    [Parameter(Mandatory = $true)] [string] $SourceRoot,
    [Parameter(Mandatory = $true)] [string] $OutputRoot,
    [int] $BandRadius = 2,
    [int] $SampleRadius = 5,
    [int] $MagentaThreshold = 25,
    [int] $MaxReplacementMagentaScore = 8
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$sourcePath = (Resolve-Path -LiteralPath $SourceRoot).Path
$outputPath = [System.IO.Path]::GetFullPath($OutputRoot)
[System.IO.Directory]::CreateDirectory($outputPath) | Out-Null

function Get-MagentaScore([System.Drawing.Color] $color) {
    return [Math]::Min([int] $color.R, [int] $color.B) - [int] $color.G
}

function Test-Foreground([System.Drawing.Bitmap] $bitmap, [int] $x, [int] $y) {
    return $x -ge 0 -and $y -ge 0 -and $x -lt $bitmap.Width -and $y -lt $bitmap.Height -and $bitmap.GetPixel($x, $y).A -gt 0
}

function Test-BoundaryBand([System.Drawing.Bitmap] $bitmap, [int] $x, [int] $y, [int] $radius) {
    for ($dy = -$radius; $dy -le $radius; $dy++) {
        for ($dx = -$radius; $dx -le $radius; $dx++) {
            if ([Math]::Abs($dx) + [Math]::Abs($dy) -gt $radius) { continue }
            if (-not (Test-Foreground $bitmap ($x + $dx) ($y + $dy))) { return $true }
        }
    }
    return $false
}

function Find-InteriorSample([System.Drawing.Bitmap] $bitmap, [int] $x, [int] $y, [int] $radius, [int] $threshold) {
    $best = $null
    $bestDistance = [int]::MaxValue
    for ($dy = -$radius; $dy -le $radius; $dy++) {
        for ($dx = -$radius; $dx -le $radius; $dx++) {
            $distance = $dx * $dx + $dy * $dy
            if ($distance -eq 0 -or $distance -ge $bestDistance) { continue }
            $sx = $x + $dx
            $sy = $y + $dy
            if (-not (Test-Foreground $bitmap $sx $sy)) { continue }
            $sample = $bitmap.GetPixel($sx, $sy)
            if ((Get-MagentaScore $sample) -ge $threshold) { continue }
            if (Test-BoundaryBand $bitmap $sx $sy 1) { continue }
            $best = $sample
            $bestDistance = $distance
        }
    }
    return $best
}

function Get-Luminance([System.Drawing.Color] $color) {
    return 0.2126 * $color.R + 0.7152 * $color.G + 0.0722 * $color.B
}

function Get-ReplacementColor([System.Drawing.Color] $original, [AllowNull()] [object] $sample) {
    $luminance = Get-Luminance $original
    if ($null -eq $sample) {
        $r = [Math]::Round([Math]::Min(255, $luminance * 1.04))
        $g = [Math]::Round([Math]::Min(255, $luminance))
        $b = [Math]::Round([Math]::Min(255, $luminance * 1.08))
    } else {
        $sampleLuminance = [Math]::Max(1, (Get-Luminance $sample))
        $scale = $luminance / $sampleLuminance
        $r = [Math]::Round([Math]::Min(255, $sample.R * $scale))
        $g = [Math]::Round([Math]::Min(255, $sample.G * $scale))
        $b = [Math]::Round([Math]::Min(255, $sample.B * $scale))
    }
    $minimumGreen = [Math]::Min($r, $b) - $MaxReplacementMagentaScore
    $g = [Math]::Max($g, $minimumGreen)
    return [System.Drawing.Color]::FromArgb($original.A, $r, $g, $b)
}

$totalChanged = 0
$files = Get-ChildItem -LiteralPath $sourcePath -Recurse -Filter "*.png"
foreach ($file in $files) {
    $relative = $file.FullName.Substring($sourcePath.Length).TrimStart('\', '/')
    $target = Join-Path $outputPath $relative
    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($target)) | Out-Null

    $source = [System.Drawing.Bitmap]::FromFile($file.FullName)
    try {
        $output = New-Object System.Drawing.Bitmap($source.Width, $source.Height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($output)
            try {
                $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
                $graphics.DrawImageUnscaled($source, 0, 0)
            } finally {
                $graphics.Dispose()
            }

            $changed = 0
            for ($y = 0; $y -lt $source.Height; $y++) {
                for ($x = 0; $x -lt $source.Width; $x++) {
                    $pixel = $source.GetPixel($x, $y)
                    if ($pixel.A -eq 0 -or (Get-MagentaScore $pixel) -lt $MagentaThreshold) { continue }
                    if (-not (Test-BoundaryBand $source $x $y $BandRadius)) { continue }
                    $sample = Find-InteriorSample $source $x $y $SampleRadius $MagentaThreshold
                    $output.SetPixel($x, $y, (Get-ReplacementColor $pixel $sample))
                    $changed++
                }
            }

            $output.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
            $totalChanged += $changed
            Write-Output "$relative`t$changed"
        } finally {
            $output.Dispose()
        }
    } finally {
        $source.Dispose()
    }
}

Write-Output "TOTAL`t$totalChanged"
