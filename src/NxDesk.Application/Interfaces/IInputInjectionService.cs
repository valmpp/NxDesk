using NxDesk.Application.DTOs;

namespace NxDesk.Application.Interfaces;

/// <summary>
/// Interface for injecting hardware input events (mouse, keyboard) into the Host OS.
/// </summary>
public interface IInputInjectionService
{
    /// <summary>
    /// Inject a mouse or keyboard event into the system.
    /// </summary>
    /// <param name="inputEvent">The event data to inject.</param>
    Task InjectEventAsync(InputEventDto inputEvent);

    /// <summary>
    /// Initialize the input injection system (e.g., connect to Portal or UInput).
    /// </summary>
    /// <returns>True if initialization successful.</returns>
    Task<bool> InitializeAsync();

    /// <summary>
    /// Set the physical screen bounds of the host to scale normalized coordinates correctly.
    /// </summary>
    void SetScreenBounds(int x, int y, int width, int height);
}
