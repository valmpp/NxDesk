using System.Diagnostics;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Records incoming video frames into an MP4 file by piping JPEG frames to FFmpeg.
/// </summary>
public class ScreenRecordingService : IScreenRecordingService
{
    private Process? _ffmpegProcess;
    private Stream? _ffmpegStdin;
    private readonly object _lock = new();
    private string? _outputPath;
    private long _frameCount;
    private int _pushedFrameCount = 0;
    private bool _isRaw;

    public bool IsRecording { get; private set; }

    public Task StartAsync(string outputPath, int width, int height, int fps = 10, bool isRaw = false)
    {
        if (IsRecording) return Task.CompletedTask;

        _outputPath = outputPath;
        _frameCount = 0;
        _pushedFrameCount = 0;
        _isRaw = isRaw;

        // Ensure output directory exists
        var dir = Path.GetDirectoryName(outputPath);
        if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
        {
            Directory.CreateDirectory(dir);
            FileLogger.Log("Recording", $"Created directory: {dir}");
        }

        // FFmpeg command: read JPEG images OR raw BGRA from stdin → H.264 MP4
        // Using libopenh264 for wide compatibility.
        string args;
        if (_isRaw)
        {
            args = $"-y -f rawvideo -pixel_format bgra -video_size {width}x{height} -framerate {fps} -i pipe:0 " +
                   $"-c:v libopenh264 -pix_fmt yuv420p " +
                   $"-movflags +faststart \"{outputPath}\"";
        }
        else
        {
            args = $"-y -f mjpeg -framerate {fps} -i pipe:0 " +
                   $"-c:v libopenh264 -pix_fmt yuv420p " +
                   $"-movflags +faststart -vsync vfr \"{outputPath}\"";
        }

        FileLogger.Log("Recording", $"Starting FFmpeg ({(_isRaw ? "RAW" : "MJPEG")}): ffmpeg {args}");
        Console.WriteLine($"[Recording] Starting FFmpeg with args: {args}");

        var psi = new ProcessStartInfo
        {
            FileName = "ffmpeg",
            Arguments = args,
            UseShellExecute = false,
            RedirectStandardInput = true,
            RedirectStandardError = true,
            RedirectStandardOutput = true,
            CreateNoWindow = true
        };

        try
        {
            _ffmpegProcess = Process.Start(psi);
            if (_ffmpegProcess == null)
            {
                FileLogger.Log("Recording", "ERROR: Failed to start FFmpeg process.");
                return Task.CompletedTask;
            }

            _ffmpegStdin = _ffmpegProcess.StandardInput.BaseStream;
            IsRecording = true;

            // Log FFmpeg stderr in background for debugging
            _ = Task.Run(async () =>
            {
                try
                {
                    using var reader = _ffmpegProcess.StandardError;
                    string? line;
                    while ((line = await reader.ReadLineAsync()) != null)
                    {
                        FileLogger.Log("Recording/FFmpeg", line);
                    }
                }
                catch (Exception ex)
                {
                    FileLogger.Log("Recording/FFmpeg", $"Error reading stderr: {ex.Message}");
                }
            });

            Console.WriteLine($"[Recording] FFmpeg started (PID={_ffmpegProcess.Id}). Output: {outputPath}");
        }
        catch (Exception ex)
        {
            FileLogger.Log("Recording", $"ERROR starting FFmpeg: {ex.Message}");
            IsRecording = false;
        }

        return Task.CompletedTask;
    }

    public Task PushFrameAsync(VideoFrameDto frame)
    {
        if (!IsRecording || _ffmpegStdin == null) return Task.CompletedTask;

        try
        {
            byte[] jpegData;

            if (frame.IsCompressed && !_isRaw)
            {
                // Frame is already JPEG — use directly
                jpegData = frame.Pixels;
            }
            else if (!frame.IsCompressed && _isRaw)
            {
                // Frame is raw BGRA — use directly
                jpegData = frame.Pixels;
            }
            else
            {
                // Frame type mismatch
                if (_frameCount % 100 == 0)
                {
                    FileLogger.Log("Recording", $"WARNING: Frame type mismatch. IsCompressed={frame.IsCompressed}, ServiceIsRaw={_isRaw}. Skipping.");
                }
                return Task.CompletedTask;
            }

            lock (_lock)
            {
                if (_ffmpegStdin == null || !IsRecording) return Task.CompletedTask;
                _ffmpegStdin.Write(jpegData, 0, jpegData.Length);
                _ffmpegStdin.Flush();
                _pushedFrameCount++;
                
                if (_pushedFrameCount % 50 == 0)
                {
                    FileLogger.Log("Recording", $"Pushed {_pushedFrameCount} frames to FFmpeg.");
                }
            }

            _frameCount++;
            if (_frameCount % 30 == 0)
            {
                Console.WriteLine($"[Recording] Pushed {_frameCount} frames ({jpegData.Length} bytes last)");
            }
        }
        catch (IOException ex)
        {
            FileLogger.Log("Recording", $"ERROR writing frame (pipe broken?): {ex.Message}");
            IsRecording = false;
        }
        catch (Exception ex)
        {
            FileLogger.Log("Recording", $"ERROR writing frame: {ex.Message}");
        }

        return Task.CompletedTask;
    }

    public Task StopAsync()
    {
        if (!IsRecording) return Task.CompletedTask;

        IsRecording = false;
        Console.WriteLine($"[Recording] Stopping... ({_frameCount} frames total)");

        try
        {
            lock (_lock)
            {
                _ffmpegStdin?.Flush();
                _ffmpegStdin?.Close();
                _ffmpegStdin = null;
            }

            if (_ffmpegProcess != null && !_ffmpegProcess.HasExited)
            {
                // Give FFmpeg up to 15 seconds to finalize the MP4
                var exited = _ffmpegProcess.WaitForExit(15000);
                if (!exited)
                {
                    Console.WriteLine("[Recording] WARNING: FFmpeg did not exit in time, killing.");
                    _ffmpegProcess.Kill();
                }
                else
                {
                    FileLogger.Log("Recording", $"FFmpeg exited with code: {_ffmpegProcess.ExitCode}");
                }
            }

            // Check if file was created
            if (File.Exists(_outputPath))
            {
                var size = new FileInfo(_outputPath!).Length;
                FileLogger.Log("Recording", $"File saved: {_outputPath} ({size / 1024} KB)");
                Console.WriteLine($"[Recording] COMPLETED. File saved at: {_outputPath}");
            }
            else
            {
                FileLogger.Log("Recording", $"WARNING: Output file NOT found: {_outputPath}");
            }
        }
        catch (Exception ex)
        {
            FileLogger.Log("Recording", $"ERROR stopping: {ex.Message}");
        }
        finally
        {
            _ffmpegProcess?.Dispose();
            _ffmpegProcess = null;
            _pushedFrameCount = 0;
        }

        return Task.CompletedTask;
    }

    public void Dispose()
    {
        if (IsRecording)
        {
            StopAsync().GetAwaiter().GetResult();
        }
        _ffmpegProcess?.Dispose();
    }
}
