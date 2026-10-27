namespace NxDesk.Application.DTOs;

/// <summary>
/// Types of input events that can be sent to a remote host.
/// </summary>
public enum InputEventType
{
    MouseMove,
    MouseDown,
    MouseUp,
    MouseWheel,
    KeyDown,
    KeyUp
}

/// <summary>
/// DTO representing contact-less input from a remote user.
/// </summary>
public record InputEventDto(
    InputEventType Type,
    double X = 0,
    double Y = 0,
    string? KeyCode = null,
    string? MouseButton = null, // "Left", "Right", "Middle"
    double DeltaX = 0,
    double DeltaY = 0
);
