using Microsoft.AspNetCore.SignalR;
using NxDesk.Application.DTOs;

namespace NxDesk.SignalingServer.Hubs;

/// <summary>
/// SignalR Hub for peer-to-peer signaling.
/// Manages room-based communication for screen sharing sessions.
/// Integrates PeerTracker for online status queries (migrated from old NxDesk.Api).
/// </summary>
public class SignalingHub : Hub
{
    private readonly ILogger<SignalingHub> _logger;
    private static readonly System.Collections.Concurrent.ConcurrentDictionary<string, System.Collections.Generic.HashSet<string>> _roomOccupants = new();

    public SignalingHub(ILogger<SignalingHub> logger)
    {
        _logger = logger;
    }

    public async Task JoinRoom(string roomId)
    {
        await Groups.AddToGroupAsync(Context.ConnectionId, roomId);
        
        // Track peer by business ID (roomId acts as peerId for the host)
        PeerTracker.Instance.RegisterPeer(roomId, Context.ConnectionId);

        var occupants = _roomOccupants.GetOrAdd(roomId, _ => new System.Collections.Generic.HashSet<string>());
        lock (occupants)
        {
            occupants.Add(Context.ConnectionId);
        }

        _logger.LogInformation("Client {ConnectionId} joined room {RoomId}. Total: {Count}", 
            Context.ConnectionId, roomId, occupants.Count);
        
        // Notify everyone in the room about the current occupancy
        await Clients.Group(roomId).SendAsync("RoomOccupancyChanged", occupants.Count);
        
        // Specific notification for P2P handshake initiation
        await Clients.OthersInGroup(roomId).SendAsync("ParticipantJoined", Context.ConnectionId);
    }

    public async Task LeaveRoom(string roomId)
    {
        await Groups.RemoveFromGroupAsync(Context.ConnectionId, roomId);
        
        PeerTracker.Instance.UnregisterPeer(roomId);

        if (_roomOccupants.TryGetValue(roomId, out var occupants))
        {
            lock (occupants)
            {
                occupants.Remove(Context.ConnectionId);
                if (occupants.Count == 0)
                {
                    _roomOccupants.TryRemove(roomId, out _);
                }
            }
            await Clients.Group(roomId).SendAsync("RoomOccupancyChanged", occupants?.Count ?? 0);
        }

        _logger.LogInformation("Client {ConnectionId} left room {RoomId}", Context.ConnectionId, roomId);
        await Clients.OthersInGroup(roomId).SendAsync("ParticipantLeft", Context.ConnectionId);
    }

    public async Task RelayMessage(string roomId, SdpMessage message)
    {
        Console.WriteLine($"[Debug Hub] RelayMessage to Room: {roomId}");
        if (string.IsNullOrEmpty(roomId)) return;
        await Clients.OthersInGroup(roomId).SendAsync("ReceiveMessage", message);
    }

    public async Task SendInput(string roomId, InputEventDto inputEvent)
    {
        if (string.IsNullOrEmpty(roomId)) return;
        await Clients.OthersInGroup(roomId).SendAsync("ReceiveInput", inputEvent);
    }

    // ── Online Status Queries (from old NxDesk.Api) ──

    /// <summary>
    /// Check if a specific peer is online by their ID.
    /// </summary>
    public Task<bool> IsPeerOnline(string peerId)
    {
        return Task.FromResult(PeerTracker.Instance.IsPeerOnline(peerId));
    }

    /// <summary>
    /// Check which peers from a list are currently online.
    /// </summary>
    public Task<List<string>> CheckOnlineStatus(List<string> peerIds)
    {
        return Task.FromResult(PeerTracker.Instance.GetOnlinePeers(peerIds));
    }

    /// <summary>
    /// Get all currently online peers.
    /// </summary>
    public Task<List<string>> GetAllOnlinePeers()
    {
        return Task.FromResult(PeerTracker.Instance.GetAllOnlinePeers());
    }

    public override async Task OnDisconnectedAsync(Exception? exception)
    {
        // Cleanup PeerTracker
        PeerTracker.Instance.UnregisterByConnectionId(Context.ConnectionId);

        // Cleanup all rooms this client was in
        foreach (var room in _roomOccupants)
        {
            bool removed = false;
            int count = 0;
            lock (room.Value)
            {
                if (room.Value.Remove(Context.ConnectionId))
                {
                    removed = true;
                    count = room.Value.Count;
                }
            }
            if (removed)
            {
                await Clients.Group(room.Key).SendAsync("RoomOccupancyChanged", count);
                if (count == 0) _roomOccupants.TryRemove(room.Key, out _);
            }
        }

        _logger.LogInformation("Client {ConnectionId} disconnected", Context.ConnectionId);
        await base.OnDisconnectedAsync(exception);
    }
}
