# ==============================================================================
# NxDesk - Iniciador de Túnel Seguro TLS / HTTPS / WSS para Internet Público
# ==============================================================================
param(
    [ValidateSet("cloudflare", "caddy")]
    [string]$Metodo = "cloudflare",
    [string]$Dominio = "localhost",
    [int]$PuertoLocal = 8080
)

$ErrorActionPreference = "Stop"

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "  🔒 NxDesk Secure WAN Gateway (HTTPS & WSS)     " -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

if ($Metodo -eq "cloudflare") {
    Write-Host "`n[1/3] Verificando Cloudflare Tunnel (cloudflared)..." -ForegroundColor Yellow
    $cloudflared = Get-Command cloudflared -ErrorAction SilentlyContinue

    if (-not $cloudflared) {
        Write-Host "⚠️ 'cloudflared' no está instalado en este sistema." -ForegroundColor Yellow
        Write-Host "Instalando automáticamente mediante winget..." -ForegroundColor Cyan
        try {
            winget install --id Cloudflare.cloudflared -e --silent --accept-source-agreements --accept-package-agreements
            Write-Host "✅ cloudflared instalado exitosamente." -ForegroundColor Green
        } catch {
            Write-Host "❌ No se pudo instalar automáticamente. Descárgalo desde: https://github.com/cloudflare/cloudflared/releases" -ForegroundColor Red
            return
        }
    }

    Write-Host "`n[2/3] Verificando que NxDesk Client esté activo en puerto $PuertoLocal..." -ForegroundColor Yellow
    $testPort = Test-NetConnection -ComputerName 127.0.0.1 -Port $PuertoLocal -InformationLevel Quiet
    if (-not $testPort) {
        Write-Host "⚠️ Advertencia: No se detecta nxdesk-client en el puerto $PuertoLocal." -ForegroundColor Yellow
        Write-Host "Inicia primero: cargo run --release -p nxdesk-client -- --share" -ForegroundColor Yellow
    }

    Write-Host "`n[3/3] Iniciando Túnel Cifrado TLS (Cero apertura de puertos en router)..." -ForegroundColor Green
    Write-Host "Tu URL pública segura HTTPS/WSS aparecerá en unos segundos:`n" -ForegroundColor Green
    cloudflared tunnel --url "http://localhost:$PuertoLocal"
}
elseif ($Metodo -eq "caddy") {
    Write-Host "`n[1/2] Verificando Caddy Web Server..." -ForegroundColor Yellow
    $caddy = Get-Command caddy -ErrorAction SilentlyContinue
    if (-not $caddy) {
        Write-Host "Instalando Caddy automáticamente mediante winget..." -ForegroundColor Cyan
        try {
            winget install --id CaddyServer.Caddy -e --silent --accept-source-agreements --accept-package-agreements
            Write-Host "✅ Caddy instalado exitosamente." -ForegroundColor Green
        } catch {
            Write-Host "❌ Instala Caddy manualmente desde: https://caddyserver.com/download" -ForegroundColor Red
            return
        }
    }

    $caddyfilePath = Join-Path $PSScriptRoot "caddy\Caddyfile"
    if (-not (Test-Path $caddyfilePath)) {
        $caddyfilePath = "deploy/caddy/Caddyfile"
    }

    Write-Host "`n[2/2] Iniciando Caddy Reverse Proxy con certificados SSL automáticos..." -ForegroundColor Green
    $env:NXDESK_DOMAIN = $Dominio
    caddy run --config $caddyfilePath
}
