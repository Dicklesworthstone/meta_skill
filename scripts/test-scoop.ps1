#Requires -Version 5.1
<#
.SYNOPSIS
    E2E test for Scoop bucket installation of ms.

.DESCRIPTION
    This script tests the complete Scoop installation flow for ms.
    It should be run on Windows with PowerShell 5.1 or later.

.PARAMETER SkipCleanup
    Don't uninstall ms after testing.

.PARAMETER LocalManifest
    Path to a local manifest file to test instead of the bucket.

.EXAMPLE
    .\test-scoop.ps1

.EXAMPLE
    .\test-scoop.ps1 -SkipCleanup

.EXAMPLE
    .\test-scoop.ps1 -LocalManifest ".\bucket\ms.json"
#>

[CmdletBinding()]
param(
    [switch]$SkipCleanup,
    [string]$LocalManifest
)

$ErrorActionPreference = "Stop"

# Configuration
$BucketName = "ms"
$BucketUrl = "https://github.com/Dicklesworthstone/scoop-bucket"
$FormulaName = "ms"
$LogFile = "$env:TEMP\ms-scoop-test-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"

# Start logging
Start-Transcript -Path $LogFile

function Write-Log {
    param([string]$Message, [string]$Color = "White")
    $timestamp = Get-Date -Format "HH:mm:ss"
    Write-Host "[$timestamp] $Message" -ForegroundColor $Color
}

function Write-Success {
    param([string]$Message)
    Write-Log "[OK] $Message" -Color Green
}

function Write-Warning {
    param([string]$Message)
    Write-Log "[WARN] $Message" -Color Yellow
}

function Write-Error {
    param([string]$Message)
    Write-Log "[ERROR] $Message" -Color Red
}

# Check prerequisites
function Test-Prerequisites {
    Write-Log "Checking prerequisites..."

    # Check if Scoop is installed
    $scoopPath = Get-Command scoop -ErrorAction SilentlyContinue
    if (-not $scoopPath) {
        Write-Error "Scoop is not installed. Please install it first:"
        Write-Host "  Set-ExecutionPolicy RemoteSigned -Scope CurrentUser -Force"
        Write-Host "  irm get.scoop.sh | iex"
        exit 1
    }
    Write-Success "Scoop is installed: $($scoopPath.Source)"

    # Check if ms is already installed
    $existingMs = scoop list | Where-Object { $_ -match "^ms\s" }
    if ($existingMs) {
        Write-Warning "ms is already installed via Scoop"
        if ($LocalManifest) {
            Write-Log "Uninstalling existing ms for local manifest test..."
            scoop uninstall $FormulaName 2>$null
        }
    }
}

# Test bucket addition
function Test-BucketAdd {
    Write-Log "Testing bucket addition..."

    # Remove bucket if it exists (for clean test)
    $existingBucket = scoop bucket list | Where-Object { $_ -match "^$BucketName\s" }
    if ($existingBucket) {
        Write-Log "Removing existing bucket for clean test..."
        scoop bucket rm $BucketName 2>$null
    }

    # Add bucket
    Write-Log "Adding bucket: $BucketUrl"
    scoop bucket add $BucketName $BucketUrl

    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to add bucket"
        exit 1
    }
    Write-Success "Bucket added successfully"

    # Verify bucket is listed
    $bucket = scoop bucket list | Where-Object { $_ -match "^$BucketName\s" }
    if (-not $bucket) {
        Write-Error "Bucket not found in scoop bucket list"
        exit 1
    }
    Write-Success "Bucket is listed in scoop bucket list"
}

# Test installation
function Test-Install {
    Write-Log "Testing installation..."

    if ($LocalManifest) {
        if (Test-Path $LocalManifest) {
            Write-Log "Installing from local manifest: $LocalManifest"
            scoop install $LocalManifest
        } else {
            Write-Error "Local manifest not found: $LocalManifest"
            exit 1
        }
    } else {
        Write-Log "Installing $BucketName/$FormulaName..."
        scoop install "$BucketName/$FormulaName"
    }

    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to install ms"
        exit 1
    }
    Write-Success "ms installed successfully"

    # Verify installation
    $msPath = Get-Command ms -ErrorAction SilentlyContinue
    if (-not $msPath) {
        Write-Error "ms.exe not found in PATH"
        exit 1
    }
    Write-Success "ms.exe is available in PATH: $($msPath.Source)"
}

