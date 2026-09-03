# Screenshot a page of the docs site at desktop and phone widths.
#
#   powershell -ExecutionPolicy Bypass -File tools\site\shoot.ps1 <page.html> [tag] [theme]
#
# Needs a static server on http://localhost:8765 serving docs/site
# (python3 -m http.server 8765 from that directory) and Chrome. Writes
# tools/site/shots/<page>-<tag>-{desktop,desktop-full,mobile,mobile-full}.png.
# theme = dark | light (default dark); pages honour ?theme=light.
#
# Mobile shots render the page inside a 390px iframe (docs/site/dev-frame.html)
# because Chrome will not open a window narrower than ~500px: a plain
# --window-size=390 screenshot is a 500px layout cropped to 390, which looks
# like horizontal overflow when there is none. dev-overflow.html?page=X lists
# every element wider than the viewport when there is.
param(
  [Parameter(Mandatory = $true)][string]$Page,
  [string]$Tag = "v",
  [string]$Theme = "dark"
)
$chrome = @(
  "C:\Program Files\Google\Chrome\Application\chrome.exe",
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $chrome) { Write-Error "no Chrome or Edge found"; exit 1 }

$dir = Join-Path $PSScriptRoot "shots"
New-Item -ItemType Directory -Force $dir | Out-Null
$name = [IO.Path]::GetFileNameWithoutExtension($Page)
$base = "http://localhost:8765"
$themeQ = ""
$frameTheme = ""
if ($Theme -eq "light") { $themeQ = "?theme=light"; $frameTheme = "&theme=light" }

$jobs = @(
  @{ size = "1440,900";  file = "$name-$Tag-desktop.png";      url = "$base/$Page$themeQ" },
  @{ size = "1440,3200"; file = "$name-$Tag-desktop-full.png"; url = "$base/$Page$themeQ" },
  @{ size = "520,860";   file = "$name-$Tag-mobile.png";       url = "$base/dev-frame.html?page=$Page&w=390&h=844$frameTheme" },
  @{ size = "520,3020";  file = "$name-$Tag-mobile-full.png";  url = "$base/dev-frame.html?page=$Page&w=390&h=3000$frameTheme" }
)
foreach ($j in $jobs) {
  $out = Join-Path $dir $j.file
  $args = @(
    "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run",
    "--virtual-time-budget=5000", "--window-size=$($j.size)", "--screenshot=$out", $j.url
  )
  $p = Start-Process -FilePath $chrome -ArgumentList $args -PassThru -WindowStyle Hidden
  $p.WaitForExit(30000) | Out-Null
  if (-not $p.HasExited) { $p.Kill() }
  if (Test-Path $out) { Write-Output "wrote $out" } else { Write-Output "FAILED $($j.file)" }
}
