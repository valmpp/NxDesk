using System.Diagnostics;
using System.Runtime.InteropServices;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Cross-platform implementation of IRemoteTerminalService.
/// Spawns a local shell process (bash on Linux, cmd on Windows)
/// and relays stdin/stdout/stderr via events.
/// </summary>
public class RemoteTerminalService : IRemoteTerminalService
{
    private Process? _shellProcess;
    private bool _disposed;

    public Action<string>? OnOutputReceived { get; set; }
    public bool IsRunning => _shellProcess != null && !_shellProcess.HasExited;

    public Task StartAsync()
    {
        if (IsRunning)
        {
            FileLogger.Log("Terminal", "StartAsync called but shell is already running.");
            return Task.CompletedTask;
        }

        // Reset disposed flag so the singleton can be reused after Disconnect
        _disposed = false;

        var startInfo = new ProcessStartInfo
        {
            FileName = RuntimeInformation.IsOSPlatform(OSPlatform.Windows)
                ? "cmd.exe"
                : (File.Exists("/usr/bin/script") ? "/usr/bin/script" : "/bin/bash"),
            Arguments = RuntimeInformation.IsOSPlatform(OSPlatform.Windows)
                ? ""
                : (File.Exists("/usr/bin/script") ? "-q /dev/null /bin/bash -i" : "-i"), 
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            Environment =
            {
                ["TERM"] = "xterm-256color",
                ["LANG"] = "en_US.UTF-8"
            }
        };

        _shellProcess = new Process { StartInfo = startInfo, EnableRaisingEvents = true };

        _shellProcess.Exited += (_, _) =>
        {
            FileLogger.Log("Terminal", "Shell process exited.");
            OnOutputReceived?.Invoke("[Terminal] El proceso ha terminado.\n");
        };

        try
        {
            _shellProcess.Start();
        }
        catch (Exception ex)
        {
            FileLogger.Log("Terminal", $"FAILED to start shell: {ex.Message}");
            _shellProcess = null;
            return Task.CompletedTask;
        }

        // Read stdout async
        _ = Task.Run(async () =>
        {
            try
            {
                var buffer = new char[1024];
                while (_shellProcess != null && !_shellProcess.HasExited)
                {
                    int count = await _shellProcess.StandardOutput.ReadAsync(buffer, 0, buffer.Length);
                    if (count > 0)
                    {
                        var data = new string(buffer, 0, count);
                        FileLogger.Log("Terminal", $"stdout READ: {count} chars");
                        OnOutputReceived?.Invoke(data);
                    }
                }
            }
            catch (Exception ex)
            {
                FileLogger.Log("Terminal", $"stdout read loop error: {ex.Message}");
            }
        });

        // Read stderr async
        _ = Task.Run(async () =>
        {
            try
            {
                var buffer = new char[1024];
                while (_shellProcess != null && !_shellProcess.HasExited)
                {
                    int count = await _shellProcess.StandardError.ReadAsync(buffer, 0, buffer.Length);
                    if (count > 0)
                    {
                        var data = new string(buffer, 0, count);
                        FileLogger.Log("Terminal", $"stderr READ: {count} chars");
                        OnOutputReceived?.Invoke(data);
                    }
                }
            }
            catch (Exception ex)
            {
                FileLogger.Log("Terminal", $"stderr read loop error: {ex.Message}");
            }
        });

        FileLogger.Log("Terminal", $"Shell started: {startInfo.FileName}, PID: {_shellProcess.Id}");
        return Task.CompletedTask;
    }

    public Task WriteInputAsync(string input)
    {
        if (!IsRunning || _shellProcess?.StandardInput == null)
        {
            FileLogger.Log("Terminal", $"WriteInputAsync DROPPED: IsRunning={IsRunning}, HasStdin={_shellProcess?.StandardInput != null}, Input='{input}'");
            return Task.CompletedTask;
        }

        try
        {
            FileLogger.Log("Terminal", $"WriteInputAsync: writing '{input}'");
            
            // Explicitly handle line endings. 
            // If the Shell is cmd.exe (Windows), it MUST have \r\n.
            // Even if the client is on Linux, if the Host is Windows, we need \r\n.
            string lineEnding = RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "\r\n" : "\n";
            
            string toWrite = input;
            if (!toWrite.EndsWith(lineEnding))
            {
                toWrite += lineEnding;
            }

            _shellProcess.StandardInput.Write(toWrite);
            _shellProcess.StandardInput.Flush();
        }
        catch (Exception ex)
        {
            FileLogger.Log("Terminal", $"WriteInputAsync error: {ex.Message}");
        }

        return Task.CompletedTask;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        try
        {
            if (_shellProcess != null && !_shellProcess.HasExited)
            {
                _shellProcess.Kill(entireProcessTree: true);
                FileLogger.Log("Terminal", "Shell process killed.");
            }
            _shellProcess?.Dispose();
            _shellProcess = null;
        }
        catch (Exception ex)
        {
            FileLogger.Log("Terminal", $"Dispose error: {ex.Message}");
        }
    }
}
