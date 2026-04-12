Set-StrictMode -Version Latest

$TestRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
$ScriptRoot = Resolve-Path (Join-Path $TestRoot "..")
$RustVerifyGatePath = Resolve-Path (Join-Path $ScriptRoot "rust-verify-gate.ps1")

. $RustVerifyGatePath

Describe "test bootstrap" {
  It "loads rust verify gate functions" {
    (Get-Command Test-RequiresRustEvidence -ErrorAction SilentlyContinue) | Should Not BeNullOrEmpty
    (Get-Command Write-RustVerifyEvidence -ErrorAction SilentlyContinue) | Should Not BeNullOrEmpty
  }
}

Describe "Test-RequiresRustEvidence" {
  It "returns true when at least one .rs file changed" {
    $requires = Test-RequiresRustEvidence -ChangedFiles @(
      "apps/desktop/src-tauri/src/lib.rs",
      "README.md"
    )

    $requires | Should Be $true
  }

  It "returns false when no .rs files changed" {
    $requires = Test-RequiresRustEvidence -ChangedFiles @(
      "apps/desktop/src/routes/+page.svelte",
      "package.json"
    )

    $requires | Should Be $false
  }
}

Describe "Write-RustVerifyEvidence" {
  It "writes mandatory sections when Rust evidence is required" {
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    $evidencePath = Join-Path $tmp "rust-verify-evidence.md"

    Write-RustVerifyEvidence -EvidencePath $evidencePath -RequiresRustEvidence $true -Classification @{
      coverage = "pass"
      fmt = "report-only"
      clippy = "infra-error"
    }

    $content = Get-Content -Path $evidencePath -Raw
    $content | Should Match "Rust Coverage Baseline"
    $content | Should Match "Rust Quality Report"
    $content | Should Match "Classification"
    $content | Should Match "coverage: pass"
  }

  It "writes explicit out-of-scope justification when no Rust files changed" {
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    $evidencePath = Join-Path $tmp "rust-verify-evidence.md"

    Write-RustVerifyEvidence -EvidencePath $evidencePath -RequiresRustEvidence $false -Classification @{}

    $content = Get-Content -Path $evidencePath -Raw
    $content | Should Match "out-of-scope"
  }
}