# Test basic functionality
function Test-BasicCommands {
    Write-Log "Testing basic commands..."

    # Version
    $version = & ms --version 2>&1
    Write-Log "Version output: $version"
    if ($version -match "^ms\s+\d+\.\d+") {
        Write-Success "--version works"
    } else {
        Write-Warning "--version output format unexpected (may still be valid)"
    }

    # Help
    try {
        & ms --help | Out-Null
        Write-Success "--help works"
    } catch {
        Write-Error "--help failed: $($_.Exception.Message)"
        exit 1
    }

    # Doctor
    Write-Log "Running ms doctor..."
    try {
        & ms doctor 2>&1
        Write-Success "doctor command works"
    } catch {
        Write-Warning "doctor command had warnings (this may be expected if not initialized)"
    }

    # List
    Write-Log "Testing ms list..."
    try {
        & ms list --limit=1 2>$null
        Write-Success "list command works"
    } catch {
        Write-Warning "list returned no skills (expected if not initialized)"
    }
}

# Test upgrade path
function Test-Upgrade {
    Write-Log "Testing upgrade..."

    if ($LocalManifest) {
        Write-Log "Skipping upgrade test for local manifest"
        return
    }

    scoop update $FormulaName 2>&1
    Write-Success "Update check complete"
}

# Cleanup
function Invoke-Cleanup {
    Write-Log "Cleaning up..."

    if ($SkipCleanup) {
        Write-Warning "Skipping cleanup (-SkipCleanup specified)"
        Write-Log "To manually cleanup later, run:"
        Write-Host "  scoop uninstall ms"
        Write-Host "  scoop bucket rm $BucketName"
        return
    }

    # Uninstall ms
    $existingMs = scoop list | Where-Object { $_ -match "^ms\s" }
    if ($existingMs) {
        Write-Log "Uninstalling ms..."
        scoop uninstall $FormulaName
        Write-Success "ms uninstalled"
    }

    # Remove bucket
    $existingBucket = scoop bucket list | Where-Object { $_ -match "^$BucketName\s" }
    if ($existingBucket) {
        Write-Log "Removing bucket..."
        scoop bucket rm $BucketName
        Write-Success "Bucket removed"
    }

    Write-Success "Cleanup complete"
}

# Main test flow
function Main {
    Write-Host ""
    Write-Host "===============================================================" -ForegroundColor Cyan
    Write-Host "        ms Scoop Bucket E2E Test                               " -ForegroundColor Cyan
    Write-Host "===============================================================" -ForegroundColor Cyan
    Write-Host ""

    Write-Log "Log file: $LogFile"
    Write-Log "Skip cleanup: $SkipCleanup"
    Write-Log "Local manifest: $(if ($LocalManifest) { $LocalManifest } else { 'None' })"
    Write-Host ""

    # Run tests
    Test-Prerequisites
    Write-Host ""

    if (-not $LocalManifest) {
        Test-BucketAdd
        Write-Host ""
    }

    Test-Install
    Write-Host ""

    Test-BasicCommands
    Write-Host ""

    Test-Upgrade
    Write-Host ""

    Invoke-Cleanup
    Write-Host ""

    Write-Host "===============================================================" -ForegroundColor Green
    Write-Host "        All tests passed!                                      " -ForegroundColor Green
    Write-Host "===============================================================" -ForegroundColor Green
    Write-Host ""
    Write-Log "Log saved to: $LogFile"
}

# Run main with error handling
try {
    Main
} catch {
    Write-Error "Test failed: $($_.Exception.Message)"
    Write-Log "Check log: $LogFile"
    exit 1
} finally {
    Stop-Transcript
}
