using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Screen capture service for Linux using FFmpeg.
/// Uses libX11 to detect resolution and ffmpeg for robust capture.
/// </summary>
public class FfmpegCaptureService : IScreenCaptureService
{
    private Process? _ffmpegProcess;
    private Stream? _ffmpegOutput;
    private int _width;
    private int _height;
    private byte[]? _frameBuffer;
    private int _frameCount = 0;
    private bool _initialized = false;
    private readonly CancellationTokenSource _cts = new();

    public int Width => _width;
    public int Height => _height;
    public bool IsInitialized => _initialized;

    [DllImport("libX11.so.6")]
    private static extern IntPtr XOpenDisplay(IntPtr display_name);

    [DllImport("libX11.so.6")]
    private static extern int XCloseDisplay(IntPtr display);

    [DllImport("libX11.so.6")]
    private static extern IntPtr XDefaultRootWindow(IntPtr display);

    [StructLayout(LayoutKind.Sequential)]
    private struct XWindowAttributes
    {
        public int x, y;
        public int width, height;
        public int border_width;
        public int depth;
        public IntPtr visual;
        public IntPtr root;
        public int @class;
        public int bit_gravity;
        public int win_gravity;
        public int backing_store;
        public uint backing_planes;
        public uint backing_pixel;
        public int save_under;
        public IntPtr colormap;
        public int map_installed;
        public int map_state;
        public long all_event_masks;
        public long your_event_mask;
        public long do_not_propagate_mask;
        public int override_redirect;
        public IntPtr screen;
    }

    [DllImport("libX11.so.6")]
    private static extern int XGetWindowAttributes(IntPtr display, IntPtr window, ref XWindowAttributes attributes);

    public bool Initialize(int x = 0, int y = 0, int width = 0, int height = 0, int monitorIndex = 0)
    {
        try
        {
            Dispose();

            // Detect resolution using X11 (works on XWayland too) for fallback
            IntPtr displayPtr = XOpenDisplay(IntPtr.Zero);
            if (displayPtr != IntPtr.Zero && width == 0)
            {
                IntPtr root = XDefaultRootWindow(displayPtr);
                XWindowAttributes attrs = new XWindowAttributes();
                XGetWindowAttributes(displayPtr, root, ref attrs);
                _width = attrs.width;
                _height = attrs.height;
                XCloseDisplay(displayPtr);
            }
            else
            {
                _width = width > 0 ? width : 1366;
                _height = height > 0 ? height : 768;
            }

            int origWidth = _width;
            int origHeight = _height;
            string display = Environment.GetEnvironmentVariable("DISPLAY") ?? ":0";

            // If a specific display portion is selected, adjust the input string:
            // ffmpeg -i :0.0+1920,0 ...
            string displayInput = $"{display}.0+{x},{y}";

            // Auto-scale if the resolution is too large for raw streaming
            // 2732x768 is ~8MB per frame. 1920 is a safe maximum width.
            string scaleFilter = "";
            if (_width > 1920)
            {
                int newWidth = 1280;
                int newHeight = (int)(_height * (newWidth / (double)_width));
                if (newHeight % 2 != 0) newHeight--; 
                scaleFilter = $",scale={newWidth}:{newHeight}";
                _width = newWidth;
                _height = newHeight;
                Console.WriteLine($"[FFmpeg] Downscaling from {origWidth} to {_width} for bandwidth stability");
            }

            // Optimization flags:
            // -probesize 32 -analyzeduration 0: Start instantly without probing
            // -thread_queue_size: Increase queue to prevent frame drops
            string args = $"-loglevel error -probesize 32 -thread_queue_size 512 -f x11grab -video_size {origWidth}x{origHeight} -i {displayInput} -f rawvideo -pix_fmt bgra -vf \"fps=15{scaleFilter}\" -";
            Console.WriteLine($"[FFmpeg] Running: ffmpeg {args}");

            var startInfo = new ProcessStartInfo
            {
                FileName = "ffmpeg",
                Arguments = args,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
                CreateNoWindow = true
            };

            _ffmpegProcess = new Process { StartInfo = startInfo };
            _ffmpegProcess.ErrorDataReceived += (s, e) => {
                if (!string.IsNullOrEmpty(e.Data))
                    Console.WriteLine($"[FFmpeg Error] {e.Data}");
            };

            if (!_ffmpegProcess.Start()) return false;
            
            _ffmpegProcess.BeginErrorReadLine();
            _ffmpegOutput = _ffmpegProcess.StandardOutput.BaseStream;
            _frameBuffer = new byte[_width * _height * 4];
            
            _initialized = true;
            return true;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[FFmpeg] Failed to start: {ex.Message}");
            return false;
        }
    }

    private int _pendingRead = 0;

    public VideoFrameDto? CaptureFrame(int timeoutMs = 100)
    {
        if (!_initialized || _ffmpegOutput == null || _frameBuffer == null) return null;

        try
        {
            // IMPORTANT: We must NOT read partially and throw away.
            // If we don't finish a frame, we must KEEP the bytes for the next call.
            
            int bytesToRead = _frameBuffer.Length;
            
            // Read as much as possible without blocking too long
            // FFmpeg stdout will buffer up to 64KB or more.
            
            var stopwatch = Stopwatch.StartNew();
            while (_pendingRead < bytesToRead && stopwatch.ElapsedMilliseconds < timeoutMs)
            {
                int read = _ffmpegOutput.Read(_frameBuffer, _pendingRead, bytesToRead - _pendingRead);
                if (read <= 0) break;
                _pendingRead += read;
            }

            if (_pendingRead < bytesToRead) 
            {
                return null; // Keep bytes in buffer and try again in next call
            }

            // Reset for next frame
            _pendingRead = 0;
            _frameCount++;

            // Detect if the frame is black (all zeros) for debugging Wayland issues
            bool isBlack = true;
            for (int i = 0; i < 1000 && i < _frameBuffer.Length; i += 4)
            {
                if (_frameBuffer[i] != 0 || _frameBuffer[i+1] != 0 || _frameBuffer[i+2] != 0)
                {
                    isBlack = false;
                    break;
                }
            }

            if (_frameCount % 50 == 0)
            {
                string status = isBlack ? "BLACK" : "OK";
                Console.WriteLine($"[FFmpeg] Heartbeat: Streaming frame {_frameCount} ({_width}x{_height}) - Status: {status}");
            }

            // Return a COPY for safety because the buffer is reused
            byte[] frameCopy = new byte[_frameBuffer.Length];
            Buffer.BlockCopy(_frameBuffer, 0, frameCopy, 0, _frameBuffer.Length);

            return new VideoFrameDto(frameCopy, _width, _height);
        }
        catch (Exception ex)
        {
            if (_frameCount < 5) Console.WriteLine($"[FFmpeg] Capture Loop Error: {ex.Message}");
            return null;
        }
    }

    public void Dispose()
    {
        _initialized = false;
        _cts.Cancel();
        try
        {
            if (_ffmpegProcess != null && !_ffmpegProcess.HasExited)
            {
                _ffmpegProcess.Kill();
                _ffmpegProcess.Dispose();
            }
        }
        catch { }
        _ffmpegProcess = null;
        _ffmpegOutput = null;
    }
}
