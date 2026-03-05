using Microsoft.AspNetCore.SignalR.Client;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.Logging;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

/// <summary>
/// SignalR-based implementation of signaling service for peer-to-peer communication.
/// Handles WebSocket connection, room management, and message relay with automatic reconnection.
/// </summary>
public class SignalRSignalingService : ISignalingService
{
    private string _serverUrl;
    private readonly ILogger<SignalRSignalingService>? _logger;
    private HubConnection? _hubConnection;
    private string? _roomId;

    // Retry configuration
    private const int MAX_RETRY_ATTEMPTS = 3;
    private const int INITIAL_RETRY_DELAY_MS = 1000;

    public event Func<SdpMessage, Task>? OnMessageReceived;
    public event Func<InputEventDto, Task>? OnInputReceived;
    public event Action<int>? OnRoomOccupancyChanged;

    public SignalRSignalingService(IConfiguration configuration, ILogger<SignalRSignalingService>? logger = null)
    {
        _logger = logger;
        
        // Get server URL from configuration or use default
        string? configUrl = configuration["SignalR:ServerUrl"];
        _serverUrl = !string.IsNullOrEmpty(configUrl) ? configUrl : "http://localhost:5000/signalinghub";
        FileLogger.Log("SignalR", $"Initial server URL: {_serverUrl}");

        InitializeConnection();
    }

    private void InitializeConnection()
    {
        _hubConnection = new HubConnectionBuilder()
            .WithUrl(_serverUrl, options =>
            {
                // Force WebSockets and LongPolling for better Linux compatibility
                options.Transports = Microsoft.AspNetCore.Http.Connections.HttpTransportType.WebSockets | 
                                    Microsoft.AspNetCore.Http.Connections.HttpTransportType.LongPolling;
                
                // Allow self-signed certificates in development
                options.HttpMessageHandlerFactory = _ => new HttpClientHandler
                {
                    ServerCertificateCustomValidationCallback = (message, cert, chain, errors) => true
                };

                // Increase message size limits on the underlying transport
                options.ApplicationMaxBufferSize = 32 * 1024 * 1024;
                options.TransportMaxBufferSize = 32 * 1024 * 1024;
            })
            .WithAutomaticReconnect(new[]
            {
                TimeSpan.FromSeconds(0),
                TimeSpan.FromSeconds(2),
                TimeSpan.FromSeconds(5),
                TimeSpan.FromSeconds(10),
                TimeSpan.FromSeconds(30)
            })
            .Build();

        // Increase message size limit for video frames (32MB)
        // Set it on the connection and also try to set it via options in InitializeConnection
        // _hubConnection.MaximumReceiveMessageSize = 32 * 1024 * 1024; // Handled in WithUrl options now
        
        // Reconnection event handlers
        _hubConnection.Reconnecting += error =>
        {
            _logger?.LogWarning("SignalR reconnecting... Error: {Error}", error?.Message);
            return Task.CompletedTask;
        };

        _hubConnection.Reconnected += async connectionId =>
        {
            _logger?.LogInformation("SignalR reconnected with ID: {ConnectionId}", connectionId);
            
            // Rejoin room after reconnection
            if (!string.IsNullOrEmpty(_roomId))
            {
                try
                {
                    await _hubConnection.InvokeAsync("JoinRoom", _roomId);
                    _logger?.LogInformation("Rejoined room: {RoomId}", _roomId);
                }
                catch (Exception ex)
                {
                    _logger?.LogError(ex, "Failed to rejoin room after reconnection");
                }
            }
        };

        _hubConnection.Closed += error =>
        {
            _logger?.LogWarning("SignalR connection closed. Error: {Error}", error?.Message);
            return Task.CompletedTask;
        };

        // Register message handler
        _hubConnection.On<NxDesk.Core.Models.SignalingMessage>("ReceiveMessage", async (incoming) =>
        {
            Console.WriteLine($"[SignalR] RAW RECEIVED: Type={incoming.Type}, Sender={incoming.SenderAlias}, HasPayload={!string.IsNullOrEmpty(incoming.Payload)}, ByteLen={incoming.ByteData?.Length ?? 0}");
            _logger?.LogDebug("Message received from peer. Type: {Type}", incoming.Type);
            
            if (OnMessageReceived != null)
            {
                // Map from Core.Models.SignalingMessage to Application.DTOs.SdpMessage
                var sdp = new SdpMessage
                {
                    Type = incoming.Type,
                    Payload = incoming.Payload,
                    ByteData = incoming.ByteData,
                    SenderId = incoming.SenderId,
                    SenderAlias = incoming.SenderAlias,
                    Metadata = incoming.Metadata
                };
                
                await OnMessageReceived.Invoke(sdp);
            }
        });

        _hubConnection.On<int>("RoomOccupancyChanged", (count) =>
        {
            _logger?.LogInformation("Room occupancy changed: {Count}", count);
            OnRoomOccupancyChanged?.Invoke(count);
        });

        _hubConnection.On<InputEventDto>("ReceiveInput", async (inputEvent) =>
        {
            _logger?.LogDebug("Input event received from peer: {Type}", inputEvent.Type);
            if (OnInputReceived != null)
            {
                await OnInputReceived.Invoke(inputEvent);
            }
        });
    }

