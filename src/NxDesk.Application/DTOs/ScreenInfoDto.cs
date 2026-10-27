namespace NxDesk.Application.DTOs;

/// <summary>
/// A Data Transfer Object representing a specific monitor/screen's physical information.
/// </summary>
public record ScreenInfoDto(
    int Index,
    string Name,
    int Width,
    int Height,
    int X,
    int Y
);
