using System;

namespace NxDesk.Application.Interfaces;

public enum AppLanguage
{
    Spanish,
    English
}

public enum AppFontSize
{
    Small,
    Normal,
    Large
}

public interface IConfigurationService
{
    AppLanguage Language { get; set; }
    AppFontSize FontSize { get; set; }
    string? MachineId { get; set; }
    string? Alias { get; set; }
    bool IsAdmin { get; set; }
    AppTheme Theme { get; set; }
    
    event Action<AppLanguage> LanguageChanged;
    event Action<AppFontSize> FontSizeChanged;
    event Action<bool> IsAdminChanged;
    event Action<AppTheme> ThemeChanged;
    
    void Load();
    void Save();
}
