namespace NxDesk.Core.Models;

public class PeerIdentity
{
    public string Id { get; set; } = string.Empty;
    public string Alias { get; set; } = string.Empty;
    public string IpAddress { get; set; } = string.Empty;
    public int Port { get; set; } = 5000;
}

public class SignalingMessage
{
    public string Type { get; set; } = string.Empty;
    public string Payload { get; set; } = string.Empty;

    /// <summary>
    /// Fallback alias for Payload to support older client versions.
    /// </summary>
    public string Data { get => Payload; set => Payload = value; }

    public byte[]? ByteData { get; set; }
    public string? SenderId { get; set; }
    public string? SenderAlias { get; set; }
    public System.Collections.Generic.Dictionary<string, string>? Metadata { get; set; }
}
