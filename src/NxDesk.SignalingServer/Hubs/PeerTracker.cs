using System.Collections.Concurrent;

namespace NxDesk.SignalingServer.Hubs;

/// <summary>
/// Thread-safe singleton that tracks connected peers by their business ID (PeerId).
/// Allows querying online status before attempting a connection.
/// Migrated from old NxDesk.Api project.
/// </summary>
public sealed class PeerTracker
{
    private static readonly Lazy<PeerTracker> _instance = new(() => new PeerTracker());
    public static PeerTracker Instance => _instance.Value;

    // PeerId → ConnectionId
    private readonly ConcurrentDictionary<string, string> _connectedPeers = new();

    private PeerTracker() { }

    public void RegisterPeer(string peerId, string connectionId)
    {
        _connectedPeers[peerId] = connectionId;
        Console.WriteLine($"[PeerTracker] Registered: {peerId} ({connectionId})");
    }

    public void UnregisterPeer(string peerId)
    {
        _connectedPeers.TryRemove(peerId, out _);
        Console.WriteLine($"[PeerTracker] Unregistered: {peerId}");
    }

    public void UnregisterByConnectionId(string connectionId)
    {
        var peer = _connectedPeers.FirstOrDefault(kvp => kvp.Value == connectionId);
        if (!string.IsNullOrEmpty(peer.Key))
        {
            _connectedPeers.TryRemove(peer.Key, out _);
            Console.WriteLine($"[PeerTracker] Disconnected peer: {peer.Key}");
        }
    }

    public bool IsPeerOnline(string peerId) => _connectedPeers.ContainsKey(peerId);

    public List<string> GetOnlinePeers(List<string> peerIds) =>
        peerIds.Where(id => _connectedPeers.ContainsKey(id)).ToList();

    public List<string> GetAllOnlinePeers() => _connectedPeers.Keys.ToList();
}
