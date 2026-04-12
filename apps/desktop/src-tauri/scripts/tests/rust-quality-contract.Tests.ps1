Set-StrictMode -Version Latest

$TestRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
$ScriptRoot = Resolve-Path (Join-Path $TestRoot "..")
$RustQualityContractPath = Resolve-Path (Join-Path $ScriptRoot "rust-quality-contract.ps1")
$RustCoveragePath = Resolve-Path (Join-Path $ScriptRoot "rust-coverage.ps1")

. $RustQualityContractPath
. $RustCoveragePath

Describe "test bootstrap" {
  It "loads rust quality contract functions" {
    (Get-Command Classify-RustSignal -ErrorAction SilentlyContinue) | Should Not BeNullOrEmpty
    (Get-Command Get-RustCoverageArgs -ErrorAction SilentlyContinue) | Should Not BeNullOrEmpty
    (Get-Command Write-RustQualitySummary -ErrorAction SilentlyContinue) | Should Not BeNullOrEmpty
  }
}

Describe "Classify-RustSignal" {
  It "returns pass when command succeeds" {
    $result = Classify-RustSignal -Tool "coverage" -ExitCode 0 -Output "ok"
    $result | Should Be "pass"
  }

  It "returns report-only for fmt debt findings" {
    $result = Classify-RustSignal -Tool "fmt" -ExitCode 1 -Output "Diff in src/main.rs"
    $result | Should Be "report-only"
  }

  It "returns report-only for clippy debt findings" {
    $result = Classify-RustSignal -Tool "clippy" -ExitCode 101 -Output "warning: use of unwrap"
    $result | Should Be "report-only"
  }

  It "returns infra-error for missing coverage tooling" {
    $result = Classify-RustSignal -Tool "coverage" -ExitCode 1 -Output "no such command: llvm-cov"
    $result | Should Be "infra-error"
  }

  It "returns infra-error for fmt invocation errors" {
    $result = Classify-RustSignal -Tool "fmt" -ExitCode 1 -Output "error: unexpected argument '--bogus' found"
    $result | Should Be "infra-error"
  }

  It "returns infra-error for clippy invocation errors" {
    $result = Classify-RustSignal -Tool "clippy" -ExitCode 1 -Output "error: Found argument '--bogus' which wasn't expected"
    $result | Should Be "infra-error"
  }
}

Describe "Get-RustCoverageArgs" {
  It "builds contractual coverage command with no-default-features" {
    $args = Get-RustCoverageArgs -ManifestPath "apps/desktop/src-tauri/Cargo.toml" -OutputDir "target/coverage-rust"
    $joined = $args -join " "

    $joined | Should Match "llvm-cov"
    $joined | Should Match "--manifest-path apps/desktop/src-tauri/Cargo.toml"
    $joined | Should Match "--no-default-features"
    $joined | Should Match "--lcov"
  }
}

Describe "Write-RustQualitySummary" {
  It "writes report-first markdown with classifications" {
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null

    $summaryPath = Join-Path $tmp "summary.md"
    $result = @{
      coverage = @{ status = "pass"; note = "coverage generated" }
      fmt      = @{ status = "report-only"; note = "formatting debt" }
      clippy   = @{ status = "infra-error"; note = "tool missing" }
    }

    Write-RustQualitySummary -SummaryPath $summaryPath -Results $result
    $content = Get-Content -Path $summaryPath -Raw

    $content | Should Match "Rust Coverage Baseline"
    $content | Should Match "coverage: pass"
    $content | Should Match "fmt: report-only"
    $content | Should Match "clippy: infra-error"
  }
}
