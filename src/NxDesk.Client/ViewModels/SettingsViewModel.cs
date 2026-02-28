using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using NxDesk.Application.Interfaces;
using NxDesk.Client.Services;

namespace NxDesk.Client.ViewModels;

public partial class SettingsViewModel : ObservableObject
{
    private readonly IConfigurationService _configService;
    private readonly IThemeService _themeService;

    public SettingsViewModel(IConfigurationService configService, IThemeService themeService)
    {
        _configService = configService;
        _themeService = themeService;

        Languages = new ObservableCollection<string> { "Español", "English" };
        FontSizes = new ObservableCollection<string> { "Pequeño", "Normal", "Grande" };

        _selectedLanguageIndex = (int)_configService.Language;
        _selectedFontSizeIndex = (int)_configService.FontSize;
        _isDarkTheme = _themeService.CurrentTheme == AppTheme.Dark;
    }

    public ObservableCollection<string> Languages { get; }
    public ObservableCollection<string> FontSizes { get; }

    private int _selectedLanguageIndex;
    public int SelectedLanguageIndex
    {
        get => _selectedLanguageIndex;
        set
        {
            if (SetProperty(ref _selectedLanguageIndex, value))
            {
                _configService.Language = (AppLanguage)value;
            }
        }
    }

    private int _selectedFontSizeIndex;
    public int SelectedFontSizeIndex
    {
        get => _selectedFontSizeIndex;
        set
        {
            if (SetProperty(ref _selectedFontSizeIndex, value))
            {
                _configService.FontSize = (AppFontSize)value;
            }
        }
    }

    private bool _isDarkTheme;
    public bool IsDarkTheme
    {
        get => _isDarkTheme;
        set
        {
            if (SetProperty(ref _isDarkTheme, value))
            {
                _themeService.SetTheme(value ? AppTheme.Dark : AppTheme.Light);
            }
        }
    }
}
