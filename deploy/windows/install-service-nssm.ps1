#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Instala y configura el servidor de señalización de NxDesk como Servicio de Windows nativo con NSSM.
.DESCRIPTION
    Configura inicio automático con Windows, reinicio ante fallos (< 5 seg),
    redirección de logs y monitoreo continuo del proceso.
.PARAMETER BinaryPath
    Ruta absoluta al binario ejecutable nxdesk-server.exe.
.PARAMETER Port
    Puerto TCP de escucha para señalización (por defecto: 5000).
#>

param(
    [string]$BinaryPath = "$PSScriptRoot\..\..\nxdesk-rust\target\release\nxdesk-server.exe",
    [int]$Port = 5000
)

$ServiceName = "NxDeskSignalingServer"
$DisplayName = "NxDesk Signaling Server (Rust)"
$Description = "Servidor central asíncrono de señalización y salas multi-nodo de NxDesk"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  🛡️  Instalador de Servicio Windows - NxDesk Signaling   " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Validar que el binario exista
$ResolvedBinary = Resolve-Path $BinaryPath -ErrorAction SilentlyContinue
if (-not $ResolvedBinary -or -not (Test-Path $ResolvedBinary)) {
    Write-Host "❌ Error: No se encontró el binario en: $BinaryPath" -ForegroundColor Red
    Write-Host "Por favor compila el proyecto en Release primero: 'cargo build --release -p nxdesk-server'" -ForegroundColor Yellow
    exit 1
}

$BinaryDir = Split-Path -Parent $ResolvedBinary
$LogsDir = Join-Path $BinaryDir "logs"
if (-not (Test-Path $LogsDir)) {
    New-Item -ItemType Directory -Path $LogsDir -Force | Out-Null
}

$StdoutLog = Join-Path $LogsDir "service-stdout.log"
$StderrLog = Join-Path $LogsDir "service-stderr.log"

# 2. Localizar o advertir sobre NSSM (Non-Sucking Service Manager)
$NssmCmd = Get-Command "nssm.exe" -ErrorAction SilentlyContinue
if (-not $NssmCmd) {
    Write-Host "⚠️  nssm.exe no está en el PATH." -ForegroundColor Yellow
    Write-Host "Instalando NSSM vía winget o descargando binario..." -ForegroundColor Yellow
    try {
        winget install -e --id OliverKowalke.NSSM --accept-package-agreements --accept-source-agreements
        $NssmPath = (Get-Command "nssm.exe" -ErrorAction SilentlyContinue).Source
    } catch {
        $NssmPath = $null
    }

    if (-not $NssmPath) {
        Write-Host "❌ No se pudo localizar NSSM automáticamente." -ForegroundColor Red
        Write-Host "Descárgalo de https://nssm.cc/download y colócalo en C:\Windows\System32 o en el PATH." -ForegroundColor Yellow
        exit 1
    }
} else {
    $NssmPath = $NssmCmd.Source
}

Write-Host "✅ Utilizando NSSM desde: $NssmPath" -ForegroundColor Green

# 3. Detener y remover servicio previo si existía
$ExistingService = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($ExistingService) {
    Write-Host "Deteniendo servicio previo..." -ForegroundColor Yellow
    & $NssmPath stop $ServiceName | Out-Null
    Start-Sleep -Seconds 2
    & $NssmPath remove $ServiceName confirm | Out-Null
    Write-Host "Servicio previo removido." -ForegroundColor Yellow
}

# 4. Crear e Instalar el Servicio con NSSM
Write-Host "Instalando servicio '$ServiceName'..." -ForegroundColor Cyan
& $NssmPath install $ServiceName "$ResolvedBinary" "--bind 0.0.0.0:$Port"
& $NssmPath set $ServiceName AppDirectory "$BinaryDir"
& $NssmPath set $ServiceName DisplayName "$DisplayName"
& $NssmPath set $ServiceName Description "$Description"
& $NssmPath set $ServiceName Start SERVICE_AUTO_START

# 5. Configurar Reinicio Automático y Resiliencia
& $NssmPath set $ServiceName AppRestartDelay 5000
& $NssmPath set $ServiceName AppExit Default Restart

# 6. Configurar Logs y Rotación de Archivos
& $NssmPath set $ServiceName AppStdout "$StdoutLog"
& $NssmPath set $ServiceName AppStderr "$StderrLog"
& $NssmPath set $ServiceName AppRotateFiles 1
& $NssmPath set $ServiceName AppRotateOnline 1
& $NssmPath set $ServiceName AppRotateSeconds 86400
& $NssmPath set $ServiceName AppRotateBytes 52428800 # 50 MB

# 7. Iniciar el Servicio
Write-Host "Iniciando servicio..." -ForegroundColor Cyan
& $NssmPath start $ServiceName
Start-Sleep -Seconds 2

# 8. Verificar Estado
$Status = Get-Service -Name $ServiceName
if ($Status.Status -eq "Running") {
    Write-Host "==========================================================" -ForegroundColor Green
    Write-Host "  🎉 SERVICIO INSTALADO Y EJECUTÁNDOSE CON ÉXITO 🟢       " -ForegroundColor Green
    Write-Host "  Nombre:       $ServiceName" -ForegroundColor White
    Write-Host "  Estado:       $($Status.Status)" -ForegroundColor White
    Write-Host "  Puerto:       0.0.0.0:$Port" -ForegroundColor White
    Write-Host "  Logs:         $LogsDir" -ForegroundColor White
    Write-Host "==========================================================" -ForegroundColor Green
} else {
    Write-Host "⚠️ El servicio se instaló pero su estado es: $($Status.Status)" -ForegroundColor Red
    Write-Host "Revisa el log en: $StderrLog" -ForegroundColor Yellow
}
