using CommunityToolkit.Mvvm.ComponentModel;
using NxDesk.Core.Models;
using System;

namespace NxDesk.Client.ViewModels;

public partial class PeerViewModel : ObservableObject
{
    private readonly PeerIdentity _model;

    public PeerViewModel(PeerIdentity model)
    {
        _model = model;
        _alias = model.Alias;
        _id = model.Id;
        _ipAddress = model.IpAddress;
        _port = model.Port;
        _status = model.Status;
        _lastSeen = model.LastSeen;
    }

    public PeerIdentity Model => _model;

    [ObservableProperty]
    private string _alias;

    [ObservableProperty]
    private string _id;

    [ObservableProperty]
    private string _ipAddress;

    [ObservableProperty]
    private int _port;

    [ObservableProperty]
    private PeerStatus _status;

    [ObservableProperty]
    private DateTime _lastSeen;

    public void Update(PeerIdentity peer)
    {
        Alias = peer.Alias;
        IpAddress = peer.IpAddress;
        Port = peer.Port;
        Status = peer.Status;
        LastSeen = DateTime.Now;
        
        // Update underlying model
        _model.Alias = peer.Alias;
        _model.IpAddress = peer.IpAddress;
        _model.Port = peer.Port;
        _model.Status = peer.Status;
    }
}
