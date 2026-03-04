using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Screen capture service for Linux using GStreamer (PipeWire).
/// Native support for Wayland/GNOME portals.
/// </summary>
public class GstreamerCaptureService : IScreenCaptureService
{
    private static Process? _cachedPortalProcess;
    private static string? _cachedRestoreToken;
    private static readonly string _tokenFilePath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "nxdesk", "portal_token.txt");

    // Global accessor for RemoteDesktop input injection
    public static StreamWriter? PortalStdin => _cachedPortalProcess?.StandardInput;

    private Process? _gstProcess;
    private Stream? _gstOutput;
    private int _width = 1280;
    private int _height = 720;
    private int _frameCount = 0;
    private bool _initialized = false;
    private readonly byte[] _readBuffer = new byte[65536];
    private int _readBufferPos = 0;
    private int _readBufferSize = 0;

    public int Width => _width;
    public int Height => _height;
    public bool IsInitialized => _initialized;

    public bool Initialize(int x = 0, int y = 0, int width = 0, int height = 0, int monitorIndex = 0)
    {
        try
        {
            FileLogger.Log("GStreamer", $"Initialize(x={x}, y={y}, w={width}, h={height}, idx={monitorIndex})");
            StopGstreamer();

            string requestedSize = $"{width}x{height}";
            string? nodeId = null;

            // 1. Always start/check portal helper (no nodeId caching to avoid stale nodes)
            FileLogger.Log("GStreamer", $"Requesting fresh Portal session (target: {requestedSize})");
            if (!StartPortalHelper(requestedSize, out nodeId))
            {
                FileLogger.Log("GStreamer", "Portal negotiation failed.");
                return false;
            }

            if (string.IsNullOrEmpty(nodeId))
            {
                FileLogger.Log("GStreamer", $"No matching Node ID found for size {requestedSize}");
                return false;
            }

            // 3. Start GStreamer pipeline using the Node ID
            string pipeline = $"pipewiresrc path={nodeId} do-timestamp=true ! videorate ! " +
                             "video/x-raw,framerate=20/1 ! videoconvert ! videoscale ! " +
                             "video/x-raw,width=1280,height=720 ! videoconvert ! " +
                             "video/x-raw,format=I420 ! jpegenc quality=75 idct-method=ifast ! " +
                             "queue max-size-buffers=1 leaky=downstream ! fdsink fd=1 sync=false";

            Console.WriteLine($"[GStreamer] Final Pipeline: gst-launch-1.0 {pipeline}");

            var startInfo = new ProcessStartInfo
            {
                FileName = "gst-launch-1.0",
                Arguments = pipeline,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
                CreateNoWindow = true
            };

            _gstProcess = new Process { StartInfo = startInfo };
            _gstProcess.ErrorDataReceived += (s, e) => {
                if (!string.IsNullOrEmpty(e.Data))
                    FileLogger.Log("GStreamer", $"[GStreamer Log] {e.Data}");
            };

            if (!_gstProcess.Start()) 
            {
                Console.WriteLine("[GStreamer] ERROR: Failed to start gst-launch-1.0");
                return false;
            }
            
            _gstProcess.BeginErrorReadLine();
            _gstOutput = _gstProcess.StandardOutput.BaseStream;
            _initialized = true;
            _frameCount = 0;
            return true;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[GStreamer] CRITICAL ERROR in Initialize: {ex.Message}");
            return false;
        }
    }

    private bool StartPortalHelper(string targetSize, out string? matchingNodeId)
    {
        matchingNodeId = null;
        try
        {
            if (_cachedPortalProcess != null && !_cachedPortalProcess.HasExited)
            {
                _cachedPortalProcess.Kill();
            }

            string helperPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "portal_helper.py");
            if (!File.Exists(helperPath)) 
                helperPath = "/home/hackyou/Música/Projects/NxDesk/src/NxDesk.Infrastructure/Services/portal_helper.py";

            // Load restore token from file
            if (string.IsNullOrEmpty(_cachedRestoreToken) && File.Exists(_tokenFilePath))
            {
                _cachedRestoreToken = File.ReadAllText(_tokenFilePath).Trim();
            }

            string pythonArgs = "-u " + helperPath;
            if (!string.IsNullOrEmpty(_cachedRestoreToken))
                pythonArgs += $" --restore-token={_cachedRestoreToken}";

            FileLogger.Log("GStreamer", $"Executing Portal Helper: python3 {pythonArgs}");

            _cachedPortalProcess = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "python3",
                    Arguments = pythonArgs,
                    RedirectStandardInput = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    UseShellExecute = false,
                    CreateNoWindow = true
                }
            };

            // Force PYTHONPATH to include system packages to fix "ModuleNotFoundError: No module named 'gi'"
            // On Linux Fedora, these are usually in /usr/lib64/python3.X/site-packages
            string currentPythonPath = Environment.GetEnvironmentVariable("PYTHONPATH") ?? "";
            string systemPackages = "/usr/lib64/python3.14/site-packages:/usr/lib/python3.14/site-packages:/usr/local/lib/python3.14/site-packages";
            _cachedPortalProcess.StartInfo.EnvironmentVariables["PYTHONPATH"] = string.IsNullOrEmpty(currentPythonPath) 
                ? systemPackages 
                : $"{systemPackages}:{currentPythonPath}";

            FileLogger.Log("GStreamer", $"Portal Helper Environment: PYTHONPATH={_cachedPortalProcess.StartInfo.EnvironmentVariables["PYTHONPATH"]}");

            if (!_cachedPortalProcess.Start()) return false;
            
            _cachedPortalProcess.ErrorDataReceived += (s, e) => {
                if (!string.IsNullOrEmpty(e.Data))
                    FileLogger.Log("Portal", $"[Portal Helper Log] {e.Data}");
            };
            _cachedPortalProcess.BeginErrorReadLine(); 

            var stopwatch = Stopwatch.StartNew();
            var localNodeIds = new System.Collections.Generic.Dictionary<string, string>();
            string? newRestoreToken = null;

            // Wait up to 30 seconds
            while (stopwatch.ElapsedMilliseconds < 30000)
            {
                if (_cachedPortalProcess.HasExited) break;
                if (_cachedPortalProcess.StandardOutput.EndOfStream) break;

                string? line = _cachedPortalProcess.StandardOutput.ReadLine();
                if (line != null)
                {
                    if (line.Contains("NODE_ID:"))
                    {
                        var parts = line.Split(',');
                        string nodeId = parts[0].Split(':')[1].Trim();
                        
                        if (parts.Length > 1 && parts[1].Contains("SIZE:"))
                        {
                            string sizePart = parts[1].Split(':')[1].Trim();
                            localNodeIds[sizePart] = nodeId;
                            if (sizePart == targetSize) matchingNodeId = nodeId;
                        }
                        
                        // Fallback: if we haven't found a match by size yet, 
                        // take the first Node ID available (most common case for single-screen share)
                        if (matchingNodeId == null) matchingNodeId = nodeId;
                    }
                    if (line.Contains("RESTORE_TOKEN:"))
                    {
                        newRestoreToken = line.Split(':')[1].Trim();
                        _cachedRestoreToken = newRestoreToken;
                        SaveToken(_cachedRestoreToken);
                    }

                    if (line == "READY")
                    {
                        if (matchingNodeId != null) break;
                    }
                }
                Thread.Sleep(10);
            }

            return matchingNodeId != null;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Portal Error] failed to launch helper: {ex.Message}");
            return false;
        }
    }

    private void SaveToken(string token)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(_tokenFilePath)!);
            File.WriteAllText(_tokenFilePath, token);
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[GStreamer] Failed to save restore token: {ex.Message}");
        }
    }

    public VideoFrameDto? CaptureFrame(int timeoutMs = 100)
    {
        if (!_initialized || _gstOutput == null) return null;

        try
        {
            var stopwatch = Stopwatch.StartNew();
            
            // 1. Search for JPEG Start of Image (SOI): 0xFF 0xD8
            bool foundSoi = false;
            while (!foundSoi && stopwatch.ElapsedMilliseconds < timeoutMs)
            {
                int b = ReadByteWithBuffer();
                if (b == -1) break;
                if (b == 0xFF)
                {
                    int b2 = ReadByteWithBuffer();
                    if (b2 == 0xD8)
                    {
                        foundSoi = true;
                        break;
                    }
                }
            }

            if (!foundSoi) return null;

            // 2. Read until JPEG End of Image (EOI): 0xFF 0xD9
            using var ms = new MemoryStream();
            ms.WriteByte(0xFF);
            ms.WriteByte(0xD8);

            bool foundEoi = false;
            while (!foundEoi && stopwatch.ElapsedMilliseconds < timeoutMs + 500)
            {
                int b = ReadByteWithBuffer();
                if (b == -1) break;
                ms.WriteByte((byte)b);

                if (b == 0xFF)
                {
                    int b2 = ReadByteWithBuffer();
                    if (b2 == -1) break;
                    ms.WriteByte((byte)b2);
                    if (b2 == 0xD9)
                    {
                        foundEoi = true;
                        break;
                    }
                }
            }

            if (!foundEoi) return null;

            byte[] payload = ms.ToArray();
            _frameCount++;
            
            if (_frameCount % 100 == 0)
                Console.WriteLine($"[GStreamer] Captured Frame {_frameCount}");

            // Check if process is still alive
            if (_gstProcess == null || _gstProcess.HasExited)
            {
                _initialized = false;
                return null;
            }

            return new VideoFrameDto(payload, _width, _height, isCompressed: true);
        }
        catch (Exception ex)
        {
            if (_frameCount < 5) Console.WriteLine($"[GStreamer] Capture Loop Error: {ex.Message}");
            return null;
        }
    }

    private int ReadByteWithBuffer()
    {
        if (_gstOutput == null) return -1;

        if (_readBufferPos >= _readBufferSize)
        {
            try 
            {
                _readBufferSize = _gstOutput.Read(_readBuffer, 0, _readBuffer.Length);
                _readBufferPos = 0;
                if (_readBufferSize <= 0) return -1;
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[GStreamer] Error reading from stdout: {ex.Message}");
                return -1;
            }
        }

        return _readBuffer[_readBufferPos++];
    }

    private void StopGstreamer()
    {
        _initialized = false;
        try
        {
            if (_gstProcess != null && !_gstProcess.HasExited)
            {
                _gstProcess.Kill();
                _gstProcess.Dispose();
            }
        }
        catch { }
        _gstProcess = null;
        _gstOutput = null;
    }

    public void Dispose()
    {
        StopGstreamer();
        // Portal process is left alive in _cachedPortalProcess for reuse
        // It will be killed when the app closes or StartPortalHelper is called again
    }
}
