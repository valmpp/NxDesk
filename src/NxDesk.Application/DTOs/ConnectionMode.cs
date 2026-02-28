namespace NxDesk.Application.DTOs;

/// <summary>
/// Defines the type of remote connection session.
/// </summary>
public enum ConnectionMode
{
    /// <summary>
    /// View-only mode: the user can observe but not interact with the remote desktop.
    /// </summary>
    ViewOnly = 0,

    /// <summary>
    /// Full control mode: the user can interact with the remote desktop (mouse, keyboard).
    /// </summary>
    ViewWithControl = 1
}
