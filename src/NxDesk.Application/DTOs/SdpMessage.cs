namespace NxDesk.Application.DTOs;

/// <summary>
/// Signaling message for WebRTC-like peer coordination.
/// Used for offers, answers, and frame transmission.
/// </summary>
public class SdpMessage
{
    /// <summary>
    /// Message type: "offer", "answer", "frame", "ice-candidate", etc.
    /// </summary>
    public string Type { get; set; } = string.Empty;

    /// <summary>
    /// Message payload (Base64 encoded string - used for JSON signaling)
    /// </summary>
    public string Payload { get; set; } = string.Empty;

    /// <summary>
    /// Binary payload (preferred for video frames to avoid Base64 overhead)
    /// </summary>
    public byte[]? ByteData { get; set; }

    /// <summary>
    /// Sender's connection ID (optional)
    /// </summary>
    public string? SenderId { get; set; }

    /// <summary>
    /// Sender's display name (optional)
    /// </summary>
    public string? SenderAlias { get; set; }

    /// <summary>
    /// Additional metadata (width, height, codec, etc.)
    /// </summary>
    public Dictionary<string, string>? Metadata { get; set; }

    /// <summary>
    /// Deserializes the JSON payload into the specified type.
    /// </summary>
    public T? GetPayload<T>()
    {
        if (string.IsNullOrEmpty(Payload)) return default;
        try
        {
            return System.Text.Json.JsonSerializer.Deserialize<T>(Payload);
        }
        catch
        {
            return default;
        }
    }

    /// <summary>
    /// Serializes the given object into the JSON payload.
    /// </summary>
    public void SetPayload<T>(T payload)
    {
        if (payload != null)
        {
            Payload = System.Text.Json.JsonSerializer.Serialize(payload);
        }
    }
}
