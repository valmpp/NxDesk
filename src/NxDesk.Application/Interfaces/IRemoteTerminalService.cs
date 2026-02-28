namespace NxDesk.Application.Interfaces;

/// <summary>
/// Port for managing a local shell process on the Host side.
/// The shell stdout/stderr is relayed to the Receiver via signaling.
/// </summary>
public interface IRemoteTerminalService : IDisposable
{
    /// <summary>
    /// Spawn the shell process (bash on Linux, cmd on Windows).
    /// </summary>
    Task StartAsync();

    /// <summary>
    /// Write user input to the shell's stdin.
    /// </summary>
    Task WriteInputAsync(string input);

    /// <summary>
    /// Fired when the shell produces output (stdout or stderr).
    /// </summary>
    event Action<string> OnOutputReceived;

    /// <summary>
    /// Whether the shell process is currently running.
    /// </summary>
    bool IsRunning { get; }
}
