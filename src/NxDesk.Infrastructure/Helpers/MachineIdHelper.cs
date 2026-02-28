using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Linq;

namespace NxDesk.Infrastructure.Helpers;

public static class MachineIdHelper
{
    public static string GetPersistentId()
    {
        string rawId = GetRawMachineId();
        
        using (var sha = SHA256.Create())
        {
            byte[] hash = sha.ComputeHash(Encoding.UTF8.GetBytes(rawId));
            // Use first 4 bytes to generate a 9-digit number
            uint value = BitConverter.ToUInt32(hash, 0);
            uint nineDigitId = value % 900000000 + 100000000; // Ensures 9 digits (100,000,000 to 999,999,999)
            
            string s = nineDigitId.ToString();
            return $"{s.Substring(0, 3)} {s.Substring(3, 3)} {s.Substring(6, 3)}";
        }
    }

    private static string GetRawMachineId()
    {
        try
        {
            if (OperatingSystem.IsLinux())
            {
                if (File.Exists("/etc/machine-id"))
                    return File.ReadAllText("/etc/machine-id").Trim();
                if (File.Exists("/var/lib/dbus/machine-id"))
                    return File.ReadAllText("/var/lib/dbus/machine-id").Trim();
            }
            else if (OperatingSystem.IsWindows())
            {
                // In a real app we would use Registry, but for now we fallback to MachineName
                // as we might not have access to Microsoft.Win32.Registry here without adding a package.
                return Environment.MachineName; 
            }
        }
        catch { }

        // Fallback to MachineName + ProcessorCount + OSVersion
        return $"{Environment.MachineName}-{Environment.ProcessorCount}-{Environment.OSVersion}";
    }
}
