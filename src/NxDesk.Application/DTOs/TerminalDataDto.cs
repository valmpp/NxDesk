namespace NxDesk.Application.DTOs;

/// <summary>
/// DTO for terminal data relay between peers via signaling.
/// </summary>
public record TerminalDataDto(string Data, TerminalDataType Type);

/// <summary>
/// Indicates whether the data is input (command) or output (response).
/// </summary>
public enum TerminalDataType
{
    Input,
    Output
}
