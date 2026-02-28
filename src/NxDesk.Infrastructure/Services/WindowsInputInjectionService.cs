using System.Runtime.InteropServices;
using Microsoft.Extensions.Logging;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Windows-specific implementation of input injection using the Win32 SendInput API.
/// Handles mouse (move, click, scroll) and keyboard (key press/release) events.
/// </summary>
public class WindowsInputInjectionService : IInputInjectionService
{
    private readonly ILogger<WindowsInputInjectionService>? _logger;
    private int _screenWidth = 1920;
    private int _screenHeight = 1080;
    private int _screenX = 0;
    private int _screenY = 0;

    // Win32 virtual screen dimensions (used to normalize absolute mouse coordinates)
    private int _virtualScreenWidth;
    private int _virtualScreenHeight;

    public WindowsInputInjectionService(ILogger<WindowsInputInjectionService>? logger = null)
    {
        _logger = logger;
    }

    public Task<bool> InitializeAsync()
    {
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            return Task.FromResult(false);

        _virtualScreenWidth = GetSystemMetrics(SM_CXSCREEN);
        _virtualScreenHeight = GetSystemMetrics(SM_CYSCREEN);

        if (_virtualScreenWidth == 0 || _virtualScreenHeight == 0)
        {
            _virtualScreenWidth = 1920;
            _virtualScreenHeight = 1080;
        }

        _logger?.LogInformation(
            "WindowsInputInjectionService initialized. Virtual screen: {W}x{H}",
            _virtualScreenWidth, _virtualScreenHeight);

        return Task.FromResult(true);
    }

    public Task InjectEventAsync(InputEventDto inputEvent)
    {
        try
        {
            switch (inputEvent.Type)
            {
                case InputEventType.MouseMove:
                    InjectMouseMove(inputEvent.X, inputEvent.Y);
                    break;
                case InputEventType.MouseDown:
                    InjectMouseButton(inputEvent.MouseButton, isDown: true);
                    break;
                case InputEventType.MouseUp:
                    InjectMouseButton(inputEvent.MouseButton, isDown: false);
                    break;
                case InputEventType.MouseWheel:
                    InjectMouseWheel(inputEvent.DeltaX, inputEvent.DeltaY);
                    break;
                case InputEventType.KeyDown:
                    InjectKey(inputEvent.KeyCode, isDown: true);
                    break;
                case InputEventType.KeyUp:
                    InjectKey(inputEvent.KeyCode, isDown: false);
                    break;
            }
        }
        catch (Exception ex)
        {
            _logger?.LogError(ex, "Error injecting input event: {Type}", inputEvent.Type);
        }

        return Task.CompletedTask;
    }

    public void SetScreenBounds(int x, int y, int width, int height)
    {
        _screenX = x;
        _screenY = y;
        _screenWidth = width;
        _screenHeight = height;
    }

    // ── Mouse Move ──
    private void InjectMouseMove(double normalizedX, double normalizedY)
    {
        // Convert normalized (0..1) coordinates to absolute screen pixels
        int absoluteX = _screenX + (int)(normalizedX * _screenWidth);
        int absoluteY = _screenY + (int)(normalizedY * _screenHeight);

        // SendInput requires coordinates in [0, 65535] range mapped to the virtual screen
        int mappedX = (int)((absoluteX * 65535.0) / _virtualScreenWidth);
        int mappedY = (int)((absoluteY * 65535.0) / _virtualScreenHeight);

        var input = new INPUT
        {
            type = INPUT_MOUSE,
            u = new InputUnion
            {
                mi = new MOUSEINPUT
                {
                    dx = mappedX,
                    dy = mappedY,
                    dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE
                }
            }
        };
        SendInput(1, new[] { input }, Marshal.SizeOf<INPUT>());
    }

