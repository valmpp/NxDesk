using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Text;
using Newtonsoft.Json;
using NxDesk.Application.Interfaces;
using NxDesk.Core.Models;

namespace NxDesk.Infrastructure.Services;

public class DiscoveryService : IDiscoveryService, IDisposable
{
    private const int Port = 5001;
    private UdpClient? _listener;
    private UdpClient? _broadcaster;
    private bool _isRunning;
    private string? _myId;
    private string? _myAlias;
    private int _myPort = 5000;
    private PeerStatus _myStatus = PeerStatus.Background;

    public event Action<PeerIdentity>? OnPeerDiscovered;
    // Removed HashSet to allow continuous pulse updates for status/LastSeen tracking.

    public void SetStatus(PeerStatus status)
    {
        _myStatus = status;
        FileLogger.Log("Discovery", $"My status updated to: {status}");
    }

    public void StartDiscovery(string id, string alias, int port)
    {
        if (_isRunning) return;

        _myId = id;
        _myAlias = alias;
        _myPort = port;
        _isRunning = true;

        FileLogger.Log("Discovery", $"Starting for {alias} (ID={id}) on port {Port}");

        // ── LISTENER: separate UdpClient bound to port ──
        try
        {
            _listener = new UdpClient();
            _listener.Client.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
            
            // On Linux/macOS, ReusePort allows multiple processes to bind to the same port
            try { 
                // SO_REUSEPORT is usually 15 on Linux
                _listener.Client.SetSocketOption(SocketOptionLevel.Socket, (SocketOptionName)15, true); 
            } catch { /* Not supported on all OS versions */ }

            _listener.ExclusiveAddressUse = false; // Required on Windows for port reuse
            _listener.Client.Bind(new IPEndPoint(IPAddress.Any, Port));
            _listener.EnableBroadcast = true;

            FileLogger.Log("Discovery", $"Listener STARTED on port {Port}");

            Task.Run(async () =>
            {
                while (_isRunning)
                {
                    try
                    {
                        var result = await _listener.ReceiveAsync();
                        var raw = Encoding.UTF8.GetString(result.Buffer);
                        FileLogger.Log("Discovery", $"RAW UDP received from {result.RemoteEndPoint}: {raw}");

                        var peer = JsonConvert.DeserializeObject<PeerIdentity>(raw);

                        if (peer != null && peer.Id != _myId)
                        {
                            peer.IpAddress = result.RemoteEndPoint.Address.ToString();
                            
                            // Pulse notification (always notify so ViewModel can update LastSeen)
                            FileLogger.Log("Discovery", $"PEER PULSE: {peer.Alias} (ID={peer.Id}) at {peer.IpAddress} - Status: {peer.Status}");
                            OnPeerDiscovered?.Invoke(peer);
                        }
                        else if (peer != null && peer.Id == _myId)
                        {
                            FileLogger.Log("Discovery", $"Ignored own broadcast from {result.RemoteEndPoint}");
                        }
                    }
                    catch (ObjectDisposedException) { break; }
                    catch (Exception ex)
                    {
                        FileLogger.Log("Discovery", $"LISTEN ERROR: {ex.Message}");
                    }
                }
            });
        }
        catch (SocketException ex)
        {
            FileLogger.Log("Discovery", $"FATAL: Could not bind listener to port {Port}: {ex.Message}");
            _isRunning = false;
            return;
        }

        // ── BROADCASTER: separate UdpClient (any port) ──
        try
        {
            _broadcaster = new UdpClient();
            _broadcaster.Client.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
            _broadcaster.ExclusiveAddressUse = false;
            _broadcaster.Client.Bind(new IPEndPoint(IPAddress.Any, 0));
            _broadcaster.EnableBroadcast = true;

            // Log network interfaces
            foreach (var broadcastAddr in GetSubnetBroadcastAddresses())
            {
                FileLogger.Log("Discovery", $"Subnet broadcast target: {broadcastAddr}");
            }

            Task.Run(async () =>
            {
                await Task.Delay(500);

                while (_isRunning)
                {
                    try
                    {
                        var peerPulse = new PeerIdentity 
                        { 
                            Id = _myId!, 
                            Alias = _myAlias!, 
                            Port = _myPort,
                            Status = _myStatus 
                        };
                        var message = JsonConvert.SerializeObject(peerPulse);
                        var data = Encoding.UTF8.GetBytes(message);

                        // General broadcast
                        await _broadcaster.SendAsync(data, data.Length, new IPEndPoint(IPAddress.Broadcast, Port));

                        // Subnet-directed broadcasts
                        foreach (var broadcastAddr in GetSubnetBroadcastAddresses())
                        {
                            try
                            {
                                await _broadcaster.SendAsync(data, data.Length, new IPEndPoint(broadcastAddr, Port));
                                FileLogger.Log("Discovery", $"BROADCAST sent ({broadcastAddr}:{Port})");
                            }
                            catch (Exception ex)
                            {
                                FileLogger.Log("Discovery", $"Subnet broadcast to {broadcastAddr} failed: {ex.Message}");
                            }
                        }
                    }
                    catch (Exception ex)
                    {
                        FileLogger.Log("Discovery", $"BROADCAST ERROR: {ex.Message}");
                    }
                    await Task.Delay(3000);
                }
            });

            FileLogger.Log("Discovery", "Broadcaster STARTED");
        }
        catch (Exception ex)
        {
            FileLogger.Log("Discovery", $"FATAL: Could not start broadcaster: {ex.Message}");
        }
    }

    private static List<IPAddress> GetSubnetBroadcastAddresses()
    {
        var broadcasts = new List<IPAddress>();
        try
        {
            foreach (var iface in NetworkInterface.GetAllNetworkInterfaces())
            {
                FileLogger.Log("Discovery", $"Interface: {iface.Name} ({iface.NetworkInterfaceType}), Status: {iface.OperationalStatus}");
                foreach (var unicast in iface.GetIPProperties().UnicastAddresses)
                {
                    if (unicast.Address.AddressFamily != AddressFamily.InterNetwork) continue;

                    // Support for interfaces without a mask (e.g. some virtual/tun devices)
                    if (unicast.IPv4Mask == null)
                    {
                        FileLogger.Log("Discovery", $"  IP: {unicast.Address} (No Mask, skipping subnet broadcast)");
                        continue;
                    }

                    FileLogger.Log("Discovery", $"  IP: {unicast.Address}, Mask: {unicast.IPv4Mask}");

                    var ip = unicast.Address.GetAddressBytes();
                    var mask = unicast.IPv4Mask.GetAddressBytes();
                    var broadcast = new byte[4];
                    for (int i = 0; i < 4; i++)
                        broadcast[i] = (byte)(ip[i] | ~mask[i]);

                    broadcasts.Add(new IPAddress(broadcast));
                }
            }
        }
        catch { }
        return broadcasts;
    }

    public void StopDiscovery()
    {
        _isRunning = false;
        _listener?.Close();
        _broadcaster?.Close();
        FileLogger.Log("Discovery", "Stopped");
    }

    public void Dispose() => StopDiscovery();
}
