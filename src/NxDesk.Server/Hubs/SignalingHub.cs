using Microsoft.AspNetCore.SignalR;
using NxDesk.Core.Models;

namespace NxDesk.Server.Hubs;

public class SignalingHub : Hub
{
    public async Task JoinRoom(string roomId)
    {
        await Groups.AddToGroupAsync(Context.ConnectionId, roomId);
    }

    public async Task RelayMessage(string roomId, SignalingMessage message)
    {
        Console.WriteLine($"[Hub] Relay: Room={roomId}, Type={message.Type}, Sender={message.SenderAlias}");
        message.SenderId = Context.ConnectionId;
        await Clients.OthersInGroup(roomId).SendAsync("ReceiveMessage", message);
    }
}
