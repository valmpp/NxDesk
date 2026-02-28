using NxDesk.Application.DTOs;

namespace NxDesk.Application.Interfaces;

/// <summary>
/// Service for capturing screen content.
/// Platform-specific implementations handle the actual capture mechanism.
/// </summary>
public interface IScreenCaptureService : IDisposable
{
    /// <summary>
    /// Initialize the capture service for a specific monitor region
    /// </summary>
    /// <param name="x">X offset of the monitor</param>
    /// <param name="y">Y offset of the monitor</param>
    /// <param name="width">Width of the monitor</param>
    /// <param name="height">Height of the monitor</param>
    /// <param name="monitorIndex">Monitor index (0 = primary)</param>
    /// <returns>True if initialization successful</returns>
    bool Initialize(int x = 0, int y = 0, int width = 0, int height = 0, int monitorIndex = 0);

    /// <summary>
    /// Capture a single frame
    /// </summary>
    /// <param name="timeoutMs">Timeout in milliseconds</param>
    /// <returns>Frame data or null if no new frame available</returns>
    VideoFrameDto? CaptureFrame(int timeoutMs = 100);

    /// <summary>
    /// Captured frame width in pixels
    /// </summary>
    int Width { get; }

    /// <summary>
    /// Captured frame height in pixels
    /// </summary>
    int Height { get; }

    /// <summary>
    /// Whether the service is initialized and ready
    /// </summary>
    bool IsInitialized { get; }
}
