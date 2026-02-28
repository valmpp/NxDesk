namespace NxDesk.Application.DTOs;

/// <summary>
/// Represents a raw video frame with BGRA pixel data.
/// Optimized for use with Avalonia's WriteableBitmap.
/// </summary>
public record VideoFrameDto
{
    /// <summary>
    /// Raw pixel data in BGRA format (4 bytes per pixel)
    /// </summary>
    public byte[] Pixels { get; init; }

    /// <summary>
    /// Frame width in pixels. For compressed frames, this is the original width.
    /// </summary>
    public int Width { get; init; }

    /// <summary>
    /// Frame height in pixels. For compressed frames, this is the original height.
    /// </summary>
    public int Height { get; init; }

    /// <summary>
    /// Stride (bytes per row). Defaults to Width * 4 for BGRA.
    /// </summary>
    public int Stride { get; init; }

    /// <summary>
    /// Whether the pixel data is compressed (e.g., JPEG)
    /// </summary>
    public bool IsCompressed { get; init; }

    public VideoFrameDto(byte[] pixels, int width, int height, int stride = 0, bool isCompressed = false)
    {
        Pixels = pixels;
        Width = width;
        Height = height;
        Stride = stride > 0 ? stride : width * 4;
        IsCompressed = isCompressed;
    }
}
