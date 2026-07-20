param(
    [string]$BaseUrl,
    [string]$DatabasePath = (Join-Path $env:APPDATA "dev.relaypool.desktop\relay-pool-desktop.sqlite3"),
    [string]$Bearer = $env:RELAY_POOL_LOCAL_BEARER,
    [string]$Model = $env:RELAY_POOL_E2E_MODEL,
    [switch]$Smoke,
    [switch]$SkipDbVerify
)

$ErrorActionPreference = "Stop"

function Fail($Message) {
    Write-Error $Message
    exit 1
}

function Mask-Text($Value) {
    if ($null -eq $Value) { return $null }
    $text = [string]$Value
    $text = $text -replace '(?i)bearer\s+[A-Za-z0-9._~+/=-]+', 'Bearer [REDACTED]'
    $text = $text -replace 'sk-[A-Za-z0-9._~+/=-]{8,}', 'sk-[REDACTED]'
    $text = $text -replace '(?i)(authorization|cookie)\s*[:=]\s*[^,\s}]+', '$1=[REDACTED]'
    return $text
}

function Read-ProxyPort($Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $python = Get-Command python -ErrorAction SilentlyContinue
    if (-not $python) {
        return $null
    }
    $script = @'
import sqlite3
import sys
connection = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
row = connection.execute("SELECT value FROM settings WHERE key = 'local_proxy_port'").fetchone()
print(row[0] if row and row[0] else "")
'@
    $encodedScript = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script))
    $bootstrap = "import base64; exec(base64.b64decode('$encodedScript').decode('utf-8'))"
    $port = (& $python.Source -c $bootstrap $Path).Trim()
    if ($port -match '^\d+$') { return [int]$port }
    return $null
}

function New-JsonContent($Object) {
    return [System.Net.Http.StringContent]::new(
        ($Object | ConvertTo-Json -Depth 20 -Compress),
        [System.Text.Encoding]::UTF8,
        "application/json"
    )
}

function Invoke-LifecycleRequest($Name, $Method, $Path, $BodyObject, [switch]$Stream, [switch]$CancelAfterFirstChunk) {
    $client = [System.Net.Http.HttpClient]::new()
    $client.Timeout = [TimeSpan]::FromMinutes(5)
    $request = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::$Method, "$BaseUrl$Path")
    $request.Headers.Authorization = [System.Net.Http.Headers.AuthenticationHeaderValue]::new("Bearer", $Bearer)
    if ($Stream) {
        $request.Headers.Accept.ParseAdd("text/event-stream")
    }
    if ($null -ne $BodyObject) {
        $request.Content = New-JsonContent $BodyObject
    }

    try {
        $completion = if ($Stream -or $CancelAfterFirstChunk) {
            [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
        } else {
            [System.Net.Http.HttpCompletionOption]::ResponseContentRead
        }
        $response = $client.SendAsync($request, $completion).GetAwaiter().GetResult()
        $requestId = if ($response.Headers.Contains("x-relay-request-id")) {
            ($response.Headers.GetValues("x-relay-request-id") | Select-Object -First 1)
        } else {
            $null
        }
        $bytes = 0
        $terminal = $null
        if ($Stream -or $CancelAfterFirstChunk) {
            $streamReader = [System.IO.StreamReader]::new($response.Content.ReadAsStream())
            try {
                while (-not $streamReader.EndOfStream) {
                    $line = $streamReader.ReadLine()
                    if ($null -eq $line) { break }
                    $bytes += [System.Text.Encoding]::UTF8.GetByteCount($line)
                    if ($line -eq "data: [DONE]" -or $line -match '"type"\s*:\s*"response\.(completed|failed|incomplete)"') {
                        $terminal = $line
                    }
                    if ($CancelAfterFirstChunk -and $bytes -gt 0) {
                        break
                    }
                }
            } finally {
                $streamReader.Dispose()
            }
        } else {
            $content = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
            $bytes = [System.Text.Encoding]::UTF8.GetByteCount($content)
            $terminal = if ($content.Length -gt 0) { "buffered-body" } else { "empty-body" }
        }

        [pscustomobject]@{
            name = $Name
            method = $Method
            path = $Path
            status = [int]$response.StatusCode
            request_id = $requestId
            body_bytes = $bytes
            terminal_seen = if ($terminal) { $true } else { $false }
            cancelled = [bool]$CancelAfterFirstChunk
        }
    } finally {
        $request.Dispose()
        $client.Dispose()
    }
}

if (-not $Bearer) {
    Fail "Set RELAY_POOL_LOCAL_BEARER in the process environment; this script never reads or prints the bearer from SQLite."
}
if (-not $Model) {
    Fail "Set RELAY_POOL_E2E_MODEL to a model that the configured real station/key can serve."
}
if (-not $BaseUrl) {
    $port = Read-ProxyPort $DatabasePath
    if (-not $port) { $port = 8787 }
    $BaseUrl = "http://127.0.0.1:$port"
}

$matrix = @(
    @{ name = "models"; method = "Get"; path = "/v1/models"; body = $null; stream = $false; cancel = $false },
    @{ name = "chat-non-stream"; method = "Post"; path = "/v1/chat/completions"; body = @{ model = $Model; messages = @(@{ role = "user"; content = "Reply with one short lifecycle verification sentence." }); stream = $false }; stream = $false; cancel = $false },
    @{ name = "chat-stream"; method = "Post"; path = "/v1/chat/completions"; body = @{ model = $Model; messages = @(@{ role = "user"; content = "Stream one short lifecycle verification sentence." }); stream = $true }; stream = $true; cancel = $false },
    @{ name = "responses-non-stream"; method = "Post"; path = "/v1/responses"; body = @{ model = $Model; input = "Reply with one short lifecycle verification sentence."; stream = $false }; stream = $false; cancel = $false },
    @{ name = "responses-stream"; method = "Post"; path = "/v1/responses"; body = @{ model = $Model; input = "Stream one short lifecycle verification sentence."; stream = $true }; stream = $true; cancel = $false },
    @{ name = "embeddings"; method = "Post"; path = "/v1/embeddings"; body = @{ model = $Model; input = "lifecycle verification" }; stream = $false; cancel = $false },
    @{ name = "chat-stream-cancel"; method = "Post"; path = "/v1/chat/completions"; body = @{ model = $Model; messages = @(@{ role = "user"; content = "Stream slowly enough that the client can cancel after the first chunk." }); stream = $true }; stream = $true; cancel = $true }
)

if ($Smoke) {
    $matrix = $matrix | Where-Object { $_.name -in @("models", "chat-non-stream") }
}

$results = New-Object System.Collections.Generic.List[object]
foreach ($case in $matrix) {
    $result = Invoke-LifecycleRequest $case.name $case.method $case.path $case.body -Stream:([bool]$case.stream) -CancelAfterFirstChunk:([bool]$case.cancel)
    if (-not $result.request_id) {
        Fail "No x-relay-request-id returned for $($case.name)"
    }
    if ($result.status -lt 200 -or $result.status -ge 500) {
        Fail "Request $($case.name) returned HTTP $($result.status) before DB verification"
    }
    $results.Add($result) | Out-Null

    if (-not $SkipDbVerify) {
        & (Join-Path $PSScriptRoot "verify-request-lifecycle-db.ps1") -DatabasePath $DatabasePath -RequestId $result.request_id
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
}

$safe = $results | ConvertTo-Json -Depth 8
Write-Output (Mask-Text $safe)
