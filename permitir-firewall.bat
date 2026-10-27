@echo off
:: Auto-elevacion con permisos de Administrador
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo Solicitando permisos de Administrador a Windows...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

title Desbloqueo y Permisos de Red para NxDesk
color 0A
echo ===================================================================
echo   ELIMINANDO BLOQUEOS Y HABILITANDO NXDESK EN FIREWALL WINDOWS
echo ===================================================================
echo.

:: 1. Eliminar cualquier regla previa de bloqueo para nxdesk
echo [*] Limpiando reglas de bloqueo previas...
netsh advfirewall firewall delete rule name="nxdesk-client.exe" >nul 2>&1
netsh advfirewall firewall delete rule name="nxdesk-server.exe" >nul 2>&1
netsh advfirewall firewall delete rule name="NxDesk Transmission 8080" >nul 2>&1
netsh advfirewall firewall delete rule name="NxDesk Signaling 5000" >nul 2>&1
netsh advfirewall firewall delete rule name="NxDesk Ports 8080-5000" >nul 2>&1
netsh advfirewall firewall delete rule name="NxDesk Client Debug" >nul 2>&1
netsh advfirewall firewall delete rule name="NxDesk Server Debug" >nul 2>&1

:: 2. Crear regla de apertura para los puertos 8080 (Streaming Web) y 5000 (Senales)
echo [*] Abriendo puertos TCP 8080 y 5000 para redes privadas y publicas...
netsh advfirewall firewall add rule name="NxDesk Ports 8080-5000" dir=in action=allow protocol=TCP localport=8080,5000 profile=any enable=yes >nul

:: 3. Crear reglas explicitas de permiso para los binarios de NxDesk
echo [*] Autorizando ejecutables de nxdesk-client y nxdesk-server...
netsh advfirewall firewall add rule name="NxDesk Client Debug" dir=in action=allow program="%~dp0nxdesk-rust\target\debug\nxdesk-client.exe" profile=any enable=yes >nul
netsh advfirewall firewall add rule name="NxDesk Server Debug" dir=in action=allow program="%~dp0nxdesk-rust\target\debug\nxdesk-server.exe" profile=any enable=yes >nul

echo.
echo ===================================================================
echo  EXITO: Bloqueos removidos y permisos aplicados en Windows Defender.
echo.
echo  Tu enlace para ver la pantalla desde otra PC es:
echo  --^> http://10.10.40.24:8080
echo ===================================================================
echo.
pause
