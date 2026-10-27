using System;
using System.IO;
using Newtonsoft.Json;
using NxDesk.Application.Interfaces;

namespace NxDesk.Infrastructure.Services;

public class ConfigurationData
{
    public AppLanguage Language { get; set; } = AppLanguage.Spanish;
    public AppFontSize FontSize { get; set; } = AppFontSize.Normal;
    public string? MachineId { get; set; }
    public string? Alias { get; set; }
    public bool IsAdmin { get; set; }
    public AppTheme Theme { get; set; } = AppTheme.Dark;
}

public class ConfigurationService : IConfigurationService
{
    private readonly string _configPath;
    private ConfigurationData _data = new();

    public ConfigurationService()
    {
        var appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        var configFolder = Path.Combine(appData, "NxDesk");
        if (!Directory.Exists(configFolder)) Directory.CreateDirectory(configFolder);
        _configPath = Path.Combine(configFolder, "settings.json");
        Load();
    }

    public AppLanguage Language
    {
        get => _data.Language;
        set
        {
            if (_data.Language != value)
            {
                _data.Language = value;
                LanguageChanged?.Invoke(value);
                Save();
            }
        }
    }

    public AppFontSize FontSize
    {
        get => _data.FontSize;
        set
        {
            if (_data.FontSize != value)
            {
                _data.FontSize = value;
                FontSizeChanged?.Invoke(value);
                Save();
            }
        }
    }

    public string? MachineId
    {
        get => _data.MachineId;
        set
        {
            if (_data.MachineId != value)
            {
                _data.MachineId = value;
                Save();
            }
        }
    }

    public string? Alias
    {
        get => _data.Alias;
        set
        {
            if (_data.Alias != value)
            {
                _data.Alias = value;
                Save();
            }
        }
    }

    public bool IsAdmin
    {
        get => _data.IsAdmin;
        set
        {
            if (_data.IsAdmin != value)
            {
                _data.IsAdmin = value;
                IsAdminChanged?.Invoke(value);
                Save();
            }
        }
    }

    public AppTheme Theme
    {
        get => _data.Theme;
        set
        {
            if (_data.Theme != value)
            {
                _data.Theme = value;
                ThemeChanged?.Invoke(value);
                Save();
            }
        }
    }

    public event Action<AppLanguage>? LanguageChanged;
    public event Action<AppFontSize>? FontSizeChanged;
    public event Action<bool>? IsAdminChanged;
    public event Action<AppTheme>? ThemeChanged;

    public void Load()
    {
        try
        {
            if (File.Exists(_configPath))
            {
                var json = File.ReadAllText(_configPath);
                _data = JsonConvert.DeserializeObject<ConfigurationData>(json) ?? new();
            }
        }
        catch
        {
            _data = new();
        }
    }

    public void Save()
    {
        try
        {
            var json = JsonConvert.SerializeObject(_data, Formatting.Indented);
            File.WriteAllText(_configPath, json);
        }
        catch { /* Ignore save errors for now */ }
    }
}