    // ── Mouse Button ──
    private void InjectMouseButton(string? button, bool isDown)
    {
        uint flags = button switch
        {
            "Right" => isDown ? MOUSEEVENTF_RIGHTDOWN : MOUSEEVENTF_RIGHTUP,
            "Middle" => isDown ? MOUSEEVENTF_MIDDLEDOWN : MOUSEEVENTF_MIDDLEUP,
            _ => isDown ? MOUSEEVENTF_LEFTDOWN : MOUSEEVENTF_LEFTUP  // Default = Left
        };

        var input = new INPUT
        {
            type = INPUT_MOUSE,
            u = new InputUnion { mi = new MOUSEINPUT { dwFlags = flags } }
        };
        SendInput(1, new[] { input }, Marshal.SizeOf<INPUT>());
    }

    // ── Mouse Wheel ──
    private void InjectMouseWheel(double deltaX, double deltaY)
    {
        // Vertical scroll
        if (deltaY != 0)
        {
            var input = new INPUT
            {
                type = INPUT_MOUSE,
                u = new InputUnion
                {
                    mi = new MOUSEINPUT
                    {
                        dwFlags = MOUSEEVENTF_WHEEL,
                        mouseData = (int)(deltaY * WHEEL_DELTA)
                    }
                }
            };
            SendInput(1, new[] { input }, Marshal.SizeOf<INPUT>());
        }

        // Horizontal scroll
        if (deltaX != 0)
        {
            var input = new INPUT
            {
                type = INPUT_MOUSE,
                u = new InputUnion
                {
                    mi = new MOUSEINPUT
                    {
                        dwFlags = MOUSEEVENTF_HWHEEL,
                        mouseData = (int)(deltaX * WHEEL_DELTA)
                    }
                }
            };
            SendInput(1, new[] { input }, Marshal.SizeOf<INPUT>());
        }
    }

    // ── Keyboard ──
    private void InjectKey(string? keyCode, bool isDown)
    {
        ushort vk = MapKeyToVirtualKey(keyCode);
        if (vk == 0) return;

        ushort scanCode = (ushort)MapVirtualKey(vk, MAPVK_VK_TO_VSC);

        var input = new INPUT
        {
            type = INPUT_KEYBOARD,
            u = new InputUnion
            {
                ki = new KEYBDINPUT
                {
                    wVk = vk,
                    wScan = scanCode,
                    dwFlags = isDown ? 0u : KEYEVENTF_KEYUP
                }
            }
        };
        SendInput(1, new[] { input }, Marshal.SizeOf<INPUT>());
    }

