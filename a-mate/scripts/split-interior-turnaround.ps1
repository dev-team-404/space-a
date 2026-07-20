param(
    [Parameter(Mandatory = $true)] [string] $Source,
    [Parameter(Mandatory = $true)] [string] $OutputRoot,
    [Parameter(Mandatory = $true)] [string] $Category,
    [Parameter(Mandatory = $true)] [string] $Names,
    [int] $Rows = 2,
    [string] $Directions = "ne,sw"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$sourcePath = (Resolve-Path -LiteralPath $Source).Path
$rootPath = [System.IO.Path]::GetFullPath($OutputRoot)
$itemNames = $Names.Split(",", [System.StringSplitOptions]::RemoveEmptyEntries)
$directionNames = $Directions.Split(",", [System.StringSplitOptions]::RemoveEmptyEntries)

if ($Rows -ne $directionNames.Count) {
    throw "Rows ($Rows) and direction count ($($directionNames.Count)) must match"
}

$sourceBitmap = [System.Drawing.Bitmap]::FromFile($sourcePath)
try {
    for ($row = 0; $row -lt $Rows; $row++) {
        $top = [Math]::Floor($row * $sourceBitmap.Height / $Rows)
        $bottom = [Math]::Floor(($row + 1) * $sourceBitmap.Height / $Rows)

        for ($column = 0; $column -lt $itemNames.Count; $column++) {
            $left = [Math]::Floor($column * $sourceBitmap.Width / $itemNames.Count)
            $right = [Math]::Floor(($column + 1) * $sourceBitmap.Width / $itemNames.Count)
            $cellWidth = $right - $left
            $cellHeight = $bottom - $top
            $cell = New-Object System.Drawing.Bitmap($cellWidth, $cellHeight, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)

            $minX = $cellWidth
            $minY = $cellHeight
            $maxX = -1
            $maxY = -1

            for ($y = 0; $y -lt $cellHeight; $y++) {
                for ($x = 0; $x -lt $cellWidth; $x++) {
                    $pixel = $sourceBitmap.GetPixel($left + $x, $top + $y)
                    $magentaDistance = [Math]::Min([int]$pixel.R, [int]$pixel.B) - [int]$pixel.G
                    $isBackground = $pixel.R -gt 180 -and $pixel.B -gt 150 -and $pixel.G -lt 110 -and $magentaDistance -gt 75
                    if ($isBackground) {
                        $cell.SetPixel($x, $y, [System.Drawing.Color]::Transparent)
                    } else {
                        $cell.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $pixel.R, $pixel.G, $pixel.B))
                        $minX = [Math]::Min($minX, $x)
                        $minY = [Math]::Min($minY, $y)
                        $maxX = [Math]::Max($maxX, $x)
                        $maxY = [Math]::Max($maxY, $y)
                    }
                }
            }

            # A generated neighbour can occasionally cross a logical sheet boundary by a few
            # pixels. Remove every non-primary connected component that touches the crop edge.
            # This makes sprite bleeding a build-time failure mode instead of a runtime artifact.
            $visited = New-Object 'bool[]' ($cellWidth * $cellHeight)
            $components = New-Object System.Collections.Generic.List[object]
            for ($seedY = 0; $seedY -lt $cellHeight; $seedY++) {
                for ($seedX = 0; $seedX -lt $cellWidth; $seedX++) {
                    $seedIndex = $seedY * $cellWidth + $seedX
                    if ($visited[$seedIndex] -or $cell.GetPixel($seedX, $seedY).A -eq 0) { continue }
                    $queue = New-Object System.Collections.Generic.Queue[int]
                    $pixels = New-Object System.Collections.Generic.List[int]
                    $queue.Enqueue($seedIndex)
                    $visited[$seedIndex] = $true
                    $touchesEdge = $false
                    while ($queue.Count -gt 0) {
                        $index = $queue.Dequeue()
                        $pixels.Add($index)
                        $px = $index % $cellWidth
                        $py = [Math]::Floor($index / $cellWidth)
                        if ($px -le 1 -or $py -le 1 -or $px -ge $cellWidth - 2 -or $py -ge $cellHeight - 2) { $touchesEdge = $true }
                        foreach ($delta in @(@(-1, 0), @(1, 0), @(0, -1), @(0, 1), @(-1, -1), @(1, -1), @(-1, 1), @(1, 1))) {
                            $nx = $px + $delta[0]
                            $ny = $py + $delta[1]
                            if ($nx -lt 0 -or $ny -lt 0 -or $nx -ge $cellWidth -or $ny -ge $cellHeight) { continue }
                            $nextIndex = $ny * $cellWidth + $nx
                            if (-not $visited[$nextIndex] -and $cell.GetPixel($nx, $ny).A -gt 0) {
                                $visited[$nextIndex] = $true
                                $queue.Enqueue($nextIndex)
                            }
                        }
                    }
                    $components.Add([PSCustomObject]@{ Pixels = $pixels; Count = $pixels.Count; TouchesEdge = $touchesEdge })
                }
            }
            $primary = $components | Sort-Object Count -Descending | Select-Object -First 1
            foreach ($component in $components) {
                if ($component -ne $primary -and $component.TouchesEdge) {
                    foreach ($index in $component.Pixels) {
                        $cell.SetPixel(($index % $cellWidth), [Math]::Floor($index / $cellWidth), [System.Drawing.Color]::Transparent)
                    }
                }
            }

            # Recompute the tight bounds after boundary-fragment removal.
            $minX = $cellWidth
            $minY = $cellHeight
            $maxX = -1
            $maxY = -1
            for ($y = 0; $y -lt $cellHeight; $y++) {
                for ($x = 0; $x -lt $cellWidth; $x++) {
                    if ($cell.GetPixel($x, $y).A -gt 0) {
                        $minX = [Math]::Min($minX, $x)
                        $minY = [Math]::Min($minY, $y)
                        $maxX = [Math]::Max($maxX, $x)
                        $maxY = [Math]::Max($maxY, $y)
                    }
                }
            }

            if ($maxX -lt $minX -or $maxY -lt $minY) {
                $cell.Dispose()
                throw "No foreground pixels found for $Category/$($itemNames[$column])/$($directionNames[$row])"
            }

            $padding = 2
            $trimmed = New-Object System.Drawing.Bitmap(($maxX - $minX + 1 + 2 * $padding), ($maxY - $minY + 1 + 2 * $padding), [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
            try {
                $graphics = [System.Drawing.Graphics]::FromImage($trimmed)
                try {
                    $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
                    $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
                    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
                    $sourceRect = New-Object System.Drawing.Rectangle($minX, $minY, ($maxX - $minX + 1), ($maxY - $minY + 1))
                    $destRect = New-Object System.Drawing.Rectangle($padding, $padding, $sourceRect.Width, $sourceRect.Height)
                    $graphics.DrawImage($cell, $destRect, $sourceRect, [System.Drawing.GraphicsUnit]::Pixel)
                } finally {
                    $graphics.Dispose()
                }

                $targetDirectory = Join-Path $rootPath "$Category\$($itemNames[$column])"
                [System.IO.Directory]::CreateDirectory($targetDirectory) | Out-Null
                $target = Join-Path $targetDirectory "$($directionNames[$row]).png"
                $trimmed.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
                Write-Output $target
            } finally {
                $trimmed.Dispose()
                $cell.Dispose()
            }
        }
    }
} finally {
    $sourceBitmap.Dispose()
}

# The source sheet uses magenta as its chroma background. The hard background
# cut above intentionally keeps uncertain antialiased pixels opaque, so clean
# only the transparent boundary after every split to prevent chroma fringe.
$categoryRoot = Join-Path $rootPath $Category
$cleanupScript = Join-Path $PSScriptRoot "clean-interior-chroma-fringe.ps1"
$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$cleanupOutput = Join-Path $tempRoot ("space-a-chroma-clean-" + [Guid]::NewGuid().ToString("N"))
try {
    & $cleanupScript -SourceRoot $categoryRoot -OutputRoot $cleanupOutput | Write-Output
    Get-ChildItem -LiteralPath $cleanupOutput -Recurse -File -Filter "*.png" | ForEach-Object {
        $relative = $_.FullName.Substring($cleanupOutput.Length).TrimStart('\', '/')
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $categoryRoot $relative) -Force
    }
} finally {
    if (Test-Path -LiteralPath $cleanupOutput) {
        $resolvedCleanupOutput = (Resolve-Path -LiteralPath $cleanupOutput).Path
        $expectedPrefix = Join-Path $tempRoot "space-a-chroma-clean-"
        if (-not $resolvedCleanupOutput.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove unexpected cleanup directory: $resolvedCleanupOutput"
        }
        Remove-Item -LiteralPath $cleanupOutput -Recurse -Force
    }
}
