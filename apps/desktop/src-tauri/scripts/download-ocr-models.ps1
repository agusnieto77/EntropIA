<#
.SYNOPSIS
    Downloads OCR model files required by the ocrs engine if they are missing.
.DESCRIPTION
    The ocrs Rust OCR engine requires two .rten model files:
    - text-detection.rten (~2.4 MB)
    - text-recognition.rten (~9.3 MB)

    These files are NOT committed to the repository. This script downloads them
    from the official ocrs-models S3 bucket into the Tauri resources directory,
    where tauri.conf.json bundles them automatically.

    Safe to run multiple times - skips files that already exist.
.NOTES
    Model source: https://github.com/robertknight/ocrs
    Bucket: https://ocrs-models.s3-accelerate.amazonaws.com/
#>

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$ResourcesDir = Join-Path $ScriptDir '..\resources'
$ResourcesDir = Resolve-Path $ResourcesDir

$DetectionName = 'text-detection.rten'
$RecognitionName = 'text-recognition.rten'
$BaseUrl = 'https://ocrs-models.s3-accelerate.amazonaws.com'

# Ensure resources directory exists
if (-not (Test-Path $ResourcesDir)) {
    New-Item -ItemType Directory -Path $ResourcesDir -Force | Out-Null
}

$Downloaded = 0

# Process detection model
$TargetPath = Join-Path $ResourcesDir $DetectionName
if (Test-Path $TargetPath) {
    $Size = [math]::Round((Get-Item $TargetPath).Length / 1MB, 1)
    Write-Host ('[OK]   ' + $DetectionName + ' already exists (' + $Size + ' MB) - skipping')
} else {
    $Url = $BaseUrl + '/' + $DetectionName
    Write-Host ('[..] Downloading ' + $DetectionName + ' (~2.4 MB)...')
    Invoke-WebRequest -Uri $Url -OutFile $TargetPath -UseBasicParsing
    $Size = [math]::Round((Get-Item $TargetPath).Length / 1MB, 1)
    Write-Host ('[OK]   ' + $DetectionName + ' downloaded (' + $Size + ' MB)')
    $Downloaded++
}

# Process recognition model
$TargetPath = Join-Path $ResourcesDir $RecognitionName
if (Test-Path $TargetPath) {
    $Size = [math]::Round((Get-Item $TargetPath).Length / 1MB, 1)
    Write-Host ('[OK]   ' + $RecognitionName + ' already exists (' + $Size + ' MB) - skipping')
} else {
    $Url = $BaseUrl + '/' + $RecognitionName
    Write-Host ('[..] Downloading ' + $RecognitionName + ' (~9.3 MB)...')
    Invoke-WebRequest -Uri $Url -OutFile $TargetPath -UseBasicParsing
    $Size = [math]::Round((Get-Item $TargetPath).Length / 1MB, 1)
    Write-Host ('[OK]   ' + $RecognitionName + ' downloaded (' + $Size + ' MB)')
    $Downloaded++
}

Write-Host ''
if ($Downloaded -gt 0) {
    Write-Host ('Downloaded ' + $Downloaded + ' model(s). OCR engine is ready.')
} else {
    Write-Host 'All models already present. Nothing to do.'
}
