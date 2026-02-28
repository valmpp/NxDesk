using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.SignalR;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using NxDesk.Application.DTOs;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// Embedded SignalR signaling server that runs inside the NxDesk client process.
/// Starts a Kestrel web server on 0.0.0.0:5000 in a background thread so that
/// the app works as a single executable — no separate server process needed.
/// </summary>
public static class EmbeddedSignalingServer
{
    private static WebApplication? _app;
    private static Thread? _serverThread;
    private static readonly ManualResetEventSlim _ready = new(false);
    public static int ActualPort { get; private set; } = 5000;

    /// <summary>
    /// Starts the embedded signaling server on a background thread.
    /// Safe to call multiple times — subsequent calls are no-ops.
    /// </summary>
    public static void Start()
    {
        if (_serverThread != null) return;

        _serverThread = new Thread(RunServer)
        {
            IsBackground = true,
            Name = "NxDesk-SignalingServer"
        };
        _serverThread.Start();

        // Wait up to 5 seconds for the server to be ready
        if (!_ready.Wait(TimeSpan.FromSeconds(5)))
        {
            FileLogger.Log("Server", "WARNING: Embedded server did not start within 5 seconds");
        }
    }

    private static async void RunServer()
    {
        int port = 5000;
        bool started = false;

        while (!started && port < 5100)
        {
            try
            {
                var builder = WebApplication.CreateBuilder();
                builder.WebHost.UseUrls($"http://0.0.0.0:{port}");

                // Suppress noisy logs from embedded server
                builder.Logging.SetMinimumLevel(Microsoft.Extensions.Logging.LogLevel.Warning);

                builder.Services.AddSignalR(options =>
                {
                    options.EnableDetailedErrors = true;
                    options.MaximumReceiveMessageSize = 32 * 1024 * 1024; // 32MB for video frames
                });

                builder.Services.AddCors(options =>
                {
                    options.AddDefaultPolicy(policy =>
                    {
                        policy.AllowAnyOrigin()
                               .AllowAnyHeader()
                               .AllowAnyMethod();
                    });
                });

                _app = builder.Build();
                _app.UseCors();
                _app.MapHub<EmbeddedHub>("/signalinghub");
                _app.MapGet("/", () => "NxDesk Signaling Server (Embedded)");

                // StartAsync returns once the server is listening or fails if bind fails
                FileLogger.Log("Server", $"Attempting to start embedded server on port {port}...");
                await _app.StartAsync();

                ActualPort = port;
                FileLogger.Log("Server", $"SignalR embedded server STARTED on http://0.0.0.0:{port}");
                _ready.Set(); // Signal that server is ready
                started = true;
                
                // Keep the server running
                await _app.WaitForShutdownAsync();
            }
            catch (Exception ex)
            {
                // Check if it's a "port in use" error
                bool isPortInUse = ex.Message.Contains("address already in use", StringComparison.OrdinalIgnoreCase) || 
                                   ex.Message.Contains("already in use", StringComparison.OrdinalIgnoreCase) ||
                                   ex.Message.Contains("en uso", StringComparison.OrdinalIgnoreCase) || // Spanish
                                   ex.HResult == -2147467259;

                if (isPortInUse)
                {
                    FileLogger.Log("Server", $"Port {port} in use, trying next...");
                }
                else
                {
                    FileLogger.Log("Server", $"FATAL: Failed to start embedded server on port {port}: {ex.Message}");
                    if (ex.InnerException != null)
                        FileLogger.Log("Server", $"Inner Error: {ex.InnerException.Message}");
                    
                    // If it's not a port conflict, some other major error happened
                    break;
                }

                // Clean up the failed app instance
                if (_app != null)
                {
                    await _app.StopAsync();
                    await _app.DisposeAsync();
                    _app = null;
                }
                
                port++;
            }
        }
    }

    public static void Stop()
    {
        _app?.StopAsync().Wait(TimeSpan.FromSeconds(3));
        _app = null;
        _serverThread = null;
    }
}

/// <summary>
/// Embedded SignalR hub — mirrors NxDesk.SignalingServer.Hubs.SignalingHub
/// but lives inside the Infrastructure layer so it can be embedded in the client.
/// </summary>
public class EmbeddedHub : Hub
{
    private static readonly System.Collections.Concurrent.ConcurrentDictionary<string, HashSet<string>> _roomOccupants = new();
    // PeerId → ConnectionId
    private static readonly System.Collections.Concurrent.ConcurrentDictionary<string, string> _connectedPeers = new();

    public async Task JoinRoom(string roomId)
    {
        await Groups.AddToGroupAsync(Context.ConnectionId, roomId);

        _connectedPeers[roomId] = Context.ConnectionId;

        var occupants = _roomOccupants.GetOrAdd(roomId, _ => new HashSet<string>());
        lock (occupants) { occupants.Add(Context.ConnectionId); }

        FileLogger.Log("Hub", $"{Context.ConnectionId} JOINED room {roomId} (total: {occupants.Count})");

        await Clients.Group(roomId).SendAsync("RoomOccupancyChanged", occupants.Count);
        await Clients.OthersInGroup(roomId).SendAsync("ParticipantJoined", Context.ConnectionId);
    }

    public async Task LeaveRoom(string roomId)
    {
        await Groups.RemoveFromGroupAsync(Context.ConnectionId, roomId);
        _connectedPeers.TryRemove(roomId, out _);

        if (_roomOccupants.TryGetValue(roomId, out var occupants))
        {
            lock (occupants)
            {
                occupants.Remove(Context.ConnectionId);
                if (occupants.Count == 0) _roomOccupants.TryRemove(roomId, out _);
            }
            await Clients.Group(roomId).SendAsync("RoomOccupancyChanged", occupants?.Count ?? 0);
        }

        await Clients.OthersInGroup(roomId).SendAsync("ParticipantLeft", Context.ConnectionId);
    }

    public async Task RelayMessage(string roomId, SdpMessage message)
    {
        if (string.IsNullOrEmpty(roomId)) return;
        await Clients.OthersInGroup(roomId).SendAsync("ReceiveMessage", message);
    }

    public async Task SendInput(string roomId, InputEventDto inputEvent)
    {
        if (string.IsNullOrEmpty(roomId)) return;
        await Clients.OthersInGroup(roomId).SendAsync("ReceiveInput", inputEvent);
    }

    public Task<bool> IsPeerOnline(string peerId) =>
        Task.FromResult(_connectedPeers.ContainsKey(peerId));

    public Task<List<string>> CheckOnlineStatus(List<string> peerIds) =>
        Task.FromResult(peerIds.Where(id => _connectedPeers.ContainsKey(id)).ToList());

    public Task<List<string>> GetAllOnlinePeers() =>
        Task.FromResult(_connectedPeers.Keys.ToList());

    public override async Task OnDisconnectedAsync(Exception? exception)
    {
        // Cleanup PeerTracker
        var peer = _connectedPeers.FirstOrDefault(kvp => kvp.Value == Context.ConnectionId);
        if (!string.IsNullOrEmpty(peer.Key))
            _connectedPeers.TryRemove(peer.Key, out _);

        // Cleanup all rooms
        foreach (var room in _roomOccupants)
        {
            bool removed;
            int count;
            lock (room.Value)
            {
                removed = room.Value.Remove(Context.ConnectionId);
                count = room.Value.Count;
            }
            if (removed)
            {
                await Clients.Group(room.Key).SendAsync("RoomOccupancyChanged", count);
                if (count == 0) _roomOccupants.TryRemove(room.Key, out _);
            }
        }

        await base.OnDisconnectedAsync(exception);
    }
}
