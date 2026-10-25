param([switch]$Check)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$masterDir = Join-Path $root 'assets\brand'
$tauriIcons = Join-Path $root 'src-tauri\icons'
$webImages = Join-Path $root 'src\assets\image'

$appIconSizes = 16, 24, 32, 48, 64, 72, 96, 128, 256
$watchdogIconSizes = 16, 32, 48, 64, 128, 256
$faviconSizes = 16, 32, 48
$brandIconSize = 56

# 标准母版：应用与 Watchdog 1024x1024（覆盖到 256px 输出仍有 4 倍余量），托盘按母版原尺寸发布。
# 更大的正方形母版同样接受，小于 1024 直接报错。
$masters = @(
    @{ Name = 'app-icon.png'; MinSize = 1024; ExactSize = 0 },
    @{ Name = 'watchdog-icon.png'; MinSize = 1024; ExactSize = 0 },
    @{ Name = 'tray-normal.png'; MinSize = 16; ExactSize = 0 },
    @{ Name = 'tray-system-proxy.png'; MinSize = 16; ExactSize = 0 },
    @{ Name = 'tray-tun.png'; MinSize = 16; ExactSize = 0 }
)

$tauriOutputs = @(
    'icon.ico',
    '32x32.png',
    '128x128.png',
    '128x128@2x.png',
    'watchdog.ico',
    'tray-icon.png',
    'tray-icon-sys.png',
    'tray-icon-tun.png'
)

$webOutputs = @('favicon.ico', 'brand-icon.png')

function Assert-Master {
    param([string]$Name, [int]$MinSize, [int]$ExactSize)

    $path = Join-Path $masterDir $Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing master: assets/brand/$Name"
    }

    $bitmap = [Drawing.Bitmap]::new((Resolve-Path -LiteralPath $path).Path)
    try {
        if ($bitmap.Width -ne $bitmap.Height) {
            throw "Master must be square: assets/brand/$Name ($($bitmap.Width)x$($bitmap.Height))"
        }
        if ($ExactSize -gt 0 -and $bitmap.Width -ne $ExactSize) {
            throw "Master must be ${ExactSize}x${ExactSize}: assets/brand/$Name"
        }
        if ($ExactSize -eq 0 -and $bitmap.Width -lt $MinSize) {
            throw "Master must be at least ${MinSize}x${MinSize}: assets/brand/$Name"
        }
    } finally {
        $bitmap.Dispose()
    }
}

function New-ScaledBitmap {
    param([Drawing.Bitmap]$Source, [int]$Size)

    $target = [Drawing.Bitmap]::new($Size, $Size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [Drawing.Graphics]::FromImage($target)
    try {
        $graphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
        $graphics.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
        $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::HighQuality
        $graphics.Clear([Drawing.Color]::Transparent)
        $graphics.DrawImage($Source, [Drawing.Rectangle]::new(0, 0, $Size, $Size))
    } finally {
        $graphics.Dispose()
    }
    return $target
}

function Convert-BitmapToPngBytes {
    param([Drawing.Bitmap]$Bitmap)

    $stream = [IO.MemoryStream]::new()
    try {
        $Bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
        return $stream.ToArray()
    } finally {
        $stream.Dispose()
    }
}

function Write-PngFile {
    param([string]$Path, [Drawing.Bitmap]$Source, [int]$Size)

    $bitmap = New-ScaledBitmap $Source $Size
    try {
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bitmap.Dispose()
    }
}

function Write-IcoFile {
    param([string]$Path, [Drawing.Bitmap]$Source, [int[]]$Sizes)

    $frames = @()
    foreach ($size in $Sizes) {
        $bitmap = New-ScaledBitmap $Source $size
        try {
            $frames += [pscustomobject]@{ Size = $size; Bytes = (Convert-BitmapToPngBytes $bitmap) }
        } finally {
            $bitmap.Dispose()
        }
    }

    $stream = [IO.File]::Create($Path)
    $writer = [IO.BinaryWriter]::new($stream)
    try {
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]$frames.Count)
        $offset = 6 + 16 * $frames.Count
        foreach ($frame in $frames) {
            $dimension = if ($frame.Size -ge 256) { 0 } else { $frame.Size }
            $writer.Write([byte]$dimension)
            $writer.Write([byte]$dimension)
            $writer.Write([byte]0)
            $writer.Write([byte]0)
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]$frame.Bytes.Length)
            $writer.Write([uint32]$offset)
            $offset += $frame.Bytes.Length
        }
        foreach ($frame in $frames) {
            $writer.Write([byte[]]$frame.Bytes)
        }
    } finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

