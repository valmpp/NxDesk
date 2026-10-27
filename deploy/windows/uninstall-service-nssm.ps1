#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Desinstala el servicio de Windows de NxDesk Signaling Server.
#>

$ServiceName = "NxDeskSignalingServer"

Write-Host "Deteniendo servicio $ServiceName..." -ForegroundColor Yellow
Stop-Service -Name $ServiceName -ErrorAction SilentlyContinue

$NssmCmd = Get-Command "nssm.exe" -ErrorAction SilentlyContinue
if ($NssmCmd) {
    & $NssmCmd.Source remove $ServiceName confirm
} else {
    sc.exe delete $ServiceName
}

Write-Host "✅ Servicio $ServiceName desinstalado correctamente." -ForegroundColor Green
