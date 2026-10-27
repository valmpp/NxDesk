using System;

namespace NxDesk.Application.DTOs;

public enum FileTransferStatus
{
    Pending,
    Accepted,
    Rejected,
    InProgress,
    Completed,
    Error,
    Cancelled
}

public class FileTransferRequest
{
    public string TransferId { get; set; } = Guid.NewGuid().ToString();
    public string FileName { get; set; } = string.Empty;
    public long TotalSize { get; set; }
    public bool IsDirectory { get; set; }
    public string? RelativePath { get; set; } // For folder structure preservation
    public string TargetDirectory { get; set; } = "Downloads/NxDesk";
}

public class FileChunkDto
{
    public string TransferId { get; set; } = string.Empty;
    public byte[] Data { get; set; } = Array.Empty<byte>();
    public long Offset { get; set; }
    public bool IsLast { get; set; }
}

public class FileTransferResponse
{
    public string TransferId { get; set; } = string.Empty;
    public FileTransferStatus Status { get; set; }
    public string? Message { get; set; }
}
public class RemoteFolderRequest
{
    public string RequestId { get; set; } = Guid.NewGuid().ToString();
}

public class RemoteFolderResponse
{
    public string RequestId { get; set; } = string.Empty;
    public string SelectedPath { get; set; } = string.Empty;
    public bool Cancelled { get; set; }
}
public class DirectoryItemDto
{
    public string Name { get; set; } = string.Empty;
    public bool IsDirectory { get; set; }
    public long Size { get; set; }
    public DateTime LastModified { get; set; }
}

public class ListDirectoryRequest
{
    public string RequestId { get; set; } = Guid.NewGuid().ToString();
    public string Path { get; set; } = string.Empty; // Empty means Root/Home
}

public class ListDirectoryResponse
{
    public string RequestId { get; set; } = string.Empty;
    public string CurrentPath { get; set; } = string.Empty;
    public List<DirectoryItemDto> Items { get; set; } = new();
    public string? Error { get; set; }
}