function Get-PixelHash {
    param([Drawing.Bitmap]$Bitmap)

    $source = $Bitmap
    $converted = $null
    if ($Bitmap.PixelFormat -ne [Drawing.Imaging.PixelFormat]::Format32bppArgb) {
        $converted = [Drawing.Bitmap]::new($Bitmap.Width, $Bitmap.Height, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [Drawing.Graphics]::FromImage($converted)
        try {
            $graphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.DrawImage($Bitmap, [Drawing.Rectangle]::new(0, 0, $Bitmap.Width, $Bitmap.Height))
        } finally {
            $graphics.Dispose()
        }
        $source = $converted
    }

    try {
        $rect = [Drawing.Rectangle]::new(0, 0, $source.Width, $source.Height)
        $data = $source.LockBits($rect, [Drawing.Imaging.ImageLockMode]::ReadOnly, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $length = [Math]::Abs($data.Stride) * $source.Height
            $buffer = New-Object byte[] $length
            [Runtime.InteropServices.Marshal]::Copy($data.Scan0, $buffer, 0, $length)
        } finally {
            $source.UnlockBits($data)
        }
        $sha = [Security.Cryptography.SHA256]::Create()
        try {
            return ([BitConverter]::ToString($sha.ComputeHash($buffer)) -replace '-', '')
        } finally {
            $sha.Dispose()
        }
    } finally {
        if ($converted) { $converted.Dispose() }
    }
}

function Get-BitmapHash {
    param([string]$Path)

    $bitmap = [Drawing.Bitmap]::new((Resolve-Path -LiteralPath $Path).Path)
    try {
        return [pscustomobject]@{ Width = $bitmap.Width; Height = $bitmap.Height; Hash = (Get-PixelHash $bitmap) }
    } finally {
        $bitmap.Dispose()
    }
}

function Get-IcoFrames {
    param([string]$Path)

    $bytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Path).Path)
    if ($bytes.Length -lt 22) { throw "ICO is too small: $Path" }
    $count = [BitConverter]::ToUInt16($bytes, 4)
    $frames = @()
    for ($index = 0; $index -lt $count; $index++) {
        $entry = 6 + $index * 16
        $size = $bytes[$entry]
        if ($size -eq 0) { $size = 256 }
        $length = [BitConverter]::ToUInt32($bytes, $entry + 8)
        $offset = [BitConverter]::ToUInt32($bytes, $entry + 12)
        if ($bytes[$offset] -ne 0x89 -or $bytes[$offset + 1] -ne 0x50) {
            throw "ICO frame is not PNG-compressed (run icons to regenerate): $Path"
        }
        $payload = New-Object byte[] $length
        [Array]::Copy($bytes, $offset, $payload, 0, $length)
        $stream = [IO.MemoryStream]::new($payload)
        $bitmap = [Drawing.Bitmap]::new($stream)
        try {
            $frames += [pscustomobject]@{ Size = $size; Hash = (Get-PixelHash $bitmap) }
        } finally {
            $bitmap.Dispose()
            $stream.Dispose()
        }
    }
    return $frames
}

