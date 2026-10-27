using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Extensions.Logging;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Linux-specific implementation of input injection using Wayland Desktop Portals
/// (and potentially xdotool fallback if needed, but primarily focused on Wayland).
/// </summary>
public class LinuxInputInjectionService : IInputInjectionService, IDisposable
{
    private readonly ILogger<LinuxInputInjectionService>? _logger;
    private int _screenWidth = 1920;
    private int _screenHeight = 1080;
    private int _screenX = 0;
    private int _screenY = 0;

    public LinuxInputInjectionService(ILogger<LinuxInputInjectionService>? logger = null)
    {
        _logger = logger;
    }

    public Task<bool> InitializeAsync()
    {
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Linux))
            return Task.FromResult(false);

        // Readiness is now managed by GstreamerCaptureService which shares its portal session
        _logger?.LogInformation("LinuxInputInjectionService bound to unified portal helper.");
        return Task.FromResult(true);
    }

    public async Task InjectEventAsync(InputEventDto inputEvent)
    {
        var stdin = GstreamerCaptureService.PortalStdin;
        if (stdin == null) 
        {
            FileLogger.Log("Input", $"PortalStdin is NULL — cannot inject {inputEvent.Type}");
            return;
        }

        try
        {
            string command = inputEvent.Type switch
            {
                InputEventType.MouseMove => $"MOVE|{inputEvent.X:F6}|{inputEvent.Y:F6}",
                InputEventType.MouseDown => $"BUTTON|{MapButton(inputEvent.MouseButton)}|1",
                InputEventType.MouseUp => $"BUTTON|{MapButton(inputEvent.MouseButton)}|0",
                InputEventType.MouseWheel => $"SCROLL|{inputEvent.DeltaX:F1}|{inputEvent.DeltaY:F1}",
                InputEventType.KeyDown => $"KEY|{MapKey(inputEvent.KeyCode)}|1",
                InputEventType.KeyUp => $"KEY|{MapKey(inputEvent.KeyCode)}|0",
                _ => ""
            };

            if (!string.IsNullOrEmpty(command))
            {
                await stdin.WriteLineAsync(command);
                // Flush removed for performance; StreamWriter/Pipe handles this efficiently
            }
        }
        catch (Exception ex)
        {
            FileLogger.Log("Input", $"ERROR injecting {inputEvent.Type}: {ex.Message}");
        }
    }

    private int MapButton(string? button)
    {
        return button switch
        {
            "Left" => 272,
            "Right" => 273,
            "Middle" => 274,
            _ => 272
        };
    }

    private int MapKey(string? keyCode)
    {
        if (int.TryParse(keyCode, out int code)) return code;
        
        return keyCode?.ToUpperInvariant() switch
        {
            "ESCAPE" => 1,
            "D1" => 2, "D2" => 3, "D3" => 4, "D4" => 5, "D5" => 6,
            "D6" => 7, "D7" => 8, "D8" => 9, "D9" => 10, "D0" => 11,
            "OEMMINUS" => 12, "OEMPLUS" => 13,
            "BACK" => 14, "BACKSPACE" => 14,
            "TAB" => 15,
            "Q" => 16, "W" => 17, "E" => 18, "R" => 19, "T" => 20, "Y" => 21,
            "U" => 22, "I" => 23, "O" => 24, "P" => 25,
            "OEMOPENBRACKETS" => 26, "OEMCLOSEBRACKETS" => 27,
            "ENTER" => 28, "RETURN" => 28,
            "LEFTCTRL" => 29, "CTRL" => 29,
            "A" => 30, "S" => 31, "D" => 32, "F" => 33, "G" => 34,
            "H" => 35, "J" => 36, "K" => 37, "L" => 38,
            "OEMSEMICOLON" => 39, "OEMQUOTES" => 40, "OEMTILDE" => 41,
            "LEFTSHIFT" => 42, "SHIFT" => 42,
            "OEMBACKSLASH" => 43,
            "Z" => 44, "X" => 45, "C" => 46, "V" => 47, "B" => 48,
            "N" => 49, "M" => 50,
            "OEMCOMMA" => 51, "OEMPERIOD" => 52, "OEMQUESTION" => 53,
            "RIGHTSHIFT" => 54,
            "MULTIPLY" => 55,
            "LEFTALT" => 56, "ALT" => 56,
            "SPACE" => 57,
            "CAPSLOCK" => 58,
            "F1" => 59, "F2" => 60, "F3" => 61, "F4" => 62, "F5" => 63,
            "F6" => 64, "F7" => 65, "F8" => 66, "F9" => 67, "F10" => 68,
            "NUMLOCK" => 69, "SCROLL" => 70,
            "NUMPAD7" => 71, "NUMPAD8" => 72, "NUMPAD9" => 73, "SUBTRACT" => 74,
            "NUMPAD4" => 75, "NUMPAD5" => 76, "NUMPAD6" => 77, "ADD" => 78,
            "NUMPAD1" => 79, "NUMPAD2" => 80, "NUMPAD3" => 81, "NUMPAD0" => 82, "DECIMAL" => 83,
            "F11" => 87, "F12" => 88,
            "RIGHTCTRL" => 97,
            "DIVIDE" => 98,
            "RIGHTALT" => 100,
            "HOME" => 102, "UP" => 103, "PAGEUP" => 104,
            "LEFT" => 105, "RIGHT" => 106,
            "END" => 107, "DOWN" => 108, "PAGEDOWN" => 109,
            "INSERT" => 110, "DELETE" => 111,
            "LWIN" => 125, "RWIN" => 126,
            _ => 0
        };
    }

    public void Dispose()
    {
        // No longer owns the Python process.
    }

    public void SetScreenBounds(int x, int y, int width, int height)
    {
        _screenX = x;
        _screenY = y;
        _screenWidth = width;
        _screenHeight = height;
    }
}
