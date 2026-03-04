using NxDesk.Core.Models;

namespace NxDesk.Application.Interfaces;

public interface IDiscoveryService
{
    void StartDiscovery(string id, string alias, int port);
    void StopDiscovery();
    void SetStatus(PeerStatus status);
    event Action<PeerIdentity> OnPeerDiscovered;
}