    public async Task SetServerUrl(string serverUrl)
    {
        if (_serverUrl == serverUrl) return;

        FileLogger.Log("SignalR", $"Switching server URL: {_serverUrl} → {serverUrl}");
        _serverUrl = serverUrl;

        // Tear down existing connection and rebuild with the new URL
        if (_hubConnection != null)
        {
            try 
            {
                FileLogger.Log("SignalR", "Disposing old connection...");
                await _hubConnection.DisposeAsync();
            }
            catch (Exception ex)
            {
                FileLogger.Log("SignalR", $"Error disposing connection: {ex.Message}");
            }
            finally 
            {
                _hubConnection = null;
            }
        }
        InitializeConnection();
    }

    public string? GetConnectionId() => _hubConnection?.ConnectionId;

    public async Task<bool> ConnectAsync(string roomId)
    {
        FileLogger.Log("SignalR", $"ConnectAsync called for room: {roomId}");
        // Leave previous room if any
        if (!string.IsNullOrEmpty(_roomId) && _roomId != (roomId?.Replace(" ", "")))
        {
            await LeaveRoomAsync();
        }

        // Normalize roomId: remove spaces to ensure matching between peers
        _roomId = roomId?.Replace(" ", "");

        // Retry logic with exponential backoff
        int attempt = 0;
        int delayMs = INITIAL_RETRY_DELAY_MS;

        while (attempt < MAX_RETRY_ATTEMPTS)
        {
            try
            {
                if (_hubConnection!.State == HubConnectionState.Disconnected)
                {
                    FileLogger.Log("SignalR", $"Connecting to {_serverUrl} (attempt {attempt + 1}/{MAX_RETRY_ATTEMPTS})");
                    _logger?.LogInformation("Connecting to SignalR server: {ServerUrl} (Attempt {Attempt}/{MaxAttempts})", 
                        _serverUrl, attempt + 1, MAX_RETRY_ATTEMPTS);

                    // Connection timeout
                    using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(10));
                    await _hubConnection.StartAsync(cts.Token);

                    FileLogger.Log("SignalR", $"Connected! State: {_hubConnection.State}");
                    _logger?.LogInformation("SignalR connection established");
                }

                // Join room
                FileLogger.Log("SignalR", $"Joining room: {_roomId}");
                await _hubConnection.InvokeAsync("JoinRoom", _roomId);
                FileLogger.Log("SignalR", $"JOINED room: {_roomId}");
                _logger?.LogInformation("Joined room: {RoomId}", _roomId);
                return true;
            }
            catch (OperationCanceledException)
            {
                FileLogger.Log("SignalR", $"Connection TIMEOUT (attempt {attempt + 1})");
                _logger?.LogWarning("Connection timeout (attempt {Attempt})", attempt + 1);
            }
            catch (Exception ex)
            {
                FileLogger.Log("SignalR", $"Connection FAILED (attempt {attempt + 1}): {ex.Message}");
                _logger?.LogError(ex, "Connection failed (attempt {Attempt}): {Message}", attempt + 1, ex.Message);
            }

            attempt++;
            if (attempt < MAX_RETRY_ATTEMPTS)
            {
                _logger?.LogInformation("Retrying in {Delay}ms...", delayMs);
                await Task.Delay(delayMs);
                delayMs *= 2; // Exponential backoff
            }
        }

        _logger?.LogError("Failed to connect after {MaxAttempts} attempts", MAX_RETRY_ATTEMPTS);
        return false;
    }

    public async Task LeaveRoomAsync()
    {
        if (!string.IsNullOrEmpty(_roomId) && _hubConnection?.State == HubConnectionState.Connected)
        {
            try
            {
                await _hubConnection.InvokeAsync("LeaveRoom", _roomId);
                _logger?.LogInformation("Left room: {RoomId}", _roomId);
            }
            catch (Exception ex)
            {
                _logger?.LogError(ex, "Error leaving room: {Message}", ex.Message);
            }
            finally
            {
                _roomId = null;
            }
        }
    }

    public async Task RelayMessageAsync(SdpMessage message)
    {
        if (_hubConnection?.State == HubConnectionState.Connected)
        {
            try
            {
                if (string.IsNullOrEmpty(_roomId))
                {
                    _logger?.LogWarning("Cannot relay message: _roomId is null or empty.");
                    return;
                }
                
                FileLogger.Log("SignalR", $"RelayMessageAsync: Room={_roomId}, Type={message.Type}");

                // Map from Application.DTOs.SdpMessage to Core.Models.SignalingMessage
                var wireMsg = new NxDesk.Core.Models.SignalingMessage
                {
                    Type = message.Type,
                    Payload = message.Payload,
                    ByteData = message.ByteData,
                    SenderId = message.SenderId,
                    SenderAlias = message.SenderAlias,
                    Metadata = message.Metadata
                };

                await _hubConnection.SendAsync("RelayMessage", _roomId, wireMsg);
            }
            catch (Exception ex)
            {
                _logger?.LogError(ex, "Error sending message: {Message}", ex.Message);
            }
        }
    }

    public async Task SendInputAsync(InputEventDto inputEvent)
    {
        if (_hubConnection?.State == HubConnectionState.Connected)
        {
            try
            {
                _logger?.LogDebug("Sending input event: {Type}", inputEvent.Type);
                await _hubConnection.SendAsync("SendInput", _roomId, inputEvent);
            }
            catch (Exception ex)
            {
                _logger?.LogError(ex, "Error sending input: {Message}", ex.Message);
            }
        }
    }
}
