using System.Threading.Tasks;
using NxDesk.Application.DTOs;

namespace NxDesk.Application.Interfaces;

public interface IFileTransferService
{
    Task<string> InitiateFileTransferAsync(string filePath, string targetDir);
    Task<string[]> InitiateFolderTransferAsync(string folderPath, string targetDir);
    Task<byte[]> GetFileChunkAsync(string transferId, long offset, int size);
    Task ProcessIncomingRequestAsync(FileTransferRequest request);
    Task WriteChunkAsync(string transferId, byte[] data, long offset, bool isLast);
    Task<ListDirectoryResponse> ListDirectoryAsync(string path);
    void CancelTransfer(string transferId);
}
