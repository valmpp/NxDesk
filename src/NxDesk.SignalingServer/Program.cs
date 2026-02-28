using NxDesk.SignalingServer.Hubs;

var builder = WebApplication.CreateBuilder(args);

// Listen on all interfaces (allows connections from other machines on the LAN)
builder.WebHost.UseUrls("http://0.0.0.0:5000");

// Add SignalR
builder.Services.AddSignalR(options =>
{
    options.EnableDetailedErrors = true;
    // Increase message size limit for video frames (32MB)
    options.MaximumReceiveMessageSize = 32 * 1024 * 1024;
});

// Add CORS — allow any origin for LAN connections
builder.Services.AddCors(options =>
{
    options.AddDefaultPolicy(policy =>
    {
        policy.AllowAnyOrigin()
              .AllowAnyHeader()
              .AllowAnyMethod();
    });
});

var app = builder.Build();

// Enable CORS
app.UseCors();

// Map SignalR hub
app.MapHub<SignalingHub>("/signalinghub");

// Health check endpoint
app.MapGet("/", () => "NxDesk Signaling Server is running");

app.Run();
