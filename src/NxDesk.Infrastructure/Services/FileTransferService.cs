using System;
using System.Collections.Concurrent;
using System.IO;
using System.Threading.Tasks;
using NxDesk.Application.DTOs;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

public class FileTransferService : IFileTransferService
{
    private readonly ConcurrentDictionary<string, string> _activeTransfers = new();

    public async Task<string> InitiateFileTransferAsync(string filePath, string targetDir)
    {
        if (!File.Exists(filePath))
            throw new FileNotFoundException("File not found", filePath);

        var transferId = Guid.NewGuid().ToString();
        _activeTransfers[transferId] = filePath;
        return transferId;
    }

    public async Task<string[]> InitiateFolderTransferAsync(string folderPath, string targetDir)
    {
        if (!Directory.Exists(folderPath))
            throw new DirectoryNotFoundException("Directory not found: " + folderPath);

        var files = Directory.GetFiles(folderPath, "*", SearchOption.AllDirectories);
        var transferIds = new string[files.Length];
        
        for (int i = 0; i < files.Length; i++)
        {
            transferIds[i] = await InitiateFileTransferAsync(files[i], targetDir);
        }

        return transferIds;
    }

    public async Task<byte[]> GetFileChunkAsync(string transferId, long offset, int size)
    {
        if (!_activeTransfers.TryGetValue(transferId, out var filePath))
            throw new KeyNotFoundException("Active transfer not found for ID: " + transferId);

        using var fs = new FileStream(filePath, FileMode.Open, FileAccess.Read, FileShare.Read);
        fs.Position = offset;
        
        var buffer = new byte[Math.Min(size, fs.Length - offset)];
        int read = await fs.ReadAsync(buffer, 0, buffer.Length);
        
        if (read < buffer.Length)
        {
            Array.Resize(ref buffer, read);
        }
        
        return buffer;
    }

    public async Task ProcessIncomingRequestAsync(FileTransferRequest request)
    {
        // Determine base path: if TargetDirectory is absolute, use it directly;
        // otherwise treat it as relative to user profile (legacy behavior)
        string targetBasePath;
        if (Path.IsPathRooted(request.TargetDirectory))
        {
            targetBasePath = request.TargetDirectory;
        }
        else
        {
            targetBasePath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), 
                request.TargetDirectory);
        }
        
        string fullPath;
        if (!string.IsNullOrEmpty(request.RelativePath))
        {
            fullPath = Path.Combine(targetBasePath, request.RelativePath, request.FileName);
        }
        else
        {
            fullPath = Path.Combine(targetBasePath, request.FileName);
        }

        var directory = Path.GetDirectoryName(fullPath);
        if (directory != null && !Directory.Exists(directory))
        {
            Directory.CreateDirectory(directory);
        }

        _activeTransfers[request.TransferId] = fullPath;
        Console.WriteLine($"[File] Receiving: RelativePath='{request.RelativePath}', FileName='{request.FileName}', FullPath='{fullPath}'");
        
        // Ensure file exists for writing chunks
        if (File.Exists(fullPath)) File.Delete(fullPath);
        using var _ = File.Create(fullPath); 
    }

    public async Task WriteChunkAsync(string transferId, byte[] data, long offset, bool isLast)
    {
        if (!_activeTransfers.TryGetValue(transferId, out var filePath))
            throw new KeyNotFoundException("No path mapped for transfer ID: " + transferId);

        using (var fs = new FileStream(filePath, FileMode.OpenOrCreate, FileAccess.Write, FileShare.None))
        {
            fs.Position = offset;
            await fs.WriteAsync(data, 0, data.Length);
        }

        if (isLast)
        {
            _activeTransfers.TryRemove(transferId, out _);
        }
    }

    public void CancelTransfer(string transferId)
    {
        _activeTransfers.TryRemove(transferId, out _);
    }

    public async Task<ListDirectoryResponse> ListDirectoryAsync(string path)
    {
        var response = new ListDirectoryResponse { CurrentPath = path };
        try
        {
            if (string.IsNullOrEmpty(path))
            {
                // Default to a safe starting point (Home or Root)
                path = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
                response.CurrentPath = path;
            }

            if (!Directory.Exists(path))
            {
                response.Error = "Directory not found";
                return response;
            }

            var dirInfo = new DirectoryInfo(path);
            foreach (var dir in dirInfo.GetDirectories())
            {
                response.Items.Add(new DirectoryItemDto
                {
                    Name = dir.Name,
                    IsDirectory = true,
                    LastModified = dir.LastWriteTime
                });
            }

            foreach (var file in dirInfo.GetFiles())
            {
                response.Items.Add(new DirectoryItemDto
                {
                    Name = file.Name,
                    IsDirectory = false,
                    Size = file.Length,
                    LastModified = file.LastWriteTime
                });
            }
        }
        catch (Exception ex)
        {
            response.Error = ex.Message;
        }
        return response;
    }
}
