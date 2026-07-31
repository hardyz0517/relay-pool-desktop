[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BaseUrl,

    [string]$Model = "",

    [string]$OutputPath = "output\architecture-scale\qualification\live-provider\openai-compatible-live-qualification-summary.json"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($env:RELAY_POOL_LIVE_API_KEY)) {
    throw "RELAY_POOL_LIVE_API_KEY is required"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$outputFullPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path $repoRoot $OutputPath
}
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outputFullPath) | Out-Null

function Join-ApiPath {
    param([string]$Root, [string]$Path)
    return $Root.TrimEnd("/") + "/" + $Path.TrimStart("/")
}

function Redact-Text {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return "" }
    $redacted = $Value -replace "sk-[A-Za-z0-9_-]+", "sk-redacted"
    $redacted = $redacted -replace "(?i)bearer\s+[A-Za-z0-9._~+/=-]+", "Bearer [REDACTED]"
    $redacted = $redacted -replace "(?i)(authorization|cookie|api[-_]?key|token)\s*[:=]\s*[^,\s}]+", '$1=[REDACTED]'
    $redacted = $redacted -replace "https?://[^\s`"')\]}]+", "[url-redacted]"
    if ($redacted.Length -gt 240) {
        return $redacted.Substring(0, 240)
    }
    return $redacted
}