    // ── Key Mapping: Avalonia Key Names → Win32 Virtual Key Codes ──
    private static ushort MapKeyToVirtualKey(string? keyCode)
    {
        if (string.IsNullOrEmpty(keyCode)) return 0;
        if (ushort.TryParse(keyCode, out ushort numeric)) return numeric;

        return keyCode.ToUpperInvariant() switch
        {
            // Letters
            "A" => 0x41, "B" => 0x42, "C" => 0x43, "D" => 0x44, "E" => 0x45,
            "F" => 0x46, "G" => 0x47, "H" => 0x48, "I" => 0x49, "J" => 0x4A,
            "K" => 0x4B, "L" => 0x4C, "M" => 0x4D, "N" => 0x4E, "O" => 0x4F,
            "P" => 0x50, "Q" => 0x51, "R" => 0x52, "S" => 0x53, "T" => 0x54,
            "U" => 0x55, "V" => 0x56, "W" => 0x57, "X" => 0x58, "Y" => 0x59,
            "Z" => 0x5A,

            // Numbers
            "D0" => 0x30, "D1" => 0x31, "D2" => 0x32, "D3" => 0x33, "D4" => 0x34,
            "D5" => 0x35, "D6" => 0x36, "D7" => 0x37, "D8" => 0x38, "D9" => 0x39,

            // Function keys
            "F1" => 0x70, "F2" => 0x71, "F3" => 0x72, "F4" => 0x73, "F5" => 0x74,
            "F6" => 0x75, "F7" => 0x76, "F8" => 0x77, "F9" => 0x78, "F10" => 0x79,
            "F11" => 0x7A, "F12" => 0x7B,

            // Modifiers
            "LEFTSHIFT" or "SHIFT" => 0xA0,
            "RIGHTSHIFT" => 0xA1,
            "LEFTCTRL" or "CTRL" => 0xA2,
            "RIGHTCTRL" => 0xA3,
            "LEFTALT" or "ALT" => 0xA4,
            "RIGHTALT" => 0xA5,
            "LWIN" => 0x5B,
            "RWIN" => 0x5C,

            // Navigation
            "ESCAPE" => 0x1B,
            "TAB" => 0x09,
            "CAPSLOCK" => 0x14,
            "SPACE" => 0x20,
            "ENTER" or "RETURN" => 0x0D,
            "BACK" or "BACKSPACE" => 0x08,
            "DELETE" => 0x2E,
            "INSERT" => 0x2D,
            "HOME" => 0x24,
            "END" => 0x23,
            "PAGEUP" => 0x21,
            "PAGEDOWN" => 0x22,
            "UP" => 0x26,
            "DOWN" => 0x28,
            "LEFT" => 0x25,
            "RIGHT" => 0x27,

            // Numpad
            "NUMPAD0" => 0x60, "NUMPAD1" => 0x61, "NUMPAD2" => 0x62,
            "NUMPAD3" => 0x63, "NUMPAD4" => 0x64, "NUMPAD5" => 0x65,
            "NUMPAD6" => 0x66, "NUMPAD7" => 0x67, "NUMPAD8" => 0x68,
            "NUMPAD9" => 0x69,
            "MULTIPLY" => 0x6A,
            "ADD" => 0x6B,
            "SUBTRACT" => 0x6D,
            "DECIMAL" => 0x6E,
            "DIVIDE" => 0x6F,
            "NUMLOCK" => 0x90,

            // OEM & misc
            "OEMSEMICOLON" => 0xBA,
            "OEMPLUS" => 0xBB,
            "OEMCOMMA" => 0xBC,
            "OEMMINUS" => 0xBD,
            "OEMPERIOD" => 0xBE,
            "OEMQUESTION" => 0xBF,
            "OEMTILDE" => 0xC0,
            "OEMOPENBRACKETS" => 0xDB,
            "OEMBACKSLASH" => 0xDC,
            "OEMCLOSEBRACKETS" => 0xDD,
            "OEMQUOTES" => 0xDE,
            "SCROLL" => 0x91,

            _ => 0
        };
    }

    // ════════════════════════════════════════════
    //  Win32 P/Invoke declarations
    // ════════════════════════════════════════════

    private const int INPUT_MOUSE = 0;
    private const int INPUT_KEYBOARD = 1;

    // Mouse event flags
    private const uint MOUSEEVENTF_MOVE       = 0x0001;
    private const uint MOUSEEVENTF_LEFTDOWN   = 0x0002;
    private const uint MOUSEEVENTF_LEFTUP     = 0x0004;
    private const uint MOUSEEVENTF_RIGHTDOWN  = 0x0008;
    private const uint MOUSEEVENTF_RIGHTUP    = 0x0010;
    private const uint MOUSEEVENTF_MIDDLEDOWN = 0x0020;
    private const uint MOUSEEVENTF_MIDDLEUP   = 0x0040;
    private const uint MOUSEEVENTF_WHEEL      = 0x0800;
    private const uint MOUSEEVENTF_HWHEEL     = 0x1000;
    private const uint MOUSEEVENTF_ABSOLUTE   = 0x8000;
    private const int  WHEEL_DELTA            = 120;

    // Keyboard event flags
    private const uint KEYEVENTF_KEYUP = 0x0002;

    // GetSystemMetrics constants
    private const int SM_CXSCREEN = 0;
    private const int SM_CYSCREEN = 1;

    // MapVirtualKey mapping types
    private const uint MAPVK_VK_TO_VSC = 0;

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int nIndex);

    [DllImport("user32.dll")]
    private static extern uint MapVirtualKey(uint uCode, uint uMapType);

    // ── Structs for SendInput ──

    [StructLayout(LayoutKind.Sequential)]
    private struct INPUT
    {
        public int type;
        public InputUnion u;
    }

    [StructLayout(LayoutKind.Explicit)]
    private struct InputUnion
    {
        [FieldOffset(0)] public MOUSEINPUT mi;
        [FieldOffset(0)] public KEYBDINPUT ki;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MOUSEINPUT
    {
        public int dx;
        public int dy;
        public int mouseData;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct KEYBDINPUT
    {
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }
}
