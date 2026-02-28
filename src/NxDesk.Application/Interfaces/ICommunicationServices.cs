using NxDesk.Core.Models;

namespace NxDesk.Application.Interfaces;

public interface IDiscoveryService
{
    void StartDiscovery(string id, string alias, int port);
    void StopDiscovery();
    event Action<PeerIdentity> OnPeerDiscovered;
}

