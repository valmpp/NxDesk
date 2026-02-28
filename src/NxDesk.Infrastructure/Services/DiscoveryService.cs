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

    public event Action<PeerIdentity>? OnPeerDiscovered;
    private readonly HashSet<string> _knownPeerIds = new();

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
                            // Only fire event for genuinely new peers
                            lock (_knownPeerIds)
                            {
                                if (_knownPeerIds.Add(peer.Id))
                                {
                                    FileLogger.Log("Discovery", $"PEER FOUND: {peer.Alias} (ID={peer.Id}) at {peer.IpAddress}");
                                    OnPeerDiscovered?.Invoke(peer);
                                }
                            }
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
            _broadcaster.EnableBroadcast = true;

            // Log network interfaces
            foreach (var broadcastAddr in GetSubnetBroadcastAddresses())
            {
                FileLogger.Log("Discovery", $"Subnet broadcast target: {broadcastAddr}");
            }

            Task.Run(async () =>
            {
                var peer = new PeerIdentity { Id = _myId, Alias = _myAlias, Port = _myPort };
                var message = JsonConvert.SerializeObject(peer);
                var data = Encoding.UTF8.GetBytes(message);

                await Task.Delay(500);

                while (_isRunning)
                {
                    try
                    {
                        // General broadcast
                        await _broadcaster.SendAsync(data, data.Length, new IPEndPoint(IPAddress.Broadcast, Port));
                        FileLogger.Log("Discovery", $"BROADCAST sent (255.255.255.255:{Port}) - {data.Length} bytes");

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
                if (iface.OperationalStatus != OperationalStatus.Up) continue;
                if (iface.NetworkInterfaceType == NetworkInterfaceType.Loopback) continue;

                foreach (var unicast in iface.GetIPProperties().UnicastAddresses)
                {
                    if (unicast.Address.AddressFamily != AddressFamily.InterNetwork) continue;

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
