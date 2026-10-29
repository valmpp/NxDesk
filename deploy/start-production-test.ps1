# ==============================================================================
# NxDesk - Lanzador del Entorno de Pruebas de Produccion (Docker + Cloudflare / Ngrok)
# ==============================================================================
param(
    [ValidateSet("cloudflare", "ngrok")]
    [string]$Provider = "cloudflare",
    [string]$NgrokToken = ""
)

$ErrorActionPreference = "Continue"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  NxDesk Production Test Gateway (Docker WAN Tunnel)      " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Comprobar que Docker este activo
Write-Host "`n[1/4] Verificando Docker Engine..." -ForegroundColor Yellow
try {
    $null = docker ps
    Write-Host "  Docker daemon esta activo y operacional." -ForegroundColor Green
} catch {
    Write-Host "  Error: Docker no se encuentra en ejecucion. Abre Docker Desktop e intenta de nuevo." -ForegroundColor Red
    return
}

# 2. Configurar variables de entorno y directorio
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $projectRoot
$envPath = Join-Path $projectRoot ".env"
$envExamplePath = Join-Path $projectRoot ".env.example"

if (-not (Test-Path $envPath)) {
    if (Test-Path $envExamplePath) {
        Copy-Item $envExamplePath $envPath
        Write-Host "  Archivo .env generado a partir de .env.example." -ForegroundColor Green
    }
}

if ($Provider -eq "ngrok" -and -not [string]::IsNullOrWhiteSpace($NgrokToken)) {
    (Get-Content $envPath) -replace "NGROK_AUTHTOKEN=.*", "NGROK_AUTHTOKEN=$NgrokToken" | Set-Content $envPath
    Write-Host "  Token de Ngrok actualizado en .env." -ForegroundColor Green
}

# 3. Levantar contenedores segun el proveedor seleccionado
Write-Host "`n[2/4] Levantando contenedores con proveedor: $Provider..." -ForegroundColor Yellow

if ($Provider -eq "cloudflare") {
    Write-Host "  Modo Cloudflare Quick Tunnel (Cero token ni registro requerido)..." -ForegroundColor Cyan
    docker compose up -d --build nxdesk-server cloudflare
} else {
    Write-Host "  Modo Ngrok Tunnel..." -ForegroundColor Cyan
    docker compose --profile ngrok up -d --build nxdesk-server ngrok
}

Write-Host "  Contenedores iniciados en segundo plano." -ForegroundColor Green

# 4. Obtener URL publica WAN
Write-Host "`n[3/4] Esperando inicializacion del tunel publico WAN..." -ForegroundColor Yellow
$maxRetries = 20
$publicUrl = $null
$wssUrl = $null

if ($Provider -eq "cloudflare") {
    for ($i = 1; $i -le $maxRetries; $i++) {
        Start-Sleep -Seconds 2
        $rawLogs = cmd.exe /c "docker logs nxdesk-cloudflare-tunnel 2>&1"
        $logs = [string]($rawLogs -join "`n")
        if ($logs -match "https://[a-zA-Z0-9-]+\.trycloudflare\.com") {
            $publicUrl = $matches[0]
            $wssUrl = $publicUrl -replace "^https://", "wss://"
            $wssUrl = "$wssUrl/signalinghub"
            break
        }
        Write-Host "  Conectando con Cloudflare Edge... (intento $i/$maxRetries)" -ForegroundColor Gray
    }
} else {
    for ($i = 1; $i -le $maxRetries; $i++) {
        Start-Sleep -Seconds 2
        try {
            $ngrokResponse = Invoke-RestMethod -Uri "http://localhost:4040/api/tunnels" -TimeoutSec 2 -ErrorAction SilentlyContinue
            if ($ngrokResponse -and $ngrokResponse.tunnels -and $ngrokResponse.tunnels.Count -gt 0) {
                $httpsTunnel = $ngrokResponse.tunnels | Where-Object { $_.proto -eq "https" } | Select-Object -First 1
                if (-not $httpsTunnel) {
                    $httpsTunnel = $ngrokResponse.tunnels[0]
                }
                $publicUrl = $httpsTunnel.public_url
                $wssUrl = $publicUrl -replace "^https://", "wss://" -replace "^http://", "ws://"
                $wssUrl = "$wssUrl/signalinghub"
                break
            }
        } catch {}
        Write-Host "  Conectando con Ngrok API... (intento $i/$maxRetries)" -ForegroundColor Gray
    }
}

