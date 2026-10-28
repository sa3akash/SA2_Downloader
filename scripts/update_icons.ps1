Add-Type -AssemblyName System.Drawing

$src = "C:\Users\SHAKIL\.gemini\antigravity\brain\063107ba-1ae9-4c8b-a937-e333c4fdd7c1\.user_uploaded\media_1793202797218_cd134e6b.png"
if (-not (Test-Path $src)) {
    Write-Error "Source image not found: $src"
    exit 1
}

$orig = [System.Drawing.Bitmap]::FromFile($src)
Write-Host "Source image size: $($orig.Width)x$($orig.Height)"

function Resize-Image($source, $w, $h, $destPath) {
    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.DrawImage($source, 0, 0, $w, $h)
    $g.Dispose()
    $bmp.Save($destPath, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host "Saved $w x $h to $destPath"
}

# 1. Update assets/icon.png and browser-extension/icons/icon.png
Copy-Item $src "assets/icon.png" -Force
Copy-Item $src "browser-extension/icons/icon.png" -Force
Write-Host "Updated full-res icon.png in assets and browser-extension/icons"

# 2. Generate browser extension icons
Resize-Image $orig 128 128 "browser-extension/icons/icon-128.png"
Resize-Image $orig 48 48 "browser-extension/icons/icon-48.png"
Resize-Image $orig 32 32 "browser-extension/icons/icon-32.png"
Resize-Image $orig 16 16 "browser-extension/icons/icon-16.png"

# 3. Create Windows multi-size .ico file
# An ICO file consists of ICONDIR header, ICONDIRENTRY array, and PNG/BMP image data
$sizes = @(16, 32, 48, 64, 128, 256)
$pngBytesList = @()

foreach ($sz in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap($sz, $sz)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.DrawImage($orig, 0, 0, $sz, $sz)
    $g.Dispose()
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $pngBytesList += ,$ms.ToArray()
    $ms.Dispose()
}

$icoFs = [System.IO.File]::Create("assets/icon.ico")
$bw = New-Object System.IO.BinaryWriter($icoFs)

# ICONDIR: idReserved (0), idType (1 for icon), idCount
$bw.Write([uint16]0)
$bw.Write([uint16]1)
$bw.Write([uint16]$sizes.Length)

$offset = 6 + ($sizes.Length * 16)

for ($i = 0; $i -lt $sizes.Length; $i++) {
    $sz = $sizes[$i]
    $data = $pngBytesList[$i]
    $bWidth = if ($sz -ge 256) { [byte]0 } else { [byte]$sz }
    $bHeight = if ($sz -ge 256) { [byte]0 } else { [byte]$sz }
    
    $bw.Write([byte]$bWidth)             # bWidth
    $bw.Write([byte]$bHeight)            # bHeight
    $bw.Write([byte]0)                   # bColorCount
    $bw.Write([byte]0)                   # bReserved
    $bw.Write([uint16]1)                 # wPlanes
    $bw.Write([uint16]32)                # wBitCount
    $bw.Write([uint32]$data.Length)      # dwBytesInRes
    $bw.Write([uint32]$offset)           # dwImageOffset
    $offset += $data.Length
}

for ($i = 0; $i -lt $sizes.Length; $i++) {
    $bw.Write($pngBytesList[$i])
}

$bw.Flush()
$bw.Close()
$icoFs.Close()
$orig.Dispose()

Write-Host "Successfully generated assets/icon.ico"
