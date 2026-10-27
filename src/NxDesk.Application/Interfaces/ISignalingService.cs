using NxDesk.Application.DTOs;

namespace NxDesk.Application.Interfaces;

/// <summary>
/// Service for WebSocket-based signaling between peers.
/// Handles room management and message relay for screen sharing.
/// </summary>
public interface ISignalingService
{
    /// <summary>
    /// Connect to signaling server and join a room
    /// </summary>
    /// <param name="roomId">Room/Session ID to join</param>
    /// <returns>True if connection successful</returns>
    Task<bool> ConnectAsync(string roomId);

    /// <summary>
    /// Leave current room and cleanup
    /// </summary>
    Task LeaveRoomAsync();

    /// <summary>
    /// Relay an SDP/Frame message to others in the room
    /// </summary>
    Task RelayMessageAsync(SdpMessage message);

    /// <summary>
    /// Get the current connection ID
    /// </summary>
    string? GetConnectionId();

    /// <summary>
    /// Send an input event to other peers in the room (Remote Control)
    /// </summary>
    /// <param name="inputEvent">The event to relay</param>
    Task SendInputAsync(InputEventDto inputEvent);

    /// <summary>
    /// Event fired when a message is received from another peer
    /// </summary>
    event Func<SdpMessage, Task> OnMessageReceived;

    /// <summary>
    /// Event fired when a remote input event is received
    /// </summary>
    event Func<InputEventDto, Task> OnInputReceived;

    /// <summary>
    /// Event fired when the number of participants in a room changes
    /// </summary>
    event Action<int> OnRoomOccupancyChanged;

    /// <summary>
    /// Dynamically change the SignalR server URL (e.g., when connecting to a remote peer).
    /// The connection will be rebuilt with the new URL on the next ConnectAsync call.
    /// </summary>
    /// <param name="serverUrl">Full URL, e.g. "http://192.168.1.50:5000/signalinghub"</param>
    Task SetServerUrl(string serverUrl);

    /// <summary>
    /// Current number of occupants in the joined room
    /// </summary>
    int CurrentRoomOccupancy { get; }
}