function Build-Icons {
    param([string]$TauriOut, [string]$WebOut)

    New-Item -ItemType Directory -Force -Path $TauriOut, $WebOut | Out-Null

    foreach ($master in $masters) {
        Assert-Master -Name $master.Name -MinSize $master.MinSize -ExactSize $master.ExactSize
    }

    $app = [Drawing.Bitmap]::new((Resolve-Path -LiteralPath (Join-Path $masterDir 'app-icon.png')).Path)
    try {
        Write-IcoFile -Path (Join-Path $TauriOut 'icon.ico') -Source $app -Sizes $appIconSizes
        Write-PngFile -Path (Join-Path $TauriOut '32x32.png') -Source $app -Size 32
        Write-PngFile -Path (Join-Path $TauriOut '128x128.png') -Source $app -Size 128
        Write-PngFile -Path (Join-Path $TauriOut '128x128@2x.png') -Source $app -Size 256
        Write-IcoFile -Path (Join-Path $WebOut 'favicon.ico') -Source $app -Sizes $faviconSizes
        Write-PngFile -Path (Join-Path $WebOut 'brand-icon.png') -Source $app -Size $brandIconSize
    } finally {
        $app.Dispose()
    }

    $watchdog = [Drawing.Bitmap]::new((Resolve-Path -LiteralPath (Join-Path $masterDir 'watchdog-icon.png')).Path)
    try {
        Write-IcoFile -Path (Join-Path $TauriOut 'watchdog.ico') -Source $watchdog -Sizes $watchdogIconSizes
    } finally {
        $watchdog.Dispose()
    }

    $trayMasters = @(
        @{ Master = 'tray-normal.png'; Output = 'tray-icon.png' },
        @{ Master = 'tray-system-proxy.png'; Output = 'tray-icon-sys.png' },
        @{ Master = 'tray-tun.png'; Output = 'tray-icon-tun.png' }
    )
    foreach ($tray in $trayMasters) {
        # 托盘图按母版原尺寸发布，不做缩放；换新图时直接给最终尺寸（建议 32x32）
        Copy-Item -LiteralPath (Join-Path $masterDir $tray.Master) -Destination (Join-Path $TauriOut $tray.Output) -Force
    }
}

function Compare-PngPair {
    param([string]$Generated, [string]$Committed, [string]$Label, [System.Collections.ArrayList]$Errors)

    if (-not (Test-Path -LiteralPath $Committed -PathType Leaf)) {
        [void]$Errors.Add("Missing generated file: $Label")
        return
    }

    $left = Get-BitmapHash $Generated
    $right = Get-BitmapHash $Committed
    if ($left.Width -ne $right.Width -or $left.Height -ne $right.Height) {
        [void]$Errors.Add("Size mismatch: $Label (expected $($left.Width)x$($left.Height), found $($right.Width)x$($right.Height))")
        return
    }
    if ($left.Hash -ne $right.Hash) {
        [void]$Errors.Add("Outdated generated file: $Label")
    }
}

function Compare-IcoPair {
    param([string]$Generated, [string]$Committed, [string]$Label, [System.Collections.ArrayList]$Errors)

    if (-not (Test-Path -LiteralPath $Committed -PathType Leaf)) {
        [void]$Errors.Add("Missing generated file: $Label")
        return
    }

    $left = Get-IcoFrames $Generated
    $right = Get-IcoFrames $Committed
    $leftSizes = ($left | ForEach-Object Size) -join ','
    $rightSizes = ($right | ForEach-Object Size) -join ','
    if ($leftSizes -ne $rightSizes) {
        [void]$Errors.Add("ICO frames mismatch: $Label (expected [$leftSizes], found [$rightSizes])")
        return
    }
    for ($index = 0; $index -lt $left.Count; $index++) {
        if ($left[$index].Hash -ne $right[$index].Hash) {
            [void]$Errors.Add("Outdated ICO frame $($left[$index].Size)x$($left[$index].Size): $Label")
        }
    }
}