# 5. Actualizar la configuracion del cliente portatil para mayor comodidad
if ($wssUrl) {
    $portableConfigPath = Join-Path $projectRoot "dist\NxDesk-Portable-v1.0\config.json"
    if (Test-Path $portableConfigPath) {
        try {
            $cfg = Get-Content $portableConfigPath | ConvertFrom-Json
            $cfg.signaling_server_url = $wssUrl
            $cfg | ConvertTo-Json -Depth 4 | Set-Content $portableConfigPath
            # Re-empaquetar el ZIP con la URL preconfigurada
            Compress-Archive -Path (Join-Path $projectRoot "dist\NxDesk-Portable-v1.0\*") -DestinationPath (Join-Path $projectRoot "dist\NxDesk-Portable-v1.0-Windows-x64.zip") -Force
            Write-Host "`n[4/4] Paquete portatil ZIP actualizado con la URL publica pre-configurada." -ForegroundColor Green
        } catch {
            Write-Host "`n[4/4] Paquete portatil listo en dist\NxDesk-Portable-v1.0" -ForegroundColor Gray
        }
    }
}

Write-Host "`n==========================================================" -ForegroundColor Green
Write-Host "  ENTORNO DE PRUEBAS DE PRODUCCION OPERACIONAL            " -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green

if ($wssUrl) {
    Write-Host "`nURL PUBLICA DEL SERVIDOR DE SENALIZACION (WAN):" -ForegroundColor Cyan
    Write-Host "   $wssUrl" -ForegroundColor Yellow

    if ($Provider -eq "ngrok") {
        Write-Host "`nPANEL DE INSPECCION NGROK:" -ForegroundColor Cyan
        Write-Host "   http://localhost:4040" -ForegroundColor White
    }
} else {
    Write-Host "`nAdvertencia: El tunel aun no reporta URLs publicas." -ForegroundColor Yellow
    Write-Host "Revisa los logs con: docker compose logs" -ForegroundColor White
}

Write-Host "`nDISTRIBUCION DEL CLIENTE PORTATIL PARA TESTERS:" -ForegroundColor Cyan
Write-Host "   Carpeta:  dist/NxDesk-Portable-v1.0" -ForegroundColor White
Write-Host "   Archivo:  dist/NxDesk-Portable-v1.0-Windows-x64.zip (4.15 MB)" -ForegroundColor White

Write-Host "`nINSTRUCCIONES PARA LOS TESTERS EN OTRAS COMPUTADORAS:" -ForegroundColor Cyan
Write-Host "   1. Comparte el archivo ZIP con los usuarios de prueba." -ForegroundColor Gray
Write-Host "   2. El tester descomprime y hace doble clic en Iniciar-NxDesk.bat." -ForegroundColor Gray
Write-Host "   3. Ya viene conectado a la URL WAN de prueba: $wssUrl" -ForegroundColor Yellow
Write-Host "   4. Introduce el ID del anfitrion y pulsa Conectar." -ForegroundColor Gray

Write-Host "`nComandos de gestion:" -ForegroundColor Gray
Write-Host "  docker compose logs -f        -> Ver logs de senalizacion en vivo" -ForegroundColor Gray
Write-Host "  docker compose down           -> Detener el entorno de prueba" -ForegroundColor Gray
Write-Host "==========================================================`n" -ForegroundColor Green
