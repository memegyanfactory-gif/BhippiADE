$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$icons = Join-Path $root 'crates\bhippi-app\icons'
New-Item -ItemType Directory -Force -Path $icons | Out-Null
Add-Type -AssemblyName System.Drawing

function New-Icon([int]$size, [string]$path) {
  $bmp = New-Object System.Drawing.Bitmap($size, $size)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.Clear([System.Drawing.Color]::Transparent)

  $dark = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255,11,12,14))
  $green = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255,74,222,155))
  $greenPenW = [Math]::Max(1.0, $size * 0.045)
  $greenPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255,74,222,155), $greenPenW)

  # rounded square background
  $r = [int]($size * 0.22)
  $gp = New-Object System.Drawing.Drawing2D.GraphicsPath
  $gp.AddArc(0, 0, $r, $r, 180, 90)
  $gp.AddArc($size - $r, 0, $r, $r, 270, 90)
  $gp.AddArc($size - $r, $size - $r, $r, $r, 0, 90)
  $gp.AddArc(0, $size - $r, $r, $r, 90, 90)
  $gp.CloseFigure()
  $g.FillPath($dark, $gp)

  # orbit ring + node dot (the "mind map" mark)
  $inset = [int]($size * 0.24)
  $d = $size - 2 * $inset
  $g.DrawEllipse($greenPen, $inset, $inset, $d, $d)
  $dot = [Math]::Max(2.0, $size * 0.14)
  $cx = $size / 2 - $dot / 2
  $cy = $size * 0.26
  $g.FillEllipse($green, [float]$cx, [float]$cy, [float]$dot, [float]$dot)

  $g.Dispose()
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
}

New-Icon 32 (Join-Path $icons '32x32.png')
New-Icon 128 (Join-Path $icons '128x128.png')
New-Icon 512 (Join-Path $icons 'icon.png')

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
