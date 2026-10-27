namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Simple file logger that writes to nxdesk.log next to the executable.
/// Thread-safe, appends with timestamps. Useful for diagnosing cross-PC
/// discovery and connection issues.
/// </summary>
public static class FileLogger
{
    private static readonly object _lock = new();
    private static readonly string _logPath;

    static FileLogger()
    {
        // Log file in the same folder as the executable
        var dir = AppContext.BaseDirectory;
        _logPath = Path.Combine(dir, "nxdesk.log");

        // Clear old log on startup
        try { File.WriteAllText(_logPath, $"=== NxDesk Log - {DateTime.Now:yyyy-MM-dd HH:mm:ss} ===\n"); }
        catch { /* ignore if path not writable */ }
    }

    public static void Log(string category, string message)
    {
        var line = $"[{DateTime.Now:HH:mm:ss.fff}] [{category}] {message}";
        Console.WriteLine(line);

        lock (_lock)
        {
            try { File.AppendAllText(_logPath, line + "\n"); }
            catch { /* ignore write errors */ }
        }
    }

    public static string LogFilePath => _logPath;
}
