[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string]$Version,

    [switch]$Push,

    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Invoke-Git {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)

    & git @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git $($Arguments -join ' ') failed."
    }
}

function Set-VersionInFile {
    param(
        [string]$Path,
        [string]$Pattern,
        [string]$Label
    )

    $content = [System.IO.File]::ReadAllText($Path)
    $matches = [regex]::Matches($content, $Pattern)
    if ($matches.Count -ne 1) {
        throw "Expected exactly one $Label version in $Path, found $($matches.Count)."
    }

    $updated = [regex]::Replace(
        $content,
        $Pattern,
        [System.Text.RegularExpressions.MatchEvaluator]{
            param($match)
            "$($match.Groups[1].Value)$Version$($match.Groups[2].Value)"
        },
        1
    )

    if (-not $DryRun) {
        [System.IO.File]::WriteAllText($Path, $updated, [System.Text.UTF8Encoding]::new($false))
    }
}

if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$') {
    throw "Version '$Version' is not valid SemVer (for example, 0.1.8 or 0.1.8-rc.1)."
}

$repositoryRoot = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($repositoryRoot)) {
    throw "Run this command from inside the Pulse git repository."
}
Set-Location $repositoryRoot

$tag = "v$Version"
$versionFiles = @(
    "apps/pulse-app/package.json",
    "apps/pulse-app/package-lock.json",
    "apps/pulse-app/src-tauri/Cargo.toml",
    "apps/pulse-app/src-tauri/tauri.conf.json"
)

foreach ($file in $versionFiles) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        throw "Required version file is missing: $file"
    }
}

if ((Invoke-Git tag -l $tag).Count -gt 0) {
    throw "Tag $tag already exists. Choose a new version."
}

if (-not $DryRun) {
    $workingTreeChanges = @(Invoke-Git status --porcelain --untracked-files=all)
    if ($workingTreeChanges.Count -gt 0) {
        throw "The working tree is not clean. Commit or stash your changes before creating a release."
    }
}

Set-VersionInFile -Path "apps/pulse-app/package.json" -Label "package" -Pattern '(?s)(\A\s*\{\s*"name"\s*:\s*"pulse-app"\s*,\s*"private"\s*:\s*true\s*,\s*"version"\s*:\s*")[^"]+(")'
Set-VersionInFile -Path "apps/pulse-app/package-lock.json" -Label "package-lock root" -Pattern '(?s)(\A\s*\{\s*"name"\s*:\s*"pulse-app"\s*,\s*"version"\s*:\s*")[^"]+(")'
Set-VersionInFile -Path "apps/pulse-app/package-lock.json" -Label "package-lock workspace" -Pattern '(?s)(\A.*?"packages"\s*:\s*\{\s*""\s*:\s*\{\s*"name"\s*:\s*"pulse-app"\s*,\s*"version"\s*:\s*")[^"]+(")'
Set-VersionInFile -Path "apps/pulse-app/src-tauri/Cargo.toml" -Label "Tauri Cargo" -Pattern '(?ms)(^\[package\]\s+.*?^version\s*=\s*")[^"]+(")'
Set-VersionInFile -Path "apps/pulse-app/src-tauri/tauri.conf.json" -Label "Tauri configuration" -Pattern '(?s)(\A\s*\{.*?"productName"\s*:\s*"Pulse"\s*,\s*"version"\s*:\s*")[^"]+(")'

if ($DryRun) {
    Write-Host "Would update the Pulse app version files to $Version."
    Write-Host "Would commit 'chore(release): $tag' and create annotated tag $tag."
    if ($Push) {
        Write-Host "Would push the release commit and $tag to origin."
    }
    exit 0
}

$gitAddArguments = @("add", "--") + $versionFiles
Invoke-Git @gitAddArguments
Invoke-Git commit -m "chore(release): $tag"
Invoke-Git tag -a $tag -m "Pulse $tag"

if ($Push) {
    Invoke-Git push origin HEAD
    Invoke-Git push origin $tag
}

Write-Host "Created release commit and tag $tag."
if (-not $Push) {
    Write-Host "Publish it with: git push origin HEAD; git push origin $tag"
}