function Get-Sha256Hex {
    param([string]$Value)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        return (($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") }) -join "")
    } finally {
        $sha.Dispose()
    }
}

function Get-EndpointHostClass {
    param([AllowNull()][string]$Host)
    if ([string]::IsNullOrWhiteSpace($Host)) { return "unknown" }
    $normalized = $Host.Trim().ToLowerInvariant()
    if ($normalized -in @("localhost", "127.0.0.1", "::1")) { return "loopback" }
    $ip = $null
    if ([System.Net.IPAddress]::TryParse($normalized, [ref]$ip)) {
        $bytes = $ip.GetAddressBytes()
        if ($ip.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork) {
            if ($bytes[0] -eq 10) { return "private" }
            if ($bytes[0] -eq 172 -and $bytes[1] -ge 16 -and $bytes[1] -le 31) { return "private" }
            if ($bytes[0] -eq 192 -and $bytes[1] -eq 168) { return "private" }
            if ($bytes[0] -eq 127) { return "loopback" }
            return "public-ip"
        }
        if ($ip.IsIPv6LinkLocal -or $ip.IsIPv6SiteLocal) { return "private" }
        if ([System.Net.IPAddress]::IsLoopback($ip)) { return "loopback" }
        return "public-ip"
    }
    return "hostname-redacted"
}

function Get-EndpointEvidence {
    param([string]$RawBaseUrl)
    $normalized = $RawBaseUrl.TrimEnd("/")
    $uri = $null
    $parseOk = [System.Uri]::TryCreate($normalized, [System.UriKind]::Absolute, [ref]$uri)
    return [ordered]@{
        redacted = $true
        raw_url_stored = $false
        sha256 = Get-Sha256Hex $normalized
        scheme = if ($parseOk) { $uri.Scheme } else { "invalid" }
        host_class = if ($parseOk) { Get-EndpointHostClass $uri.Host } else { "invalid" }
        explicit_port = if ($parseOk) { -not $uri.IsDefaultPort } else { $false }
        path_present = if ($parseOk) { -not [string]::IsNullOrWhiteSpace($uri.AbsolutePath.Trim("/")) } else { $false }
    }
}

function Invoke-JsonGet {
    param([string]$Uri)
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $response = Invoke-RestMethod `
            -Method Get `
            -Uri $Uri `
            -Headers @{ Authorization = "Bearer $env:RELAY_POOL_LIVE_API_KEY" } `
            -TimeoutSec 60
        $watch.Stop()
        return [ordered]@{
            ok = $true
            status = 200
            latency_ms = [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
            value = $response
            error = $null
        }
    } catch {
        $watch.Stop()
        $status = 0
        try {
            if ($_.Exception.Response) { $status = [int]$_.Exception.Response.StatusCode }
        } catch {
            $status = 0
        }
        return [ordered]@{
            ok = $false
            status = $status
            latency_ms = [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
            value = $null
            error = (Redact-Text $_.Exception.Message)
        }
    }
}

function Invoke-CurlPost {
    param([string]$Uri, [string]$Body)
    $temp = [System.IO.Path]::GetTempFileName()
    $bodyPath = [System.IO.Path]::GetTempFileName()
    try {
        Set-Content -Encoding UTF8 -NoNewline -Path $bodyPath -Value $Body
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        $curlResult = & curl.exe `
            -sS `
            -o $temp `
            -w "%{http_code} %{time_total}" `
            -X POST $Uri `
            -H "Authorization: Bearer $env:RELAY_POOL_LIVE_API_KEY" `
            -H "Content-Type: application/json" `
            --data-binary "@$bodyPath" `
            --max-time 120
        $watch.Stop()
        $parts = $curlResult -split " "
        $status = if ($parts.Count -gt 0 -and $parts[0] -match "^\d+$") { [int]$parts[0] } else { 0 }
        $latencyMs = if ($parts.Count -gt 1) {
            [math]::Round(([double]$parts[1]) * 1000, 2)
        } else {
            [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
        }
        $content = Get-Content -Raw -Path $temp -ErrorAction SilentlyContinue
        return [ordered]@{
            ok = ($status -ge 200 -and $status -lt 300)
            status = $status
            latency_ms = $latencyMs
            body_non_empty = (-not [string]::IsNullOrWhiteSpace($content))
            preview = (Redact-Text $content.Trim())
        }
    } finally {
        Remove-Item -LiteralPath $temp -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $bodyPath -Force -ErrorAction SilentlyContinue
    }
}

function New-ProbeBody {
    param([string]$Kind, [string]$SelectedModel, [bool]$Stream)
    if ($Kind -eq "responses") {
        return @{
            model = $SelectedModel
            input = "hi"
            store = $false
            stream = $Stream
            max_output_tokens = 32
        } | ConvertTo-Json -Depth 8 -Compress
    }
    return @{
        model = $SelectedModel
        messages = @(@{ role = "user"; content = "hi" })
        stream = $Stream
        max_tokens = 32
    } | ConvertTo-Json -Depth 8 -Compress
}

function Invoke-ProtocolProbe {
    param([string]$Kind, [string]$SelectedModel)
    $path = if ($Kind -eq "responses") { "/v1/responses" } else { "/v1/chat/completions" }
    $uri = Join-ApiPath $BaseUrl $path
    $stream = Invoke-CurlPost -Uri $uri -Body (New-ProbeBody $Kind $SelectedModel $true)
    if ($stream.ok) {
        return [ordered]@{
            final = $stream
            response_mode = "stream"
            stream_attempt = $stream
            non_stream_attempt = $null
            stream_fallback_reason = $null
        }
    }
    $nonStream = Invoke-CurlPost -Uri $uri -Body (New-ProbeBody $Kind $SelectedModel $false)
    return [ordered]@{
        final = $nonStream
        response_mode = "non_stream_fallback"
        stream_attempt = $stream
        non_stream_attempt = $nonStream
        stream_fallback_reason = $stream.preview
    }
}

function Test-AllowsChatFallback {
    param([int]$Status, [string]$Message)
    if ($Status -in @(404, 405, 501)) { return $true }
    if ($Status -ge 500) { return $true }
    if ($Status -ne 400) { return $false }
    $normalized = $Message.ToLowerInvariant()
    foreach ($needle in @(
        "failed to parse request body",
        "unknown parameter",
        "unrecognized request argument",
        "unsupported parameter",
        "does not support responses",
        "responses api is not supported"
    )) {
        if ($normalized.Contains($needle)) { return $true }
    }
    return $false
}

$startedAt = (Get-Date).ToUniversalTime().ToString("o")
$models = Invoke-JsonGet (Join-ApiPath $BaseUrl "/v1/models")
if (-not $models.ok) {
    throw "models endpoint failed with HTTP $($models.status): $($models.error)"
}

$modelIds = @($models.value.data | ForEach-Object { $_.id } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
if ($modelIds.Count -eq 0) {
    throw "models endpoint returned no model ids"
}

$selectedModel = $Model.Trim()
if ([string]::IsNullOrWhiteSpace($selectedModel) -or -not ($modelIds -contains $selectedModel)) {
    $selectedModel = $modelIds[0]
}

$responses = Invoke-ProtocolProbe "responses" $selectedModel
$chat = $null
$protocol = "responses"
if (-not $responses.final.ok -and (Test-AllowsChatFallback $responses.final.status $responses.final.preview)) {
    $chat = Invoke-ProtocolProbe "chat_completions" $selectedModel
    $protocol = if ($chat.final.ok) { "chat_completions_after_responses_fallback" } else { "failed_after_chat_fallback" }
} elseif (-not $responses.final.ok) {
    $protocol = "failed_no_chat_fallback"
}

$success = $responses.final.ok -or ($null -ne $chat -and $chat.final.ok)
$finalStatus = if ($null -ne $chat -and $chat.final.ok) {
    $chat.final.status
} elseif ($responses.final.ok) {
    $responses.final.status
} elseif ($null -ne $chat) {
    $chat.final.status
} else {
    $responses.final.status
}
$responseMode = if ($null -ne $chat -and $chat.final.ok) {
    $chat.response_mode
} elseif ($responses.final.ok) {
    $responses.response_mode
} else {
    "none"
}

$summary = [ordered]@{
    task = 27
    kind = "product-shaped station key connectivity live probe"
    generated_at_local = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")
    source_revision = (& git -C $repoRoot rev-parse HEAD).Trim()
    endpoint = Get-EndpointEvidence $BaseUrl
    auth = [ordered]@{
        scheme = "bearer"
        credential = "redacted"
    }
    upstream_api_format = "custom_openai_compatible"
    selected_model = $selectedModel
    models = [ordered]@{
        path = "/v1/models"
        success = $true
        latency_ms = $models.latency_ms
        count = $modelIds.Count
        sample_ids = @($modelIds | Select-Object -First 8)
    }
    responses_attempt = $responses
    chat_completions_attempt = $chat
    final_result = [ordered]@{
        success = $success
        protocol = $protocol
        status = $finalStatus
        response_mode = $responseMode
    }
    started_at_utc = $startedAt
    ended_at_utc = (Get-Date).ToUniversalTime().ToString("o")
}

$summary | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 -Path $outputFullPath

if (-not $success) {
    throw "live provider probe failed with protocol=$protocol status=$finalStatus; summary=$OutputPath"
}

Write-Host "live provider probe passed; protocol=$protocol status=$finalStatus mode=$responseMode summary=$OutputPath"
