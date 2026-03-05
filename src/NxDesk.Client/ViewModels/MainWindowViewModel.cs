using System;
using System.IO;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;
using NxDesk.Core.Models;
using System.Runtime.InteropServices;

namespace NxDesk.Client.ViewModels;

public partial class MainWindowViewModel : ViewModelBase
{
    private readonly IDiscoveryService _discoveryService = default!;
    private readonly ISignalingService _signalingService = default!;
    private readonly IScreenCaptureService _captureService = default!;
    private readonly IThemeService _themeService = default!;
    private readonly IConfigurationService _configService = default!;
    private readonly IInputInjectionService _inputService = default!;
    private readonly IRemoteTerminalService _terminalService = default!;
    private readonly IFileTransferService _fileTransferService = default!;
    private readonly IScreenRecordingService _recordingService = default!;
    private readonly System.Collections.Concurrent.ConcurrentDictionary<string, TaskCompletionSource<RemoteFolderResponse>> _pendingRemoteFolderRequests = new();
    private readonly System.Collections.Concurrent.ConcurrentDictionary<string, TaskCompletionSource<ListDirectoryResponse>> _pendingListDirRequests = new();
    
    private System.Threading.CancellationTokenSource? _streamingCts;
    public event Action<NxDesk.Application.DTOs.VideoFrameDto>? OnFrameReceived;
    public event Action? OnSessionClosed;

    [ObservableProperty]
    private string _myAlias = "Usuario";

    [ObservableProperty]
    private string _myId = "000 000 000";

    [ObservableProperty]
    private string _statusMessage = "Listo";

    [ObservableProperty]
    private bool _isSessionActive;

