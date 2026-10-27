using NxDesk.Application.DTOs;

namespace NxDesk.Application.Interfaces;

/// <summary>
/// Port for recording received video frames into a video file.
/// </summary>
public interface IScreenRecordingService : IDisposable
{
    /// <summary>Whether a recording session is currently active.</summary>
    bool IsRecording { get; }

    /// <summary>Start recording to the specified output file path.</summary>
    Task StartAsync(string outputPath, int width, int height, int fps = 10, bool isRaw = false);

    /// <summary>Push a single video frame into the recording pipeline.</summary>
    Task PushFrameAsync(VideoFrameDto frame);

    /// <summary>Stop recording and finalize the output file.</summary>
    Task StopAsync();
}
