param()

$ErrorActionPreference = "Stop"

$archFiles = Get-ChildItem -Path "src\arch" -Filter "*.rs" -Recurse -File
$patterns = @(
    "crate\s*::\s*kernel",
    "use\s+crate\s*::\s*\{[^}]*kernel"
)
$violations = @()

foreach ($file in $archFiles) {
    foreach ($pattern in $patterns) {
        $patternMatches = Select-String -Path $file.FullName -Pattern $pattern
        foreach ($match in $patternMatches) {
            $violations += [PSCustomObject]@{
                Path = Resolve-Path -Path $file.FullName -Relative
                Line = $match.LineNumber
                Text = $match.Line.Trim()
            }
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Host "[architecture-boundaries] FAIL: src\arch must not depend on kernel modules"
    foreach ($violation in $violations) {
        Write-Host ("{0}:{1}: {2}" -f $violation.Path, $violation.Line, $violation.Text)
    }
    exit 1
}

Write-Host "[architecture-boundaries] PASS"
