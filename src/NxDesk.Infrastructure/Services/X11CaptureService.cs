using System;
using System.Runtime.InteropServices;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Screen capture service for Linux using X11 (libX11).
/// Uses XGetImage to capture the root window.
/// </summary>
public class X11CaptureService : IScreenCaptureService
{
    private IntPtr _display = IntPtr.Zero;
    private IntPtr _rootWindow = IntPtr.Zero;
    private int _width;
    private int _height;
    private bool _initialized = false;
    private int _frameCount = 0;
    private byte[]? _frameBuffer;

    public int Width => _width;
    public int Height => _height;
    public bool IsInitialized => _initialized;

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

    [StructLayout(LayoutKind.Sequential)]
    private struct XImage
    {
        public int width, height;
        public int xoffset;
        public int format;
        public IntPtr data;
        public int byte_order;
        public int bitmap_unit;
        public int bitmap_bit_order;
        public int bitmap_pad;
        public int depth;
        public int bytes_per_line;
        public int bits_per_pixel;
        public uint red_mask;
        public uint green_mask;
        public uint blue_mask;
        public IntPtr obdata;
        // Other fields omitted for simplicity as we use P/Invoke helpers
    }

    [DllImport("libX11.so.6")]
    private static extern IntPtr XOpenDisplay(IntPtr display_name);

    [DllImport("libX11.so.6")]
    private static extern int XCloseDisplay(IntPtr display);

    [DllImport("libX11.so.6")]
    private static extern IntPtr XDefaultRootWindow(IntPtr display);

    [DllImport("libX11.so.6")]
    private static extern int XGetWindowAttributes(IntPtr display, IntPtr window, ref XWindowAttributes attributes);

    [DllImport("libX11.so.6")]
    private static extern IntPtr XGetImage(IntPtr display, IntPtr drawable, int x, int y, uint width, uint height, uint plane_mask, int format);

    [DllImport("libX11.so.6")]
    private static extern int XDestroyImage(IntPtr image);

    private const int ZPixmap = 2;
    private bool _isWayland = false;

    private int _xOffset;
    private int _yOffset;

    public bool Initialize(int x = 0, int y = 0, int width = 0, int height = 0, int monitorIndex = 0)
    {
        try
        {
            Dispose();

            // Detect Wayland
            var sessionType = Environment.GetEnvironmentVariable("XDG_SESSION_TYPE");
            _isWayland = sessionType?.ToLower() == "wayland";
            
            _display = XOpenDisplay(IntPtr.Zero);
            if (_display == IntPtr.Zero)
            {
                return false;
            }

            _rootWindow = XDefaultRootWindow(_display);
            XWindowAttributes attrs = new XWindowAttributes();
            XGetWindowAttributes(_display, _rootWindow, ref attrs);

            // If specific bounds are provided, use them. Otherwise, default to full screen.
            _xOffset = x;
            _yOffset = y;
            _width = width > 0 ? width : attrs.width;
            _height = height > 0 ? height : attrs.height;

            // Failsafe: limit capture area if it's too large for XGetImage on some drivers
            if (_width > 2560) _width = 1920; 
            if (_height > 1440) _height = 1080;

            if (_width == 0 || _height == 0) return false;

            // Ensure even dimensions
            if (_width % 2 != 0) _width--;
            if (_height % 2 != 0) _height--;

            _frameBuffer = new byte[_width * _height * 4];
            _initialized = true;

            Console.WriteLine($"[X11] Initialized: {_width}x{_height} at ({_xOffset},{_yOffset}) (Wayland={_isWayland})");
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    public VideoFrameDto? CaptureFrame(int timeoutMs = 100)
    {
        if (!_initialized || _display == IntPtr.Zero) return null;

        IntPtr imagePtr = IntPtr.Zero;
        try
        {
            // Try with 32-bit plane mask 0xFFFFFFFF (AllPlanes)
            imagePtr = XGetImage(_display, _rootWindow, _xOffset, _yOffset, (uint)_width, (uint)_height, 0xFFFFFFFF, ZPixmap);

            if (imagePtr == IntPtr.Zero) 
            {
                if (_frameCount < 1) Console.WriteLine("[X11] XGetImage failed. Use FFmpeg fallback.");
                return null;
            }

            // XImage structure offsets for 64-bit Linux:
            // 0: width (4), 4: height (4), 8: xoffset (4), 12: format (4)
            // 16: data (8) <- IntPtr
            // 24: byte_order (4), 28: bitmap_unit (4), 32: bitmap_bit_order (4), 36: bitmap_pad (4)
            // 40: depth (4), 44: bytes_per_line (4), 48: bits_per_pixel (4)
            
            IntPtr dataPtr = Marshal.ReadIntPtr(imagePtr, 16);
            int bytesPerLine = Marshal.ReadInt32(imagePtr, 44);
            int bpp = Marshal.ReadInt32(imagePtr, 48);
            int depth = Marshal.ReadInt32(imagePtr, 40);

            if (_frameCount == 0)
            {
                Console.WriteLine($"[X11] Capture Debug: Depth={depth}, BPP={bpp}, Stride={bytesPerLine}, DataPtr={dataPtr}");
            }
            _frameCount++;

            // X11 usually returns BGRA32 on most modern systems
            if (bpp != 32)
            {
                // Fallback or warning if not 32bpp
                System.Diagnostics.Debug.WriteLine($"[X11] Warning: Unexpected BPP: {bpp}");
            }

            // Copy pixels to our buffer
            // Ensure we don't exceed buffer limits
            int rowSize = Math.Min(bytesPerLine, _width * 4);
            for (int y = 0; y < _height; y++)
            {
                Marshal.Copy(dataPtr + (y * bytesPerLine), _frameBuffer!, y * _width * 4, rowSize);
            }

            return new VideoFrameDto(_frameBuffer!, _width, _height);
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[X11] Capture error: {ex.Message}");
            return null;
        }
        finally
        {
            if (imagePtr != IntPtr.Zero)
            {
                XDestroyImage(imagePtr);
            }
        }
    }

    public void Dispose()
    {
        _initialized = false;
        if (_display != IntPtr.Zero)
        {
            XCloseDisplay(_display);
            _display = IntPtr.Zero;
        }
    }
}
