$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$icons = Join-Path $root 'crates\bhippi-app\icons'
$sourcePath = Join-Path $root 'ui\public\bhippi-logo.png'
New-Item -ItemType Directory -Force -Path $icons | Out-Null
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Image]::FromFile($sourcePath)

function New-Icon([int]$size, [string]$path) {
  $bmp = New-Object System.Drawing.Bitmap($size, $size)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
  $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)
  $g.DrawImage($source, 0, 0, $size, $size)
  $g.Dispose()
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

New-Icon 32 (Join-Path $icons '32x32.png')
New-Icon 128 (Join-Path $icons '128x128.png')
New-Icon 512 (Join-Path $icons 'icon.png')
$source.Dispose()

# PNG-in-ICO wrapper for icon.ico (Vista+ format)
$png32 = [IO.File]::ReadAllBytes((Join-Path $icons '32x32.png'))
$ms = New-Object IO.MemoryStream
$bw = New-Object IO.BinaryWriter($ms)
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]1)   # ICONDIR: 1 image
$bw.Write([byte]32); $bw.Write([byte]32); $bw.Write([byte]0); $bw.Write([byte]0)
$bw.Write([uint16]1); $bw.Write([uint16]32)                        # planes, bpp
$bw.Write([uint32]$png32.Length); $bw.Write([uint32]22)
$bw.Write($png32)
$bw.Flush()
[IO.File]::WriteAllBytes((Join-Path $icons 'icon.ico'), $ms.ToArray())
$bw.Dispose(); $ms.Dispose()
Write-Output "icons written to $icons"