function Invoke-Check {
    $errors = [System.Collections.ArrayList]::new()
    $temp = Join-Path $root ('.tmp\icons-check\' + [guid]::NewGuid().ToString('N'))

    try {
        $generatedTauri = Join-Path $temp 'tauri'
        $generatedWeb = Join-Path $temp 'web'
        Build-Icons -TauriOut $generatedTauri -WebOut $generatedWeb

        Compare-IcoPair -Generated (Join-Path $generatedTauri 'icon.ico') -Committed (Join-Path $tauriIcons 'icon.ico') -Label 'src-tauri/icons/icon.ico' -Errors $errors
        Compare-PngPair -Generated (Join-Path $generatedTauri '32x32.png') -Committed (Join-Path $tauriIcons '32x32.png') -Label 'src-tauri/icons/32x32.png' -Errors $errors
        Compare-PngPair -Generated (Join-Path $generatedTauri '128x128.png') -Committed (Join-Path $tauriIcons '128x128.png') -Label 'src-tauri/icons/128x128.png' -Errors $errors
        Compare-PngPair -Generated (Join-Path $generatedTauri '128x128@2x.png') -Committed (Join-Path $tauriIcons '128x128@2x.png') -Label 'src-tauri/icons/128x128@2x.png' -Errors $errors
        Compare-IcoPair -Generated (Join-Path $generatedTauri 'watchdog.ico') -Committed (Join-Path $tauriIcons 'watchdog.ico') -Label 'src-tauri/icons/watchdog.ico' -Errors $errors
        Compare-PngPair -Generated (Join-Path $generatedTauri 'tray-icon.png') -Committed (Join-Path $tauriIcons 'tray-icon.png') -Label 'src-tauri/icons/tray-icon.png' -Errors $errors
        Compare-PngPair -Generated (Join-Path $generatedTauri 'tray-icon-sys.png') -Committed (Join-Path $tauriIcons 'tray-icon-sys.png') -Label 'src-tauri/icons/tray-icon-sys.png' -Errors $errors
        Compare-PngPair -Generated (Join-Path $generatedTauri 'tray-icon-tun.png') -Committed (Join-Path $tauriIcons 'tray-icon-tun.png') -Label 'src-tauri/icons/tray-icon-tun.png' -Errors $errors
        Compare-IcoPair -Generated (Join-Path $generatedWeb 'favicon.ico') -Committed (Join-Path $webImages 'favicon.ico') -Label 'src/assets/image/favicon.ico' -Errors $errors
        Compare-PngPair -Generated (Join-Path $generatedWeb 'brand-icon.png') -Committed (Join-Path $webImages 'brand-icon.png') -Label 'src/assets/image/brand-icon.png' -Errors $errors

        $extra = Get-ChildItem -LiteralPath $tauriIcons -File | Where-Object { $tauriOutputs -notcontains $_.Name }
        foreach ($file in $extra) {
            [void]$errors.Add("Unexpected file in src-tauri/icons: $($file.Name)")
        }
        foreach ($name in $tauriOutputs) {
            if (-not (Test-Path -LiteralPath (Join-Path $tauriIcons $name) -PathType Leaf)) {
                [void]$errors.Add("Missing generated file: src-tauri/icons/$name")
            }
        }
        foreach ($name in $webOutputs) {
            if (-not (Test-Path -LiteralPath (Join-Path $webImages $name) -PathType Leaf)) {
                [void]$errors.Add("Missing generated file: src/assets/image/$name")
            }
        }
    } finally {
        if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    }

    if ($errors.Count -gt 0) {
        Write-Host '[FAIL] icon check:'
        foreach ($message in $errors) { Write-Host " - $message" }
        exit 1
    }

    Write-Host '[OK] generated icons match assets/brand masters'
}

if ($Check) {
    Invoke-Check
    exit 0
}

Build-Icons -TauriOut $tauriIcons -WebOut $webImages
Write-Host '[OK] icons generated from assets/brand masters'