    partial void OnIsSessionActiveChanged(bool value)
    {
        _discoveryService.SetStatus(value ? PeerStatus.Connected : PeerStatus.Background);
        
        if (value)
        {
            _connectionStartTime = DateTime.Now;
            ConnectionDuration = "00:00:00";
            if (_connectionTimer == null)
            {
                _connectionTimer = new System.Timers.Timer(1000);
                _connectionTimer.Elapsed += (s, e) =>
                {
                    var duration = DateTime.Now - _connectionStartTime;
                    string formatted = duration.TotalHours >= 1 
                        ? $"{(int)duration.TotalHours:D2}:{duration.Minutes:D2}:{duration.Seconds:D2}"
                        : $"{duration.Minutes:D2}:{duration.Seconds:D2}";

                    Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                    {
                        ConnectionDuration = formatted;
                    });
                };
            }
            _connectionTimer.Start();
        }
        else
        {
            _connectionTimer?.Stop();
            ConnectionDuration = "00:00:00";
        }
    }

    [ObservableProperty]
    private bool _isConnecting;

    [ObservableProperty]
    private string _statusText = "Listo";

    [ObservableProperty]
    private string _connectionDuration = "00:00";

    [ObservableProperty]
    private string _latencyText = "- ms";

    [ObservableProperty]
    private int _latencyQuality = 0; // 0: Good, 1: Warning, 2: Bad

    [ObservableProperty]
    private double _currentFps = 0;

    [ObservableProperty]
    private bool _isRecording;

    [ObservableProperty]
    private string _recordingDuration = "";

    [ObservableProperty]
    private string _recordingPath = "";

    private System.Timers.Timer? _recordingTimer;
    private DateTime _recordingStartTime;

    private System.Timers.Timer? _connectionTimer;
    private DateTime _connectionStartTime;

    [ObservableProperty]
    private bool _isRemoteControlEnabled = true;

    [ObservableProperty]
    private int _peerCount = 0;

    [ObservableProperty]
    private string _connectedPeerAlias = string.Empty;

    [ObservableProperty]
    private string _connectedPeerId = string.Empty;

    [ObservableProperty]
    private string _connectedPeerIp = string.Empty;

    private bool _connectedPeerIsAdmin = false;

    // --- Connection Type Dialog State ---
    [ObservableProperty]
    private bool _isConnectionDialogVisible;

    [ObservableProperty]
    private bool _isControlModeSelected; // false = ViewOnly (default), true = ViewWithControl

    [ObservableProperty]
    private string _pendingPeerAlias = string.Empty;

    private string _pendingPeerId = string.Empty;

    public bool IsAdmin => _configService.IsAdmin;

    [ObservableProperty]
    private NxDesk.Application.DTOs.ScreenInfoDto? _currentRemoteScreen;

    // --- Terminal State ---
    [ObservableProperty]
    private string _terminalInput = string.Empty;

    [ObservableProperty]
    private bool _isTerminalVisible;

    [ObservableProperty]
    private bool _isTerminalExpanded = true;

    // --- Screen Lock State ---
    [ObservableProperty]
    private bool _isScreenLocked;

    private NxDesk.Client.Views.ScreenLockOverlay? _screenLockOverlay;

    public ObservableCollection<string> TerminalLines { get; } = new();

    private string _fullTerminalOutput = string.Empty;
    public string FullTerminalOutput
    {
        get => _fullTerminalOutput;
        set => SetProperty(ref _fullTerminalOutput, value);
    }
    public event Action? OnTerminalOutputReceived;
    
    // --- Admin Access Toggle ---
    [ObservableProperty]
    private string _adminPassword = string.Empty;

    [ObservableProperty]
    private bool _isAdminPasswordDialogVisible;

    [ObservableProperty]
    private string _adminPasswordError = string.Empty;

    [ObservableProperty]
    private bool _isAdminLeaveConfirmationVisible;

    [RelayCommand]
    private void ToggleAdminMode()
    {
        if (IsAdmin)
        {
            IsAdminLeaveConfirmationVisible = true;
        }
        else
        {
            AdminPassword = string.Empty;
            AdminPasswordError = string.Empty;
            IsAdminPasswordDialogVisible = true;
        }
    }

    [RelayCommand]
    private void ConfirmAdminPassword()
    {
        if (AdminPassword == "0000")
        {
            _configService.IsAdmin = true;
            IsAdminPasswordDialogVisible = false;
            OnPropertyChanged(nameof(IsAdmin));
            AddNotification("Admin", "Modo administrador activado", "Success", 3);

            // If a session is already active, enable admin features immediately
            if (IsSessionActive)
            {
                IsTerminalVisible = true;
                IsRemoteControlEnabled = true;

                // Start the terminal shell if not already running
                _ = Task.Run(async () => await StartTerminalIfNeeded());
            }
        }
        else
        {
            AdminPasswordError = "Contraseña incorrecta";
        }
    }

    [RelayCommand]
    private void CancelAdminPassword()
    {
        IsAdminPasswordDialogVisible = false;
    }

    [RelayCommand]
    private void LeaveAdminMode()
    {
        _configService.IsAdmin = false;
        IsAdminLeaveConfirmationVisible = false;

        // Disable restricted features when leaving Admin mode
        IsRemoteControlEnabled = false;
        IsTerminalVisible = false;

        OnPropertyChanged(nameof(IsAdmin));
        AddNotification("Admin", "Modo administrador desactivado", "Info", 3);
    }

    [RelayCommand]
    private void CancelLeaveAdminMode()
    {
        IsAdminLeaveConfirmationVisible = false;
    }

    // --- Screen Lock (remote) ---
    [RelayCommand]
    private async Task ToggleScreenLock()
    {
        if (!IsSessionActive) return;

        IsScreenLocked = !IsScreenLocked;
        var msgType = IsScreenLocked ? "lock_screen" : "unlock_screen";

        var msg = new NxDesk.Application.DTOs.SdpMessage
        {
            Type = msgType,
            SenderId = MyId,
            SenderAlias = MyAlias
        };
        await _signalingService.RelayMessageAsync(msg);
        Console.WriteLine($"[ScreenLock] Sent {msgType} signal.");

        AddNotification("Bloqueo",
            IsScreenLocked ? "Pantalla del host bloqueada" : "Pantalla del host desbloqueada",
            IsScreenLocked ? "Warning" : "Success", 3);
    }

    // --- File Transfer Interaction ---
    public Func<Task<string?>>? RequestFilePicker { get; set; }
    public Func<Task<string?>>? RequestFolderPicker { get; set; }

    public ObservableCollection<PeerViewModel> DiscoveredPeers { get; } = new();
    public ObservableCollection<NotificationItemViewModel> Notifications { get; } = new();

    // --- Remote File Explorer ---
    public ObservableCollection<DirectoryItemDto> RemoteItems { get; } = new();

    [ObservableProperty]
    private string _currentRemotePath = string.Empty;

    [ObservableProperty]
    private bool _isRemoteExplorerVisible;

    [ObservableProperty]
    private bool _isRemoteExplorerLoading;
    public ObservableCollection<NxDesk.Application.DTOs.ScreenInfoDto> RemoteScreens { get; } = new();

    public MainWindowViewModel(
        NxDesk.Application.Interfaces.IDiscoveryService discoveryService, 
        NxDesk.Application.Interfaces.ISignalingService signalingService,
        NxDesk.Application.Interfaces.IScreenCaptureService captureService,
        NxDesk.Application.Interfaces.IThemeService themeService,
        NxDesk.Application.Interfaces.IConfigurationService configService,
        NxDesk.Application.Interfaces.IInputInjectionService inputService,
        NxDesk.Application.Interfaces.IRemoteTerminalService terminalService,
        NxDesk.Application.Interfaces.IFileTransferService fileTransferService,
        NxDesk.Application.Interfaces.IScreenRecordingService recordingService,
        string[] args)
    {
        _discoveryService = discoveryService;
        _signalingService = signalingService;
        _captureService = captureService;
        _themeService = themeService;
        _configService = configService;
        _inputService = inputService;
        _terminalService = terminalService;
        _fileTransferService = fileTransferService;
        _recordingService = recordingService;

        // Check for ID override in args (for testing)
        var idOverride = args?.FirstOrDefault(a => a.StartsWith("--id="))?.Split('=')[1];
        if (string.IsNullOrEmpty(idOverride))
            idOverride = args?.SkipWhile(a => a != "--id").Skip(1).FirstOrDefault();

        // Load or generate persistent PC ID
        if (!string.IsNullOrEmpty(idOverride))
        {
            MyId = idOverride;
        }
        else
        {
            if (string.IsNullOrEmpty(_configService.MachineId))
            {
                _configService.MachineId = NxDesk.Infrastructure.Helpers.MachineIdHelper.GetPersistentId();
            }
            MyId = _configService.MachineId;
        }

        // Load or set default Alias
        if (string.IsNullOrEmpty(_configService.Alias))
        {
            _configService.Alias = Environment.MachineName;
        }
        MyAlias = _configService.Alias;

        StatusMessage = "Conectado al servidor de señales";
        
        // Add welcome notification with auto-hide
        AddNotification("Sistema", "Bienvenido a NxDesk", "Info", 2);

        _discoveryService.OnPeerDiscovered += peer =>
        {
            Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
            {
                var existing = DiscoveredPeers.FirstOrDefault(p => p.Id == peer.Id);
                if (existing != null)
                {
                    existing.Update(peer);
                }
                else
                {
                    Console.WriteLine($"[Discovery] Peer found: {peer.Alias} ({peer.Id}) at {peer.IpAddress} - Status: {peer.Status}");
                    DiscoveredPeers.Add(new PeerViewModel(peer));
                }
            });
        };

        // Timer to detect offline peers
        var offlineTimer = new System.Timers.Timer(5000);
        offlineTimer.Elapsed += (s, e) => 
        {
            Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
            {
                foreach (var peer in DiscoveredPeers.ToList())
                {
                    if ((DateTime.Now - peer.LastSeen).TotalSeconds > 15)
                    {
                        if (peer.Status != PeerStatus.Offline)
                        {
                            peer.Status = PeerStatus.Offline;
                        }
                    }
                }
            });
        };
        offlineTimer.Start();

        _signalingService.OnMessageReceived += async msg => 
        {
            Console.WriteLine($"[Signaling] Message received: {msg.Type}");
            await HandleSignalingMessage(msg);
        };
        
        _signalingService.OnInputReceived += async inputEvent => 
        {
            if (_inputService != null && _isSharingStarted)
            {
                await _inputService.InjectEventAsync(inputEvent);
            }
            else
            {
                NxDesk.Infrastructure.Services.FileLogger.Log("Input", $"DROPPED input {inputEvent.Type}: _inputService={(_inputService != null ? "OK" : "NULL")}, _isSharingStarted={_isSharingStarted}");
            }
        };
        _signalingService.OnRoomOccupancyChanged += count => 
        {
            Console.WriteLine($"[Signaling] Room occupancy: {count}");
            Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
            {
                PeerCount = count;
                UpdateConnectionStatus();
            });
        };

        if (args != null)
        {
            ProcessArgs(args);
        }
        
        // Start signaling server and discovery
        NxDesk.Infrastructure.Services.EmbeddedSignalingServer.OnServerStarted += async port =>
        {
            NxDesk.Infrastructure.Services.FileLogger.Log("SignalR", $"Local server started on port {port}. Synchronizing signaling URL...");
            await _signalingService.SetServerUrl($"http://localhost:{port}/signalinghub");
            
            // Restart discovery with the new port
            _discoveryService.StopDiscovery();
            StartDiscovery();
        };

        _configService.IsAdminChanged += isAdmin => 
        {
            Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
            {
                OnPropertyChanged(nameof(IsAdmin));
            });
        };

        NxDesk.Infrastructure.Services.EmbeddedSignalingServer.Start();
        StartDiscovery();
    }

    public void InitializeFromArgs(string[]? args)
    {
        ProcessArgs(args ?? Array.Empty<string>());
    }

    private string? _currentRoomId;

    private bool _isAutoShare = false;
    private bool _isAutoConnect = false;

    [ObservableProperty]
    private bool _isSharingStarted = false;

    private NxDesk.Client.Views.HostDisconnectWindow? _hostDisconnectWindow;

    partial void OnIsSharingStartedChanged(bool value)
    {
        Console.WriteLine($"[HostWindow] OnIsSharingStartedChanged => {value}");
        
        // Start/stop connection timer for the Host side
        if (value)
        {
            _connectionStartTime = DateTime.Now;
            ConnectionDuration = "00:00";
            if (_connectionTimer == null)
            {
                _connectionTimer = new System.Timers.Timer(1000);
                _connectionTimer.Elapsed += (s, e) =>
                {
                    var duration = DateTime.Now - _connectionStartTime;
                    string formatted = duration.TotalHours >= 1 
                        ? $"{(int)duration.TotalHours:D2}:{duration.Minutes:D2}:{duration.Seconds:D2}"
                        : $"{duration.Minutes:D2}:{duration.Seconds:D2}";

                    Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                    {
                        ConnectionDuration = formatted;
                    });
                };
            }
            _connectionTimer.Start();
        }
        else
        {
            _connectionTimer?.Stop();
            ConnectionDuration = "00:00";
        }
        
        Avalonia.Threading.Dispatcher.UIThread.Post(async () =>
        {
            if (value)
            {
                // Give it a second for peer_hello to arrive and indicate if it's an admin
                await Task.Delay(1000);

                // If the connection was already closed or if the peer is an admin, don't show the window
                if (!IsSharingStarted || _connectedPeerIsAdmin)
                {
                    Console.WriteLine($"[HostWindow] Suppressing HostDisconnectWindow (IsSharingStarted={IsSharingStarted}, IsAdmin={_connectedPeerIsAdmin})");
                    return;
                }

                if (_hostDisconnectWindow == null)
                {
                    Console.WriteLine("[HostWindow] Creating and showing HostDisconnectWindow.");
                    _hostDisconnectWindow = new NxDesk.Client.Views.HostDisconnectWindow
                    {
                        DataContext = this
                    };
                    _hostDisconnectWindow.Show();
                }
            }
            else
            {
                Console.WriteLine("[HostWindow] Closing HostDisconnectWindow.");
                if (_hostDisconnectWindow != null)
                {
                    _hostDisconnectWindow.ForceClose();
                    _hostDisconnectWindow = null;
                }
                ConnectedPeerAlias = string.Empty;
                ConnectedPeerId = string.Empty;
                ConnectedPeerIp = string.Empty;
                _connectedPeerIsAdmin = false;
            }
        });
    }

    private bool _lastFrameWasCompressed = false;
    private int _lastFrameWidth = 1920;
    private int _lastFrameHeight = 1080;

    /// <summary>
    /// Centralised cleanup of all resources from a previous session.
    /// Must be called before starting any new host/streaming session to avoid
    /// stale CancellationTokenSource, GStreamer processes, or terminal handles.
    /// </summary>
    private void CleanupPreviousSession()
    {
        Console.WriteLine("[Cleanup] Cleaning up previous session resources...");
        NxDesk.Infrastructure.Services.FileLogger.Log("Cleanup", "Starting previous session cleanup");

        // 1. Cancel and dispose streaming CTS
        if (_streamingCts != null)
        {
            Console.WriteLine("[Cleanup] Cancelling stale streaming CTS.");
            try { _streamingCts.Cancel(); } catch { }
            _streamingCts = null;
        }

        // 2. Dispose capture service (kills GStreamer process)
        try
        {
            _captureService.Dispose();
            Console.WriteLine("[Cleanup] CaptureService disposed.");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Cleanup] CaptureService dispose error (non-fatal): {ex.Message}");
        }

        // 3. Dispose terminal service
        try
        {
            _terminalService.Dispose();
            _isTerminalInitialized = false;
            Console.WriteLine("[Cleanup] TerminalService disposed.");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Cleanup] TerminalService dispose error (non-fatal): {ex.Message}");
        }

        // 4. Dispose input injection service
        if (_inputService is IDisposable disposableInput)
        {
            try
            {
                disposableInput.Dispose();
                Console.WriteLine("[Cleanup] InputService disposed.");
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Cleanup] InputService dispose error (non-fatal): {ex.Message}");
            }
        }

        // 5. Kill orphan GStreamer / portal_helper processes on Linux
        if (System.Runtime.InteropServices.RuntimeInformation.IsOSPlatform(
                System.Runtime.InteropServices.OSPlatform.Linux))
        {
            try
            {
                var killGst = new System.Diagnostics.Process
                {
                    StartInfo = new System.Diagnostics.ProcessStartInfo
                    {
                        FileName = "pkill",
                        Arguments = "-f gst-launch",
                        UseShellExecute = false,
                        CreateNoWindow = true
                    }
                };
                killGst.Start();
                killGst.WaitForExit(2000);
                Console.WriteLine("[Cleanup] Killed orphan gst-launch processes.");

                var killPortal = new System.Diagnostics.Process
                {
                    StartInfo = new System.Diagnostics.ProcessStartInfo
                    {
                        FileName = "pkill",
                        Arguments = "-f portal_helper.py",
                        UseShellExecute = false,
                        CreateNoWindow = true
                    }
                };
                killPortal.Start();
                killPortal.WaitForExit(2000);
                Console.WriteLine("[Cleanup] Killed orphan portal_helper processes.");
            }
            catch { /* pkill may fail if no processes found — that's OK */ }
        }

        // 6. Reset sharing flag
        IsSharingStarted = false;

        Console.WriteLine("[Cleanup] Previous session cleanup complete.");
        NxDesk.Infrastructure.Services.FileLogger.Log("Cleanup", "Previous session cleanup complete");
    }
    private void UpdateConnectionStatus()
    {
        if (PeerCount < 2)
        {
            StatusMessage = "Esperando a otro compañero...";
            StatusText = "Solo en la sala - Esperando...";
            IsSharingStarted = false;
            
            if (IsSessionActive)
            {
                AddNotification("Listo", "Estás solo en la sala. Comparte tu ID con un compañero para empezar.", "Info", 5);
            }

            // [FIX] If we were hosting, stop the loop so resources are freed for the next session
            if (_streamingCts != null)
            {
                Console.WriteLine("[Host] Stopping screen sharing because the room is empty.");
                _streamingCts.Cancel();
                _streamingCts = null;
            }
            CleanupPreviousSession();
            IsSharingStarted = false; // Reset so we can start again if someone reconnects

            // Auto-dismiss screen lock overlay when peer leaves
            Avalonia.Threading.Dispatcher.UIThread.Post(() => DismissScreenLockOverlay());
        }
        else
        {
            StatusMessage = "Compañero conectado. Iniciando sesión...";
            StatusText = "Conectado";

            // If we are in our own room and someone joined, we are the HOST
            // Use normalized comparison to be safe
            string normalizedRoomId = _currentRoomId?.Replace(" ", "") ?? "";
            string normalizedMyId = MyId?.Replace(" ", "") ?? "";

            if (normalizedRoomId == normalizedMyId && !_isSharingStarted)
            {
                Console.WriteLine("[Host] UpdateConnectionStatus: Host mode detected. Cleaning up & starting share.");
                NxDesk.Infrastructure.Services.FileLogger.Log("Host", $"Host mode detected. _currentRoomId={_currentRoomId}, MyId={MyId}, _isSharingStarted={_isSharingStarted}");
                CleanupPreviousSession();
                IsSharingStarted = true;
                StartScreenSharing();
            }
            // Auto-start sharing only if we were started with --share
            else if (_isAutoShare && !_isSharingStarted)
            {
                Console.WriteLine("[Host] UpdateConnectionStatus: AutoShare detected. Starting share.");
                IsSharingStarted = true;
                StartScreenSharing();
            }
        }
    }

    private bool _isProcessingFrame = false;
    private async Task HandleSignalingMessage(NxDesk.Application.DTOs.SdpMessage message)
    {
        try 
        {
            // Capture the connected peer's identity for the Host UI
            if (_isSharingStarted && !string.IsNullOrEmpty(message.SenderAlias) && message.SenderId != MyId)
            {
                if (string.IsNullOrEmpty(ConnectedPeerAlias) || ConnectedPeerAlias != message.SenderAlias)
                {
                    // Try to resolve IP from discovered peers
                    var discoveredPeer = DiscoveredPeers.FirstOrDefault(p => p.Id == message.SenderId);
                    string peerIp = discoveredPeer?.IpAddress ?? "";
                    
                    Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                    {
                        ConnectedPeerAlias = message.SenderAlias ?? "Desconocido";
                        ConnectedPeerId = message.SenderId ?? "";
                        ConnectedPeerIp = peerIp;
                    });
                }
            }

            if (message.Type == "frame")
        {
            // Drop frame if we are still processing the previous one
            // This prevents "queuing lag" where the receiver falls behind
            if (_isProcessingFrame) return;
            _isProcessingFrame = true;

            try
            {
                NxDesk.Application.DTOs.VideoFrameDto? frame = null;
                
                if (message.ByteData != null && message.Metadata != null)
                {
                    // Use binary data + metadata (Efficient)
                    int width = int.Parse(message.Metadata["Width"]);
                    int height = int.Parse(message.Metadata["Height"]);
                    bool isCompressed = message.Metadata.ContainsKey("IsCompressed") && bool.Parse(message.Metadata["IsCompressed"]);
                    _lastFrameWasCompressed = isCompressed;
                    _lastFrameWidth = width;
                    _lastFrameHeight = height;
                    frame = new NxDesk.Application.DTOs.VideoFrameDto(message.ByteData, width, height, isCompressed: isCompressed);
                }
                else if (!string.IsNullOrEmpty(message.Payload))
                {
                    // Fallback to JSON payload (Legacy)
                    frame = System.Text.Json.JsonSerializer.Deserialize<NxDesk.Application.DTOs.VideoFrameDto>(message.Payload);
                }

                if (frame != null)
                {
                    if (_frameCount % 50 == 0 && message.Metadata != null && message.Metadata.ContainsKey("Timestamp"))
                    {
                        var sentTicks = long.Parse(message.Metadata["Timestamp"]);
                        var latency = (DateTime.UtcNow.Ticks - sentTicks) / 10000;
                        Console.WriteLine($"[Signaling] Frame received: {frame.Width}x{frame.Height}. Latency: {latency}ms");
                    }
                    
                    if (_frameCount % 50 == 0)
                        Console.WriteLine($"[Receiver] Frame received. Invoking UI event.");
                        
                    OnFrameReceived?.Invoke(frame);
                    
                    // Push frame to recording pipeline if active
                    if (IsRecording)
                    {
                        _ = _recordingService.PushFrameAsync(frame);
                    }
                    
                    UpdateFps();
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Signaling] Error decoding frame: {ex.Message}");
            }
            finally
            {
                _isProcessingFrame = false;
            }
        }
        else if (message.Type == "screen_list")
        {
            Console.WriteLine($"[Signaling] Screen list received from {message.SenderId}");
            try
            {
                var screens = message.GetPayload<NxDesk.Application.DTOs.ScreenInfoDto[]>();
                if (screens != null)
                {
                    _ = Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
                    {
                        RemoteScreens.Clear();
                        foreach (var s in screens)
                        {
                            RemoteScreens.Add(s);
                        }

                        // Set the first screen as current if none is selected
                        if (CurrentRemoteScreen == null && screens.Length > 0)
                        {
                            CurrentRemoteScreen = screens[0];
                        }
                    });
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Signaling] Error parsing screen list: {ex.Message}");
            }
        }
        else if (message.Type == "switch_screen")
        {
            try
            {
                var screen = message.GetPayload<NxDesk.Application.DTOs.ScreenInfoDto>();
                if (screen != null && _isSharingStarted)
                {
                    Console.WriteLine($"[Host] Switching capture to: {screen.Name} ({screen.Width}x{screen.Height} @ {screen.X},{screen.Y})");
                    // Stop current stream
                    _streamingCts?.Cancel();
                    _captureService.Dispose();

                    // Wait a moment for tasks to cancel
                    await Task.Delay(500);

                    // Re-start sharing with specific monitor coordinates
                    _streamingCts = new System.Threading.CancellationTokenSource();
                    
                    // Update input service with new screen bounds
                    _ = Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
                    {
                        _inputService.SetScreenBounds(screen.X, screen.Y, screen.Width, screen.Height);
                    });

                    _ = Task.Run(async () => 
                    {
                        await StreamingLoop(_streamingCts.Token, screen.X, screen.Y, screen.Width, screen.Height, screen.Index);
                    });
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Signaling] Error switching screen: {ex.Message}");
            }
        }
        else if (message.Type == "input_event")
        {
            var inputEvent = message.GetPayload<NxDesk.Application.DTOs.InputEventDto>();
            if (inputEvent != null && _inputService != null && _isSharingStarted)
            {
                await _inputService.InjectEventAsync(inputEvent);
            }
        }
        else if (message.Type == "terminal_input")
        {
            // HOST side: received command from the Receiver → write to local shell
            Console.WriteLine($"[Terminal] [HOST] Received terminal_input. Raw Payload Len={message.Payload?.Length ?? 0}");
            var termData = message.GetPayload<NxDesk.Application.DTOs.TerminalDataDto>();
            Console.WriteLine($"[Terminal] [HOST] Parsed TerminalDataDto: Data='{termData?.Data}', Type={termData?.Type}");
            NxDesk.Infrastructure.Services.FileLogger.Log("Terminal", $"[HOST] Received terminal_input: Data='{termData?.Data}', IsRunning={_terminalService.IsRunning}");
            
            if (termData != null && !string.IsNullOrEmpty(termData.Data))
            {
                // DEBUG ECHO: Send immediate confirmation back to Receiver to verify signaling round-trip
                var echoDto = new NxDesk.Application.DTOs.TerminalDataDto($"[System] Comando recibido en Host: {termData.Data}", NxDesk.Application.DTOs.TerminalDataType.Output);
                var echoMsg = new NxDesk.Application.DTOs.SdpMessage { Type = "terminal_output", SenderId = MyId, SenderAlias = MyAlias };
                echoMsg.SetPayload(echoDto);
                _ = _signalingService.RelayMessageAsync(echoMsg);
                Console.WriteLine("[Terminal] [HOST] Sent Debug Echo back to Receiver.");

                await StartTerminalIfNeeded();

                if (_terminalService.IsRunning)
                {
                    Console.WriteLine($"[Terminal] [HOST] Writing to shell: '{termData.Data}'");
                    await _terminalService.WriteInputAsync(termData.Data);
                }
                else
                {
                    Console.WriteLine("[Terminal] [HOST] FAILED to start shell.");
                }
            }
        }
        else if (message.Type == "terminal_output")
        {
            // RECEIVER side: received output from Host → show in terminal panel
            Console.WriteLine($"[Terminal] [RECEIVER] Received terminal_output. Raw Payload Len={message.Payload?.Length ?? 0}");
            var termData = message.GetPayload<NxDesk.Application.DTOs.TerminalDataDto>();
            NxDesk.Infrastructure.Services.FileLogger.Log("Terminal", $"[RECEIVER] Received terminal_output: Length={termData?.Data?.Length ?? 0}");
            
            if (termData != null && !string.IsNullOrEmpty(termData.Data))
            {
                Console.WriteLine($"[Terminal] [RECEIVER] Processing output: '{termData.Data.Substring(0, Math.Min(termData.Data.Length, 20))}...'");
                _ = Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() =>
                {
                    // Clean carriage returns 
                    string clean = termData.Data.Replace("\r", "");
                    
                    // Split by newline if multiple lines
                    var lines = clean.Split('\n');
                    foreach(var line in lines)
                    {
                        if (string.IsNullOrEmpty(line) && lines.Length > 1) continue;
                        TerminalLines.Add(line);
                    }
                    
                    while (TerminalLines.Count > 1000)
                        TerminalLines.RemoveAt(0);

                    FullTerminalOutput = string.Join(Environment.NewLine, TerminalLines);
                    OnTerminalOutputReceived?.Invoke();
                });
            }
        }
        else if (message.Type == "remote_folder_picker_request")
        {
            var request = message.GetPayload<NxDesk.Application.DTOs.RemoteFolderRequest>();
            if (request != null && RequestFolderPicker != null)
            {
                // Host side: open its folder picker and send back path
                // MUST run on UI thread
                var path = await Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(async () => await RequestFolderPicker());
                
                var response = new RemoteFolderResponse 
                { 
                    RequestId = request.RequestId, 
                    SelectedPath = path ?? "", 
                    Cancelled = string.IsNullOrEmpty(path) 
                };
                var respMsg = new SdpMessage { Type = "remote_folder_picker_response", SenderId = MyId, SenderAlias = MyAlias };
                respMsg.SetPayload(response);
                await _signalingService.RelayMessageAsync(respMsg);
            }
        }
        else if (message.Type == "remote_folder_picker_response")
        {
            var response = message.GetPayload<NxDesk.Application.DTOs.RemoteFolderResponse>();
            if (response != null && _pendingRemoteFolderRequests.TryRemove(response.RequestId, out var tcs))
            {
                tcs.SetResult(response);
            }
        }
        else if (message.Type == "list_directory_request")
        {
            var request = message.GetPayload<NxDesk.Application.DTOs.ListDirectoryRequest>();
            if (request != null)
            {
                var result = await _fileTransferService.ListDirectoryAsync(request.Path);
                result.RequestId = request.RequestId;
                
                var respMsg = new SdpMessage { Type = "list_directory_response", SenderId = MyId, SenderAlias = MyAlias };
                respMsg.SetPayload(result);
                await _signalingService.RelayMessageAsync(respMsg);
            }
        }
        else if (message.Type == "list_directory_response")
        {
            var response = message.GetPayload<NxDesk.Application.DTOs.ListDirectoryResponse>();
            if (response != null && _pendingListDirRequests.TryRemove(response.RequestId, out var tcs))
            {
                tcs.SetResult(response);
            }
        }
        else if (message.Type == "kick_peer")
        {
            Console.WriteLine("[Receiver] Received kick_peer from Host.");
            Avalonia.Threading.Dispatcher.UIThread.Post(() => 
            {
                AddNotification("Sesión finalizada", "El anfitrión ha finalizado la sesión compartida.", "Warning", 5);
                DisconnectCommand.Execute(null);
            });
        }
        else if (message.Type == "peer_hello")
        {
            // The connecting peer tells us about their admin status
            Console.WriteLine($"[Host] Received peer_hello from {message.SenderAlias} (SenderId={message.SenderId})");
            try
            {
                var helloData = System.Text.Json.JsonSerializer.Deserialize<System.Text.Json.JsonElement>(message.Payload ?? "{}");
                bool peerIsAdmin = helloData.TryGetProperty("IsAdmin", out var isAdminProp) && isAdminProp.GetBoolean();
                _connectedPeerIsAdmin = peerIsAdmin;
                Console.WriteLine($"[Host] Connected peer is admin: {peerIsAdmin}");

                if (peerIsAdmin && _hostDisconnectWindow != null)
                {
                    // Admin connected — close the disconnect window, the host cannot kick an admin
                    Console.WriteLine("[Host] Admin connection detected. Closing disconnect window.");
                    Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                    {
                        _hostDisconnectWindow?.ForceClose();
                        _hostDisconnectWindow = null;
                    });
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Host] Error parsing peer_hello: {ex.Message}");
            }
        }
        else if (message.Type == "file_transfer_request")
        {
            var request = message.GetPayload<NxDesk.Application.DTOs.FileTransferRequest>();
            if (request != null)
            {
                Console.WriteLine($"[File] Incoming request: {request.FileName} ({request.TotalSize} bytes)");
                await _fileTransferService.ProcessIncomingRequestAsync(request);
                
                // Auto-accept for now (in a real app, show a dialog)
                var response = new FileTransferResponse { TransferId = request.TransferId, Status = FileTransferStatus.Accepted };
                var respMsg = new SdpMessage { Type = "file_transfer_response", SenderId = MyId, SenderAlias = MyAlias };
                respMsg.SetPayload(response);
                await _signalingService.RelayMessageAsync(respMsg);
                AddNotification("Archivo", $"Recibiendo: {request.FileName}", "Info", 3);
            }
        }
        else if (message.Type == "file_transfer_response")
        {
            var response = message.GetPayload<NxDesk.Application.DTOs.FileTransferResponse>();
            if (response != null && response.Status == FileTransferStatus.Accepted)
            {
                Console.WriteLine($"[File] Transfer accepted: {response.TransferId}. Starting upload...");
                _ = Task.Run(() => StartFileUpload(response.TransferId));
            }
        }
        else if (message.Type == "file_chunk")
        {
            var chunk = message.GetPayload<NxDesk.Application.DTOs.FileChunkDto>();
            if (chunk != null)
            {
                await _fileTransferService.WriteChunkAsync(chunk.TransferId, chunk.Data, chunk.Offset, chunk.IsLast);
                if (chunk.IsLast)
                {
                    AddNotification("Archivo", "Transferencia completada", "Success", 3);
                }
            }
        }
        else if (message.Type == "lock_screen")
        {
            Console.WriteLine("[ScreenLock] Received lock_screen signal from remote admin.");
            Avalonia.Threading.Dispatcher.UIThread.Post(() => ShowScreenLockOverlay());
        }
        else if (message.Type == "unlock_screen")
        {
            Console.WriteLine("[ScreenLock] Received unlock_screen signal from remote admin.");
            Avalonia.Threading.Dispatcher.UIThread.Post(() => DismissScreenLockOverlay());
        }
            else
            {
                Console.WriteLine($"[Signaling] Message: {message.Type} from {message.SenderAlias}");
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Signaling] ERROR in HandleSignalingMessage for {message.Type}: {ex.Message}");
            NxDesk.Infrastructure.Services.FileLogger.Log("Signaling", $"ERROR: {ex.Message}\n{ex.StackTrace}");
        }
    }

    // --- Screen Lock Overlay Helpers ---
    private void ShowScreenLockOverlay()
    {
        if (_screenLockOverlay != null) return; // Already showing

        _screenLockOverlay = new NxDesk.Client.Views.ScreenLockOverlay();
        _screenLockOverlay.Show();
        Console.WriteLine("[ScreenLock] Overlay shown.");
    }

    private void DismissScreenLockOverlay()
    {
        if (_screenLockOverlay == null) return;

        try
        {
            _screenLockOverlay.ForceClose();
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[ScreenLock] Error closing overlay: {ex.Message}");
        }
        _screenLockOverlay = null;
        Console.WriteLine("[ScreenLock] Overlay dismissed.");
    }

    private int _frameCount = 0;
    private DateTime _lastFpsUpdate = DateTime.Now;
    private void UpdateFps()
    {
        _frameCount++;
        var now = DateTime.Now;
        var elapsed = (now - _lastFpsUpdate).TotalSeconds;
        if (elapsed >= 1.0)
        {
            CurrentFps = _frameCount / elapsed;
            _frameCount = 0;
            _lastFpsUpdate = now;
        }
    }

    private void StartDiscovery()
    {
        int signalingPort = NxDesk.Infrastructure.Services.EmbeddedSignalingServer.ActualPort;
        _discoveryService.StartDiscovery(MyId, MyAlias, signalingPort);
        StatusMessage = "Buscando equipos en la red local...";
    }



    [ObservableProperty]
    private string _targetId = "";

    [RelayCommand]
    private void ShowConnectionDialog(PeerIdentity peer)
    {
        if (peer == null) return;
        _pendingPeerId = peer.Id;
        PendingPeerAlias = peer.Alias;
        IsControlModeSelected = IsAdmin; // Only admins can select manipulation by default
        IsConnectionDialogVisible = true;
    }

    [RelayCommand]
    private async Task ConfirmConnection()
    {
        IsConnectionDialogVisible = false;
        IsRemoteControlEnabled = IsControlModeSelected;
        await ConnectToId(_pendingPeerId, PendingPeerAlias);
    }

    [RelayCommand]
    private void CancelConnectionDialog()
    {
        IsConnectionDialogVisible = false;
        _pendingPeerId = string.Empty;
        PendingPeerAlias = string.Empty;
    }

    [RelayCommand]
    private void ToggleTerminalExpanded()
    {
        IsTerminalExpanded = !IsTerminalExpanded;
    }

    [RelayCommand]
    private async Task ConnectToPeer(PeerIdentity peer)
    {
        await ConnectToId(peer.Id, peer.Alias);
    }

    [RelayCommand]
    private async Task SendTerminalInput()
    {
        if (string.IsNullOrEmpty(TerminalInput) || !IsSessionActive) return;

        string input = TerminalInput;
        TerminalInput = string.Empty;

        // Echo local output
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            TerminalLines.Add($"> {input}");
            TerminalLines.Add("[System] Enviando...");
            while (TerminalLines.Count > 1000) TerminalLines.RemoveAt(0);
            FullTerminalOutput = string.Join(Environment.NewLine, TerminalLines);
            OnTerminalOutputReceived?.Invoke();
        });

        // If we are in our own room (HOST), write directly to local shell
        if (_currentRoomId == MyId)
        {
            NxDesk.Infrastructure.Services.FileLogger.Log("Terminal", $"[HOST-LOCAL] SendTerminalInput: '{input}', IsRunning={_terminalService.IsRunning}");
            if (_terminalService.IsRunning)
            {
                await _terminalService.WriteInputAsync(input);
            }
            else
            {
                NxDesk.Infrastructure.Services.FileLogger.Log("Terminal", "[HOST-LOCAL] Terminal not running, attempting auto-start...");
                await StartTerminalIfNeeded();
                if (_terminalService.IsRunning)
                {
                    await _terminalService.WriteInputAsync(input);
                }
                else
                {
                    Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                    {
                        TerminalLines.Add("[Error] No se pudo iniciar el terminal.");
                        FullTerminalOutput = string.Join(Environment.NewLine, TerminalLines);
                        OnTerminalOutputReceived?.Invoke();
                    });
                }
            }
        }
        else
        {
            // RECEIVER: send terminal_input to Host via signaling
            NxDesk.Infrastructure.Services.FileLogger.Log("Terminal", $"[RECEIVER] Sending terminal_input via signaling: '{input}'");
            var dto = new NxDesk.Application.DTOs.TerminalDataDto(input, NxDesk.Application.DTOs.TerminalDataType.Input);
            var msg = new NxDesk.Application.DTOs.SdpMessage
            {
                Type = "terminal_input",
                SenderId = MyId,
                SenderAlias = MyAlias
            };
            msg.SetPayload(dto);
            Console.WriteLine($"[Terminal] [RECEIVER] Relaying terminal_input to Signaling: '{input}'");
            await _signalingService.RelayMessageAsync(msg);
        }
    }

    [RelayCommand]
    private async Task ConnectById()
    {
        if (!string.IsNullOrEmpty(TargetId))
        {
            await ConnectToId(TargetId, $"Equipo {TargetId}");
        }
    }

    private void ProcessArgs(string[] args)
    {
        Console.WriteLine($"[Debug] Received Args: {string.Join(" ", args)}");
        bool hasConnectArg = false;
        
        if (args.Contains("--share"))
        {
            _isAutoShare = true;
            // Removed ID override here to favor persistent ID or --id arg
            Console.WriteLine("[Auto] Mode: SENDER (Sharing started)");
        }
        
        if (args.Contains("--connect"))
        {
            var index = Array.IndexOf(args, "--connect");
            if (index >= 0 && index < args.Length - 1)
            {
                string target = args[index + 1];
                hasConnectArg = true;
                _isAutoConnect = true;
                TargetId = target;
                Console.WriteLine($"[Auto] Mode: RECEIVER (Connecting to: {target})");
                
                // Auto connect
                _ = ConnectToId(target, "Auto Receiver");
            }
        }

        if (!hasConnectArg)
        {
            // If not connecting to someone, join our own room to be reachable by default
            _currentRoomId = MyId;
            Console.WriteLine($"[Auto] Mode: LISTEN (Reachable at: {MyId})");
            _ = _signalingService.ConnectAsync(MyId!);
        }
    }

    public async Task ConnectToId(string id, string? alias = null, string? ip = null, int port = 5000)
    {
        TerminalLines.Clear();
        FullTerminalOutput = string.Empty;
        if (string.IsNullOrEmpty(id)) return;

        // Support manual IP fallback via "ID @ IP" or "ID@IP"
        string targetId = id;
        string? manualIp = null;
        if (id.Contains('@'))
        {
            var parts = id.Split('@');
            targetId = parts[0].Trim();
            manualIp = parts[1].Trim();
            NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Manual IP overwrite detected: {manualIp} for ID {targetId}");
        }

        // Normalize: remove spaces for comparison
        string normalizedId = targetId.Replace(" ", "");
        string normalizedMyId = MyId?.Replace(" ", "") ?? "";

        // Prevent connecting to self (only in manual UI mode, not --connect auto mode)
        if (!_isAutoConnect && normalizedId == normalizedMyId)
        {
            NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Blocked: connecting to self (id={targetId}, MyId={MyId})");
            AddNotification("Aviso", "No puedes conectar a tu propio equipo. Usa el ID de otro compañero.", "Info", 5);
            return;
        }
        NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"ConnectToId: targetId={targetId}, alias={alias}, MyId={MyId}, manualIp={manualIp ?? "N/A"}");

        IsConnecting = true;
        StatusMessage = $"Conectando a {alias ?? targetId}...";
        try
        {
            // [FIX] Removed the early-return "if (_currentRoomId == id)" because we might need to 
            // switch server URLs even if the room ID stays the same (e.g. going from local to remote)
            
            _currentRoomId = normalizedId;

            // Resolve peer IP: Manual > Discovery > Default(localhost)
            if (!string.IsNullOrEmpty(manualIp))
            {
                string remoteUrl = $"http://{manualIp}:{port}/signalinghub";
                NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Using MANUAL IP: {manualIp} → {remoteUrl}");
                await _signalingService.SetServerUrl(remoteUrl);
            }
            else
            {
                // Auto-resolve the peer's IP from Discovery
                // [FIX] Wait for discovery if peer not yet found (especially for --connect at startup)
                var discoveredPeer = DiscoveredPeers.FirstOrDefault(p => 
                    p.Id.Replace(" ", "") == normalizedId);
                
                if (discoveredPeer == null && (_isAutoConnect || !string.IsNullOrEmpty(targetId)))
                {
                    NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Peer {targetId} not in discovery. Waiting up to 5s...");
                    StatusMessage = $"Buscando a {alias ?? targetId} en la red...";
                    
                    for (int i = 0; i < 10; i++) // 10 * 500ms = 5s
                    {
                        await Task.Delay(500);
                        discoveredPeer = DiscoveredPeers.FirstOrDefault(p => 
                            p.Id.Replace(" ", "") == normalizedId);
                        if (discoveredPeer != null) break;
                    }
                }

                if (discoveredPeer != null && !string.IsNullOrEmpty(discoveredPeer.IpAddress))
                {
                    int targetPort = discoveredPeer.Port > 0 ? discoveredPeer.Port : 5000;
                    string remoteUrl = $"http://{discoveredPeer.IpAddress}:{targetPort}/signalinghub";
                    NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"RESOLVED peer IP: {discoveredPeer.IpAddress} → switching to {remoteUrl}");
                    await _signalingService.SetServerUrl(remoteUrl);

                    // Use the peer's original ID (with spaces) as the room ID so both sides match
                    _currentRoomId = discoveredPeer.Id;
                }
                else
                {
                    // [SECURITY/FIX] If the target ID is NOT our own, and we haven't found an IP,
                    // we MUST NOT connect to localhost as a fallback.
                    if (normalizedId != normalizedMyId)
                    {
                        NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"ERROR: Cannot resolve IP for remote peer {targetId}. Aborting.");
                        throw new Exception($"No se pudo encontrar al equipo {targetId}. Asegúrate de que está en la misma red y que el ID es correcto.");
                    }

                    NxDesk.Infrastructure.Services.FileLogger.Log("Connect", "Connecting to local server (self-connect or host mode)");
                }
            }

            NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Calling ConnectAsync with roomId='{_currentRoomId}'");
            bool connected = await _signalingService.ConnectAsync(_currentRoomId);
            if (!connected)
            {
                throw new Exception("No se pudo establecer conexión con el servidor de señales");
            }

            NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"SUCCESS: Connected to room '{_currentRoomId}'");
            IsSessionActive = true;
            IsTerminalVisible = IsAdmin;
            IsConnecting = false;
            UpdateConnectionStatus();

            // Send peer_hello to the host so they know our identity and admin status
            var helloMsg = new NxDesk.Application.DTOs.SdpMessage
            {
                Type = "peer_hello",
                SenderId = MyId,
                SenderAlias = MyAlias,
                Payload = System.Text.Json.JsonSerializer.Serialize(new { IsAdmin = IsAdmin })
            };
            await _signalingService.RelayMessageAsync(helloMsg);
            Console.WriteLine($"[Client] Sent peer_hello (IsAdmin={IsAdmin})");
        }
        catch (Exception ex)
        {
            NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"FAILED: {ex.Message}");
            IsConnecting = false;
            StatusMessage = "Error de conexión";
            AddNotification("Error", ex.Message, "Error", 7);
        }
    }

    private void StartScreenSharing()
    {
        Console.WriteLine("[Host] StartScreenSharing: Checking CTS.");
        // [FIX] Cancel stale CTS instead of silently aborting — this was the root cause
        // of "Esperando señal del Host" on Linux when a previous session left a non-null CTS.
        if (_streamingCts != null)
        {
            Console.WriteLine("[Host] StartScreenSharing: Stale CTS found, cancelling it.");
            try { _streamingCts.Cancel(); } catch { }
            _streamingCts = null;
        }
        
        _streamingCts = new System.Threading.CancellationTokenSource();

        Console.WriteLine("[Host] StartScreenSharing: Dispatching to UI thread for screen info.");
        // Pass physical screen dimensions down to infrastructure for accurate absolute pointer motion
        Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
        {
            try 
            {
                if (Avalonia.Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop)
                {
                    var screens = desktop.MainWindow?.Screens?.All;
                    if (screens != null)
                    {
                        Console.WriteLine($"[Host] Screen list found. Count: {screens.Count}");
                        // Set primary screen boundaries for injection scaling
                        var primary = screens.FirstOrDefault(s => s.IsPrimary) ?? screens.First();
                        _inputService.SetScreenBounds(primary.Bounds.X, primary.Bounds.Y, primary.Bounds.Width, primary.Bounds.Height);

                        // Broadcast all screens to the receiver
                        var screenDtos = screens.Select((s, index) => new NxDesk.Application.DTOs.ScreenInfoDto(
                            index,
                            s.IsPrimary ? "Pantalla Principal" : $"Pantalla {index + 1}",
                            s.Bounds.Width,
                            s.Bounds.Height,
                            s.Bounds.X,
                            s.Bounds.Y
                        )).ToArray();

                        var listMessage = new NxDesk.Application.DTOs.SdpMessage
                        {
                            Type = "screen_list",
                            SenderId = MyId,
                            SenderAlias = MyAlias
                        };
                        listMessage.SetPayload(screenDtos);

                        _ = _signalingService.RelayMessageAsync(listMessage);
                        Console.WriteLine("[Host] screen_list relayed.");
                    }
                    else
                    {
                        Console.WriteLine("[Host] Error: desktop.MainWindow?.Screens?.All is NULL");
                    }
                }
                else
                {
                    Console.WriteLine("[Host] Error: Not classic desktop lifetime.");
                }
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Host] StartScreenSharing exception UI: {ex.Message}");
            }
        });

        Console.WriteLine("[Host] StartScreenSharing: Starting Task.Run for streaming loop.");
        NxDesk.Infrastructure.Services.FileLogger.Log("Host", "StartScreenSharing: Launching streaming Task.Run");
        Task.Run(async () => 
        {
            NxDesk.Infrastructure.Services.FileLogger.Log("Host", "Task.Run ENTERED — preparing capture");
            StatusMessage = "Iniciando servicio de control remoto y captura...";
            
            // Do not block screen sharing on the remote control dialog
            _ = Task.Run(async () => 
            {
                try 
                {
                    await _inputService.InitializeAsync();
                } 
                catch (Exception ex) 
                {
                    Console.WriteLine($"[Host] Input Init Error: {ex.Message}");
                }
            });

            /* [LAZY TERMINAL] Defer terminal startup until actual input is received to avoid TTY issues on Linux
            _ = Task.Run(async () =>
            {
                await StartTerminalIfNeeded();
            });
            */

            int w = 0, h = 0;
            NxDesk.Infrastructure.Services.FileLogger.Log("Host", "Requesting screen dimensions from UI thread...");
            await Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() => 
            {
                if (Avalonia.Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop)
                {
                    var screens = desktop.MainWindow?.Screens?.All;
                    if (screens != null)
                    {
                        var primary = screens.FirstOrDefault(s => s.IsPrimary) ?? screens.First();
                        w = (int)primary.Bounds.Width;
                        h = (int)primary.Bounds.Height;
                    }
                }
            });

            NxDesk.Infrastructure.Services.FileLogger.Log("Host", $"Screen dimensions obtained: w={w}, h={h}. Starting StreamingLoop...");
            await StreamingLoop(_streamingCts.Token, 0, 0, w, h);
        });
    }

    private async Task StreamingLoop(System.Threading.CancellationToken token, int x = 0, int y = 0, int width = 0, int height = 0, int index = 0)
    {
        try
        {
            NxDesk.Infrastructure.Services.FileLogger.Log("Streaming", $"StreamingLoop START: x={x}, y={y}, w={width}, h={height}, idx={index}");
            StatusMessage = "Iniciando captura...";
            bool initSuccess = _captureService.Initialize(x, y, width, height, index);
            NxDesk.Infrastructure.Services.FileLogger.Log("Streaming", $"CaptureService.Initialize returned: {initSuccess}");
            Console.WriteLine($"[Streaming] CaptureService.Initialize returned: {initSuccess}");
            
            if (!initSuccess)
            {
                NxDesk.Infrastructure.Services.FileLogger.Log("Streaming", "FAILED: CaptureService.Initialize returned false — aborting StreamingLoop");
                Console.WriteLine("[Streaming] FAILED: CaptureService.Initialize returned false — aborting StreamingLoop");
                StatusMessage = "Error: No se pudo iniciar la captura de pantalla.";
                AddNotification("Error", "La captura de pantalla falló. Asegúrate de tener permisos o soporte para X11/DXGI.", "Error", 10);
                return;
            }

            StatusMessage = "Compartiendo pantalla...";
            _lastFpsUpdate = DateTime.Now;
            _frameCount = 0;

            while (!token.IsCancellationRequested)
            {
                _frameCount++;
                var captureStart = DateTime.Now;
                var frame = _captureService.CaptureFrame();
                if (frame != null)
                {
                    var captureTime = (DateTime.Now - captureStart).TotalMilliseconds;
                    
                    var message = new NxDesk.Application.DTOs.SdpMessage
                    {
                        Type = "frame",
                        ByteData = frame.Pixels,
                        SenderId = MyId,
                        SenderAlias = MyAlias,
                        Metadata = new System.Collections.Generic.Dictionary<string, string>
                        {
                            { "Width", frame.Width.ToString() },
                            { "Height", frame.Height.ToString() },
                            { "IsCompressed", frame.IsCompressed.ToString() },
                            { "Timestamp", DateTime.UtcNow.Ticks.ToString() }
                        }
                    };

                    var sendStart = DateTime.Now;
                    await _signalingService.RelayMessageAsync(message);
                    var sendTime = (DateTime.Now - sendStart).TotalMilliseconds;
                    
                    if (_frameCount % 50 == 0)
                        Console.WriteLine($"[Streaming] Frame {_frameCount}: Capture={captureTime:F1}ms, Send={sendTime:F1}ms, Size={frame.Pixels.Length / 1024}KB");
                }
                
                // No artificial delay. CaptureFrame blocks on the GStreamer stream,
                // and GStreamer's videorate handles the FPS control (20 FPS).
                // This ensures we are always synchronized with the encoder.
                await Task.Yield(); 
            }
        }
        catch (OperationCanceledException) { }
        catch (Exception ex)
        {
            StatusMessage = $"Error en streaming: {ex.Message}";
            System.Diagnostics.Debug.WriteLine($"Streaming error: {ex.Message}");
        }
        finally
        {
            _captureService.Dispose();
            StatusMessage = "Captura finalizada";
        }
    }

    private void AddNotification(string title, string message, string type, int durationSeconds)
    {
        var notification = new NotificationItemViewModel(title, message, type, durationSeconds);
        Notifications.Add(notification);
        
        // Auto-remove after duration
        Task.Delay(TimeSpan.FromSeconds(durationSeconds)).ContinueWith(_ =>
        {
            Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() =>
            {
                Notifications.Remove(notification);
            });
        });
    }

    [RelayCommand]
    public async Task Disconnect()
    {
        TerminalLines.Clear();
        FullTerminalOutput = string.Empty;
        _streamingCts?.Cancel();
        _streamingCts = null;
        
        await _signalingService.LeaveRoomAsync();
        
        IsSessionActive = false;
        IsTerminalVisible = false;
        IsScreenLocked = false;
        StatusText = "Desconectado";
        StatusMessage = "Sesión finalizada";
        _currentRoomId = null;
        OnSessionClosed?.Invoke();

        // Dismiss any screen lock overlay
        DismissScreenLockOverlay();

        // Comprehensive cleanup
        CleanupPreviousSession();

        if (_inputService is IDisposable disposable)
        {
            disposable.Dispose();
        }

        // [FIX] After disconnecting from a remote session, re-join our OWN room 
        // to be reachable again for incoming connections.
        if (!string.IsNullOrEmpty(MyId))
        {
            var localUrl = $"http://localhost:{NxDesk.Infrastructure.Services.EmbeddedSignalingServer.ActualPort}/signalinghub";
            NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Resetting signaling URL to local: {localUrl}");
            await _signalingService.SetServerUrl(localUrl);

            NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Re-joining local room after disconnect: {MyId}");
            _currentRoomId = MyId.Replace(" ", ""); // Store normalized
            IsSharingStarted = false; // Reset sharing flag for the new local session
            
            // Actually wait for connection or run in background but ensure it's tracked
            _ = Task.Run(async () => {
                try {
                    await _signalingService.ConnectAsync(MyId);
                } catch (Exception ex) {
                    NxDesk.Infrastructure.Services.FileLogger.Log("Connect", $"Failed to re-join local room: {ex.Message}");
                }
            });
        }
    }

    [RelayCommand]
    private async Task HostDisconnect()
    {
        Console.WriteLine("[Host] Initiating Host-side Disconnect (kick clients).");
        
        // Broadcast a kick message to connected clients
        var kickMsg = new NxDesk.Application.DTOs.SdpMessage
        {
            Type = "kick_peer",
            SenderId = MyId,
            SenderAlias = MyAlias
        };
        await _signalingService.RelayMessageAsync(kickMsg);

        // Now locally clear and stop everything just as a standard disconnect
        DisconnectCommand.Execute(null);
        
        AddNotification("Sesión", "Compartición de pantalla finalizada.", "Info", 4);
    }

    [RelayCommand]
    private void ToggleRemoteControl()
    {
        if (!IsAdmin)
        {
            IsRemoteControlEnabled = false;
            return;
        }

        IsRemoteControlEnabled = !IsRemoteControlEnabled;
        if (IsRemoteControlEnabled)
        {
            AddNotification("Control Remoto", "Control de entrada activado.", "Success", 3);
        }
        else
        {
            AddNotification("Control Remoto", "Modo de solo lectura activado.", "Info", 3);
        }
    }

    [RelayCommand]
    private async Task SelectRemoteScreen(NxDesk.Application.DTOs.ScreenInfoDto screen)
    {
        if (screen == null || string.IsNullOrEmpty(_currentRoomId)) return;

        var message = new NxDesk.Application.DTOs.SdpMessage
        {
            Type = "switch_screen",
            SenderId = MyId,
            SenderAlias = MyAlias
        };
        message.SetPayload(screen);

        await _signalingService.RelayMessageAsync(message);
        CurrentRemoteScreen = screen;
        AddNotification("Pantalla", $"Solicitando cambiar a {screen.Name}...", "Info", 3);
    }

    private bool _isTerminalInitialized = false;

    private async Task StartTerminalIfNeeded()
    {
        try
        {
            if (!_isTerminalInitialized)
            {
                _terminalService.OnOutputReceived = async output =>
                {
                    Console.WriteLine($"[Terminal] [HOST] Output captured from shell ({output.Length} chars). Relaying...");
                    // Relay output to receiver via signaling
                    var dto = new NxDesk.Application.DTOs.TerminalDataDto(output, NxDesk.Application.DTOs.TerminalDataType.Output);
                    var msg = new NxDesk.Application.DTOs.SdpMessage
                    {
                        Type = "terminal_output",
                        SenderId = MyId,
                        SenderAlias = MyAlias
                    };
                    msg.SetPayload(dto);
                    await _signalingService.RelayMessageAsync(msg);
                    Console.WriteLine("[Terminal] [HOST] terminal_output relay call completed.");

                    // Also add to local terminal lines (Host can see too)
                    _ = Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() =>
                    {
                        // Clean carriage returns 
                        string clean = output.Replace("\r", "");
                        
                        // Split by newline if multiple lines
                        var lines = clean.Split('\n');
                        foreach(var line in lines)
                        {
                            if (string.IsNullOrEmpty(line) && lines.Length > 1) continue;
                            TerminalLines.Add(line);
                        }
                        
                        while (TerminalLines.Count > 1000)
                            TerminalLines.RemoveAt(0);
                            
                        FullTerminalOutput = string.Join(Environment.NewLine, TerminalLines);
                        OnTerminalOutputReceived?.Invoke();
                    });
                };
                _isTerminalInitialized = true;
                Console.WriteLine("[Terminal] [HOST] Signaling handler attached to RemoteTerminalService.");
            }

            if (_terminalService.IsRunning) 
            {
                Console.WriteLine("[Terminal] [HOST] Terminal is already running.");
                return;
            }

            Console.WriteLine("[Terminal] [HOST] Starting RemoteTerminalService shell...");
            await _terminalService.StartAsync();
            Avalonia.Threading.Dispatcher.UIThread.Post(() => IsTerminalVisible = IsAdmin);
            NxDesk.Infrastructure.Services.FileLogger.Log("Terminal", "[HOST] Remote terminal started via StartTerminalIfNeeded.");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Terminal] [HOST] Error in StartTerminalIfNeeded: {ex.Message}");
            NxDesk.Infrastructure.Services.FileLogger.Log("Terminal", $"[HOST] Terminal Init Error: {ex.Message}");
        }
    }

    public async Task SendInput(NxDesk.Application.DTOs.InputEventDto inputEvent)
    {
        if (IsSessionActive && IsRemoteControlEnabled && !string.IsNullOrEmpty(_currentRoomId))
        {
            await _signalingService.SendInputAsync(inputEvent);
        }
        else
        {
            NxDesk.Infrastructure.Services.FileLogger.Log("Sender", $"SendInput BLOCKED: IsSessionActive={IsSessionActive}, IsRemoteControl={IsRemoteControlEnabled}, Room={_currentRoomId}");
        }
    }

    [RelayCommand]
    public async Task ToggleRecording()
    {
        if (IsRecording)
        {
            // Stop recording
            _recordingTimer?.Stop();
            _recordingTimer?.Dispose();
            _recordingTimer = null;

            await _recordingService.StopAsync();
            IsRecording = false;
            Console.WriteLine($"[Recording] Stopped. File: {RecordingPath}");
            AddNotification("Grabación", $"Grabación guardada en: {RecordingPath}", "Info", 5);
            RecordingDuration = "";
        }
        else
        {
            // Start recording
            string recordingsDir;
            
            // Get the system's Videos folder path
            var videosPath = Environment.GetFolderPath(Environment.SpecialFolder.MyVideos);
            
            // Fallback for systems where MyVideos might be empty (like some Linux distros)
            if (string.IsNullOrEmpty(videosPath))
            {
                var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
                videosPath = Path.Combine(home, "Videos");
            }
            
            recordingsDir = Path.Combine(videosPath, "NxDesk_Recordings");

            try
            {
                Directory.CreateDirectory(recordingsDir);
            }
            catch (Exception ex)
            {
                AddNotification("Error", $"No se pudo crear carpeta de grabaciones: {ex.Message}", "Error", 5);
                return;
            }

            var timestamp = DateTime.Now.ToString("yyyy-MM-dd_HH-mm-ss");
            var outputPath = Path.Combine(recordingsDir, $"NxDesk_{timestamp}.mp4");
            RecordingPath = outputPath;

            // Determine if we are using RAW frames (based on last received frame)
            bool isRaw = !_lastFrameWasCompressed;
            
            // Explicitly force MJPEG if we know frames are compressed, otherwise default to raw
            // This is more robust than relying on local OS type.
            
            // Use actual frame dimensions to ensure native quality (no blurry scaling)
            await _recordingService.StartAsync(outputPath, _lastFrameWidth, _lastFrameHeight, fps: 8, isRaw: isRaw);
            IsRecording = true;
            _recordingStartTime = DateTime.Now;
            RecordingDuration = "00:00";

            // Start the timer to update recording duration every second
            _recordingTimer = new System.Timers.Timer(1000);
            _recordingTimer.Elapsed += (s, e) =>
            {
                var elapsed = DateTime.Now - _recordingStartTime;
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    RecordingDuration = elapsed.ToString(@"mm\:ss");
                });
            };
            _recordingTimer.Start();

            AddNotification("Grabación", $"Grabando en: {outputPath}", "Info", 5);
        }
    }

    [RelayCommand]
    public async Task SendFile()
    {
        if (RequestFilePicker == null) return;
        var path = await RequestFilePicker();
        if (string.IsNullOrEmpty(path)) return;

        // Show Remote File Explorer to select destination
        var targetDir = await ShowRemoteExplorerAsync();
        if (string.IsNullOrEmpty(targetDir))
        {
            AddNotification("Cancelado", "Envío cancelado: no se seleccionó destino.", "Warning", 3);
            return;
        }

        try
        {
            var fileName = Path.GetFileName(path);
            var size = new FileInfo(path).Length;
            var transferId = await _fileTransferService.InitiateFileTransferAsync(path, targetDir);

            var request = new FileTransferRequest 
            { 
                TransferId = transferId, 
                FileName = fileName, 
                TotalSize = size, 
                IsDirectory = false,
                TargetDirectory = targetDir
            };
            
            var msg = new SdpMessage { Type = "file_transfer_request", SenderId = MyId, SenderAlias = MyAlias };
            msg.SetPayload(request);
            await _signalingService.RelayMessageAsync(msg);
            AddNotification("Archivo", $"Enviando {fileName}...", "Info", 3);
        }
        catch (Exception ex)
        {
            AddNotification("Error", $"Error al iniciar envío: {ex.Message}", "Error", 5);
        }
    }

    [RelayCommand]
    public async Task SendFolder()
    {
        if (RequestFolderPicker == null) return;
        var path = await RequestFolderPicker();
        if (string.IsNullOrEmpty(path)) return;

        // Show Remote File Explorer to select destination
        var targetDir = await ShowRemoteExplorerAsync();
        if (string.IsNullOrEmpty(targetDir))
        {
            AddNotification("Cancelado", "Envío cancelado: no se seleccionó destino.", "Warning", 3);
            return;
        }

        try
        {
            var folderName = Path.GetFileName(path);
            var parentDir = Path.GetDirectoryName(path) ?? path;
            var files = Directory.GetFiles(path, "*", SearchOption.AllDirectories);
            AddNotification("Carpeta", $"Iniciando envío de {files.Length} archivos a {targetDir}...", "Info", 3);

            foreach (var file in files)
            {
                // Compute relative path from the PARENT of the selected folder
                // so the folder name itself is preserved in the structure
                var relPath = Path.GetRelativePath(parentDir, Path.GetDirectoryName(file) ?? "");
                var fileName = Path.GetFileName(file);
                var size = new FileInfo(file).Length;
                var transferId = await _fileTransferService.InitiateFileTransferAsync(file, targetDir);

                var request = new FileTransferRequest 
                { 
                    TransferId = transferId, 
                    FileName = fileName, 
                    TotalSize = size, 
                    IsDirectory = false,
                    RelativePath = relPath == "." ? folderName : relPath,
                    TargetDirectory = targetDir
                };
                
                var msg = new SdpMessage { Type = "file_transfer_request", SenderId = MyId, SenderAlias = MyAlias };
                msg.SetPayload(request);
                Console.WriteLine($"[File] Folder item: RelativePath='{request.RelativePath}', FileName='{request.FileName}', TargetDir='{request.TargetDirectory}'");
                await _signalingService.RelayMessageAsync(msg);
            }
        }
        catch (Exception ex)
        {
            AddNotification("Error", $"Error al iniciar envío de carpeta: {ex.Message}", "Error", 5);
        }
    }

    private async Task StartFileUpload(string transferId)
    {
        const int chunkSize = 64 * 1024; // 64KB chunks
        long offset = 0;
        bool isLast = false;

        try
        {
            while (!isLast)
            {
                var chunk = await _fileTransferService.GetFileChunkAsync(transferId, offset, chunkSize);
                isLast = chunk.Length < chunkSize;
                
                var chunkDto = new FileChunkDto
                {
                    TransferId = transferId,
                    Data = chunk,
                    Offset = offset,
                    IsLast = isLast
                };

                var msg = new SdpMessage { Type = "file_chunk", SenderId = MyId, SenderAlias = MyAlias };
                msg.SetPayload(chunkDto);
                await _signalingService.RelayMessageAsync(msg);

                offset += chunk.Length;
                
                // Small delay to prevent overwhelming the connection
                await Task.Delay(5);
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[File] Error during upload: {ex.Message}");
        }
    }

    private async Task<string?> RequestRemoteFolderAsync()
    {
        // NO LONGER USED - Replaced by Remote File Explorer
        return null; 
    }

    [RelayCommand]
    public async Task NavigateRemote(DirectoryItemDto? item)
    {
        if (item != null && !item.IsDirectory) return;
        
        string newPath;
        if (item == null) // Parent directory
        {
            newPath = Path.GetDirectoryName(CurrentRemotePath) ?? "";
        }
        else
        {
            newPath = Path.Combine(CurrentRemotePath, item.Name);
        }

        await LoadRemoteDirectoryAsync(newPath);
    }

    private async Task LoadRemoteDirectoryAsync(string path)
    {
        IsRemoteExplorerLoading = true;
        try
        {
            var result = await RequestRemoteDirectoryAsync(path);
            if (result != null)
            {
                if (string.IsNullOrEmpty(result.Error))
                {
                    CurrentRemotePath = result.CurrentPath;
                    RemoteItems.Clear();
                    foreach (var item in result.Items.OrderByDescending(i => i.IsDirectory).ThenBy(i => i.Name))
                    {
                        RemoteItems.Add(item);
                    }
                }
                else
                {
                    AddNotification("Error", $"Error al listar carpeta: {result.Error}", "Error", 5);
                }
            }
        }
        finally
        {
            IsRemoteExplorerLoading = false;
        }
    }

    private async Task<ListDirectoryResponse?> RequestRemoteDirectoryAsync(string path)
    {
        var request = new ListDirectoryRequest { Path = path };
        var tcs = new TaskCompletionSource<ListDirectoryResponse>();
        _pendingListDirRequests[request.RequestId] = tcs;

        var msg = new SdpMessage { Type = "list_directory_request", SenderId = MyId, SenderAlias = MyAlias };
        msg.SetPayload(request);
        await _signalingService.RelayMessageAsync(msg);

        try
        {
            return await tcs.Task.WaitAsync(TimeSpan.FromSeconds(30));
        }
        catch (TimeoutException)
        {
            AddNotification("Error", "Tiempo de espera agotado al listar carpeta remota", "Error", 5);
            return null;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[File] Directory list request failed: {ex.Message}");
            return null;
        }
        finally
        {
            _pendingListDirRequests.TryRemove(request.RequestId, out _);
        }
    }

    [RelayCommand]
    public void CloseRemoteExplorer()
    {
        IsRemoteExplorerVisible = false;
        // Resolve with null so the awaiting command unblocks and treats it as cancelled
        _remoteTargetTcs?.TrySetResult(null);
    }

    [RelayCommand]
    public async Task ConfirmRemoteTarget()
    {
        if (string.IsNullOrEmpty(CurrentRemotePath)) return;
        IsRemoteExplorerVisible = false;
        
        // This will be awaited by the sender
        _remoteTargetTcs?.TrySetResult(CurrentRemotePath);
    }

    private TaskCompletionSource<string?>? _remoteTargetTcs;

    private async Task<string?> ShowRemoteExplorerAsync()
    {
        _remoteTargetTcs = new TaskCompletionSource<string?>();
        IsRemoteExplorerVisible = true;
        
        // Initial load
        await LoadRemoteDirectoryAsync(""); 
        
        return await _remoteTargetTcs.Task;
    }

    partial void OnMyAliasChanged(string value)
    {
        if (_configService != null)
        {
            _configService.Alias = value;
        }
    }
}

public class NotificationItemViewModel : ObservableObject
{
    public string Title { get; }
    public string Message { get; }
    public string Type { get; }
    public int Duration { get; }

    public NotificationItemViewModel(string title, string message, string type, int duration)
    {
        Title = title;
        Message = message;
        Type = type;
        Duration = duration;
    }
}
