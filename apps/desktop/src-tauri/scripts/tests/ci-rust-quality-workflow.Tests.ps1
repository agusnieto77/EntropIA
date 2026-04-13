Set-StrictMode -Version Latest

Describe "rust-quality-report workflow" {
  BeforeAll {
    function Assert-True {
      param(
        [bool]$Condition,
        [string]$Message
      )

      if (-not $Condition) {
        throw $Message
      }
    }

    function Assert-Match {
      param(
        [string]$Value,
        [string]$Pattern,
        [string]$Message
      )

      if ($Value -notmatch $Pattern) {
        throw $Message
      }
    }

    $script:TestRoot = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
    $script:RepoRoot = (Resolve-Path -Path (Join-Path $script:TestRoot "../../../../..")).Path
    $script:workflowPath = Join-Path -Path $script:RepoRoot -ChildPath ".github/workflows/ci.yml"
  }

  It "resolves workflow path robustly" {
    Assert-True -Condition (-not [string]::IsNullOrWhiteSpace($script:workflowPath)) -Message "workflowPath should not be null or empty"
    Assert-True -Condition (Test-Path -Path $script:workflowPath) -Message "workflowPath should exist"
    Assert-Match -Value $script:workflowPath -Pattern "[\\/]\.github[\\/]workflows[\\/]ci\.yml$" -Message "workflowPath should target .github/workflows/ci.yml"
  }

  It "installs rustfmt and clippy components" {
    $content = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $content -Pattern "rustup component add llvm-tools-preview rustfmt clippy" -Message "workflow must install llvm-tools-preview rustfmt clippy"
  }

  It "keeps cargo-llvm-cov installation step" {
    $content = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $content -Pattern "Install cargo-llvm-cov" -Message "workflow must keep cargo-llvm-cov installation step"
  }

  It "runs Rust quality Pester suites explicitly in CI" {
    $content = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $content -Pattern "Run Rust quality Pester suites" -Message "workflow must contain explicit Pester suites step"
    Assert-Match -Value $content -Pattern "Invoke-Pester" -Message "workflow must invoke Pester explicitly"
    Assert-Match -Value $content -Pattern "rust-quality-contract.Tests.ps1" -Message "workflow must run rust-quality-contract suite"
    Assert-Match -Value $content -Pattern "rust-verify-gate.Tests.ps1" -Message "workflow must run rust-verify-gate suite"
    Assert-Match -Value $content -Pattern "ci-rust-quality-workflow.Tests.ps1" -Message "workflow must run workflow contract suite"
  }

  It "runs pnpm pre-install forensics before installing dependencies" {
    $content = Get-Content -Path $script:workflowPath -Raw

    $forensicsIndex = $content.IndexOf("Run pnpm pre-install forensics (rust-quality-report)")
    $installIndex = $content.IndexOf("name: Install dependencies")

    Assert-True -Condition ($forensicsIndex -ge 0) -Message "workflow must include rust-quality-report pre-install forensic step"
    Assert-True -Condition ($installIndex -ge 0) -Message "workflow must include Install dependencies step"
    Assert-True -Condition ($forensicsIndex -lt $installIndex) -Message "forensic step must run before Install dependencies"
  }

  It "uploads rust-quality-report pre-install forensics artifact with always policy" {
    $content = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $content -Pattern "Upload pnpm pre-install forensics \(rust-quality-report\)" -Message "workflow must include rust-quality-report forensics upload"
    Assert-Match -Value $content -Pattern "Upload pnpm pre-install forensics \(rust-quality-report\)[\s\S]*?if:\s*always\(\)" -Message "rust-quality-report forensics upload must run with if: always()"
    Assert-Match -Value $content -Pattern "Upload pnpm pre-install forensics \(rust-quality-report\)[\s\S]*?\.ci-evidence/pnpm-preinstall/rust-quality-report/" -Message "rust-quality-report forensics upload must include rust-quality-report evidence folder"
  }

  It "uploads baseline coverage artifacts lcov and summary" {
    $content = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $content -Pattern "Upload rust quality artifacts" -Message "workflow must upload rust quality artifacts"
    Assert-Match -Value $content -Pattern "coverage-rust/lcov.info" -Message "workflow artifacts must include lcov.info"
    Assert-Match -Value $content -Pattern "coverage-rust/summary.md" -Message "workflow artifacts must include summary.md"
  }
}
