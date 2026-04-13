Set-StrictMode -Version Latest

Describe "lockfile-reset diagnostic workflow contract" {
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
    $script:workflowPath = Join-Path -Path $script:RepoRoot -ChildPath ".github/workflows/lockfile-reset.yml"
  }

  It "defines standalone lockfile reset probe workflow" {
    Assert-True -Condition (Test-Path -Path $script:workflowPath) -Message "lockfile-reset workflow file must exist"

    $workflow = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $workflow -Pattern "name:\s*Lockfile Reset Probe" -Message "workflow must be explicitly named Lockfile Reset Probe"
    Assert-Match -Value $workflow -Pattern "on:\s*[\s\S]*?workflow_dispatch:" -Message "workflow must support manual workflow_dispatch trigger"
    Assert-Match -Value $workflow -Pattern "on:\s*[\s\S]*?push:\s*[\s\S]*?paths:" -Message "workflow must define push path filters"
    Assert-True -Condition (-not ($workflow -match "pull_request:")) -Message "workflow must stay isolated and not trigger on pull_request"
  }

  It "limits push paths to lockfile/install-related files" {
    $workflow = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $workflow -Pattern "push:\s*[\s\S]*?paths:\s*[\s\S]*?-\s*pnpm-lock\.yaml" -Message "push filter must include pnpm-lock.yaml"
    Assert-Match -Value $workflow -Pattern "push:\s*[\s\S]*?paths:\s*[\s\S]*?-\s*package\.json" -Message "push filter must include package.json"
    Assert-Match -Value $workflow -Pattern "push:\s*[\s\S]*?paths:\s*[\s\S]*?-\s*pnpm-workspace\.yaml" -Message "push filter must include pnpm-workspace.yaml"
    Assert-Match -Value $workflow -Pattern "push:\s*[\s\S]*?paths:\s*[\s\S]*?-\s*\.npmrc" -Message "push filter must include .npmrc"
    Assert-Match -Value $workflow -Pattern "push:\s*[\s\S]*?paths:\s*[\s\S]*?-\s*\.github/workflows/lockfile-reset\.yml" -Message "push filter must include the workflow file itself"
  }

  It "pins ubuntu, Node 20 and pnpm 9.15.4 in a single diagnostic job" {
    $workflow = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $workflow -Pattern "jobs:\s*[\s\S]*?lockfile-reset-probe:" -Message "workflow must declare a single lockfile-reset-probe job"
    Assert-Match -Value $workflow -Pattern "lockfile-reset-probe:[\s\S]*?runs-on:\s*ubuntu-latest" -Message "diagnostic job must run on ubuntu-latest"
    Assert-Match -Value $workflow -Pattern "lockfile-reset-probe:[\s\S]*?name:\s*Setup pnpm[\s\S]*?uses:\s*pnpm/action-setup@v6" -Message "workflow must use pnpm/action-setup in probe job"
    Assert-Match -Value $workflow -Pattern "with:[\s\S]*?version:\s*9\.15\.4" -Message "workflow must pin pnpm 9.15.4"
    Assert-Match -Value $workflow -Pattern "lockfile-reset-probe:[\s\S]*?name:\s*Setup Node[\s\S]*?uses:\s*actions/setup-node@v6[\s\S]*?node-version:\s*20" -Message "workflow must pin Node 20"
  }

  It "emits required diagnostics and frozen-lockfile install command" {
    $workflow = Get-Content -Path $script:workflowPath -Raw

    Assert-Match -Value $workflow -Pattern "name:\s*Lockfile diagnostics" -Message "workflow must include minimal inline lockfile diagnostics step"
    Assert-Match -Value $workflow -Pattern "Lockfile diagnostics[\s\S]*?node\s+-v" -Message "diagnostics must log node version"
    Assert-Match -Value $workflow -Pattern "Lockfile diagnostics[\s\S]*?pnpm\s+-v" -Message "diagnostics must log pnpm version"
    Assert-Match -Value $workflow -Pattern "Lockfile diagnostics[\s\S]*?lockfile_diag\.sha256=" -Message "diagnostics must log lockfile sha256"
    Assert-Match -Value $workflow -Pattern "Lockfile diagnostics[\s\S]*?lockfile_diag\.first_line_is_yaml_doc=" -Message "diagnostics must log first-line YAML doc marker check"
    Assert-Match -Value $workflow -Pattern "Lockfile diagnostics[\s\S]*?lockfile_diag\.working_blob=" -Message "diagnostics must log working tree lockfile blob"
    Assert-Match -Value $workflow -Pattern "Lockfile diagnostics[\s\S]*?lockfile_diag\.head_blob=" -Message "diagnostics must log HEAD lockfile blob"
    Assert-Match -Value $workflow -Pattern "Lockfile diagnostics[\s\S]*?lockfile_diag\.matches_head_blob=" -Message "diagnostics must log working-vs-HEAD lockfile blob comparison"
    Assert-Match -Value $workflow -Pattern "name:\s*Install dependencies[\s\S]*?pnpm\s+install\s+--frozen-lockfile" -Message "workflow must probe pnpm install --frozen-lockfile"
  }
}
